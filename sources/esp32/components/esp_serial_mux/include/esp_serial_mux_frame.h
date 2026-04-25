#pragma once

#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

#define ESP_SERIAL_MUX_MAGIC "ESMX"
#define ESP_SERIAL_MUX_MAGIC_LEN 4
#define ESP_SERIAL_MUX_FRAME_VERSION 1
#define ESP_SERIAL_MUX_FRAME_HEADER_LEN 14

typedef struct {
    uint8_t version;
    uint8_t flags;
} esp_serial_mux_frame_header_t;

size_t esp_serial_mux_frame_encoded_len(size_t payload_len);

esp_err_t esp_serial_mux_frame_encode(const esp_serial_mux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);

uint32_t esp_serial_mux_crc32(const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif
