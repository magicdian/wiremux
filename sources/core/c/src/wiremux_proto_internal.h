#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_status.h"

#if defined(__GNUC__) || defined(__clang__)
#define WIREMUX_UNUSED __attribute__((unused))
#else
#define WIREMUX_UNUSED
#endif

static WIREMUX_UNUSED size_t wiremux_varint_len(uint64_t value)
{
    size_t len = 1;
    while (value >= 0x80u) {
        value >>= 7;
        len++;
    }
    return len;
}

static WIREMUX_UNUSED size_t wiremux_varint_field_len(uint32_t field_number, uint64_t value)
{
    return wiremux_varint_len(((uint64_t)field_number << 3) | 0u) + wiremux_varint_len(value);
}

static WIREMUX_UNUSED size_t wiremux_bytes_field_len(uint32_t field_number, size_t len)
{
    return wiremux_varint_len(((uint64_t)field_number << 3) | 2u) + wiremux_varint_len(len) + len;
}

static WIREMUX_UNUSED uint8_t *wiremux_write_varint(uint8_t *out, uint64_t value)
{
    while (value >= 0x80u) {
        *out++ = (uint8_t)(value | 0x80u);
        value >>= 7;
    }
    *out++ = (uint8_t)value;
    return out;
}

static WIREMUX_UNUSED uint8_t *wiremux_write_varint_field(uint8_t *out, uint32_t field_number, uint64_t value)
{
    out = wiremux_write_varint(out, ((uint64_t)field_number << 3) | 0u);
    return wiremux_write_varint(out, value);
}

static WIREMUX_UNUSED uint8_t *wiremux_write_bytes_field(uint8_t *out,
                                                         uint32_t field_number,
                                                         const uint8_t *data,
                                                         size_t len)
{
    out = wiremux_write_varint(out, ((uint64_t)field_number << 3) | 2u);
    out = wiremux_write_varint(out, len);
    if (len > 0) {
        for (size_t i = 0; i < len; ++i) {
            *out++ = data[i];
        }
    }
    return out;
}

static WIREMUX_UNUSED wiremux_status_t wiremux_read_varint(const uint8_t *data,
                                                           size_t len,
                                                           size_t *cursor,
                                                           uint64_t *value)
{
    if (data == NULL || cursor == NULL || value == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    uint64_t result = 0;
    for (uint8_t shift = 0; shift < 64; shift += 7) {
        if (*cursor >= len) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        const uint8_t byte = data[(*cursor)++];
        result |= ((uint64_t)(byte & 0x7fu)) << shift;
        if ((byte & 0x80u) == 0) {
            *value = result;
            return WIREMUX_STATUS_OK;
        }
    }

    return WIREMUX_STATUS_INVALID_SIZE;
}

static WIREMUX_UNUSED wiremux_status_t wiremux_read_len_delimited(const uint8_t *data,
                                                                  size_t len,
                                                                  size_t *cursor,
                                                                  const uint8_t **value,
                                                                  size_t *value_len)
{
    if (data == NULL || cursor == NULL || value == NULL || value_len == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    uint64_t field_len = 0;
    wiremux_status_t status = wiremux_read_varint(data, len, cursor, &field_len);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }
    if (field_len > (uint64_t)(len - *cursor)) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    *value = &data[*cursor];
    *value_len = (size_t)field_len;
    *cursor += (size_t)field_len;
    return WIREMUX_STATUS_OK;
}
