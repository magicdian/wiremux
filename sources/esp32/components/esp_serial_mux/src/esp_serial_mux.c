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
    uint8_t channel_id;
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
    uint32_t next_sequence;
    uint32_t dropped_count;
} channel_state_t;

typedef struct {
    bool initialized;
    bool started;
    esp_serial_mux_config_t config;
    QueueHandle_t queue;
    TaskHandle_t task;
    SemaphoreHandle_t lock;
    channel_state_t channels[ESP_SERIAL_MUX_MAX_CHANNELS];
} mux_context_t;

static mux_context_t s_mux;

static esp_err_t default_stdout_transport_write(const uint8_t *data,
                                                size_t len,
                                                uint32_t timeout_ms,
                                                void *user_ctx);
static void mux_task(void *arg);
static void free_pending_item(pending_item_t *item);
static size_t envelope_encoded_len(const pending_item_t *item);
static esp_err_t encode_envelope(const pending_item_t *item,
                                 uint8_t *out,
                                 size_t out_capacity,
                                 size_t *written);
static esp_err_t enqueue_item(pending_item_t *item,
                              const esp_serial_mux_channel_config_t *channel,
                              uint32_t timeout_ms);

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
