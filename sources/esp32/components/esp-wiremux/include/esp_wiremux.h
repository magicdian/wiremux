#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "freertos/FreeRTOS.h"
#include "wiremux_envelope.h"

#ifdef __cplusplus
extern "C" {
#endif

#ifndef ESP_WIREMUX_MAX_CHANNELS
#define ESP_WIREMUX_MAX_CHANNELS 8
#endif

#define ESP_WIREMUX_CHANNEL_SYSTEM 0

typedef enum {
    ESP_WIREMUX_DIRECTION_INPUT = WIREMUX_DIRECTION_INPUT,
    ESP_WIREMUX_DIRECTION_OUTPUT = WIREMUX_DIRECTION_OUTPUT,
} esp_wiremux_direction_flags_t;

typedef enum {
    ESP_WIREMUX_PAYLOAD_KIND_UNSPECIFIED = WIREMUX_PAYLOAD_KIND_UNSPECIFIED,
    ESP_WIREMUX_PAYLOAD_KIND_TEXT = WIREMUX_PAYLOAD_KIND_TEXT,
    ESP_WIREMUX_PAYLOAD_KIND_BINARY = WIREMUX_PAYLOAD_KIND_BINARY,
    ESP_WIREMUX_PAYLOAD_KIND_PROTOBUF = WIREMUX_PAYLOAD_KIND_PROTOBUF,
    ESP_WIREMUX_PAYLOAD_KIND_CONTROL = WIREMUX_PAYLOAD_KIND_CONTROL,
    ESP_WIREMUX_PAYLOAD_KIND_EVENT = WIREMUX_PAYLOAD_KIND_EVENT,
} esp_wiremux_payload_kind_t;

typedef enum {
    ESP_WIREMUX_FLUSH_IMMEDIATE = 0,
    ESP_WIREMUX_FLUSH_PERIODIC = 1,
    ESP_WIREMUX_FLUSH_HIGH_WATERMARK = 2,
} esp_wiremux_flush_policy_t;

typedef enum {
    ESP_WIREMUX_BACKPRESSURE_DROP_NEWEST = 0,
    ESP_WIREMUX_BACKPRESSURE_DROP_OLDEST = 1,
    ESP_WIREMUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT = 2,
} esp_wiremux_backpressure_policy_t;

typedef esp_err_t (*esp_wiremux_transport_write_fn)(const uint8_t *data,
                                                    size_t len,
                                                    uint32_t timeout_ms,
                                                    void *user_ctx);

typedef esp_err_t (*esp_wiremux_transport_read_fn)(uint8_t *data,
                                                   size_t capacity,
                                                   size_t *read_len,
                                                   uint32_t timeout_ms,
                                                   void *user_ctx);

typedef struct {
    esp_wiremux_transport_write_fn write;
    esp_wiremux_transport_read_fn read;
    void *user_ctx;
} esp_wiremux_transport_t;

typedef struct {
    size_t queue_depth;
    size_t max_payload_len;
    uint32_t default_write_timeout_ms;
    uint32_t task_stack_size;
    UBaseType_t task_priority;
    BaseType_t task_core_id;
    esp_wiremux_transport_t transport;
} esp_wiremux_config_t;

typedef struct {
    uint8_t channel_id;
    const char *name;
    const char *description;
    uint32_t directions;
    esp_wiremux_payload_kind_t default_payload_kind;
    esp_wiremux_flush_policy_t flush_policy;
    esp_wiremux_backpressure_policy_t backpressure_policy;
} esp_wiremux_channel_config_t;

typedef esp_err_t (*esp_wiremux_input_handler_t)(uint8_t channel_id,
                                                 const uint8_t *payload,
                                                 size_t payload_len,
                                                 void *user_ctx);

void esp_wiremux_config_init(esp_wiremux_config_t *config);

esp_err_t esp_wiremux_init(const esp_wiremux_config_t *config);
esp_err_t esp_wiremux_start(void);
esp_err_t esp_wiremux_stop(void);

esp_err_t esp_wiremux_register_channel(const esp_wiremux_channel_config_t *config);

esp_err_t esp_wiremux_register_input_handler(uint8_t channel_id,
                                             esp_wiremux_input_handler_t handler,
                                             void *user_ctx);

esp_err_t esp_wiremux_receive_bytes(const uint8_t *data, size_t len);

esp_err_t esp_wiremux_write(uint8_t channel_id,
                            uint32_t direction,
                            esp_wiremux_payload_kind_t kind,
                            uint32_t flags,
                            const uint8_t *payload,
                            size_t payload_len,
                            uint32_t timeout_ms);

esp_err_t esp_wiremux_write_text(uint8_t channel_id,
                                 uint32_t direction,
                                 const char *text,
                                 uint32_t timeout_ms);

esp_err_t esp_wiremux_emit_manifest(uint32_t timeout_ms);

#ifdef __cplusplus
}
#endif
