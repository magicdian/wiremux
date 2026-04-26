#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_status.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    WIREMUX_DIRECTION_UNSPECIFIED = 0,
    WIREMUX_DIRECTION_INPUT = 1,
    WIREMUX_DIRECTION_OUTPUT = 2,
} wiremux_direction_t;

typedef enum {
    WIREMUX_PAYLOAD_KIND_UNSPECIFIED = 0,
    WIREMUX_PAYLOAD_KIND_TEXT = 1,
    WIREMUX_PAYLOAD_KIND_BINARY = 2,
    WIREMUX_PAYLOAD_KIND_PROTOBUF = 3,
    WIREMUX_PAYLOAD_KIND_CONTROL = 4,
    WIREMUX_PAYLOAD_KIND_EVENT = 5,
} wiremux_payload_kind_t;

typedef struct {
    uint32_t channel_id;
    uint32_t direction;
    uint32_t sequence;
    uint64_t timestamp_us;
    uint32_t kind;
    const char *payload_type;
    size_t payload_type_len;
    const uint8_t *payload;
    size_t payload_len;
    uint32_t flags;
} wiremux_envelope_t;

size_t wiremux_envelope_encoded_len(const wiremux_envelope_t *envelope);

wiremux_status_t wiremux_envelope_encode(const wiremux_envelope_t *envelope,
                                         uint8_t *out,
                                         size_t out_capacity,
                                         size_t *written);

wiremux_status_t wiremux_envelope_decode(const uint8_t *data,
                                         size_t len,
                                         wiremux_envelope_t *envelope);

#ifdef __cplusplus
}
#endif
