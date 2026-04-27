#include "esp_wiremux.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "esp_timer.h"
#include "esp_wiremux_frame.h"
#include "esp_heap_caps.h"
#include "sdkconfig.h"
#include "wiremux_batch.h"
#include "wiremux_compression.h"
#include "wiremux_manifest.h"

#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
#include "driver/usb_serial_jtag.h"
#endif

#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

typedef struct pending_item {
    uint32_t channel_id;
    uint32_t direction;
    esp_wiremux_payload_kind_t kind;
    uint32_t flags;
    uint32_t sequence;
    uint64_t timestamp_us;
    const char *payload_type;
    size_t payload_type_len;
    size_t payload_len;
    esp_wiremux_direction_policy_t policy;
    struct pending_item *next;
    uint8_t payload[];
} pending_item_t;

typedef struct {
    bool registered;
    esp_wiremux_channel_config_t config;
    esp_wiremux_input_handler_t input_handler;
    void *input_handler_ctx;
    uint32_t next_sequence;
    uint32_t dropped_count;
} channel_state_t;

typedef struct {
    bool initialized;
    bool started;
    esp_wiremux_config_t config;
    QueueHandle_t queue;
    TaskHandle_t task;
    TaskHandle_t input_task;
    SemaphoreHandle_t lock;
    uint8_t *rx_buffer;
    size_t rx_len;
    size_t rx_capacity;
    esp_wiremux_diagnostics_t diagnostics;
    channel_state_t channels[ESP_WIREMUX_MAX_CHANNELS];
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
static esp_err_t prepare_default_transport(const esp_wiremux_config_t *config);
static void mux_task(void *arg);
static void mux_input_task(void *arg);
static void free_pending_item(pending_item_t *item);
static void free_pending_list(pending_item_t *item);
static esp_err_t wiremux_status_to_esp(wiremux_status_t status);
static void item_to_envelope(const pending_item_t *item, wiremux_envelope_t *envelope);
static void item_to_record(const pending_item_t *item, wiremux_record_t *record);
static esp_err_t send_single_item(pending_item_t *item);
static esp_err_t send_batch_list(pending_item_t *head, size_t item_count, uint32_t compression);
static esp_err_t send_envelope(const wiremux_envelope_t *envelope, uint32_t flags);
static esp_err_t write_typed(uint8_t channel_id,
                             uint32_t direction,
                             esp_wiremux_payload_kind_t kind,
                             uint32_t flags,
                             const char *payload_type,
                             const uint8_t *payload,
                             size_t payload_len,
                             uint32_t timeout_ms);
static esp_err_t enqueue_item(pending_item_t *item,
                              const esp_wiremux_channel_config_t *channel,
                              uint32_t timeout_ms);
static void parse_rx_buffer_locked(void);
static esp_err_t dispatch_input_envelope_locked(const wiremux_envelope_t *envelope);
static esp_err_t dispatch_input_record_locked(const wiremux_record_t *record);
static bool is_manifest_request(const wiremux_envelope_t *envelope);
static esp_err_t handle_manifest_request_locked(const wiremux_envelope_t *envelope);
static uint32_t native_endianness(void);
static const char *default_transport_name(void);
static bool is_valid_direction(uint32_t direction);
static bool are_valid_channel_directions(uint32_t directions);
static esp_wiremux_direction_policy_t default_direction_policy(void);
static esp_wiremux_direction_policy_t resolve_direction_policy(const esp_wiremux_channel_config_t *channel,
                                                               uint32_t direction);
static uint32_t normalize_compression(uint32_t compression);
static uint32_t policy_interval_ms(const esp_wiremux_direction_policy_t *policy);
static size_t policy_batch_max_bytes(const esp_wiremux_direction_policy_t *policy);
static void update_codec_stats(uint32_t compression,
                               size_t raw_bytes,
                               size_t encoded_bytes,
                               uint64_t encode_us,
                               bool fallback);

void esp_wiremux_config_init(esp_wiremux_config_t *config)
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

esp_err_t esp_wiremux_init(const esp_wiremux_config_t *config)
{
    esp_wiremux_config_t resolved;
    esp_wiremux_config_init(&resolved);
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

    s_mux.rx_capacity = ESP_WIREMUX_FRAME_HEADER_LEN + resolved.max_payload_len;
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

esp_err_t esp_wiremux_start(void)
{
    if (!s_mux.initialized || s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }

    BaseType_t result = xTaskCreatePinnedToCore(mux_task,
                                                "esp_wiremux",
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
                                         "esp_wiremux_rx",
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

esp_err_t esp_wiremux_stop(void)
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

esp_err_t esp_wiremux_register_channel(const esp_wiremux_channel_config_t *config)
{
    if (!s_mux.initialized || config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (config->channel_id >= ESP_WIREMUX_MAX_CHANNELS ||
        !are_valid_channel_directions(config->directions)) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    s_mux.channels[config->channel_id].registered = true;
    s_mux.channels[config->channel_id].config = *config;
    s_mux.channels[config->channel_id].config.input_policy.compression =
        normalize_compression(s_mux.channels[config->channel_id].config.input_policy.compression);
    s_mux.channels[config->channel_id].config.output_policy.compression =
        normalize_compression(s_mux.channels[config->channel_id].config.output_policy.compression);
    xSemaphoreGive(s_mux.lock);

    return ESP_OK;
}

esp_err_t esp_wiremux_register_input_handler(uint8_t channel_id,
                                             esp_wiremux_input_handler_t handler,
                                             void *user_ctx)
{
    if (!s_mux.initialized || handler == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (channel_id >= ESP_WIREMUX_MAX_CHANNELS) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    channel_state_t *channel = &s_mux.channels[channel_id];
    if (!channel->registered ||
        (channel->config.directions & ESP_WIREMUX_DIRECTION_INPUT) == 0) {
        xSemaphoreGive(s_mux.lock);
        return ESP_ERR_NOT_FOUND;
    }

    channel->input_handler = handler;
    channel->input_handler_ctx = user_ctx;
    xSemaphoreGive(s_mux.lock);
    return ESP_OK;
}

esp_err_t esp_wiremux_receive_bytes(const uint8_t *data, size_t len)
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

esp_err_t esp_wiremux_write(uint8_t channel_id,
                            uint32_t direction,
                            esp_wiremux_payload_kind_t kind,
                            uint32_t flags,
                            const uint8_t *payload,
                            size_t payload_len,
                            uint32_t timeout_ms)
{
    return write_typed(channel_id,
                       direction,
                       kind,
                       flags,
                       NULL,
                       payload,
                       payload_len,
                       timeout_ms);
}

static esp_err_t write_typed(uint8_t channel_id,
                             uint32_t direction,
                             esp_wiremux_payload_kind_t kind,
                             uint32_t flags,
                             const char *payload_type,
                             const uint8_t *payload,
                             size_t payload_len,
                             uint32_t timeout_ms)
{
    if (!s_mux.initialized || !s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }
    if (channel_id >= ESP_WIREMUX_MAX_CHANNELS ||
        !is_valid_direction(direction) ||
        (payload_len > 0 && payload == NULL)) {
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
    item->payload_type = payload_type;
    item->payload_type_len = payload_type != NULL ? strlen(payload_type) : 0;
    item->payload_len = payload_len;
    item->policy = resolve_direction_policy(&channel_state.config, direction);
    if (payload_len > 0) {
        memcpy(item->payload, payload, payload_len);
    }

    return enqueue_item(item, &channel_state.config, timeout_ms);
}

esp_err_t esp_wiremux_write_text(uint8_t channel_id,
                                 uint32_t direction,
                                 const char *text,
                                 uint32_t timeout_ms)
{
    if (text == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    return esp_wiremux_write(channel_id,
                             direction,
                             ESP_WIREMUX_PAYLOAD_KIND_TEXT,
                             0,
                             (const uint8_t *)text,
                             strlen(text),
                             timeout_ms);
}

esp_err_t esp_wiremux_emit_manifest(uint32_t timeout_ms)
{
    if (!s_mux.initialized || !s_mux.started) {
        return ESP_ERR_INVALID_STATE;
    }

    wiremux_channel_descriptor_t channels[ESP_WIREMUX_MAX_CHANNELS];
    size_t channel_count = 0;
    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    for (uint8_t i = 0; i < ESP_WIREMUX_MAX_CHANNELS; ++i) {
        if (!s_mux.channels[i].registered) {
            continue;
        }
        channels[channel_count++] = (wiremux_channel_descriptor_t) {
            .channel_id = i,
            .name = s_mux.channels[i].config.name,
            .description = s_mux.channels[i].config.description,
            .directions = s_mux.channels[i].config.directions,
            .default_payload_kind = s_mux.channels[i].config.default_payload_kind,
            .flags = 0,
            .default_interaction_mode = s_mux.channels[i].config.interaction_mode,
        };
    }
    const size_t max_payload_len = s_mux.config.max_payload_len;
    xSemaphoreGive(s_mux.lock);

    const wiremux_device_manifest_t manifest = {
        .device_name = "esp-wiremux",
        .firmware_version = "0.1.0",
        .protocol_version = ESP_WIREMUX_FRAME_VERSION,
        .max_channels = ESP_WIREMUX_MAX_CHANNELS,
        .channels = channels,
        .channel_count = channel_count,
        .native_endianness = native_endianness(),
        .max_payload_len = (uint32_t)max_payload_len,
        .transport = default_transport_name(),
        .feature_flags = WIREMUX_FEATURE_MANIFEST_PROTOBUF |
                         WIREMUX_FEATURE_MANIFEST_REQUEST |
                         WIREMUX_FEATURE_BATCH |
                         WIREMUX_FEATURE_COMPRESSION_HEATSHRINK |
                         WIREMUX_FEATURE_COMPRESSION_LZ4,
        .sdk_name = WIREMUX_SDK_NAME_ESP,
        .sdk_version = "0.1.0",
    };

    const size_t manifest_len = wiremux_device_manifest_encoded_len(&manifest);
    uint8_t *payload = malloc(manifest_len);
    if (payload == NULL) {
        return ESP_ERR_NO_MEM;
    }

    size_t written = 0;
    esp_err_t err = wiremux_status_to_esp(wiremux_device_manifest_encode(&manifest,
                                                                         payload,
                                                                         manifest_len,
                                                                         &written));
    if (err == ESP_OK) {
        err = write_typed(ESP_WIREMUX_CHANNEL_SYSTEM,
                          ESP_WIREMUX_DIRECTION_OUTPUT,
                          ESP_WIREMUX_PAYLOAD_KIND_CONTROL,
                          0,
                          WIREMUX_MANIFEST_PAYLOAD_TYPE,
                          payload,
                          written,
                          timeout_ms);
    }

    free(payload);
    return err;
}

esp_err_t esp_wiremux_get_diagnostics(esp_wiremux_diagnostics_t *diagnostics)
{
    if (!s_mux.initialized || diagnostics == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    *diagnostics = s_mux.diagnostics;
    xSemaphoreGive(s_mux.lock);
    return ESP_OK;
}

static esp_err_t enqueue_item(pending_item_t *item,
                              const esp_wiremux_channel_config_t *channel,
                              uint32_t timeout_ms)
{
    TickType_t wait_ticks = pdMS_TO_TICKS(timeout_ms);

    if (channel->backpressure_policy == ESP_WIREMUX_BACKPRESSURE_DROP_NEWEST) {
        wait_ticks = 0;
    }

    if (xQueueSend(s_mux.queue, &item, wait_ticks) == pdTRUE) {
        return ESP_OK;
    }

    if (channel->backpressure_policy == ESP_WIREMUX_BACKPRESSURE_DROP_OLDEST) {
        pending_item_t *old_item = NULL;
        if (xQueueReceive(s_mux.queue, &old_item, 0) == pdTRUE) {
            free_pending_item(old_item);
            if (xQueueSend(s_mux.queue, &item, 0) == pdTRUE) {
                return ESP_OK;
            }
        }
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    if (item->channel_id < ESP_WIREMUX_MAX_CHANNELS) {
        s_mux.channels[item->channel_id].dropped_count++;
    }
    xSemaphoreGive(s_mux.lock);

    free_pending_item(item);
    return ESP_ERR_TIMEOUT;
}

static void mux_task(void *arg)
{
    (void)arg;
    pending_item_t *deferred = NULL;

    while (true) {
        pending_item_t *item = deferred;
        deferred = NULL;
        if (item == NULL && xQueueReceive(s_mux.queue, &item, portMAX_DELAY) != pdTRUE) {
            continue;
        }
        if (item == NULL) {
            break;
        }

        if (item->policy.send_mode != ESP_WIREMUX_SEND_BATCHED) {
            (void)send_single_item(item);
            free_pending_item(item);
            continue;
        }

        pending_item_t *head = item;
        pending_item_t *tail = item;
        size_t item_count = 1;
        wiremux_record_t first_record = {0};
        item_to_record(item, &first_record);
        size_t batch_bytes = wiremux_record_encoded_len(&first_record);
        const uint32_t compression = normalize_compression(item->policy.compression);
        const TickType_t deadline = xTaskGetTickCount() + pdMS_TO_TICKS(policy_interval_ms(&item->policy));
        const size_t max_batch_bytes = policy_batch_max_bytes(&item->policy);

        while (batch_bytes < max_batch_bytes) {
            const TickType_t now = xTaskGetTickCount();
            const TickType_t wait_ticks = now >= deadline ? 0 : deadline - now;
            pending_item_t *next = NULL;
            if (xQueueReceive(s_mux.queue, &next, wait_ticks) != pdTRUE) {
                break;
            }
            if (next == NULL) {
                (void)send_batch_list(head, item_count, compression);
                free_pending_list(head);
                deferred = NULL;
                vTaskDelete(NULL);
                return;
            }
            if (next->policy.send_mode != ESP_WIREMUX_SEND_BATCHED ||
                normalize_compression(next->policy.compression) != compression) {
                deferred = next;
                break;
            }
            wiremux_record_t record = {0};
            item_to_record(next, &record);
            batch_bytes += wiremux_record_encoded_len(&record);
            tail->next = next;
            tail = next;
            item_count++;
            if (batch_bytes >= max_batch_bytes) {
                break;
            }
        }

        (void)send_batch_list(head, item_count, compression);
        free_pending_list(head);
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
            (void)esp_wiremux_receive_bytes(buffer, read_len);
        } else {
            vTaskDelay(pdMS_TO_TICKS(10));
        }
    }

    vTaskDelete(NULL);
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
    if (s_mux.rx_len < ESP_WIREMUX_MAGIC_LEN) {
        return SIZE_MAX;
    }
    for (size_t i = 0; i <= s_mux.rx_len - ESP_WIREMUX_MAGIC_LEN; ++i) {
        if (memcmp(s_mux.rx_buffer + i, ESP_WIREMUX_MAGIC, ESP_WIREMUX_MAGIC_LEN) == 0) {
            return i;
        }
    }
    return SIZE_MAX;
}

static size_t magic_prefix_suffix_len(void)
{
    const size_t max_len = s_mux.rx_len < ESP_WIREMUX_MAGIC_LEN - 1
                               ? s_mux.rx_len
                               : ESP_WIREMUX_MAGIC_LEN - 1;
    for (size_t len = max_len; len > 0; --len) {
        if (memcmp(s_mux.rx_buffer + s_mux.rx_len - len, ESP_WIREMUX_MAGIC, len) == 0) {
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
        if (s_mux.rx_len < ESP_WIREMUX_FRAME_HEADER_LEN) {
            return;
        }

        wiremux_frame_view_t frame = {0};
        const wiremux_status_t frame_status = wiremux_frame_decode(s_mux.rx_buffer,
                                                                   s_mux.rx_len,
                                                                   s_mux.config.max_payload_len,
                                                                   &frame);
        if (frame_status == WIREMUX_STATUS_INCOMPLETE) {
            return;
        }
        if (frame_status == WIREMUX_STATUS_BAD_VERSION ||
            frame_status == WIREMUX_STATUS_BAD_MAGIC ||
            frame_status == WIREMUX_STATUS_INVALID_SIZE) {
            rx_drop_prefix(1);
            continue;
        }
        if (frame_status == WIREMUX_STATUS_CRC_MISMATCH) {
            rx_drop_prefix(frame.frame_len > 0 ? frame.frame_len : 1);
            continue;
        }
        if (frame_status != WIREMUX_STATUS_OK) {
            rx_drop_prefix(1);
            continue;
        }

        wiremux_envelope_t envelope = {0};
        if (wiremux_envelope_decode(frame.payload, frame.payload_len, &envelope) ==
            WIREMUX_STATUS_OK) {
            (void)dispatch_input_envelope_locked(&envelope);
        }
        rx_drop_prefix(frame.frame_len);
    }
}

static esp_err_t dispatch_input_envelope_locked(const wiremux_envelope_t *envelope)
{
    if (envelope == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    if (envelope->kind == ESP_WIREMUX_PAYLOAD_KIND_BATCH ||
        (envelope->payload_type_len == strlen(WIREMUX_BATCH_PAYLOAD_TYPE) &&
         memcmp(envelope->payload_type, WIREMUX_BATCH_PAYLOAD_TYPE, envelope->payload_type_len) == 0)) {
        if (envelope->payload == NULL || envelope->payload_len == 0) {
            return ESP_ERR_INVALID_ARG;
        }
        wiremux_batch_t batch = {0};
        esp_err_t err = wiremux_status_to_esp(wiremux_batch_decode(envelope->payload,
                                                                   envelope->payload_len,
                                                                   &batch));
        if (err != ESP_OK) {
            return err;
        }
        const size_t records_capacity = batch.compression == WIREMUX_COMPRESSION_NONE
                                            ? batch.records_len
                                            : batch.uncompressed_len;
        uint8_t *records_payload = malloc(records_capacity);
        if (records_payload == NULL) {
            return ESP_ERR_NO_MEM;
        }
        size_t records_len = 0;
        err = wiremux_status_to_esp(wiremux_decompress(batch.compression,
                                                       batch.records,
                                                       batch.records_len,
                                                       records_payload,
                                                       records_capacity,
                                                       &records_len));
        if (err != ESP_OK) {
            free(records_payload);
            return err;
        }
        if (batch.compression <= WIREMUX_COMPRESSION_LZ4) {
            s_mux.diagnostics.compression[batch.compression].decode_ok++;
        }
        wiremux_record_t records[ESP_WIREMUX_MAX_CHANNELS] = {0};
        size_t record_count = 0;
        err = wiremux_status_to_esp(wiremux_batch_records_decode(records_payload,
                                                                 records_len,
                                                                 records,
                                                                 ESP_WIREMUX_MAX_CHANNELS,
                                                                 &record_count));
        if (err == ESP_OK) {
            for (size_t i = 0; i < record_count; ++i) {
                err = dispatch_input_record_locked(&records[i]);
                if (err != ESP_OK) {
                    break;
                }
            }
        }
        free(records_payload);
        return err;
    }

    if (envelope->direction != ESP_WIREMUX_DIRECTION_INPUT ||
        envelope->channel_id >= ESP_WIREMUX_MAX_CHANNELS ||
        envelope->payload_len > s_mux.config.max_payload_len) {
        return ESP_ERR_INVALID_ARG;
    }

    channel_state_t *channel = &s_mux.channels[envelope->channel_id];
    if (!channel->registered ||
        (channel->config.directions & ESP_WIREMUX_DIRECTION_INPUT) == 0) {
        return ESP_ERR_NOT_FOUND;
    }

    if (is_manifest_request(envelope)) {
        return handle_manifest_request_locked(envelope);
    }

    if (channel->input_handler == NULL) {
        return ESP_ERR_NOT_FOUND;
    }

    esp_wiremux_input_handler_t handler = channel->input_handler;
    void *handler_ctx = channel->input_handler_ctx;
    const uint8_t channel_id = (uint8_t)envelope->channel_id;
    const size_t payload_len = envelope->payload_len;
    uint8_t *payload = NULL;
    if (payload_len > 0) {
        payload = malloc(payload_len);
        if (payload == NULL) {
            return ESP_ERR_NO_MEM;
        }
        memcpy(payload, envelope->payload, payload_len);
    }

    xSemaphoreGive(s_mux.lock);
    esp_err_t err = handler(channel_id, payload, payload_len, handler_ctx);
    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    free(payload);
    return err;
}

static bool is_manifest_request(const wiremux_envelope_t *envelope)
{
    return envelope != NULL &&
           envelope->channel_id == ESP_WIREMUX_CHANNEL_SYSTEM &&
           envelope->payload_type != NULL &&
           envelope->payload_type_len == strlen(WIREMUX_MANIFEST_REQUEST_PAYLOAD_TYPE) &&
           memcmp(envelope->payload_type,
                  WIREMUX_MANIFEST_REQUEST_PAYLOAD_TYPE,
                  envelope->payload_type_len) == 0;
}

static esp_err_t handle_manifest_request_locked(const wiremux_envelope_t *envelope)
{
    if (envelope == NULL || envelope->payload_len != 0) {
        return ESP_ERR_INVALID_ARG;
    }

    xSemaphoreGive(s_mux.lock);
    esp_err_t err = esp_wiremux_emit_manifest(s_mux.config.default_write_timeout_ms);
    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    return err;
}

static esp_err_t dispatch_input_record_locked(const wiremux_record_t *record)
{
    if (record == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    const wiremux_envelope_t envelope = {
        .channel_id = record->channel_id,
        .direction = record->direction,
        .sequence = record->sequence,
        .timestamp_us = record->timestamp_us,
        .kind = record->kind,
        .payload_type = record->payload_type,
        .payload_type_len = record->payload_type_len,
        .payload = record->payload,
        .payload_len = record->payload_len,
        .flags = record->flags,
    };
    return dispatch_input_envelope_locked(&envelope);
}

static void free_pending_item(pending_item_t *item)
{
    free(item);
}

static void free_pending_list(pending_item_t *item)
{
    while (item != NULL) {
        pending_item_t *next = item->next;
        free_pending_item(item);
        item = next;
    }
}

static void item_to_envelope(const pending_item_t *item, wiremux_envelope_t *envelope)
{
    memset(envelope, 0, sizeof(*envelope));
    envelope->channel_id = item->channel_id;
    envelope->direction = item->direction;
    envelope->sequence = item->sequence;
    envelope->timestamp_us = item->timestamp_us;
    envelope->kind = item->kind;
    envelope->payload_type = item->payload_type;
    envelope->payload_type_len = item->payload_type_len;
    envelope->payload = item->payload;
    envelope->payload_len = item->payload_len;
    envelope->flags = item->flags;
}

static void item_to_record(const pending_item_t *item, wiremux_record_t *record)
{
    memset(record, 0, sizeof(*record));
    record->channel_id = item->channel_id;
    record->direction = item->direction;
    record->sequence = item->sequence;
    record->timestamp_us = item->timestamp_us;
    record->kind = item->kind;
    record->payload_type = item->payload_type;
    record->payload_type_len = item->payload_type_len;
    record->payload = item->payload;
    record->payload_len = item->payload_len;
    record->flags = item->flags;
}

static esp_err_t send_single_item(pending_item_t *item)
{
    wiremux_envelope_t envelope;
    item_to_envelope(item, &envelope);
    return send_envelope(&envelope, item->flags);
}

static esp_err_t send_batch_list(pending_item_t *head, size_t item_count, uint32_t compression)
{
    if (head == NULL || item_count == 0) {
        return ESP_ERR_INVALID_ARG;
    }

    wiremux_record_t *records = calloc(item_count, sizeof(*records));
    if (records == NULL) {
        return ESP_ERR_NO_MEM;
    }

    size_t index = 0;
    for (pending_item_t *item = head; item != NULL && index < item_count; item = item->next) {
        item_to_record(item, &records[index++]);
    }

    const size_t records_len = wiremux_batch_records_encoded_len(records, item_count);
    uint8_t *records_payload = malloc(records_len);
    if (records_payload == NULL) {
        free(records);
        return ESP_ERR_NO_MEM;
    }

    size_t records_written = 0;
    esp_err_t err = wiremux_status_to_esp(wiremux_batch_records_encode(records,
                                                                       item_count,
                                                                       records_payload,
                                                                       records_len,
                                                                       &records_written));
    free(records);
    if (err != ESP_OK) {
        free(records_payload);
        return err;
    }

    const uint32_t requested_compression = normalize_compression(compression);
    uint32_t selected_compression = requested_compression;
    const uint64_t encode_started_us = (uint64_t)esp_timer_get_time();
    size_t encoded_records_len = records_written;
    uint8_t *encoded_records = records_payload;
    uint8_t *compressed_records = NULL;
    bool fallback = false;

    if (selected_compression != WIREMUX_COMPRESSION_NONE) {
        compressed_records = malloc(records_written + 16);
        if (compressed_records != NULL) {
            size_t compressed_written = 0;
            if (wiremux_compress(selected_compression,
                                 records_payload,
                                 records_written,
                                 compressed_records,
                                 records_written + 16,
                                 &compressed_written) == WIREMUX_STATUS_OK &&
                (compressed_written < records_written || head->policy.force_compression)) {
                encoded_records = compressed_records;
                encoded_records_len = compressed_written;
            } else {
                selected_compression = WIREMUX_COMPRESSION_NONE;
                fallback = true;
            }
        } else {
            selected_compression = WIREMUX_COMPRESSION_NONE;
            fallback = true;
        }
    }
    const uint64_t encode_us = (uint64_t)esp_timer_get_time() - encode_started_us;

    const wiremux_batch_t batch = {
        .compression = selected_compression,
        .records = encoded_records,
        .records_len = encoded_records_len,
        .uncompressed_len = (uint32_t)records_written,
    };
    const size_t batch_len = wiremux_batch_encoded_len(&batch);
    uint8_t *batch_payload = malloc(batch_len);
    if (batch_payload == NULL) {
        free(compressed_records);
        free(records_payload);
        return ESP_ERR_NO_MEM;
    }

    size_t batch_written = 0;
    err = wiremux_status_to_esp(wiremux_batch_encode(&batch,
                                                     batch_payload,
                                                     batch_len,
                                                     &batch_written));
    if (err == ESP_OK) {
        const wiremux_envelope_t envelope = {
            .channel_id = ESP_WIREMUX_CHANNEL_SYSTEM,
            .direction = ESP_WIREMUX_DIRECTION_OUTPUT,
            .sequence = 0,
            .timestamp_us = (uint64_t)esp_timer_get_time(),
            .kind = ESP_WIREMUX_PAYLOAD_KIND_BATCH,
            .payload_type = WIREMUX_BATCH_PAYLOAD_TYPE,
            .payload_type_len = strlen(WIREMUX_BATCH_PAYLOAD_TYPE),
            .payload = batch_payload,
            .payload_len = batch_written,
            .flags = 0,
        };
        err = send_envelope(&envelope, 0);
    }

    update_codec_stats(requested_compression, records_written, encoded_records_len, encode_us, fallback);
    free(batch_payload);
    free(compressed_records);
    free(records_payload);
    return err;
}

static esp_err_t send_envelope(const wiremux_envelope_t *envelope, uint32_t flags)
{
    const size_t envelope_len = wiremux_envelope_encoded_len(envelope);
    uint8_t *encoded_envelope = malloc(envelope_len);
    if (encoded_envelope == NULL) {
        return ESP_ERR_NO_MEM;
    }

    size_t envelope_written = 0;
    esp_err_t err = wiremux_status_to_esp(wiremux_envelope_encode(envelope,
                                                                  encoded_envelope,
                                                                  envelope_len,
                                                                  &envelope_written));
    if (err != ESP_OK) {
        free(encoded_envelope);
        return err;
    }

    const size_t frame_len = esp_wiremux_frame_encoded_len(envelope_written);
    uint8_t *frame = malloc(frame_len);
    if (frame == NULL) {
        free(encoded_envelope);
        return ESP_ERR_NO_MEM;
    }

    size_t written = 0;
    const esp_wiremux_frame_header_t header = {
        .version = ESP_WIREMUX_FRAME_VERSION,
        .flags = (uint8_t)(flags & 0xffu),
    };
    err = esp_wiremux_frame_encode(&header,
                                   encoded_envelope,
                                   envelope_written,
                                   frame,
                                   frame_len,
                                   &written);
    if (err == ESP_OK) {
        err = s_mux.config.transport.write(frame,
                                           written,
                                           s_mux.config.default_write_timeout_ms,
                                           s_mux.config.transport.user_ctx);
    }

    free(frame);
    free(encoded_envelope);
    return err;
}

static esp_err_t wiremux_status_to_esp(wiremux_status_t status)
{
    switch (status) {
    case WIREMUX_STATUS_OK:
        return ESP_OK;
    case WIREMUX_STATUS_INVALID_ARG:
        return ESP_ERR_INVALID_ARG;
    case WIREMUX_STATUS_INVALID_SIZE:
        return ESP_ERR_INVALID_SIZE;
    case WIREMUX_STATUS_NOT_SUPPORTED:
        return ESP_ERR_NOT_SUPPORTED;
    case WIREMUX_STATUS_INCOMPLETE:
    case WIREMUX_STATUS_BAD_MAGIC:
    case WIREMUX_STATUS_BAD_VERSION:
    case WIREMUX_STATUS_CRC_MISMATCH:
        return ESP_FAIL;
    default:
        return ESP_FAIL;
    }
}

static uint32_t native_endianness(void)
{
    const uint16_t value = 0x0001u;
    return (*(const uint8_t *)&value == 0x01u) ? WIREMUX_ENDIANNESS_LITTLE
                                               : WIREMUX_ENDIANNESS_BIG;
}

static const char *default_transport_name(void)
{
#if CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG
    return "esp-usb-serial-jtag";
#else
    return "stdio";
#endif
}

static bool is_valid_direction(uint32_t direction)
{
    return direction == ESP_WIREMUX_DIRECTION_INPUT ||
           direction == ESP_WIREMUX_DIRECTION_OUTPUT;
}

static bool are_valid_channel_directions(uint32_t directions)
{
    const uint32_t allowed = ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT;
    return directions != 0 && (directions & ~allowed) == 0;
}

static esp_wiremux_direction_policy_t default_direction_policy(void)
{
    return (esp_wiremux_direction_policy_t) {
        .send_mode = ESP_WIREMUX_SEND_IMMEDIATE,
        .compression = ESP_WIREMUX_COMPRESSION_NONE,
        .batch_interval_ms = 100,
        .batch_max_bytes = 0,
        .force_compression = false,
    };
}

static esp_wiremux_direction_policy_t resolve_direction_policy(const esp_wiremux_channel_config_t *channel,
                                                               uint32_t direction)
{
    esp_wiremux_direction_policy_t policy = default_direction_policy();
    if (channel == NULL) {
        return policy;
    }

    const esp_wiremux_direction_policy_t configured =
        direction == ESP_WIREMUX_DIRECTION_INPUT ? channel->input_policy : channel->output_policy;
    if (configured.send_mode == ESP_WIREMUX_SEND_BATCHED) {
        policy.send_mode = ESP_WIREMUX_SEND_BATCHED;
    } else if (channel->flush_policy == ESP_WIREMUX_FLUSH_PERIODIC ||
               channel->flush_policy == ESP_WIREMUX_FLUSH_HIGH_WATERMARK) {
        policy.send_mode = ESP_WIREMUX_SEND_BATCHED;
    }
    policy.compression = normalize_compression(configured.compression);
    policy.batch_interval_ms = configured.batch_interval_ms != 0 ? configured.batch_interval_ms : 100;
    policy.batch_max_bytes = configured.batch_max_bytes;
    policy.force_compression = configured.force_compression;
    return policy;
}

static uint32_t normalize_compression(uint32_t compression)
{
    switch (compression) {
    case WIREMUX_COMPRESSION_NONE:
    case WIREMUX_COMPRESSION_HEATSHRINK:
    case WIREMUX_COMPRESSION_LZ4:
        return compression;
    default:
        return WIREMUX_COMPRESSION_NONE;
    }
}

static uint32_t policy_interval_ms(const esp_wiremux_direction_policy_t *policy)
{
    if (policy == NULL || policy->batch_interval_ms == 0) {
        return 100;
    }
    return policy->batch_interval_ms;
}

static size_t policy_batch_max_bytes(const esp_wiremux_direction_policy_t *policy)
{
    if (policy == NULL || policy->batch_max_bytes == 0) {
        return s_mux.config.max_payload_len > 64 ? s_mux.config.max_payload_len - 64
                                                 : s_mux.config.max_payload_len;
    }
    if (policy->batch_max_bytes > s_mux.config.max_payload_len) {
        return s_mux.config.max_payload_len;
    }
    return policy->batch_max_bytes;
}

static void update_codec_stats(uint32_t compression,
                               size_t raw_bytes,
                               size_t encoded_bytes,
                               uint64_t encode_us,
                               bool fallback)
{
    if (compression > WIREMUX_COMPRESSION_LZ4) {
        compression = WIREMUX_COMPRESSION_NONE;
    }

    xSemaphoreTake(s_mux.lock, portMAX_DELAY);
    esp_wiremux_codec_stats_t *stats = &s_mux.diagnostics.compression[compression];
    stats->raw_bytes += raw_bytes;
    stats->encoded_bytes += encoded_bytes;
    stats->encode_us += encode_us;
    if (fallback) {
        stats->fallback_count++;
    }
    const size_t free_heap = heap_caps_get_free_size(MALLOC_CAP_8BIT);
    if (stats->heap_peak == 0 || free_heap < stats->heap_peak) {
        stats->heap_peak = free_heap;
    }
    xSemaphoreGive(s_mux.lock);
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

static esp_err_t prepare_default_transport(const esp_wiremux_config_t *config)
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
    const size_t frame_buffer_size = ESP_WIREMUX_FRAME_HEADER_LEN + config->max_payload_len;
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
