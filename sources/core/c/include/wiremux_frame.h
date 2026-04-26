#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_status.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WIREMUX_MAGIC "WMUX"
#define WIREMUX_MAGIC_LEN 4
#define WIREMUX_FRAME_VERSION 1
#define WIREMUX_FRAME_HEADER_LEN 14

typedef struct {
    uint8_t version;
    uint8_t flags;
} wiremux_frame_header_t;

typedef struct {
    wiremux_frame_header_t header;
    const uint8_t *payload;
    size_t payload_len;
    size_t frame_len;
} wiremux_frame_view_t;

size_t wiremux_frame_encoded_len(size_t payload_len);

wiremux_status_t wiremux_frame_encode(const wiremux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);

wiremux_status_t wiremux_frame_decode(const uint8_t *data,
                                      size_t len,
                                      size_t max_payload_len,
                                      wiremux_frame_view_t *frame);

uint32_t wiremux_crc32(const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif
