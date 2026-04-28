#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_envelope.h"
#include "wiremux_status.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WIREMUX_MANIFEST_PAYLOAD_TYPE "wiremux.v1.DeviceManifest"
#define WIREMUX_MANIFEST_REQUEST_PAYLOAD_TYPE "wiremux.v1.DeviceManifestRequest"
#define WIREMUX_SDK_NAME_ESP "esp-wiremux"
#define WIREMUX_FEATURE_MANIFEST_PROTOBUF (1u << 0)
#define WIREMUX_FEATURE_BATCH (1u << 1)
#define WIREMUX_FEATURE_COMPRESSION_HEATSHRINK (1u << 2)
#define WIREMUX_FEATURE_COMPRESSION_LZ4 (1u << 3)
#define WIREMUX_FEATURE_MANIFEST_REQUEST (1u << 4)
#define WIREMUX_CHANNEL_NAME_MAX_BYTES 15u

typedef enum {
    WIREMUX_ENDIANNESS_UNSPECIFIED = 0,
    WIREMUX_ENDIANNESS_LITTLE = 1,
    WIREMUX_ENDIANNESS_BIG = 2,
    WIREMUX_ENDIANNESS_MIXED = 3,
} wiremux_endianness_t;

typedef enum {
    WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED = 0,
    WIREMUX_CHANNEL_INTERACTION_LINE = 1,
    WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH = 2,
} wiremux_channel_interaction_mode_t;

typedef enum {
    WIREMUX_NEWLINE_POLICY_UNSPECIFIED = 0,
    WIREMUX_NEWLINE_POLICY_PRESERVE = 1,
    WIREMUX_NEWLINE_POLICY_LF = 2,
    WIREMUX_NEWLINE_POLICY_CR = 3,
    WIREMUX_NEWLINE_POLICY_CRLF = 4,
} wiremux_newline_policy_t;

typedef enum {
    WIREMUX_ECHO_POLICY_UNSPECIFIED = 0,
    WIREMUX_ECHO_POLICY_REMOTE = 1,
    WIREMUX_ECHO_POLICY_LOCAL = 2,
    WIREMUX_ECHO_POLICY_NONE = 3,
} wiremux_echo_policy_t;

typedef enum {
    WIREMUX_CONTROL_KEY_POLICY_UNSPECIFIED = 0,
    WIREMUX_CONTROL_KEY_POLICY_HOST_HANDLED = 1,
    WIREMUX_CONTROL_KEY_POLICY_FORWARDED = 2,
} wiremux_control_key_policy_t;

typedef enum {
    WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK = 1,
    WIREMUX_PASSTHROUGH_BACKEND_LINE_DISCIPLINE = 2,
    WIREMUX_PASSTHROUGH_BACKEND_REPL = 3,
} wiremux_passthrough_backend_t;

typedef struct {
    uint32_t input_newline_policy;
    uint32_t output_newline_policy;
    uint32_t echo_policy;
    uint32_t control_key_policy;
} wiremux_passthrough_policy_t;

typedef struct {
    uint32_t channel_id;
    const char *name;
    const char *description;
    uint32_t directions;
    const uint32_t *payload_kinds;
    size_t payload_kind_count;
    const char *const *payload_types;
    size_t payload_type_count;
    uint32_t default_payload_kind;
    uint32_t flags;
    const uint32_t *interaction_modes;
    size_t interaction_mode_count;
    uint32_t default_interaction_mode;
    wiremux_passthrough_policy_t passthrough_policy;
} wiremux_channel_descriptor_t;

typedef struct {
    const char *device_name;
    const char *firmware_version;
    uint32_t protocol_version;
    uint32_t max_channels;
    const wiremux_channel_descriptor_t *channels;
    size_t channel_count;
    uint32_t native_endianness;
    uint32_t max_payload_len;
    const char *transport;
    uint32_t feature_flags;
    const char *sdk_name;
    const char *sdk_version;
} wiremux_device_manifest_t;

size_t wiremux_device_manifest_encoded_len(const wiremux_device_manifest_t *manifest);

wiremux_status_t wiremux_device_manifest_encode(const wiremux_device_manifest_t *manifest,
                                                uint8_t *out,
                                                size_t out_capacity,
                                                size_t *written);

#ifdef __cplusplus
}
#endif
