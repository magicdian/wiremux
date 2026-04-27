#include "wiremux_manifest.h"

#include <stdbool.h>
#include <string.h>

#include "wiremux_proto_internal.h"

static size_t optional_string_field_len(uint32_t field_number, const char *value);
static uint8_t *write_optional_string_field(uint8_t *out, uint32_t field_number, const char *value);
static bool channel_descriptor_is_valid(const wiremux_channel_descriptor_t *channel);
static size_t channel_descriptor_encoded_len(const wiremux_channel_descriptor_t *channel);
static uint8_t *write_channel_descriptor(uint8_t *out, const wiremux_channel_descriptor_t *channel);

size_t wiremux_device_manifest_encoded_len(const wiremux_device_manifest_t *manifest)
{
    if (manifest == NULL || (manifest->channel_count > 0 && manifest->channels == NULL)) {
        return 0;
    }

    size_t len = optional_string_field_len(1, manifest->device_name) +
                 optional_string_field_len(2, manifest->firmware_version) +
                 wiremux_varint_field_len(3, manifest->protocol_version) +
                 wiremux_varint_field_len(4, manifest->max_channels) +
                 wiremux_varint_field_len(6, manifest->native_endianness) +
                 wiremux_varint_field_len(7, manifest->max_payload_len) +
                 optional_string_field_len(8, manifest->transport) +
                 wiremux_varint_field_len(9, manifest->feature_flags) +
                 optional_string_field_len(10, manifest->sdk_name) +
                 optional_string_field_len(11, manifest->sdk_version);

    for (size_t i = 0; i < manifest->channel_count; ++i) {
        if (!channel_descriptor_is_valid(&manifest->channels[i])) {
            return 0;
        }
        len += wiremux_bytes_field_len(5, channel_descriptor_encoded_len(&manifest->channels[i]));
    }

    return len;
}

wiremux_status_t wiremux_device_manifest_encode(const wiremux_device_manifest_t *manifest,
                                                uint8_t *out,
                                                size_t out_capacity,
                                                size_t *written)
{
    if (manifest == NULL || out == NULL || written == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    if (manifest->channel_count > 0 && manifest->channels == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }
    for (size_t i = 0; i < manifest->channel_count; ++i) {
        if (!channel_descriptor_is_valid(&manifest->channels[i])) {
            return WIREMUX_STATUS_INVALID_ARG;
        }
    }

    const size_t required = wiremux_device_manifest_encoded_len(manifest);
    if (out_capacity < required) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }

    uint8_t *cursor = out;
    cursor = write_optional_string_field(cursor, 1, manifest->device_name);
    cursor = write_optional_string_field(cursor, 2, manifest->firmware_version);
    cursor = wiremux_write_varint_field(cursor, 3, manifest->protocol_version);
    cursor = wiremux_write_varint_field(cursor, 4, manifest->max_channels);
    for (size_t i = 0; i < manifest->channel_count; ++i) {
        const size_t channel_len = channel_descriptor_encoded_len(&manifest->channels[i]);
        cursor = wiremux_write_varint(cursor, ((uint64_t)5 << 3) | 2u);
        cursor = wiremux_write_varint(cursor, channel_len);
        cursor = write_channel_descriptor(cursor, &manifest->channels[i]);
    }
    cursor = wiremux_write_varint_field(cursor, 6, manifest->native_endianness);
    cursor = wiremux_write_varint_field(cursor, 7, manifest->max_payload_len);
    cursor = write_optional_string_field(cursor, 8, manifest->transport);
    cursor = wiremux_write_varint_field(cursor, 9, manifest->feature_flags);
    cursor = write_optional_string_field(cursor, 10, manifest->sdk_name);
    cursor = write_optional_string_field(cursor, 11, manifest->sdk_version);

    *written = (size_t)(cursor - out);
    return WIREMUX_STATUS_OK;
}

static size_t optional_string_field_len(uint32_t field_number, const char *value)
{
    if (value == NULL || value[0] == '\0') {
        return 0;
    }
    return wiremux_bytes_field_len(field_number, strlen(value));
}

static uint8_t *write_optional_string_field(uint8_t *out, uint32_t field_number, const char *value)
{
    if (value == NULL || value[0] == '\0') {
        return out;
    }
    return wiremux_write_bytes_field(out, field_number, (const uint8_t *)value, strlen(value));
}

