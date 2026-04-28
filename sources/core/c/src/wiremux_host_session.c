#include "wiremux_host_session.h"

#include <stdbool.h>
#include <string.h>

#include "wiremux_batch.h"
#include "wiremux_compression.h"
#include "wiremux_manifest.h"
#include "wiremux_proto_internal.h"

typedef struct {
    wiremux_string_view_t device_name;
    wiremux_string_view_t firmware_version;
    uint32_t protocol_version;
    uint32_t max_channels;
    uint32_t native_endianness;
    uint32_t max_payload_len;
    wiremux_string_view_t transport;
    uint32_t feature_flags;
    wiremux_string_view_t sdk_name;
    wiremux_string_view_t sdk_version;
} manifest_header_t;

static uint32_t read_le32(const uint8_t *data);
static void drain_prefix(wiremux_host_session_t *session, size_t len);
static void emit_bytes(wiremux_host_session_t *session,
                       wiremux_host_event_type_t type,
                       const uint8_t *data,
                       size_t len);
static void emit_decode_error(wiremux_host_session_t *session,
                              wiremux_host_decode_stage_t stage,
                              wiremux_status_t status,
                              uint32_t detail,
                              const uint8_t *payload,
                              size_t payload_len);
static size_t find_magic(const uint8_t *data, size_t len);
static size_t magic_prefix_suffix_len(const uint8_t *data, size_t len);
static void process_buffer(wiremux_host_session_t *session);
static void handle_frame_payload(wiremux_host_session_t *session,
                                 const uint8_t *payload,
                                 size_t payload_len);
static void handle_envelope(wiremux_host_session_t *session,
                            const wiremux_envelope_t *envelope);
static bool payload_type_equals(const wiremux_envelope_t *envelope, const char *expected);
static wiremux_status_t emit_manifest(wiremux_host_session_t *session,
                                      const uint8_t *data,
                                      size_t len);
static wiremux_status_t parse_manifest_header(const uint8_t *data,
                                              size_t len,
                                              manifest_header_t *header);
static wiremux_status_t emit_manifest_channels(wiremux_host_session_t *session,
                                               const uint8_t *data,
                                               size_t len);
static wiremux_status_t emit_manifest_channel(wiremux_host_session_t *session,
                                              const uint8_t *data,
                                              size_t len);
static wiremux_status_t decode_passthrough_policy(const uint8_t *data,
                                                  size_t len,
                                                  wiremux_passthrough_policy_t *policy);
static wiremux_status_t handle_batch_payload(wiremux_host_session_t *session,
                                             const uint8_t *data,
                                             size_t len);
static wiremux_status_t emit_batch_record_list(wiremux_host_session_t *session,
                                               const uint8_t *data,
                                               size_t len,
                                               size_t *record_count);
static wiremux_status_t build_control_frame(const char *payload_type,
                                            const uint8_t *payload,
                                            size_t payload_len,
                                            uint8_t *out,
                                            size_t out_capacity,
                                            size_t *written);

