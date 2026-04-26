#include <cstdint>
#include <cstring>
#include <vector>

#include <gmock/gmock.h>
#include <gtest/gtest.h>

extern "C" {
#include "wiremux_envelope.h"
#include "wiremux_frame.h"
#include "wiremux_manifest.h"
}

namespace {

using ::testing::ElementsAre;

std::vector<uint8_t> EncodeFrame(const wiremux_frame_header_t &header,
                                 const std::vector<uint8_t> &payload)
{
    std::vector<uint8_t> frame(wiremux_frame_encoded_len(payload.size()));
    size_t written = 0;
    EXPECT_EQ(wiremux_frame_encode(&header,
                                   payload.empty() ? nullptr : payload.data(),
                                   payload.size(),
                                   frame.data(),
                                   frame.size(),
                                   &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, frame.size());
    return frame;
}

wiremux_envelope_t SampleEnvelope()
{
    static const char payload_type[] = "wiremux.test.Payload";
    static const uint8_t payload[] = {0x01, 0x02, 0x03};

    return {
        3,
        WIREMUX_DIRECTION_OUTPUT,
        7,
        12345,
        WIREMUX_PAYLOAD_KIND_PROTOBUF,
        payload_type,
        std::strlen(payload_type),
        payload,
        sizeof(payload),
        9,
    };
}

wiremux_device_manifest_t SampleManifest()
{
    static const uint32_t telemetry_kinds[] = {
        WIREMUX_PAYLOAD_KIND_TEXT,
        WIREMUX_PAYLOAD_KIND_PROTOBUF,
    };
    static const char *const telemetry_types[] = {
        "wiremux.test.Telemetry",
    };
    static const wiremux_channel_descriptor_t channels[] = {
        {
            0,
            "system",
            "system control",
            WIREMUX_DIRECTION_OUTPUT,
            nullptr,
            0,
            nullptr,
            0,
            WIREMUX_PAYLOAD_KIND_CONTROL,
            0,
        },
        {
            3,
            "telemetry",
            "sample telemetry",
            WIREMUX_DIRECTION_OUTPUT,
            telemetry_kinds,
            sizeof(telemetry_kinds) / sizeof(telemetry_kinds[0]),
            telemetry_types,
            sizeof(telemetry_types) / sizeof(telemetry_types[0]),
            WIREMUX_PAYLOAD_KIND_TEXT,
            0,
        },
    };
    return {
        "test-device",
        "0.1.0",
        WIREMUX_FRAME_VERSION,
        8,
        channels,
        sizeof(channels) / sizeof(channels[0]),
        WIREMUX_ENDIANNESS_LITTLE,
        512,
        "loopback",
        WIREMUX_FEATURE_MANIFEST_PROTOBUF,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
}

}  // namespace

TEST(WiremuxCrc32Test, MatchesKnownIeeeVector)
{
    EXPECT_EQ(wiremux_crc32(reinterpret_cast<const uint8_t *>("123456789"), 9), 0xcbf43926u);
}

TEST(WiremuxFrameTest, EncodesAndDecodesFrame)
{
    const std::vector<uint8_t> payload = {0x08, 0x01, 0x10, 0x02};
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0x05,
    };

    const std::vector<uint8_t> frame = EncodeFrame(header, payload);

    wiremux_frame_view_t decoded = {};
    EXPECT_EQ(wiremux_frame_decode(frame.data(), frame.size(), 512, &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.header.version, WIREMUX_FRAME_VERSION);
    EXPECT_EQ(decoded.header.flags, 0x05);
    EXPECT_EQ(decoded.payload_len, payload.size());
    EXPECT_EQ(decoded.frame_len, frame.size());
    EXPECT_EQ(std::memcmp(decoded.payload, payload.data(), payload.size()), 0);
}

TEST(WiremuxFrameTest, EncodesEmptyPayload)
{
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };

    const std::vector<uint8_t> frame = EncodeFrame(header, {});

    wiremux_frame_view_t decoded = {};
    EXPECT_EQ(wiremux_frame_decode(frame.data(), frame.size(), 0, &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.payload_len, 0u);
    EXPECT_EQ(decoded.frame_len, static_cast<size_t>(WIREMUX_FRAME_HEADER_LEN));
}

TEST(WiremuxFrameTest, RejectsInvalidEncodeArguments)
{
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    const uint8_t payload[] = {0x01};
    uint8_t out[WIREMUX_FRAME_HEADER_LEN + sizeof(payload)] = {};
    size_t written = 0;

    EXPECT_EQ(wiremux_frame_encode(nullptr, payload, sizeof(payload), out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_frame_encode(&header, nullptr, sizeof(payload), out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_frame_encode(&header, payload, sizeof(payload), nullptr, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);
}

TEST(WiremuxFrameTest, RejectsSmallOutputBuffer)
{
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    const uint8_t payload[] = {0x01};
    uint8_t out[WIREMUX_FRAME_HEADER_LEN] = {};
    size_t written = 0;

    EXPECT_EQ(wiremux_frame_encode(&header, payload, sizeof(payload), out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_SIZE);
}

TEST(WiremuxFrameTest, DecodesDeterministicErrorBranches)
{
    const std::vector<uint8_t> payload = {0x08, 0x01};
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    std::vector<uint8_t> frame = EncodeFrame(header, payload);
    wiremux_frame_view_t decoded = {};

    EXPECT_EQ(wiremux_frame_decode(nullptr, frame.size(), 512, &decoded), WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_frame_decode(frame.data(), frame.size(), 512, nullptr), WIREMUX_STATUS_INVALID_ARG);

    EXPECT_EQ(wiremux_frame_decode(frame.data(), WIREMUX_FRAME_HEADER_LEN - 1, 512, &decoded),
              WIREMUX_STATUS_INCOMPLETE);

    std::vector<uint8_t> bad_magic = frame;
    bad_magic[0] = 'X';
    EXPECT_EQ(wiremux_frame_decode(bad_magic.data(), bad_magic.size(), 512, &decoded),
              WIREMUX_STATUS_BAD_MAGIC);

    std::vector<uint8_t> bad_version = frame;
    bad_version[4] = WIREMUX_FRAME_VERSION + 1;
    EXPECT_EQ(wiremux_frame_decode(bad_version.data(), bad_version.size(), 512, &decoded),
              WIREMUX_STATUS_BAD_VERSION);

    EXPECT_EQ(wiremux_frame_decode(frame.data(), frame.size(), payload.size() - 1, &decoded),
              WIREMUX_STATUS_INVALID_SIZE);

    EXPECT_EQ(wiremux_frame_decode(frame.data(), frame.size() - 1, 512, &decoded),
              WIREMUX_STATUS_INCOMPLETE);

    std::vector<uint8_t> bad_crc = frame;
    bad_crc[WIREMUX_FRAME_HEADER_LEN] ^= 0xffu;
    EXPECT_EQ(wiremux_frame_decode(bad_crc.data(), bad_crc.size(), 512, &decoded),
              WIREMUX_STATUS_CRC_MISMATCH);
    EXPECT_EQ(decoded.frame_len, frame.size());
}

TEST(WiremuxEnvelopeTest, EncodesAndDecodesRoundTrip)
{
    const wiremux_envelope_t envelope = SampleEnvelope();
    std::vector<uint8_t> encoded(wiremux_envelope_encoded_len(&envelope));
    size_t written = 0;

    ASSERT_EQ(wiremux_envelope_encode(&envelope, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, encoded.size());

    wiremux_envelope_t decoded = {};
    ASSERT_EQ(wiremux_envelope_decode(encoded.data(), written, &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.channel_id, envelope.channel_id);
    EXPECT_EQ(decoded.direction, envelope.direction);
    EXPECT_EQ(decoded.sequence, envelope.sequence);
    EXPECT_EQ(decoded.timestamp_us, envelope.timestamp_us);
    EXPECT_EQ(decoded.kind, envelope.kind);
    EXPECT_EQ(decoded.payload_type_len, envelope.payload_type_len);
    EXPECT_EQ(std::memcmp(decoded.payload_type, envelope.payload_type, decoded.payload_type_len), 0);
    EXPECT_EQ(decoded.payload_len, envelope.payload_len);
    EXPECT_EQ(std::memcmp(decoded.payload, envelope.payload, decoded.payload_len), 0);
    EXPECT_EQ(decoded.flags, envelope.flags);
}

TEST(WiremuxEnvelopeTest, EncodesZeroLengthOptionalFields)
{
    const wiremux_envelope_t envelope = {
        1,
        WIREMUX_DIRECTION_INPUT,
        2,
        3,
        WIREMUX_PAYLOAD_KIND_TEXT,
        nullptr,
        0,
        nullptr,
        0,
        0,
    };
    std::vector<uint8_t> encoded(wiremux_envelope_encoded_len(&envelope));
    size_t written = 0;

    ASSERT_EQ(wiremux_envelope_encode(&envelope, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);

    wiremux_envelope_t decoded = {};
    ASSERT_EQ(wiremux_envelope_decode(encoded.data(), written, &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.payload_type_len, 0u);
    EXPECT_EQ(decoded.payload_len, 0u);
}

TEST(WiremuxEnvelopeTest, RejectsInvalidEncodeArgumentsAndCapacity)
{
    const wiremux_envelope_t envelope = SampleEnvelope();
    std::vector<uint8_t> encoded(wiremux_envelope_encoded_len(&envelope));
    size_t written = 0;

    EXPECT_EQ(wiremux_envelope_encode(nullptr, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_envelope_encode(&envelope, nullptr, encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_envelope_encode(&envelope, encoded.data(), encoded.size(), nullptr),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_envelope_encode(&envelope, encoded.data(), encoded.size() - 1, &written),
              WIREMUX_STATUS_INVALID_SIZE);

    wiremux_envelope_t missing_payload = envelope;
    missing_payload.payload = nullptr;
    EXPECT_EQ(wiremux_envelope_encode(&missing_payload, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);

    wiremux_envelope_t missing_payload_type = envelope;
    missing_payload_type.payload_type = nullptr;
    EXPECT_EQ(wiremux_envelope_encode(&missing_payload_type, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);
}

TEST(WiremuxEnvelopeTest, IgnoresUnknownVarintFields)
{
    const uint8_t encoded[] = {
        0x98, 0x06, 0x7b,  // field 99, varint 123
        0x08, 0x03,        // channel_id = 3
    };

    wiremux_envelope_t decoded = {};
    ASSERT_EQ(wiremux_envelope_decode(encoded, sizeof(encoded), &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.channel_id, 3u);
}

TEST(WiremuxEnvelopeTest, RejectsInvalidDecodeInputsAndUnsupportedWireTypes)
{
    wiremux_envelope_t decoded = {};
    const uint8_t unsupported_wire_type[] = {
        0x0d,  // field 1, 32-bit wire type
    };

    EXPECT_EQ(wiremux_envelope_decode(nullptr, 0, &decoded), WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_envelope_decode(unsupported_wire_type, sizeof(unsupported_wire_type), nullptr),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_envelope_decode(unsupported_wire_type,
                                      sizeof(unsupported_wire_type),
                                      &decoded),
              WIREMUX_STATUS_NOT_SUPPORTED);
}

TEST(WiremuxEnvelopeTest, RejectsTruncatedVarintAndLengthDelimitedFields)
{
    wiremux_envelope_t decoded = {};
    const uint8_t truncated_varint[] = {
        0x80,
    };
    const uint8_t truncated_length_delimited[] = {
        0x3a,  // field 7, length-delimited
        0x05,  // length 5
        0xaa,
    };

    EXPECT_EQ(wiremux_envelope_decode(truncated_varint, sizeof(truncated_varint), &decoded),
              WIREMUX_STATUS_INVALID_SIZE);
    EXPECT_EQ(wiremux_envelope_decode(truncated_length_delimited,
                                      sizeof(truncated_length_delimited),
                                      &decoded),
              WIREMUX_STATUS_INVALID_SIZE);
}

TEST(WiremuxManifestTest, EncodesManifest)
{
    const wiremux_device_manifest_t manifest = SampleManifest();
    std::vector<uint8_t> encoded(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, encoded.size());
    EXPECT_GT(written, 0u);
}

TEST(WiremuxManifestTest, OmitsOptionalEmptyStrings)
{
    const wiremux_device_manifest_t manifest = {
        "",
        nullptr,
        WIREMUX_FRAME_VERSION,
        0,
        nullptr,
        0,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        "",
        0,
        nullptr,
        "",
    };
    const size_t len = wiremux_device_manifest_encoded_len(&manifest);
    std::vector<uint8_t> encoded(len);
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, len);
    EXPECT_THAT(encoded, ElementsAre(0x18, 0x01, 0x20, 0x00, 0x30, 0x01, 0x38, 0x80, 0x01, 0x48, 0x00));
}

TEST(WiremuxManifestTest, RejectsInvalidArgumentsAndCapacity)
{
    const wiremux_device_manifest_t manifest = SampleManifest();
    std::vector<uint8_t> encoded(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;

    EXPECT_EQ(wiremux_device_manifest_encode(nullptr, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, nullptr, encoded.size(), &written),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), nullptr),
              WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size() - 1, &written),
              WIREMUX_STATUS_INVALID_SIZE);
}

TEST(WiremuxManifestTest, RejectsInvalidChannelDescriptorPointerCounts)
{
    size_t written = 0;
    uint8_t out[64] = {};

    wiremux_device_manifest_t manifest = SampleManifest();
    manifest.channels = nullptr;
    manifest.channel_count = 1;
    EXPECT_EQ(wiremux_device_manifest_encoded_len(&manifest), 0u);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);

    const wiremux_channel_descriptor_t invalid_payload_kinds = {
        1,
        "invalid",
        nullptr,
        WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        1,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
    };
    manifest = SampleManifest();
    manifest.channels = &invalid_payload_kinds;
    manifest.channel_count = 1;
    EXPECT_EQ(wiremux_device_manifest_encoded_len(&manifest), 0u);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);

    const wiremux_channel_descriptor_t invalid_payload_types = {
        1,
        "invalid",
        nullptr,
        WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        1,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
    };
    manifest.channels = &invalid_payload_types;
    EXPECT_EQ(wiremux_device_manifest_encoded_len(&manifest), 0u);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);
}
