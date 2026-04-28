#include "esp_wiremux_frame.h"

static esp_err_t wiremux_status_to_esp(wiremux_status_t status);

size_t esp_wiremux_frame_encoded_len(size_t payload_len)
{
    return wiremux_frame_encoded_len(payload_len);
}

uint32_t esp_wiremux_crc32(const uint8_t *data, size_t len)
{
    return wiremux_crc32(data, len);
}

esp_err_t esp_wiremux_frame_encode(const esp_wiremux_frame_header_t *header,
                                   const uint8_t *payload,
                                   size_t payload_len,
                                   uint8_t *out,
                                   size_t out_capacity,
                                   size_t *written)
{
    return wiremux_status_to_esp(wiremux_frame_encode(header,
                                                      payload,
                                                      payload_len,
                                                      out,
                                                      out_capacity,
                                                      written));
}

static esp_err_t wiremux_status_to_esp(wiremux_status_t status)
{
    switch (status) {
    case WIREMUX_STATUS_OK:
        return ESP_OK;
    case WIREMUX_STATUS_INVALID_ARG:
        return ESP_ERR_INVALID_ARG;
    case WIREMUX_STATUS_INVALID_SIZE:
        return ESP_ERR_INVALID_SIZE;
    case WIREMUX_STATUS_NOT_SUPPORTED:
        return ESP_ERR_NOT_SUPPORTED;
    case WIREMUX_STATUS_INCOMPLETE:
    case WIREMUX_STATUS_BAD_MAGIC:
    case WIREMUX_STATUS_BAD_VERSION:
    case WIREMUX_STATUS_CRC_MISMATCH:
        return ESP_FAIL;
    default:
        return ESP_FAIL;
    }
}
