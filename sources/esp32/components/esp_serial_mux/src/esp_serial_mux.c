#include "esp_serial_mux.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "esp_timer.h"
#include "esp_serial_mux_frame.h"
#include "sdkconfig.h"

#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
#include "driver/usb_serial_jtag.h"
#endif

#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

typedef struct {
    uint32_t channel_id;
    uint32_t direction;
    esp_serial_mux_payload_kind_t kind;
    uint32_t flags;
    uint32_t sequence;
    uint64_t timestamp_us;
    size_t payload_len;
    uint8_t payload[];
} pending_item_t;

typedef struct {
    bool registered;
    esp_serial_mux_channel_config_t config;
    esp_serial_mux_input_handler_t input_handler;
    void *input_handler_ctx;
    uint32_t next_sequence;
    uint32_t dropped_count;
} channel_state_t;

typedef struct {
    uint8_t channel_id;
    uint32_t direction;
    uint32_t sequence;
    uint64_t timestamp_us;
    esp_serial_mux_payload_kind_t kind;
    uint32_t flags;
    const uint8_t *payload;
    size_t payload_len;
} inbound_envelope_t;

typedef struct {
    bool initialized;
    bool started;
    esp_serial_mux_config_t config;
    QueueHandle_t queue;
    TaskHandle_t task;
    TaskHandle_t input_task;
    SemaphoreHandle_t lock;
    uint8_t *rx_buffer;
    size_t rx_len;
    size_t rx_capacity;
    channel_state_t channels[ESP_SERIAL_MUX_MAX_CHANNELS];
} mux_context_t;

static mux_context_t s_mux;

static esp_err_t default_stdout_transport_write(const uint8_t *data,
                                                size_t len,
                                                uint32_t timeout_ms,
                                                void *user_ctx);
static esp_err_t default_stdin_transport_read(uint8_t *data,
                                              size_t capacity,
                                              size_t *read_len,
                                              uint32_t timeout_ms,
                                              void *user_ctx);
static esp_err_t prepare_default_transport(const esp_serial_mux_config_t *config);
static void mux_task(void *arg);
static void mux_input_task(void *arg);
static void free_pending_item(pending_item_t *item);
static size_t envelope_encoded_len(const pending_item_t *item);
static esp_err_t encode_envelope(const pending_item_t *item,
                                 uint8_t *out,
                                 size_t out_capacity,
                                 size_t *written);
static esp_err_t enqueue_item(pending_item_t *item,
                              const esp_serial_mux_channel_config_t *channel,
                              uint32_t timeout_ms);
static void parse_rx_buffer_locked(void);
static esp_err_t dispatch_input_envelope_locked(const inbound_envelope_t *envelope);
static esp_err_t decode_inbound_envelope(const uint8_t *data,
                                         size_t len,
                                         inbound_envelope_t *envelope);

void esp_serial_mux_config_init(esp_serial_mux_config_t *config)
{
    if (config == NULL) {
        return;
    }

    memset(config, 0, sizeof(*config));
    config->queue_depth = 32;
    config->max_payload_len = 512;
    config->default_write_timeout_ms = 20;
    config->task_stack_size = 4096;
    config->task_priority = 5;
    config->task_core_id = tskNO_AFFINITY;
    config->transport.write = default_stdout_transport_write;
    config->transport.read = default_stdin_transport_read;
}

