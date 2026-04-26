#include "wiremux_frame.h"

#include <stdint.h>
#include <string.h>

#define CRC32_POLY_REVERSED 0xedb88320u

static void write_le32(uint8_t *out, uint32_t value)
{
    out[0] = (uint8_t)(value & 0xffu);
    out[1] = (uint8_t)((value >> 8) & 0xffu);
    out[2] = (uint8_t)((value >> 16) & 0xffu);
    out[3] = (uint8_t)((value >> 24) & 0xffu);
}

static uint32_t read_le32(const uint8_t *data)
{
    return (uint32_t)data[0] |
           ((uint32_t)data[1] << 8) |
           ((uint32_t)data[2] << 16) |
           ((uint32_t)data[3] << 24);
}

size_t wiremux_frame_encoded_len(size_t payload_len)
{
    return WIREMUX_FRAME_HEADER_LEN + payload_len;
}

uint32_t wiremux_crc32(const uint8_t *data, size_t len)
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

wiremux_status_t wiremux_frame_encode(const wiremux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written)
{
    if (header == NULL || out == NULL || (payload_len > 0 && payload == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    if (payload_len > UINT32_MAX) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    const size_t frame_len = wiremux_frame_encoded_len(payload_len);
    if (out_capacity < frame_len) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    memcpy(out, WIREMUX_MAGIC, WIREMUX_MAGIC_LEN);
    out[4] = header->version;
    out[5] = header->flags;
    write_le32(&out[6], (uint32_t)payload_len);
    write_le32(&out[10], wiremux_crc32(payload, payload_len));
    if (payload_len > 0) {
        memcpy(&out[WIREMUX_FRAME_HEADER_LEN], payload, payload_len);
    }

    if (written != NULL) {
        *written = frame_len;
    }

    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_frame_decode(const uint8_t *data,
                                      size_t len,
                                      size_t max_payload_len,
                                      wiremux_frame_view_t *frame)
{
    if (data == NULL || frame == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    memset(frame, 0, sizeof(*frame));

    if (len < WIREMUX_FRAME_HEADER_LEN) {
        return WIREMUX_STATUS_INCOMPLETE;
    }
    if (memcmp(data, WIREMUX_MAGIC, WIREMUX_MAGIC_LEN) != 0) {
        return WIREMUX_STATUS_BAD_MAGIC;
    }

    frame->header.version = data[4];
    frame->header.flags = data[5];
    if (frame->header.version != WIREMUX_FRAME_VERSION) {
        return WIREMUX_STATUS_BAD_VERSION;
    }

    frame->payload_len = (size_t)read_le32(&data[6]);
    if (frame->payload_len > max_payload_len) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    frame->frame_len = wiremux_frame_encoded_len(frame->payload_len);
    if (len < frame->frame_len) {
        return WIREMUX_STATUS_INCOMPLETE;
    }

    const uint32_t expected_crc = read_le32(&data[10]);
    frame->payload = &data[WIREMUX_FRAME_HEADER_LEN];
    if (wiremux_crc32(frame->payload, frame->payload_len) != expected_crc) {
        return WIREMUX_STATUS_CRC_MISMATCH;
    }

    return WIREMUX_STATUS_OK;
}
