#include "wiremux_envelope.h"

#include <string.h>

#include "wiremux_proto_internal.h"

size_t wiremux_envelope_encoded_len(const wiremux_envelope_t *envelope)
{
    if (envelope == NULL) {
        return 0;
    }

    size_t len = wiremux_varint_field_len(1, envelope->channel_id) +
                 wiremux_varint_field_len(2, envelope->direction) +
                 wiremux_varint_field_len(3, envelope->sequence) +
                 wiremux_varint_field_len(4, envelope->timestamp_us) +
                 wiremux_varint_field_len(5, envelope->kind) +
                 wiremux_bytes_field_len(7, envelope->payload_len) +
                 wiremux_varint_field_len(8, envelope->flags);

    if (envelope->payload_type_len > 0) {
        len += wiremux_bytes_field_len(6, envelope->payload_type_len);
    }

    return len;
}

wiremux_status_t wiremux_envelope_encode(const wiremux_envelope_t *envelope,
                                         uint8_t *out,
                                         size_t out_capacity,
                                         size_t *written)
{
    if (envelope == NULL || out == NULL || written == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    if ((envelope->payload_len > 0 && envelope->payload == NULL) ||
        (envelope->payload_type_len > 0 && envelope->payload_type == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    const size_t required = wiremux_envelope_encoded_len(envelope);
    if (out_capacity < required) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *cursor = out;
    cursor = wiremux_write_varint_field(cursor, 1, envelope->channel_id);
    cursor = wiremux_write_varint_field(cursor, 2, envelope->direction);
    cursor = wiremux_write_varint_field(cursor, 3, envelope->sequence);
    cursor = wiremux_write_varint_field(cursor, 4, envelope->timestamp_us);
    cursor = wiremux_write_varint_field(cursor, 5, envelope->kind);
    if (envelope->payload_type_len > 0) {
        cursor = wiremux_write_bytes_field(cursor,
                                           6,
                                           (const uint8_t *)envelope->payload_type,
                                           envelope->payload_type_len);
    }
    cursor = wiremux_write_bytes_field(cursor, 7, envelope->payload, envelope->payload_len);
    cursor = wiremux_write_varint_field(cursor, 8, envelope->flags);

    *written = (size_t)(cursor - out);
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_envelope_decode(const uint8_t *data,
                                         size_t len,
                                         wiremux_envelope_t *envelope)
{
    if (data == NULL || envelope == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    memset(envelope, 0, sizeof(*envelope));

    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }

        const uint32_t field_number = (uint32_t)(key >> 3);
        const uint32_t wire_type = (uint32_t)(key & 0x07u);
        uint64_t varint = 0;

        switch (wire_type) {
        case 0:
            status = wiremux_read_varint(data, len, &cursor, &varint);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 1:
                envelope->channel_id = (uint32_t)varint;
                break;
            case 2:
                envelope->direction = (uint32_t)varint;
                break;
            case 3:
                envelope->sequence = (uint32_t)varint;
                break;
            case 4:
                envelope->timestamp_us = varint;
                break;
            case 5:
                envelope->kind = (uint32_t)varint;
                break;
            case 8:
                envelope->flags = (uint32_t)varint;
                break;
            default:
                break;
            }
            break;
        case 2: {
            const uint8_t *field = NULL;
            size_t field_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &field, &field_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            if (field_number == 6) {
                envelope->payload_type = (const char *)field;
                envelope->payload_type_len = field_len;
            } else if (field_number == 7) {
                envelope->payload = field;
                envelope->payload_len = field_len;
            }
            break;
        }
        default:
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }

    return WIREMUX_STATUS_OK;
}