esp_err_t esp_serial_mux_init(const esp_serial_mux_config_t *config)
{
    esp_serial_mux_config_t resolved;
    esp_serial_mux_config_init(&resolved);
    if (config != NULL) {
        resolved = *config;
        if (resolved.transport.write == NULL) {
            resolved.transport.write = default_stdout_transport_write;
        }
    }

    if (resolved.queue_depth == 0 || resolved.max_payload_len == 0) {
        return ESP_ERR_INVALID_ARG;
    }

    esp_err_t err = prepare_default_transport(&resolved);
    if (err != ESP_OK) {
        return err;
    }

    if (s_mux.initialized) {
        return ESP_ERR_INVALID_STATE;
    }

    memset(&s_mux, 0, sizeof(s_mux));
    s_mux.config = resolved;
    s_mux.lock = xSemaphoreCreateMutex();
    if (s_mux.lock == NULL) {
        return ESP_ERR_NO_MEM;
    }

    s_mux.queue = xQueueCreate(resolved.queue_depth, sizeof(pending_item_t *));
    if (s_mux.queue == NULL) {
        vSemaphoreDelete(s_mux.lock);
        memset(&s_mux, 0, sizeof(s_mux));
        return ESP_ERR_NO_MEM;
    }

    s_mux.rx_capacity = ESP_SERIAL_MUX_FRAME_HEADER_LEN + resolved.max_payload_len;
    s_mux.rx_buffer = malloc(s_mux.rx_capacity);
    if (s_mux.rx_buffer == NULL) {
        vQueueDelete(s_mux.queue);
        vSemaphoreDelete(s_mux.lock);
        memset(&s_mux, 0, sizeof(s_mux));
        return ESP_ERR_NO_MEM;
    }

    s_mux.initialized = true;
    return ESP_OK;
}

esp_err_t esp_serial_mux_start(void)
{
    if (!s_mux.initialized || s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }

    BaseType_t result = xTaskCreatePinnedToCore(mux_task,
                                                "esp_serial_mux",
                                                s_mux.config.task_stack_size,
                                                NULL,
                                                s_mux.config.task_priority,
                                                &s_mux.task,
                                                s_mux.config.task_core_id);
    if (result != pdPASS) {
        return ESP_ERR_NO_MEM;
    }

    s_mux.started = true;

    if (s_mux.config.transport.read != NULL) {
        result = xTaskCreatePinnedToCore(mux_input_task,
                                         "esp_serial_mux_rx",
                                         s_mux.config.task_stack_size,
                                         NULL,
                                         s_mux.config.task_priority,
                                         &s_mux.input_task,
                                         s_mux.config.task_core_id);
        if (result != pdPASS) {
            s_mux.started = false;
            pending_item_t *sentinel = NULL;
            (void)xQueueSend(s_mux.queue, &sentinel, portMAX_DELAY);
            s_mux.task = NULL;
            return ESP_ERR_NO_MEM;
        }
    }

    return ESP_OK;
}

esp_err_t esp_serial_mux_stop(void)
{
    if (!s_mux.initialized || !s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }

    pending_item_t *sentinel = NULL;
    (void)xQueueSend(s_mux.queue, &sentinel, portMAX_DELAY);
    s_mux.started = false;
    s_mux.task = NULL;
    return ESP_OK;
}

