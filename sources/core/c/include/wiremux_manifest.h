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
