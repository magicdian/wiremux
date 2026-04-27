#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_envelope.h"
#include "wiremux_frame.h"
#include "wiremux_status.h"
#include "wiremux_version.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WIREMUX_HOST_SESSION_MIN_BUFFER_CAPACITY WIREMUX_FRAME_HEADER_LEN

typedef enum {
    WIREMUX_HOST_EVENT_TERMINAL = 1,
    WIREMUX_HOST_EVENT_RECORD = 2,
    WIREMUX_HOST_EVENT_CRC_ERROR = 3,
    WIREMUX_HOST_EVENT_DECODE_ERROR = 4,
    WIREMUX_HOST_EVENT_MANIFEST_BEGIN = 5,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_BEGIN = 6,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_DIRECTION = 7,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_PAYLOAD_KIND = 8,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_PAYLOAD_TYPE = 9,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_INTERACTION_MODE = 10,
    WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_END = 11,
    WIREMUX_HOST_EVENT_MANIFEST_END = 12,
    WIREMUX_HOST_EVENT_PROTOCOL_COMPATIBILITY = 13,
    WIREMUX_HOST_EVENT_BATCH_SUMMARY = 14,
} wiremux_host_event_type_t;

typedef enum {
    WIREMUX_HOST_DECODE_ENVELOPE = 1,
    WIREMUX_HOST_DECODE_MANIFEST = 2,
    WIREMUX_HOST_DECODE_BATCH = 3,
    WIREMUX_HOST_DECODE_BATCH_RECORDS = 4,
    WIREMUX_HOST_DECODE_COMPRESSION = 5,
} wiremux_host_decode_stage_t;

typedef struct {
    const uint8_t *data;
    size_t len;
} wiremux_bytes_view_t;

typedef struct {
    const char *data;
    size_t len;
} wiremux_string_view_t;

typedef struct {
    wiremux_host_decode_stage_t stage;
    wiremux_status_t status;
    uint32_t detail;
    wiremux_bytes_view_t payload;
} wiremux_host_decode_error_t;

typedef struct {
    uint8_t version;
    uint8_t flags;
    size_t payload_len;
    uint32_t expected_crc;
    uint32_t actual_crc;
} wiremux_host_crc_error_t;

typedef struct {
    wiremux_string_view_t device_name;
    wiremux_string_view_t firmware_version;
    uint32_t protocol_version;
    uint32_t max_channels;
    uint32_t native_endianness;
    uint32_t max_payload_len;
    wiremux_string_view_t transport;
    uint32_t feature_flags;
    wiremux_string_view_t sdk_name;
    wiremux_string_view_t sdk_version;
} wiremux_host_manifest_begin_t;

typedef struct {
    uint32_t channel_id;
    wiremux_string_view_t name;
    wiremux_string_view_t description;
    uint32_t flags;
    uint32_t default_payload_kind;
    uint32_t default_interaction_mode;
} wiremux_host_manifest_channel_t;

typedef struct {
    uint32_t device_api_version;
    uint32_t host_min_api_version;
    uint32_t host_current_api_version;
    wiremux_protocol_compatibility_t compatibility;
} wiremux_host_protocol_compatibility_event_t;

typedef struct {
    uint32_t compression;
    size_t encoded_bytes;
    size_t raw_bytes;
    size_t record_count;
} wiremux_host_batch_summary_t;

typedef struct {
    wiremux_host_event_type_t type;
    union {
        wiremux_bytes_view_t terminal;
        wiremux_envelope_t record;
        wiremux_host_crc_error_t crc_error;
        wiremux_host_decode_error_t decode_error;
        wiremux_host_manifest_begin_t manifest_begin;
        wiremux_host_manifest_channel_t manifest_channel;
        uint32_t manifest_channel_value;
        wiremux_string_view_t manifest_channel_payload_type;
        wiremux_host_protocol_compatibility_event_t protocol_compatibility;
        wiremux_host_batch_summary_t batch_summary;
    } data;
} wiremux_host_event_t;

typedef void (*wiremux_host_event_fn)(const wiremux_host_event_t *event, void *user_ctx);

typedef struct {
    size_t max_payload_len;
    uint8_t *buffer;
    size_t buffer_capacity;
    uint8_t *scratch;
    size_t scratch_capacity;
    wiremux_host_event_fn on_event;
    void *user_ctx;
} wiremux_host_session_config_t;

typedef struct {
    wiremux_host_session_config_t config;
    size_t buffer_len;
    uint32_t last_device_api_version;
    wiremux_protocol_compatibility_t last_compatibility;
    uint8_t manifest_seen;
} wiremux_host_session_t;

wiremux_status_t wiremux_host_session_init(wiremux_host_session_t *session,
                                           const wiremux_host_session_config_t *config);

wiremux_status_t wiremux_host_session_feed(wiremux_host_session_t *session,
                                           const uint8_t *data,
                                           size_t len);

wiremux_status_t wiremux_host_session_finish(wiremux_host_session_t *session);

wiremux_status_t wiremux_host_build_input_frame(uint32_t channel_id,
                                                const uint8_t *payload,
                                                size_t payload_len,
                                                uint8_t *out,
                                                size_t out_capacity,
                                                size_t *written);

wiremux_status_t wiremux_host_build_manifest_request_frame(uint8_t *out,
                                                           size_t out_capacity,
                                                           size_t *written);

#ifdef __cplusplus
}
#endif