wiremux_status_t wiremux_host_session_init(wiremux_host_session_t *session,
                                           const wiremux_host_session_config_t *config)
{
    if (session == NULL || config == NULL || config->buffer == NULL ||
        config->buffer_capacity < WIREMUX_HOST_SESSION_MIN_BUFFER_CAPACITY ||
        config->on_event == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    if (config->scratch_capacity > 0 && config->scratch == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    memset(session, 0, sizeof(*session));
    session->config = *config;
    session->last_compatibility = WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_OLD;
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_host_session_feed(wiremux_host_session_t *session,
                                           const uint8_t *data,
                                           size_t len)
{
    if (session == NULL || (len > 0 && data == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    for (size_t i = 0; i < len; ++i) {
        if (session->buffer_len == session->config.buffer_capacity) {
            emit_bytes(session, WIREMUX_HOST_EVENT_TERMINAL, session->config.buffer, 1);
            drain_prefix(session, 1);
        }
        session->config.buffer[session->buffer_len++] = data[i];
        process_buffer(session);
    }
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_host_session_finish(wiremux_host_session_t *session)
{
    if (session == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    if (session->buffer_len > 0) {
        emit_bytes(session,
                   WIREMUX_HOST_EVENT_TERMINAL,
                   session->config.buffer,
                   session->buffer_len);
        session->buffer_len = 0;
    }
    return WIREMUX_STATUS_OK;
}

wiremux_status_t wiremux_host_build_input_frame(uint32_t channel_id,
                                                const uint8_t *payload,
                                                size_t payload_len,
                                                uint8_t *out,
                                                size_t out_capacity,
                                                size_t *written)
{
    if (channel_id > UINT8_MAX || (payload_len > 0 && payload == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    const wiremux_envelope_t envelope = {
        channel_id,
        WIREMUX_DIRECTION_INPUT,
        0,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        NULL,
        0,
        payload,
        payload_len,
        0,
    };
    const size_t envelope_len = wiremux_envelope_encoded_len(&envelope);
    if (envelope_len > out_capacity) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *scratch = out;
    size_t envelope_written = 0;
    wiremux_status_t status = wiremux_envelope_encode(&envelope,
                                                      scratch,
                                                      out_capacity,
                                                      &envelope_written);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }
    if (wiremux_frame_encoded_len(envelope_written) > out_capacity) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    memmove(out + WIREMUX_FRAME_HEADER_LEN, scratch, envelope_written);
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    return wiremux_frame_encode(&header,
                                out + WIREMUX_FRAME_HEADER_LEN,
                                envelope_written,
                                out,
                                out_capacity,
                                written);
}

wiremux_status_t wiremux_host_build_manifest_request_frame(uint8_t *out,
                                                           size_t out_capacity,
                                                           size_t *written)
{
    return build_control_frame(WIREMUX_MANIFEST_REQUEST_PAYLOAD_TYPE, NULL, 0, out, out_capacity, written);
}

static uint32_t read_le32(const uint8_t *data)
{
    return (uint32_t)data[0] |
           ((uint32_t)data[1] << 8) |
           ((uint32_t)data[2] << 16) |
           ((uint32_t)data[3] << 24);
}

static void drain_prefix(wiremux_host_session_t *session, size_t len)
{
    if (len >= session->buffer_len) {
        session->buffer_len = 0;
        return;
    }
    memmove(session->config.buffer, session->config.buffer + len, session->buffer_len - len);
    session->buffer_len -= len;
}

static void emit_bytes(wiremux_host_session_t *session,
                       wiremux_host_event_type_t type,
                       const uint8_t *data,
                       size_t len)
{
    if (len == 0) {
        return;
    }
    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = type;
    event.data.terminal.data = data;
    event.data.terminal.len = len;
    session->config.on_event(&event, session->config.user_ctx);
}

static void emit_decode_error(wiremux_host_session_t *session,
                              wiremux_host_decode_stage_t stage,
                              wiremux_status_t status,
                              uint32_t detail,
                              const uint8_t *payload,
                              size_t payload_len)
{
    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_DECODE_ERROR;
    event.data.decode_error.stage = stage;
    event.data.decode_error.status = status;
    event.data.decode_error.detail = detail;
    event.data.decode_error.payload.data = payload;
    event.data.decode_error.payload.len = payload_len;
    session->config.on_event(&event, session->config.user_ctx);
}

static size_t find_magic(const uint8_t *data, size_t len)
{
    if (len < WIREMUX_MAGIC_LEN) {
        return len;
    }
    for (size_t i = 0; i <= len - WIREMUX_MAGIC_LEN; ++i) {
        if (memcmp(data + i, WIREMUX_MAGIC, WIREMUX_MAGIC_LEN) == 0) {
            return i;
        }
    }
    return len;
}

static size_t magic_prefix_suffix_len(const uint8_t *data, size_t len)
{
    const size_t max_len = len < (WIREMUX_MAGIC_LEN - 1) ? len : (WIREMUX_MAGIC_LEN - 1);
    for (size_t suffix_len = max_len; suffix_len > 0; --suffix_len) {
        if (memcmp(data + len - suffix_len, WIREMUX_MAGIC, suffix_len) == 0) {
            return suffix_len;
        }
    }
    return 0;
}

static void process_buffer(wiremux_host_session_t *session)
{
    for (;;) {
        const size_t magic_pos = find_magic(session->config.buffer, session->buffer_len);
        if (magic_pos == session->buffer_len) {
            const size_t keep_len = magic_prefix_suffix_len(session->config.buffer, session->buffer_len);
            const size_t emit_len = session->buffer_len - keep_len;
            if (emit_len > 0) {
                emit_bytes(session, WIREMUX_HOST_EVENT_TERMINAL, session->config.buffer, emit_len);
                drain_prefix(session, emit_len);
            }
            return;
        }
        if (magic_pos > 0) {
            emit_bytes(session, WIREMUX_HOST_EVENT_TERMINAL, session->config.buffer, magic_pos);
            drain_prefix(session, magic_pos);
            continue;
        }
        if (session->buffer_len < WIREMUX_FRAME_HEADER_LEN) {
            return;
        }

        const uint8_t version = session->config.buffer[4];
        if (version != WIREMUX_FRAME_VERSION) {
            emit_bytes(session, WIREMUX_HOST_EVENT_TERMINAL, session->config.buffer, 1);
            drain_prefix(session, 1);
            continue;
        }

        const uint8_t flags = session->config.buffer[5];
        const size_t payload_len = (size_t)read_le32(session->config.buffer + 6);
        if (payload_len > session->config.max_payload_len) {
            emit_bytes(session, WIREMUX_HOST_EVENT_TERMINAL, session->config.buffer, 1);
            drain_prefix(session, 1);
            continue;
        }
        const size_t total_len = WIREMUX_FRAME_HEADER_LEN + payload_len;
        if (session->buffer_len < total_len) {
            return;
        }

        const uint32_t expected_crc = read_le32(session->config.buffer + 10);
        const uint8_t *payload = session->config.buffer + WIREMUX_FRAME_HEADER_LEN;
        const uint32_t actual_crc = wiremux_crc32(payload, payload_len);
        if (actual_crc != expected_crc) {
            wiremux_host_event_t event;
            memset(&event, 0, sizeof(event));
            event.type = WIREMUX_HOST_EVENT_CRC_ERROR;
            event.data.crc_error.version = version;
            event.data.crc_error.flags = flags;
            event.data.crc_error.payload_len = payload_len;
            event.data.crc_error.expected_crc = expected_crc;
            event.data.crc_error.actual_crc = actual_crc;
            session->config.on_event(&event, session->config.user_ctx);
            drain_prefix(session, total_len);
            continue;
        }

        handle_frame_payload(session, payload, payload_len);
        drain_prefix(session, total_len);
    }
}

static void handle_frame_payload(wiremux_host_session_t *session,
                                 const uint8_t *payload,
                                 size_t payload_len)
{
    wiremux_envelope_t envelope;
    const wiremux_status_t status = wiremux_envelope_decode(payload, payload_len, &envelope);
    if (status != WIREMUX_STATUS_OK) {
        emit_decode_error(session, WIREMUX_HOST_DECODE_ENVELOPE, status, 0, payload, payload_len);
        return;
    }
    handle_envelope(session, &envelope);
}

static void handle_envelope(wiremux_host_session_t *session,
                            const wiremux_envelope_t *envelope)
{
    if (payload_type_equals(envelope, WIREMUX_MANIFEST_PAYLOAD_TYPE)) {
        const wiremux_status_t status = emit_manifest(session, envelope->payload, envelope->payload_len);
        if (status != WIREMUX_STATUS_OK) {
            emit_decode_error(session,
                              WIREMUX_HOST_DECODE_MANIFEST,
                              status,
                              0,
                              envelope->payload,
                              envelope->payload_len);
        }
        return;
    }
    if (payload_type_equals(envelope, WIREMUX_BATCH_PAYLOAD_TYPE)) {
        const wiremux_status_t status = handle_batch_payload(session, envelope->payload, envelope->payload_len);
        if (status != WIREMUX_STATUS_OK) {
            emit_decode_error(session,
                              WIREMUX_HOST_DECODE_BATCH,
                              status,
                              0,
                              envelope->payload,
                              envelope->payload_len);
        }
        return;
    }

    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_RECORD;
    event.data.record = *envelope;
    session->config.on_event(&event, session->config.user_ctx);
}

static bool payload_type_equals(const wiremux_envelope_t *envelope, const char *expected)
{
    const size_t expected_len = strlen(expected);
    return envelope->payload_type != NULL &&
           envelope->payload_type_len == expected_len &&
           memcmp(envelope->payload_type, expected, expected_len) == 0;
}

static wiremux_status_t emit_manifest(wiremux_host_session_t *session,
                                      const uint8_t *data,
                                      size_t len)
{
    manifest_header_t header;
    wiremux_status_t status = parse_manifest_header(data, len, &header);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }

    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_MANIFEST_BEGIN;
    event.data.manifest_begin.device_name = header.device_name;
    event.data.manifest_begin.firmware_version = header.firmware_version;
    event.data.manifest_begin.protocol_version = header.protocol_version;
    event.data.manifest_begin.max_channels = header.max_channels;
    event.data.manifest_begin.native_endianness = header.native_endianness;
    event.data.manifest_begin.max_payload_len = header.max_payload_len;
    event.data.manifest_begin.transport = header.transport;
    event.data.manifest_begin.feature_flags = header.feature_flags;
    event.data.manifest_begin.sdk_name = header.sdk_name;
    event.data.manifest_begin.sdk_version = header.sdk_version;
    session->config.on_event(&event, session->config.user_ctx);

    status = emit_manifest_channels(session, data, len);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }

    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_MANIFEST_END;
    session->config.on_event(&event, session->config.user_ctx);

    const wiremux_protocol_compatibility_t compatibility =
        wiremux_protocol_api_compatibility(header.protocol_version);
    session->last_device_api_version = header.protocol_version;
    session->last_compatibility = compatibility;
    session->manifest_seen = 1;

    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_PROTOCOL_COMPATIBILITY;
    event.data.protocol_compatibility.device_api_version = header.protocol_version;
    event.data.protocol_compatibility.host_min_api_version = WIREMUX_PROTOCOL_API_VERSION_MIN_SUPPORTED;
    event.data.protocol_compatibility.host_current_api_version = WIREMUX_PROTOCOL_API_VERSION_CURRENT;
    event.data.protocol_compatibility.compatibility = compatibility;
    session->config.on_event(&event, session->config.user_ctx);

    return WIREMUX_STATUS_OK;
}

static wiremux_status_t parse_manifest_header(const uint8_t *data,
                                              size_t len,
                                              manifest_header_t *header)
{
    if (data == NULL || header == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    memset(header, 0, sizeof(*header));

    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        uint64_t varint = 0;
        const uint8_t *field = NULL;
        size_t field_len = 0;

        switch (wire_type) {
        case 0:
            status = wiremux_read_varint(data, len, &cursor, &varint);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 3:
                header->protocol_version = (uint32_t)varint;
                break;
            case 4:
                header->max_channels = (uint32_t)varint;
                break;
            case 6:
                header->native_endianness = (uint32_t)varint;
                break;
            case 7:
                header->max_payload_len = (uint32_t)varint;
                break;
            case 9:
                header->feature_flags = (uint32_t)varint;
                break;
            default:
                break;
            }
            break;
        case 2:
            status = wiremux_read_len_delimited(data, len, &cursor, &field, &field_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 1:
                header->device_name.data = (const char *)field;
                header->device_name.len = field_len;
                break;
            case 2:
                header->firmware_version.data = (const char *)field;
                header->firmware_version.len = field_len;
                break;
            case 8:
                header->transport.data = (const char *)field;
                header->transport.len = field_len;
                break;
            case 10:
                header->sdk_name.data = (const char *)field;
                header->sdk_name.len = field_len;
                break;
            case 11:
                header->sdk_version.data = (const char *)field;
                header->sdk_version.len = field_len;
                break;
            default:
                break;
            }
            break;
        default:
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t emit_manifest_channels(wiremux_host_session_t *session,
                                               const uint8_t *data,
                                               size_t len)
{
    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        if (field_number == 5 && wire_type == 2) {
            const uint8_t *channel = NULL;
            size_t channel_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &channel, &channel_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            status = emit_manifest_channel(session, channel, channel_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else if (wire_type == 0) {
            uint64_t ignored = 0;
            status = wiremux_read_varint(data, len, &cursor, &ignored);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else if (wire_type == 2) {
            const uint8_t *ignored = NULL;
            size_t ignored_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &ignored, &ignored_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else {
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t emit_manifest_channel(wiremux_host_session_t *session,
                                              const uint8_t *data,
                                              size_t len)
{
    wiremux_host_manifest_channel_t channel;
    memset(&channel, 0, sizeof(channel));

    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        uint64_t varint = 0;
        const uint8_t *field = NULL;
        size_t field_len = 0;
        if (wire_type == 0) {
            status = wiremux_read_varint(data, len, &cursor, &varint);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 1:
                channel.channel_id = (uint32_t)varint;
                break;
            case 7:
                channel.flags = (uint32_t)varint;
                break;
            case 8:
                channel.default_payload_kind = (uint32_t)varint;
                break;
            case 10:
                channel.default_interaction_mode = (uint32_t)varint;
                break;
            default:
                break;
            }
        } else if (wire_type == 2) {
            status = wiremux_read_len_delimited(data, len, &cursor, &field, &field_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 2:
                channel.name.data = (const char *)field;
                channel.name.len = field_len;
                break;
            case 3:
                channel.description.data = (const char *)field;
                channel.description.len = field_len;
                break;
            case 11:
                status = decode_passthrough_policy(field, field_len, &channel.passthrough_policy);
                if (status != WIREMUX_STATUS_OK) {
                    return status;
                }
                break;
            default:
                break;
            }
        } else {
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }

    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_BEGIN;
    event.data.manifest_channel = channel;
    session->config.on_event(&event, session->config.user_ctx);

    cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        uint64_t varint = 0;
        const uint8_t *field = NULL;
        size_t field_len = 0;
        if (wire_type == 0) {
            status = wiremux_read_varint(data, len, &cursor, &varint);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            memset(&event, 0, sizeof(event));
            if (field_number == 4) {
                event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_DIRECTION;
                event.data.manifest_channel_value = (uint32_t)varint;
                session->config.on_event(&event, session->config.user_ctx);
            } else if (field_number == 5) {
                event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_PAYLOAD_KIND;
                event.data.manifest_channel_value = (uint32_t)varint;
                session->config.on_event(&event, session->config.user_ctx);
            } else if (field_number == 9) {
                event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_INTERACTION_MODE;
                event.data.manifest_channel_value = (uint32_t)varint;
                session->config.on_event(&event, session->config.user_ctx);
            }
        } else if (wire_type == 2) {
            status = wiremux_read_len_delimited(data, len, &cursor, &field, &field_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            if (field_number == 6) {
                memset(&event, 0, sizeof(event));
                event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_PAYLOAD_TYPE;
                event.data.manifest_channel_payload_type.data = (const char *)field;
                event.data.manifest_channel_payload_type.len = field_len;
                session->config.on_event(&event, session->config.user_ctx);
            }
        } else {
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }

    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_END;
    session->config.on_event(&event, session->config.user_ctx);
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t decode_passthrough_policy(const uint8_t *data,
                                                  size_t len,
                                                  wiremux_passthrough_policy_t *policy)
{
    if (policy == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        if (wire_type == 0) {
            uint64_t varint = 0;
            status = wiremux_read_varint(data, len, &cursor, &varint);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            switch (field_number) {
            case 1:
                policy->input_newline_policy = (uint32_t)varint;
                break;
            case 2:
                policy->output_newline_policy = (uint32_t)varint;
                break;
            case 3:
                policy->echo_policy = (uint32_t)varint;
                break;
            case 4:
                policy->control_key_policy = (uint32_t)varint;
                break;
            default:
                break;
            }
        } else if (wire_type == 2) {
            const uint8_t *ignored = NULL;
            size_t ignored_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &ignored, &ignored_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else {
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t handle_batch_payload(wiremux_host_session_t *session,
                                             const uint8_t *data,
                                             size_t len)
{
    wiremux_batch_t batch;
    wiremux_status_t status = wiremux_batch_decode(data, len, &batch);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }

    const uint8_t *records_payload = batch.records;
    size_t records_payload_len = batch.records_len;
    if (batch.compression != WIREMUX_COMPRESSION_NONE) {
        if (batch.uncompressed_len > session->config.scratch_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        size_t written = 0;
        status = wiremux_decompress(batch.compression,
                                    batch.records,
                                    batch.records_len,
                                    session->config.scratch,
                                    session->config.scratch_capacity,
                                    &written);
        if (status != WIREMUX_STATUS_OK) {
            emit_decode_error(session,
                              WIREMUX_HOST_DECODE_COMPRESSION,
                              status,
                              batch.compression,
                              batch.records,
                              batch.records_len);
            return WIREMUX_STATUS_OK;
        }
        records_payload = session->config.scratch;
        records_payload_len = written;
    }

    size_t record_count = 0;
    status = emit_batch_record_list(session, records_payload, records_payload_len, &record_count);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }

    wiremux_host_event_t event;
    memset(&event, 0, sizeof(event));
    event.type = WIREMUX_HOST_EVENT_BATCH_SUMMARY;
    event.data.batch_summary.compression = batch.compression;
    event.data.batch_summary.encoded_bytes = batch.records_len;
    event.data.batch_summary.raw_bytes = records_payload_len;
    event.data.batch_summary.record_count = record_count;
    session->config.on_event(&event, session->config.user_ctx);
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t emit_batch_record_list(wiremux_host_session_t *session,
                                               const uint8_t *data,
                                               size_t len,
                                               size_t *record_count)
{
    if (record_count == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    *record_count = 0;
    size_t cursor = 0;
    while (cursor < len) {
        uint64_t key = 0;
        wiremux_status_t status = wiremux_read_varint(data, len, &cursor, &key);
        if (status != WIREMUX_STATUS_OK) {
            return status;
        }
        const uint64_t field_number = key >> 3;
        const uint64_t wire_type = key & 0x07u;
        if (field_number == 1 && wire_type == 2) {
            const uint8_t *record_data = NULL;
            size_t record_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &record_data, &record_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
            wiremux_record_t record;
            status = wiremux_envelope_decode(record_data, record_len, &record);
            if (status != WIREMUX_STATUS_OK) {
                emit_decode_error(session,
                                  WIREMUX_HOST_DECODE_BATCH_RECORDS,
                                  status,
                                  0,
                                  record_data,
                                  record_len);
                continue;
            }
            handle_envelope(session, &record);
            (*record_count)++;
        } else if (wire_type == 0) {
            uint64_t ignored = 0;
            status = wiremux_read_varint(data, len, &cursor, &ignored);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else if (wire_type == 2) {
            const uint8_t *ignored = NULL;
            size_t ignored_len = 0;
            status = wiremux_read_len_delimited(data, len, &cursor, &ignored, &ignored_len);
            if (status != WIREMUX_STATUS_OK) {
                return status;
            }
        } else {
            return WIREMUX_STATUS_NOT_SUPPORTED;
        }
    }
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t build_control_frame(const char *payload_type,
                                            const uint8_t *payload,
                                            size_t payload_len,
                                            uint8_t *out,
                                            size_t out_capacity,
                                            size_t *written)
{
    if (payload_type == NULL || out == NULL || written == NULL ||
        (payload_len > 0 && payload == NULL)) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    const wiremux_envelope_t envelope = {
        0,
        WIREMUX_DIRECTION_INPUT,
        0,
        0,
        WIREMUX_PAYLOAD_KIND_CONTROL,
        payload_type,
        strlen(payload_type),
        payload,
        payload_len,
        0,
    };
    const size_t envelope_len = wiremux_envelope_encoded_len(&envelope);
    if (wiremux_frame_encoded_len(envelope_len) > out_capacity) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *envelope_out = out + WIREMUX_FRAME_HEADER_LEN;
    size_t envelope_written = 0;
    wiremux_status_t status = wiremux_envelope_encode(&envelope,
                                                      envelope_out,
                                                      out_capacity - WIREMUX_FRAME_HEADER_LEN,
                                                      &envelope_written);
    if (status != WIREMUX_STATUS_OK) {
        return status;
    }
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    return wiremux_frame_encode(&header, envelope_out, envelope_written, out, out_capacity, written);
}
