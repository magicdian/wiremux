#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "freertos/FreeRTOS.h"

#ifdef __cplusplus
extern "C" {
#endif

#ifndef ESP_SERIAL_MUX_MAX_CHANNELS
#define ESP_SERIAL_MUX_MAX_CHANNELS 8
#endif

#define ESP_SERIAL_MUX_CHANNEL_SYSTEM 0

typedef enum {
    ESP_SERIAL_MUX_DIRECTION_INPUT = 1u << 0,
    ESP_SERIAL_MUX_DIRECTION_OUTPUT = 1u << 1,
} esp_serial_mux_direction_flags_t;

typedef enum {
    ESP_SERIAL_MUX_PAYLOAD_KIND_UNSPECIFIED = 0,
    ESP_SERIAL_MUX_PAYLOAD_KIND_TEXT = 1,
    ESP_SERIAL_MUX_PAYLOAD_KIND_BINARY = 2,
    ESP_SERIAL_MUX_PAYLOAD_KIND_PROTOBUF = 3,
    ESP_SERIAL_MUX_PAYLOAD_KIND_CONTROL = 4,
    ESP_SERIAL_MUX_PAYLOAD_KIND_EVENT = 5,
} esp_serial_mux_payload_kind_t;

typedef enum {
    ESP_SERIAL_MUX_FLUSH_IMMEDIATE = 0,
    ESP_SERIAL_MUX_FLUSH_PERIODIC = 1,
    ESP_SERIAL_MUX_FLUSH_HIGH_WATERMARK = 2,
} esp_serial_mux_flush_policy_t;

typedef enum {
    ESP_SERIAL_MUX_BACKPRESSURE_DROP_NEWEST = 0,
    ESP_SERIAL_MUX_BACKPRESSURE_DROP_OLDEST = 1,
    ESP_SERIAL_MUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT = 2,
} esp_serial_mux_backpressure_policy_t;

typedef esp_err_t (*esp_serial_mux_transport_write_fn)(const uint8_t *data,
                                                       size_t len,
                                                       uint32_t timeout_ms,
                                                       void *user_ctx);

typedef struct {
    esp_serial_mux_transport_write_fn write;
    void *user_ctx;
} esp_serial_mux_transport_t;

typedef struct {
    size_t queue_depth;
    size_t max_payload_len;
    uint32_t default_write_timeout_ms;
    uint32_t task_stack_size;
    UBaseType_t task_priority;
    BaseType_t task_core_id;
    esp_serial_mux_transport_t transport;
} esp_serial_mux_config_t;

typedef struct {
    uint8_t channel_id;
    const char *name;
    const char *description;
    uint32_t directions;
    esp_serial_mux_payload_kind_t default_payload_kind;
    esp_serial_mux_flush_policy_t flush_policy;
    esp_serial_mux_backpressure_policy_t backpressure_policy;
} esp_serial_mux_channel_config_t;

void esp_serial_mux_config_init(esp_serial_mux_config_t *config);

esp_err_t esp_serial_mux_init(const esp_serial_mux_config_t *config);
esp_err_t esp_serial_mux_start(void);
esp_err_t esp_serial_mux_stop(void);

esp_err_t esp_serial_mux_register_channel(const esp_serial_mux_channel_config_t *config);

esp_err_t esp_serial_mux_write(uint8_t channel_id,
                               uint32_t direction,
                               esp_serial_mux_payload_kind_t kind,
                               uint32_t flags,
                               const uint8_t *payload,
                               size_t payload_len,
                               uint32_t timeout_ms);

esp_err_t esp_serial_mux_write_text(uint8_t channel_id,
                                    uint32_t direction,
                                    const char *text,
                                    uint32_t timeout_ms);

esp_err_t esp_serial_mux_emit_manifest(uint32_t timeout_ms);

#ifdef __cplusplus
}
#endif
