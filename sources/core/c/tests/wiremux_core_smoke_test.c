#include <assert.h>
#include <stdint.h>
#include <string.h>

#include "wiremux_envelope.h"
#include "wiremux_frame.h"
#include "wiremux_manifest.h"

static void test_crc32(void)
{
    assert(wiremux_crc32((const uint8_t *)"123456789", 9) == 0xcbf43926u);
}

static void test_envelope_round_trip(void)
{
    const char payload_type[] = "wiremux.test.Payload";
    const uint8_t payload[] = {0x01, 0x02, 0x03};
    const wiremux_envelope_t envelope = {
        .channel_id = 3,
        .direction = WIREMUX_DIRECTION_OUTPUT,
        .sequence = 7,
        .timestamp_us = 12345,
        .kind = WIREMUX_PAYLOAD_KIND_PROTOBUF,
        .payload_type = payload_type,
        .payload_type_len = strlen(payload_type),
        .payload = payload,
        .payload_len = sizeof(payload),
        .flags = 9,
    };

    uint8_t encoded[128];
    size_t written = 0;
    assert(wiremux_envelope_encode(&envelope, encoded, sizeof(encoded), &written) ==
           WIREMUX_STATUS_OK);
    assert(written == wiremux_envelope_encoded_len(&envelope));

    wiremux_envelope_t decoded = {0};
    assert(wiremux_envelope_decode(encoded, written, &decoded) == WIREMUX_STATUS_OK);
    assert(decoded.channel_id == envelope.channel_id);
    assert(decoded.direction == envelope.direction);
    assert(decoded.sequence == envelope.sequence);
    assert(decoded.timestamp_us == envelope.timestamp_us);
    assert(decoded.kind == envelope.kind);
    assert(decoded.payload_type_len == envelope.payload_type_len);
    assert(memcmp(decoded.payload_type, payload_type, decoded.payload_type_len) == 0);
    assert(decoded.payload_len == sizeof(payload));
    assert(memcmp(decoded.payload, payload, decoded.payload_len) == 0);
    assert(decoded.flags == envelope.flags);
}

static void test_manifest_encode(void)
{
    const uint32_t telemetry_kinds[] = {
        WIREMUX_PAYLOAD_KIND_TEXT,
        WIREMUX_PAYLOAD_KIND_PROTOBUF,
    };
    const char *const telemetry_types[] = {
        "wiremux.test.Telemetry",
    };
    const wiremux_channel_descriptor_t channels[] = {
        {
            .channel_id = 0,
            .name = "system",
            .description = "system control",
            .directions = WIREMUX_DIRECTION_OUTPUT,
            .default_payload_kind = WIREMUX_PAYLOAD_KIND_CONTROL,
        },
        {
            .channel_id = 3,
            .name = "telemetry",
            .description = "sample telemetry",
            .directions = WIREMUX_DIRECTION_OUTPUT,
            .payload_kinds = telemetry_kinds,
            .payload_kind_count = sizeof(telemetry_kinds) / sizeof(telemetry_kinds[0]),
            .payload_types = telemetry_types,
            .payload_type_count = sizeof(telemetry_types) / sizeof(telemetry_types[0]),
            .default_payload_kind = WIREMUX_PAYLOAD_KIND_TEXT,
        },
    };
    const wiremux_device_manifest_t manifest = {
        .device_name = "test-device",
        .firmware_version = "0.1.0",
        .protocol_version = WIREMUX_FRAME_VERSION,
        .max_channels = 8,
        .channels = channels,
        .channel_count = sizeof(channels) / sizeof(channels[0]),
        .native_endianness = WIREMUX_ENDIANNESS_LITTLE,
        .max_payload_len = 512,
        .transport = "loopback",
        .feature_flags = WIREMUX_FEATURE_MANIFEST_PROTOBUF,
        .sdk_name = WIREMUX_SDK_NAME_ESP,
        .sdk_version = "0.1.0",
    };

    uint8_t encoded[512];
    size_t written = 0;
    assert(wiremux_device_manifest_encode(&manifest, encoded, sizeof(encoded), &written) ==
           WIREMUX_STATUS_OK);
    assert(written == wiremux_device_manifest_encoded_len(&manifest));
    assert(written > 0);
}

static void test_frame_decode(void)
{
    const uint8_t payload[] = {0x08, 0x01, 0x10, 0x02};
    const wiremux_frame_header_t header = {
        .version = WIREMUX_FRAME_VERSION,
        .flags = 0x05,
    };
    uint8_t frame[WIREMUX_FRAME_HEADER_LEN + sizeof(payload)];
    size_t written = 0;

    assert(wiremux_frame_encode(&header, payload, sizeof(payload), frame, sizeof(frame), &written) ==
           WIREMUX_STATUS_OK);
    assert(written == sizeof(frame));

    wiremux_frame_view_t decoded = {0};
    assert(wiremux_frame_decode(frame, written, 512, &decoded) == WIREMUX_STATUS_OK);
    assert(decoded.header.version == WIREMUX_FRAME_VERSION);
    assert(decoded.header.flags == 0x05);
    assert(decoded.payload_len == sizeof(payload));
    assert(decoded.frame_len == sizeof(frame));
    assert(memcmp(decoded.payload, payload, decoded.payload_len) == 0);

    frame[WIREMUX_FRAME_HEADER_LEN] ^= 0xffu;
    assert(wiremux_frame_decode(frame, written, 512, &decoded) == WIREMUX_STATUS_CRC_MISMATCH);
    assert(decoded.frame_len == sizeof(frame));
}

int main(void)
{
    test_crc32();
    test_envelope_round_trip();
    test_manifest_encode();
    test_frame_decode();
    return 0;
}