esp_err_t esp_serial_mux_register_channel(const esp_serial_mux_channel_config_t *config)
{
    if (!s_mux.initialized || config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (config->channel_id >= ESP_SERIAL_MUX_MAX_CHANNELS || config->directions == 0) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    s_mux.channels[config->channel_id].registered = true;
    s_mux.channels[config->channel_id].config = *config;
    xSemaphoreGive(s_mux.lock);

    return ESP_OK;
}

esp_err_t esp_serial_mux_register_input_handler(uint8_t channel_id,
                                                esp_serial_mux_input_handler_t handler,
                                                void *user_ctx)
{
    if (!s_mux.initialized || handler == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (channel_id >= ESP_SERIAL_MUX_MAX_CHANNELS) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    channel_state_t *channel = &s_mux.channels[channel_id];
    if (!channel->registered ||
        (channel->config.directions & ESP_SERIAL_MUX_DIRECTION_INPUT) == 0) {
        xSemaphoreGive(s_mux.lock);
        return ESP_ERR_NOT_FOUND;
    }

    channel->input_handler = handler;
    channel->input_handler_ctx = user_ctx;
    xSemaphoreGive(s_mux.lock);
    return ESP_OK;
}

esp_err_t esp_serial_mux_receive_bytes(const uint8_t *data, size_t len)
{
    if (!s_mux.initialized || (len > 0 && data == NULL)) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    for (size_t offset = 0; offset < len; ++offset) {
        if (s_mux.rx_len >= s_mux.rx_capacity) {
            s_mux.rx_len = 0;
        }
        s_mux.rx_buffer[s_mux.rx_len++] = data[offset];
        parse_rx_buffer_locked();
    }
    xSemaphoreGive(s_mux.lock);
    return ESP_OK;
}

esp_err_t esp_serial_mux_write(uint8_t channel_id,
                               uint32_t direction,
                               esp_serial_mux_payload_kind_t kind,
                               uint32_t flags,
                               const uint8_t *payload,
                               size_t payload_len,
                               uint32_t timeout_ms)
{
    if (!s_mux.initialized || !s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }
    if (channel_id >= ESP_SERIAL_MUX_MAX_CHANNELS || (payload_len > 0 && payload == NULL)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (payload_len > s_mux.config.max_payload_len) {
        return ESP_ERR_INVALID_SIZE;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    channel_state_t channel_state = s_mux.channels[channel_id];
    if (channel_state.registered) {
        s_mux.channels[channel_id].next_sequence++;
        channel_state.next_sequence = s_mux.channels[channel_id].next_sequence;
    }
    xSemaphoreGive(s_mux.lock);

    if (!channel_state.registered || (channel_state.config.directions & direction) == 0) {
        return ESP_ERR_NOT_FOUND;
    }

    pending_item_t *item = calloc(1, sizeof(*item) + payload_len);
    if (item == NULL) {
        return ESP_ERR_NO_MEM;
    }

    item->channel_id = channel_id;
    item->direction = direction;
    item->kind = kind;
    item->flags = flags;
    item->sequence = channel_state.next_sequence;
    item->timestamp_us = (uint64_t)esp_timer_get_time();
    item->payload_len = payload_len;
    if (payload_len > 0) {
        memcpy(item->payload, payload, payload_len);
    }

    return enqueue_item(item, &channel_state.config, timeout_ms);
}

esp_err_t esp_serial_mux_write_text(uint8_t channel_id,
                                    uint32_t direction,
                                    const char *text,
                                    uint32_t timeout_ms)
{
    if (text == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    return esp_serial_mux_write(channel_id,
                                direction,
                                ESP_SERIAL_MUX_PAYLOAD_KIND_TEXT,
                                0,
                                (const uint8_t *)text,
                                strlen(text),
                                timeout_ms);
}

esp_err_t esp_serial_mux_emit_manifest(uint32_t timeout_ms)
{
    char manifest[512];
    size_t offset = 0;

    offset += snprintf(manifest + offset,
                       sizeof(manifest) - offset,
                       "esp_serial_mux protocol=%u max_channels=%u\n",
                       ESP_SERIAL_MUX_FRAME_VERSION,
                       ESP_SERIAL_MUX_MAX_CHANNELS);

    for (uint8_t i = 0; i < ESP_SERIAL_MUX_MAX_CHANNELS && offset < sizeof(manifest); ++i) {
        if (!s_mux.channels[i].registered) {
            continue;
        }
        offset += snprintf(manifest + offset,
                           sizeof(manifest) - offset,
                           "channel=%u name=%s directions=0x%lx\n",
                           i,
                           s_mux.channels[i].config.name != NULL ? s_mux.channels[i].config.name : "",
                           (unsigned long)s_mux.channels[i].config.directions);
    }

    if (offset >= sizeof(manifest)) {
        offset = sizeof(manifest) - 1;
        manifest[offset] = '\0';
    }

    return esp_serial_mux_write(ESP_SERIAL_MUX_CHANNEL_SYSTEM,
                                ESP_SERIAL_MUX_DIRECTION_OUTPUT,
                                ESP_SERIAL_MUX_PAYLOAD_KIND_CONTROL,
                                0,
                                (const uint8_t *)manifest,
                                strlen(manifest),
                                timeout_ms);
}

static esp_err_t enqueue_item(pending_item_t *item,
                              const esp_serial_mux_channel_config_t *channel,
                              uint32_t timeout_ms)
{
    TickType_t wait_ticks = pdMS_TO_TICKS(timeout_ms);

    if (channel->backpressure_policy == ESP_SERIAL_MUX_BACKPRESSURE_DROP_NEWEST) {
        wait_ticks = 0;
    }

    if (xQueueSend(s_mux.queue, &item, wait_ticks) == pdTRUE) {
        return ESP_OK;
    }

    if (channel->backpressure_policy == ESP_SERIAL_MUX_BACKPRESSURE_DROP_OLDEST) {
        pending_item_t *old_item = NULL;
        if (xQueueReceive(s_mux.queue, &old_item, 0) == pdTRUE) {
            free_pending_item(old_item);
            if (xQueueSend(s_mux.queue, &item, 0) == pdTRUE) {
                return ESP_OK;
            }
        }
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    if (item->channel_id < ESP_SERIAL_MUX_MAX_CHANNELS) {
        s_mux.channels[item->channel_id].dropped_count++;
    }
    xSemaphoreGive(s_mux.lock);

    free_pending_item(item);
    return ESP_ERR_TIMEOUT;
}

static void mux_task(void *arg)
{
    (void)arg;

    while (true) {
        pending_item_t *item = NULL;
        if (xQueueReceive(s_mux.queue, &item, portMAX_DELAY) != pdTRUE) {
            continue;
        }
        if (item == NULL) {
            break;
        }

        const size_t envelope_len = envelope_encoded_len(item);
        uint8_t *envelope = malloc(envelope_len);
        if (envelope == NULL) {
            free_pending_item(item);
            continue;
        }

        size_t envelope_written = 0;
        if (encode_envelope(item, envelope, envelope_len, &envelope_written) != ESP_OK) {
            free(envelope);
            free_pending_item(item);
            continue;
        }

        const size_t frame_len = esp_serial_mux_frame_encoded_len(envelope_written);
        uint8_t *frame = malloc(frame_len);
        if (frame != NULL) {
            size_t written = 0;
            const esp_serial_mux_frame_header_t header = {
                .version = ESP_SERIAL_MUX_FRAME_VERSION,
                .flags = (uint8_t)(item->flags & 0xffu),
            };
            if (esp_serial_mux_frame_encode(&header,
                                            envelope,
                                            envelope_written,
                                            frame,
                                            frame_len,
                                            &written) == ESP_OK) {
                (void)s_mux.config.transport.write(frame,
                                                   written,
                                                   s_mux.config.default_write_timeout_ms,
                                                   s_mux.config.transport.user_ctx);
            }
            free(frame);
        }
        free(envelope);
        free_pending_item(item);
    }

    vTaskDelete(NULL);
}

static void mux_input_task(void *arg)
{
    (void)arg;
    uint8_t buffer[128];

    while (s_mux.started) {
        size_t read_len = 0;
        esp_err_t err = s_mux.config.transport.read(buffer,
                                                    sizeof(buffer),
                                                    &read_len,
                                                    20,
                                                    s_mux.config.transport.user_ctx);
        if (err == ESP_OK && read_len > 0) {
            (void)esp_serial_mux_receive_bytes(buffer, read_len);
        } else {
            vTaskDelay(pdMS_TO_TICKS(10));
        }
    }

    vTaskDelete(NULL);
}

static uint32_t read_le32(const uint8_t *data)
{
    return (uint32_t)data[0] |
           ((uint32_t)data[1] << 8) |
           ((uint32_t)data[2] << 16) |
           ((uint32_t)data[3] << 24);
}

static void rx_drop_prefix(size_t len)
{
    if (len >= s_mux.rx_len) {
        s_mux.rx_len = 0;
        return;
    }
    memmove(s_mux.rx_buffer, s_mux.rx_buffer + len, s_mux.rx_len - len);
    s_mux.rx_len -= len;
}

static size_t find_magic_pos(void)
{
    if (s_mux.rx_len < ESP_SERIAL_MUX_MAGIC_LEN) {
        return SIZE_MAX;
    }
    for (size_t i = 0; i <= s_mux.rx_len - ESP_SERIAL_MUX_MAGIC_LEN; ++i) {
        if (memcmp(s_mux.rx_buffer + i, ESP_SERIAL_MUX_MAGIC, ESP_SERIAL_MUX_MAGIC_LEN) == 0) {
            return i;
        }
    }
    return SIZE_MAX;
}

static size_t magic_prefix_suffix_len(void)
{
    const size_t max_len = s_mux.rx_len < ESP_SERIAL_MUX_MAGIC_LEN - 1
                               ? s_mux.rx_len
                               : ESP_SERIAL_MUX_MAGIC_LEN - 1;
    for (size_t len = max_len; len > 0; --len) {
        if (memcmp(s_mux.rx_buffer + s_mux.rx_len - len, ESP_SERIAL_MUX_MAGIC, len) == 0) {
            return len;
        }
    }
    return 0;
}

static void parse_rx_buffer_locked(void)
{
    while (s_mux.rx_len > 0) {
        size_t magic_pos = find_magic_pos();
        if (magic_pos == SIZE_MAX) {
            const size_t keep_len = magic_prefix_suffix_len();
            rx_drop_prefix(s_mux.rx_len - keep_len);
            return;
        }
        if (magic_pos > 0) {
            rx_drop_prefix(magic_pos);
            continue;
        }
        if (s_mux.rx_len < ESP_SERIAL_MUX_FRAME_HEADER_LEN) {
            return;
        }

        const uint8_t version = s_mux.rx_buffer[4];
        if (version != ESP_SERIAL_MUX_FRAME_VERSION) {
            rx_drop_prefix(1);
            continue;
        }

        const size_t payload_len = (size_t)read_le32(&s_mux.rx_buffer[6]);
        if (payload_len > s_mux.config.max_payload_len) {
            rx_drop_prefix(1);
            continue;
        }

        const size_t total_len = ESP_SERIAL_MUX_FRAME_HEADER_LEN + payload_len;
        if (s_mux.rx_len < total_len) {
            return;
        }

        const uint32_t expected_crc = read_le32(&s_mux.rx_buffer[10]);
        const uint8_t *payload = &s_mux.rx_buffer[ESP_SERIAL_MUX_FRAME_HEADER_LEN];
        if (esp_serial_mux_crc32(payload, payload_len) != expected_crc) {
            rx_drop_prefix(total_len);
            continue;
        }

        inbound_envelope_t envelope = {0};
        esp_err_t err = decode_inbound_envelope(payload, payload_len, &envelope);
        if (err == ESP_OK) {
            (void)dispatch_input_envelope_locked(&envelope);
        }
        rx_drop_prefix(total_len);
    }
}

static esp_err_t dispatch_input_envelope_locked(const inbound_envelope_t *envelope)
{
    if (envelope == NULL ||
        envelope->direction != ESP_SERIAL_MUX_DIRECTION_INPUT ||
        envelope->channel_id >= ESP_SERIAL_MUX_MAX_CHANNELS ||
        envelope->payload_len > s_mux.config.max_payload_len) {
        return ESP_ERR_INVALID_ARG;
    }

    channel_state_t *channel = &s_mux.channels[envelope->channel_id];
    if (!channel->registered ||
        (channel->config.directions & ESP_SERIAL_MUX_DIRECTION_INPUT) == 0 ||
        channel->input_handler == NULL) {
        return ESP_ERR_NOT_FOUND;
    }

    esp_serial_mux_input_handler_t handler = channel->input_handler;
    void *handler_ctx = channel->input_handler_ctx;
    const uint8_t channel_id = (uint8_t)envelope->channel_id;
    const uint8_t *payload = envelope->payload;
    const size_t payload_len = envelope->payload_len;

    xSemaphoreGive(s_mux.lock);
    esp_err_t err = handler(channel_id, payload, payload_len, handler_ctx);
    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    return err;
}

static esp_err_t read_varint_inbound(const uint8_t *data,
                                     size_t len,
                                     size_t *cursor,
                                     uint64_t *value)
{
    uint64_t result = 0;
    for (uint8_t shift = 0; shift < 64; shift += 7) {
        if (*cursor >= len) {
            return ESP_ERR_INVALID_SIZE;
        }
        const uint8_t byte = data[(*cursor)++];
        result |= ((uint64_t)(byte & 0x7fu)) << shift;
        if ((byte & 0x80u) == 0) {
            *value = result;
            return ESP_OK;
        }
    }
    return ESP_ERR_INVALID_SIZE;
}

static esp_err_t read_bytes_inbound(const uint8_t *data,
                                    size_t len,
                                    size_t *cursor,
                                    const uint8_t **value,
                                    size_t *value_len)
{
    uint64_t field_len = 0;
    esp_err_t err = read_varint_inbound(data, len, cursor, &field_len);
    if (err != ESP_OK) {
        return err;
    }
    if (field_len > (uint64_t)(len - *cursor)) {
        return ESP_ERR_INVALID_SIZE;
    }
    *value = &data[*cursor];
    *value_len = (size_t)field_len;
    *cursor += (size_t)field_len;
    return ESP_OK;
}

static esp_err_t decode_inbound_envelope(const uint8_t *data,
                                         size_t len,
                                         inbound_envelope_t *envelope)
{
    if (data == NULL || envelope == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        esp_err_t err = read_varint_inbound(data, len, &cursor, &key);
        if (err != ESP_OK) {
            return err;
        }

        const uint32_t field_number = (uint32_t)(key >> 3);
        const uint32_t wire_type = (uint32_t)(key & 0x07u);
        uint64_t varint = 0;

        switch (wire_type) {
        case 0:
            err = read_varint_inbound(data, len, &cursor, &varint);
            if (err != ESP_OK) {
                return err;
            }
            switch (field_number) {
            case 1:
                envelope->channel_id = (uint32_t)varint;
                break;
            case 2:
                envelope->direction = (uint32_t)varint;
                break;
            case 3:
                envelope->sequence = (uint32_t)varint;
                break;
            case 4:
                envelope->timestamp_us = varint;
                break;
            case 5:
                envelope->kind = (esp_serial_mux_payload_kind_t)varint;
                break;
            case 8:
                envelope->flags = (uint32_t)varint;
                break;
            default:
                break;
            }
            break;
        case 2: {
            const uint8_t *field = NULL;
            size_t field_len = 0;
            err = read_bytes_inbound(data, len, &cursor, &field, &field_len);
            if (err != ESP_OK) {
                return err;
            }
            if (field_number == 7) {
                envelope->payload = field;
                envelope->payload_len = field_len;
            }
            break;
        }
        default:
            return ESP_ERR_NOT_SUPPORTED;
        }
    }

    if (envelope->payload == NULL && envelope->payload_len != 0) {
        return ESP_ERR_INVALID_SIZE;
    }
    return ESP_OK;
}

static void free_pending_item(pending_item_t *item)
{
    free(item);
}

static size_t varint_len(uint64_t value)
{
    size_t len = 1;
    while (value >= 0x80u) {
        value >>= 7;
        len++;
    }
    return len;
}

static size_t varint_field_len(uint32_t field_number, uint64_t value)
{
    return varint_len(((uint64_t)field_number << 3) | 0u) + varint_len(value);
}

static size_t bytes_field_len(uint32_t field_number, size_t len)
{
    return varint_len(((uint64_t)field_number << 3) | 2u) + varint_len(len) + len;
}

static uint8_t *write_varint(uint8_t *out, uint64_t value)
{
    while (value >= 0x80u) {
        *out++ = (uint8_t)(value | 0x80u);
        value >>= 7;
    }
    *out++ = (uint8_t)value;
    return out;
}

static uint8_t *write_varint_field(uint8_t *out, uint32_t field_number, uint64_t value)
{
    out = write_varint(out, ((uint64_t)field_number << 3) | 0u);
    return write_varint(out, value);
}

static uint8_t *write_bytes_field(uint8_t *out, uint32_t field_number, const uint8_t *data, size_t len)
{
    out = write_varint(out, ((uint64_t)field_number << 3) | 2u);
    out = write_varint(out, len);
    if (len > 0) {
        memcpy(out, data, len);
        out += len;
    }
    return out;
}

static size_t envelope_encoded_len(const pending_item_t *item)
{
    return varint_field_len(1, item->channel_id) +
           varint_field_len(2, item->direction) +
           varint_field_len(3, item->sequence) +
           varint_field_len(4, item->timestamp_us) +
           varint_field_len(5, item->kind) +
           bytes_field_len(7, item->payload_len) +
           varint_field_len(8, item->flags);
}

static esp_err_t encode_envelope(const pending_item_t *item,
                                 uint8_t *out,
                                 size_t out_capacity,
                                 size_t *written)
{
    if (item == NULL || out == NULL || written == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    const size_t required = envelope_encoded_len(item);
    if (out_capacity < required) {
        return ESP_ERR_INVALID_SIZE;
    }

    uint8_t *cursor = out;
    cursor = write_varint_field(cursor, 1, item->channel_id);
    cursor = write_varint_field(cursor, 2, item->direction);
    cursor = write_varint_field(cursor, 3, item->sequence);
    cursor = write_varint_field(cursor, 4, item->timestamp_us);
    cursor = write_varint_field(cursor, 5, item->kind);
    cursor = write_bytes_field(cursor, 7, item->payload, item->payload_len);
    cursor = write_varint_field(cursor, 8, item->flags);

    *written = (size_t)(cursor - out);
    return ESP_OK;
}

static esp_err_t default_stdout_transport_write(const uint8_t *data,
                                                size_t len,
                                                uint32_t timeout_ms,
                                                void *user_ctx)
{
    (void)user_ctx;

#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
    const TickType_t wait_ticks = pdMS_TO_TICKS(timeout_ms);
    size_t written = 0;
    while (written < len) {
        int result = usb_serial_jtag_write_bytes(data + written, len - written, wait_ticks);
        if (result < 0) {
            return ESP_FAIL;
        }
        if (result == 0) {
            return ESP_ERR_TIMEOUT;
        }
        written += (size_t)result;
    }
    return ESP_OK;
#else
    (void)timeout_ms;
    size_t written = 0;
    while (written < len) {
        ssize_t result = write(STDOUT_FILENO, data + written, len - written);
        if (result < 0) {
            return errno == EAGAIN ? ESP_ERR_TIMEOUT : ESP_FAIL;
        }
        written += (size_t)result;
    }

    return ESP_OK;
#endif
}

static esp_err_t prepare_default_transport(const esp_serial_mux_config_t *config)
{
    if (config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
    if (config->transport.read != default_stdin_transport_read &&
        config->transport.write != default_stdout_transport_write) {
        return ESP_OK;
    }

    if (usb_serial_jtag_is_driver_installed()) {
        return ESP_OK;
    }

    usb_serial_jtag_driver_config_t driver_config = USB_SERIAL_JTAG_DRIVER_CONFIG_DEFAULT();
    const size_t frame_buffer_size = ESP_SERIAL_MUX_FRAME_HEADER_LEN + config->max_payload_len;
    if (frame_buffer_size > driver_config.rx_buffer_size) {
        driver_config.rx_buffer_size = (uint32_t)frame_buffer_size;
    }
    if (frame_buffer_size > driver_config.tx_buffer_size) {
        driver_config.tx_buffer_size = (uint32_t)frame_buffer_size;
    }

    return usb_serial_jtag_driver_install(&driver_config);
#else
    (void)config;
    return ESP_OK;
#endif
}

static esp_err_t default_stdin_transport_read(uint8_t *data,
                                              size_t capacity,
                                              size_t *read_len,
                                              uint32_t timeout_ms,
                                              void *user_ctx)
{
    (void)user_ctx;
    if (data == NULL || read_len == NULL || capacity == 0) {
        return ESP_ERR_INVALID_ARG;
    }

    *read_len = 0;

#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
    int result = usb_serial_jtag_read_bytes(data, capacity, pdMS_TO_TICKS(timeout_ms));
    if (result < 0) {
        return ESP_FAIL;
    }
    if (result == 0) {
        return ESP_ERR_TIMEOUT;
    }
    *read_len = (size_t)result;
    return ESP_OK;
#else
    (void)timeout_ms;
    ssize_t result = read(STDIN_FILENO, data, capacity);
    if (result < 0) {
        return errno == EAGAIN ? ESP_ERR_TIMEOUT : ESP_FAIL;
    }
    if (result == 0) {
        return ESP_ERR_TIMEOUT;
    }
    *read_len = (size_t)result;
    return ESP_OK;
#endif
}
