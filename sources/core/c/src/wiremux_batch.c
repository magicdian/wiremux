#include "wiremux_batch.h"

#include <string.h>

#include "wiremux_proto_internal.h"

static uint8_t *write_record(uint8_t *out, const wiremux_record_t *record);
static wiremux_status_t decode_record(const uint8_t *data, size_t len, wiremux_record_t *record);

size_t wiremux_record_encoded_len(const wiremux_record_t *record)
{
    if (record == NULL) {
        return 0;
    }

    size_t len = wiremux_varint_field_len(1, record->channel_id) +
                 wiremux_varint_field_len(2, record->direction) +
                 wiremux_varint_field_len(3, record->sequence) +
                 wiremux_varint_field_len(4, record->timestamp_us) +
                 wiremux_varint_field_len(5, record->kind) +
                 wiremux_bytes_field_len(7, record->payload_len) +
                 wiremux_varint_field_len(8, record->flags);

    if (record->payload_type_len > 0) {
        len += wiremux_bytes_field_len(6, record->payload_type_len);
    }

    return len;
}

size_t wiremux_batch_records_encoded_len(const wiremux_record_t *records, size_t record_count)
{
    if (record_count > 0 && records == NULL) {
        return 0;
    }

    size_t len = 0;
    for (size_t i = 0; i < record_count; ++i) {
        const size_t record_len = wiremux_record_encoded_len(&records[i]);
        if (record_len == 0) {
            return 0;
        }
        len += wiremux_bytes_field_len(1, record_len);
    }
    return len;
}

wiremux_status_t wiremux_batch_records_encode(const wiremux_record_t *records,
                                              size_t record_count,
                                              uint8_t *out,
                                              size_t out_capacity,
                                              size_t *written)
{
    if (out == NULL || written == NULL || (record_count > 0 && records == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    for (size_t i = 0; i < record_count; ++i) {
        if ((records[i].payload_len > 0 && records[i].payload == NULL) ||
            (records[i].payload_type_len > 0 && records[i].payload_type == NULL)) {
            return WIREMUX_STATUS_INVALID_ARG;
        }
    }

    const size_t required = wiremux_batch_records_encoded_len(records, record_count);
    if (out_capacity < required) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *cursor = out;
    for (size_t i = 0; i < record_count; ++i) {
        const size_t record_len = wiremux_record_encoded_len(&records[i]);
        cursor = wiremux_write_varint(cursor, ((uint64_t)1 << 3) | 2u);
        cursor = wiremux_write_varint(cursor, record_len);
        cursor = write_record(cursor, &records[i]);
    }

    *written = (size_t)(cursor - out);
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_batch_records_decode(const uint8_t *data,
                                              size_t len,
                                              wiremux_record_t *records,
                                              size_t record_capacity,
                                              size_t *record_count)
{
    if (data == NULL || record_count == NULL || (record_capacity > 0 && records == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    size_t cursor = 0;
    size_t count = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint32_t field_number = (uint32_t)(key >> 3);
        const uint32_t wire_type = (uint32_t)(key & 0x07u);
        if (field_number != 1 || wire_type != 2) {
            if (wire_type == 0) {
                uint64_t ignored = 0;
                status = wiremux_read_varint(data, len, &cursor, &ignored);
                if (status != WIREMUX_STATUS_OK) {
                    return status;
                }
                continue;
            }
            if (wire_type == 2) {
                const uint8_t *ignored = NULL;
                size_t ignored_len = 0;
                status = wiremux_read_len_delimited(data, len, &cursor, &ignored, &ignored_len);
                if (status != WIREMUX_STATUS_OK) {
                    return status;
                }
                continue;
            }
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }

        const uint8_t *record_data = NULL;
        size_t record_len = 0;
        status = wiremux_read_len_delimited(data, len, &cursor, &record_data, &record_len);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        if (count >= record_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        status = decode_record(record_data, record_len, &records[count]);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        count++;
    }

    *record_count = count;
    return WIREMUX_STATUS_OK;
}

size_t wiremux_batch_encoded_len(const wiremux_batch_t *batch)
{
    if (batch == NULL || (batch->records_len > 0 && batch->records == NULL)) {
        return 0;
    }

    return wiremux_varint_field_len(1, batch->compression) +
           wiremux_bytes_field_len(2, batch->records_len) +
           wiremux_varint_field_len(3, batch->uncompressed_len);
}

wiremux_status_t wiremux_batch_encode(const wiremux_batch_t *batch,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written)
{
    if (batch == NULL || out == NULL || written == NULL ||
        (batch->records_len > 0 && batch->records == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    const size_t required = wiremux_batch_encoded_len(batch);
    if (out_capacity < required) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *cursor = out;
    cursor = wiremux_write_varint_field(cursor, 1, batch->compression);
    cursor = wiremux_write_bytes_field(cursor, 2, batch->records, batch->records_len);
    cursor = wiremux_write_varint_field(cursor, 3, batch->uncompressed_len);
    *written = (size_t)(cursor - out);
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_batch_decode(const uint8_t *data, size_t len, wiremux_batch_t *batch)
{
    if (data == NULL || batch == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    memset(batch, 0, sizeof(*batch));

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
            if (field_number == 1) {
                batch->compression = (uint32_t)varint;
            } else if (field_number == 3) {
                batch->uncompressed_len = (uint32_t)varint;
            }
            break;
        case 2: {
            const uint8_t *field = NULL;
            size_t field_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &field, &field_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            if (field_number == 2) {
                batch->records = field;
                batch->records_len = field_len;
            }
            break;
        }
        default:
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }

    return WIREMUX_STATUS_OK;
}

static uint8_t *write_record(uint8_t *out, const wiremux_record_t *record)
{
    out = wiremux_write_varint_field(out, 1, record->channel_id);
    out = wiremux_write_varint_field(out, 2, record->direction);
    out = wiremux_write_varint_field(out, 3, record->sequence);
    out = wiremux_write_varint_field(out, 4, record->timestamp_us);
    out = wiremux_write_varint_field(out, 5, record->kind);
    if (record->payload_type_len > 0) {
        out = wiremux_write_bytes_field(out,
                                        6,
                                        (const uint8_t *)record->payload_type,
                                        record->payload_type_len);
    }
    out = wiremux_write_bytes_field(out, 7, record->payload, record->payload_len);
    out = wiremux_write_varint_field(out, 8, record->flags);
    return out;
}

static wiremux_status_t decode_record(const uint8_t *data, size_t len, wiremux_record_t *record)
{
    if (data == NULL || record == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    memset(record, 0, sizeof(*record));
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
                record->channel_id = (uint32_t)varint;
                break;
            case 2:
                record->direction = (uint32_t)varint;
                break;
            case 3:
                record->sequence = (uint32_t)varint;
                break;
            case 4:
                record->timestamp_us = varint;
                break;
            case 5:
                record->kind = (uint32_t)varint;
                break;
            case 8:
                record->flags = (uint32_t)varint;
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
                record->payload_type = (const char *)field;
                record->payload_type_len = field_len;
            } else if (field_number == 7) {
                record->payload = field;
                record->payload_len = field_len;
            }
            break;
        }
        default:
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }

    return WIREMUX_STATUS_OK;
}