static bool channel_descriptor_is_valid(const wiremux_channel_descriptor_t *channel)
{
    return channel != NULL &&
           (channel->payload_kind_count == 0 || channel->payload_kinds != NULL) &&
           (channel->payload_type_count == 0 || channel->payload_types != NULL) &&
           (channel->interaction_mode_count == 0 || channel->interaction_modes != NULL);
}

static size_t channel_descriptor_encoded_len(const wiremux_channel_descriptor_t *channel)
{
    size_t len = wiremux_varint_field_len(1, channel->channel_id) +
                 optional_string_field_len(2, channel->name) +
                 optional_string_field_len(3, channel->description);

    if ((channel->directions & WIREMUX_DIRECTION_INPUT) != 0) {
        len += wiremux_varint_field_len(4, WIREMUX_DIRECTION_INPUT);
    }
    if ((channel->directions & WIREMUX_DIRECTION_OUTPUT) != 0) {
        len += wiremux_varint_field_len(4, WIREMUX_DIRECTION_OUTPUT);
    }
    if (channel->payload_kind_count > 0) {
        for (size_t i = 0; i < channel->payload_kind_count; ++i) {
            len += wiremux_varint_field_len(5, channel->payload_kinds[i]);
        }
    } else if (channel->default_payload_kind != WIREMUX_PAYLOAD_KIND_UNSPECIFIED) {
        len += wiremux_varint_field_len(5, channel->default_payload_kind);
    }
    for (size_t i = 0; i < channel->payload_type_count; ++i) {
        len += optional_string_field_len(6, channel->payload_types[i]);
    }
    if (channel->default_payload_kind != WIREMUX_PAYLOAD_KIND_UNSPECIFIED) {
        len += wiremux_varint_field_len(8, channel->default_payload_kind);
    }
    if (channel->flags != 0) {
        len += wiremux_varint_field_len(7, channel->flags);
    }
    if (channel->interaction_mode_count > 0) {
        for (size_t i = 0; i < channel->interaction_mode_count; ++i) {
            len += wiremux_varint_field_len(9, channel->interaction_modes[i]);
        }
    } else if (channel->default_interaction_mode != WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED) {
        len += wiremux_varint_field_len(9, channel->default_interaction_mode);
    }
    if (channel->default_interaction_mode != WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED) {
        len += wiremux_varint_field_len(10, channel->default_interaction_mode);
    }

    return len;
}

static uint8_t *write_channel_descriptor(uint8_t *out, const wiremux_channel_descriptor_t *channel)
{
    out = wiremux_write_varint_field(out, 1, channel->channel_id);
    out = write_optional_string_field(out, 2, channel->name);
    out = write_optional_string_field(out, 3, channel->description);
    if ((channel->directions & WIREMUX_DIRECTION_INPUT) != 0) {
        out = wiremux_write_varint_field(out, 4, WIREMUX_DIRECTION_INPUT);
    }
    if ((channel->directions & WIREMUX_DIRECTION_OUTPUT) != 0) {
        out = wiremux_write_varint_field(out, 4, WIREMUX_DIRECTION_OUTPUT);
    }
    if (channel->payload_kind_count > 0) {
        for (size_t i = 0; i < channel->payload_kind_count; ++i) {
            out = wiremux_write_varint_field(out, 5, channel->payload_kinds[i]);
        }
    } else if (channel->default_payload_kind != WIREMUX_PAYLOAD_KIND_UNSPECIFIED) {
        out = wiremux_write_varint_field(out, 5, channel->default_payload_kind);
    }
    for (size_t i = 0; i < channel->payload_type_count; ++i) {
        out = write_optional_string_field(out, 6, channel->payload_types[i]);
    }
    if (channel->default_payload_kind != WIREMUX_PAYLOAD_KIND_UNSPECIFIED) {
        out = wiremux_write_varint_field(out, 8, channel->default_payload_kind);
    }
    if (channel->flags != 0) {
        out = wiremux_write_varint_field(out, 7, channel->flags);
    }
    if (channel->interaction_mode_count > 0) {
        for (size_t i = 0; i < channel->interaction_mode_count; ++i) {
            out = wiremux_write_varint_field(out, 9, channel->interaction_modes[i]);
        }
    } else if (channel->default_interaction_mode != WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED) {
        out = wiremux_write_varint_field(out, 9, channel->default_interaction_mode);
    }
    if (channel->default_interaction_mode != WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED) {
        out = wiremux_write_varint_field(out, 10, channel->default_interaction_mode);
    }

    return out;
}
