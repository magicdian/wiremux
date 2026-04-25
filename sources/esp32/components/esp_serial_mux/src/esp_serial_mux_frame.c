#include "esp_serial_mux_frame.h"

#include <string.h>

#define CRC32_POLY_REVERSED 0xedb88320u

static void write_le32(uint8_t *out, uint32_t value)
{
    out[0] = (uint8_t)(value & 0xffu);
    out[1] = (uint8_t)((value >> 8) & 0xffu);
    out[2] = (uint8_t)((value >> 16) & 0xffu);
    out[3] = (uint8_t)((value >> 24) & 0xffu);
}

size_t esp_serial_mux_frame_encoded_len(size_t payload_len)
{
    return ESP_SERIAL_MUX_FRAME_HEADER_LEN + payload_len;
}

uint32_t esp_serial_mux_crc32(const uint8_t *data, size_t len)
{
    uint32_t crc = 0xffffffffu;

    for (size_t i = 0; i < len; ++i) {
        crc ^= data[i];
        for (int bit = 0; bit < 8; ++bit) {
            uint32_t mask = 0u - (crc & 1u);
            crc = (crc >> 1) ^ (CRC32_POLY_REVERSED & mask);
        }
    }

    return ~crc;
}

esp_err_t esp_serial_mux_frame_encode(const esp_serial_mux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written)
{
    if (header == NULL || out == NULL || (payload_len > 0 && payload == NULL)) {
        return ESP_ERR_INVALID_ARG;
    }

    const size_t frame_len = esp_serial_mux_frame_encoded_len(payload_len);
    if (out_capacity < frame_len) {
        return ESP_ERR_INVALID_SIZE;
    }

    memcpy(out, ESP_SERIAL_MUX_MAGIC, ESP_SERIAL_MUX_MAGIC_LEN);
    out[4] = header->version;
    out[5] = header->flags;
    write_le32(&out[6], (uint32_t)payload_len);
    write_le32(&out[10], esp_serial_mux_crc32(payload, payload_len));
    if (payload_len > 0) {
        memcpy(&out[ESP_SERIAL_MUX_FRAME_HEADER_LEN], payload, payload_len);
    }

    if (written != NULL) {
        *written = frame_len;
    }

    return ESP_OK;
}
