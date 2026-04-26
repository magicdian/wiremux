#pragma once

#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"
#include "wiremux_frame.h"

#ifdef __cplusplus
extern "C" {
#endif

#define ESP_WIREMUX_MAGIC WIREMUX_MAGIC
#define ESP_WIREMUX_MAGIC_LEN WIREMUX_MAGIC_LEN
#define ESP_WIREMUX_FRAME_VERSION WIREMUX_FRAME_VERSION
#define ESP_WIREMUX_FRAME_HEADER_LEN WIREMUX_FRAME_HEADER_LEN

typedef wiremux_frame_header_t esp_wiremux_frame_header_t;

size_t esp_wiremux_frame_encoded_len(size_t payload_len);

esp_err_t esp_wiremux_frame_encode(const esp_wiremux_frame_header_t *header,
                                   const uint8_t *payload,
                                   size_t payload_len,
                                   uint8_t *out,
                                   size_t out_capacity,
                                   size_t *written);

uint32_t esp_wiremux_crc32(const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif
