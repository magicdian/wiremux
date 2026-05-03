#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "freertos/FreeRTOS.h"
#include "wiremux_batch.h"
#include "wiremux_envelope.h"
#include "wiremux_manifest.h"

#ifdef __cplusplus
extern "C" {
#endif

#ifndef ESP_WIREMUX_MAX_CHANNELS
#define ESP_WIREMUX_MAX_CHANNELS 8
#endif

#define ESP_WIREMUX_CHANNEL_SYSTEM 0
#define ESP_WIREMUX_VERSION "2605.3.2"

/*
 * ESP-facing aliases keep applications on the esp_wiremux public API while
 * preserving the portable core's wire-protocol numeric values. Do not renumber
 * these constants. Channel configs may OR direction flags together, but
 * esp_wiremux_write*() direction arguments must pass exactly one direction.
 */
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
    ESP_WIREMUX_PAYLOAD_KIND_BATCH = WIREMUX_PAYLOAD_KIND_BATCH,
} esp_wiremux_payload_kind_t;

typedef enum {
    ESP_WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED = WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
    ESP_WIREMUX_CHANNEL_INTERACTION_LINE = WIREMUX_CHANNEL_INTERACTION_LINE,
    ESP_WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH = WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH,
} esp_wiremux_channel_interaction_mode_t;

typedef enum {
    ESP_WIREMUX_NEWLINE_POLICY_UNSPECIFIED = WIREMUX_NEWLINE_POLICY_UNSPECIFIED,
    ESP_WIREMUX_NEWLINE_POLICY_PRESERVE = WIREMUX_NEWLINE_POLICY_PRESERVE,
    ESP_WIREMUX_NEWLINE_POLICY_LF = WIREMUX_NEWLINE_POLICY_LF,
    ESP_WIREMUX_NEWLINE_POLICY_CR = WIREMUX_NEWLINE_POLICY_CR,
    ESP_WIREMUX_NEWLINE_POLICY_CRLF = WIREMUX_NEWLINE_POLICY_CRLF,
} esp_wiremux_newline_policy_t;

typedef enum {
    ESP_WIREMUX_ECHO_POLICY_UNSPECIFIED = WIREMUX_ECHO_POLICY_UNSPECIFIED,
    ESP_WIREMUX_ECHO_POLICY_REMOTE = WIREMUX_ECHO_POLICY_REMOTE,
    ESP_WIREMUX_ECHO_POLICY_LOCAL = WIREMUX_ECHO_POLICY_LOCAL,
    ESP_WIREMUX_ECHO_POLICY_NONE = WIREMUX_ECHO_POLICY_NONE,
} esp_wiremux_echo_policy_t;

typedef enum {
    ESP_WIREMUX_CONTROL_KEY_POLICY_UNSPECIFIED = WIREMUX_CONTROL_KEY_POLICY_UNSPECIFIED,
    ESP_WIREMUX_CONTROL_KEY_POLICY_HOST_HANDLED = WIREMUX_CONTROL_KEY_POLICY_HOST_HANDLED,
    ESP_WIREMUX_CONTROL_KEY_POLICY_FORWARDED = WIREMUX_CONTROL_KEY_POLICY_FORWARDED,
} esp_wiremux_control_key_policy_t;

typedef struct {
    esp_wiremux_newline_policy_t input_newline_policy;
    esp_wiremux_newline_policy_t output_newline_policy;
    esp_wiremux_echo_policy_t echo_policy;
    esp_wiremux_control_key_policy_t control_key_policy;
} esp_wiremux_passthrough_policy_t;

typedef enum {
    ESP_WIREMUX_FLUSH_IMMEDIATE = 0,
    ESP_WIREMUX_FLUSH_PERIODIC = 1,
    ESP_WIREMUX_FLUSH_HIGH_WATERMARK = 2,
} esp_wiremux_flush_policy_t;

typedef enum {
    ESP_WIREMUX_SEND_IMMEDIATE = 0,
    ESP_WIREMUX_SEND_BATCHED = 1,
} esp_wiremux_send_mode_t;

typedef enum {
    ESP_WIREMUX_COMPRESSION_NONE = WIREMUX_COMPRESSION_NONE,
    ESP_WIREMUX_COMPRESSION_HEATSHRINK = WIREMUX_COMPRESSION_HEATSHRINK,
    ESP_WIREMUX_COMPRESSION_LZ4 = WIREMUX_COMPRESSION_LZ4,
} esp_wiremux_compression_algorithm_t;

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
    esp_wiremux_send_mode_t send_mode;
    esp_wiremux_compression_algorithm_t compression;
    uint32_t batch_interval_ms;
    size_t batch_max_bytes;
    bool force_compression;
} esp_wiremux_direction_policy_t;

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
    esp_wiremux_channel_interaction_mode_t interaction_mode;
    esp_wiremux_passthrough_policy_t passthrough_policy;
    esp_wiremux_direction_policy_t input_policy;
    esp_wiremux_direction_policy_t output_policy;
} esp_wiremux_channel_config_t;

typedef struct {
    uint64_t raw_bytes;
    uint64_t encoded_bytes;
    uint64_t encode_us;
    uint32_t decode_ok;
    uint32_t fallback_count;
    size_t heap_peak;
} esp_wiremux_codec_stats_t;

typedef struct {
    esp_wiremux_codec_stats_t compression[3];
} esp_wiremux_diagnostics_t;

typedef esp_err_t (*esp_wiremux_input_handler_t)(uint8_t channel_id,
                                                 const uint8_t *payload,
                                                 size_t payload_len,
                                                 void *user_ctx);

typedef enum {
    ESP_WIREMUX_INPUT_CONSUMER_NONE = 0,
    ESP_WIREMUX_INPUT_CONSUMER_QUEUE = 1,
    ESP_WIREMUX_INPUT_CONSUMER_CALLBACK = 2,
} esp_wiremux_input_consumer_t;

void esp_wiremux_config_init(esp_wiremux_config_t *config);

esp_err_t esp_wiremux_init(const esp_wiremux_config_t *config);
esp_err_t esp_wiremux_start(void);
esp_err_t esp_wiremux_stop(void);

esp_err_t esp_wiremux_register_channel(const esp_wiremux_channel_config_t *config);

esp_err_t esp_wiremux_is_channel_registered(uint8_t channel_id, bool *registered);

esp_err_t esp_wiremux_register_input_handler(uint8_t channel_id,
                                             esp_wiremux_input_handler_t handler,
                                             void *user_ctx);

esp_err_t esp_wiremux_register_rx_queue(uint8_t channel_id, size_t queue_depth);

esp_err_t esp_wiremux_channel_read(uint8_t channel_id,
                                   uint8_t *buffer,
                                   size_t capacity,
                                   size_t *read_len,
                                   uint32_t timeout_ms);

esp_err_t esp_wiremux_get_input_consumer(uint8_t channel_id,
                                         esp_wiremux_input_consumer_t *consumer);

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

esp_err_t esp_wiremux_get_diagnostics(esp_wiremux_diagnostics_t *diagnostics);

#ifdef __cplusplus
}
#endif
