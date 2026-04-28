#include <algorithm>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include <gmock/gmock.h>
#include <gtest/gtest.h>

extern "C" {
#include "wiremux_batch.h"
#include "wiremux_compression.h"
#include "wiremux_envelope.h"
#include "wiremux_frame.h"
#include "wiremux_host_session.h"
#include "wiremux_manifest.h"
#include "wiremux_version.h"
}

namespace {

using ::testing::ElementsAre;
using ::testing::MockFunction;
using ::testing::AtLeast;

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

std::vector<uint8_t> EncodeEnvelopeFrame(const wiremux_envelope_t &envelope)
{
    std::vector<uint8_t> payload(wiremux_envelope_encoded_len(&envelope));
    size_t written = 0;
    EXPECT_EQ(wiremux_envelope_encode(&envelope, payload.data(), payload.size(), &written),
              WIREMUX_STATUS_OK);
    payload.resize(written);
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        0,
    };
    return EncodeFrame(header, payload);
}

std::vector<uint8_t> EncodeManifestBytes(wiremux_device_manifest_t manifest)
{
    std::vector<uint8_t> bytes(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, bytes.data(), bytes.size(), &written),
              WIREMUX_STATUS_OK);
    bytes.resize(written);
    return bytes;
}

std::string ReadFile(const char *path)
{
    std::ifstream in(path, std::ios::binary);
    std::ostringstream buffer;
    buffer << in.rdbuf();
    return buffer.str();
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
    static const uint32_t console_modes[] = {
        WIREMUX_CHANNEL_INTERACTION_LINE,
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
            nullptr,
            0,
            WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
            {},
        },
        {
            1,
            "console",
            "line console",
            WIREMUX_DIRECTION_INPUT | WIREMUX_DIRECTION_OUTPUT,
            nullptr,
            0,
            nullptr,
            0,
            WIREMUX_PAYLOAD_KIND_TEXT,
            0,
            console_modes,
            sizeof(console_modes) / sizeof(console_modes[0]),
            WIREMUX_CHANNEL_INTERACTION_LINE,
            {},
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
            nullptr,
            0,
            WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
            {},
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

struct CapturedRecord {
    uint32_t channel_id = 0;
    std::string payload_type;
    std::vector<uint8_t> payload;
};

struct CapturedManifest {
    std::string device_name;
    uint32_t protocol_version = 0;
    uint32_t max_payload_len = 0;
    std::vector<std::string> channel_names;
    std::vector<wiremux_passthrough_policy_t> passthrough_policies;
};

struct SessionCapture {
    std::vector<wiremux_host_event_type_t> event_types;
    std::vector<uint8_t> terminal;
    std::vector<CapturedRecord> records;
    std::vector<wiremux_host_crc_error_t> crc_errors;
    std::vector<wiremux_host_decode_error_t> decode_errors;
    std::vector<wiremux_host_protocol_compatibility_event_t> compatibility;
    std::vector<wiremux_host_batch_summary_t> batch_summaries;
    CapturedManifest manifest;
    MockFunction<void(int)> *mock = nullptr;
};

std::string ViewToString(wiremux_string_view_t view)
{
    if (view.data == nullptr || view.len == 0) {
        return {};
    }
    return std::string(view.data, view.len);
}

void CaptureEvent(const wiremux_host_event_t *event, void *user_ctx)
{
    auto *capture = static_cast<SessionCapture *>(user_ctx);
    capture->event_types.push_back(event->type);
    if (capture->mock != nullptr) {
        capture->mock->Call(static_cast<int>(event->type));
    }

    switch (event->type) {
    case WIREMUX_HOST_EVENT_TERMINAL:
        capture->terminal.insert(capture->terminal.end(),
                                 event->data.terminal.data,
                                 event->data.terminal.data + event->data.terminal.len);
        break;
    case WIREMUX_HOST_EVENT_RECORD: {
        CapturedRecord record;
        record.channel_id = event->data.record.channel_id;
        record.payload.assign(event->data.record.payload,
                              event->data.record.payload + event->data.record.payload_len);
        if (event->data.record.payload_type != nullptr) {
            record.payload_type.assign(event->data.record.payload_type,
                                       event->data.record.payload_type_len);
        }
        capture->records.push_back(record);
        break;
    }
    case WIREMUX_HOST_EVENT_CRC_ERROR:
        capture->crc_errors.push_back(event->data.crc_error);
        break;
    case WIREMUX_HOST_EVENT_DECODE_ERROR:
        capture->decode_errors.push_back(event->data.decode_error);
        break;
    case WIREMUX_HOST_EVENT_MANIFEST_BEGIN:
        capture->manifest.device_name = ViewToString(event->data.manifest_begin.device_name);
        capture->manifest.protocol_version = event->data.manifest_begin.protocol_version;
        capture->manifest.max_payload_len = event->data.manifest_begin.max_payload_len;
        break;
    case WIREMUX_HOST_EVENT_MANIFEST_CHANNEL_BEGIN:
        capture->manifest.channel_names.push_back(ViewToString(event->data.manifest_channel.name));
        capture->manifest.passthrough_policies.push_back(event->data.manifest_channel.passthrough_policy);
        break;
    case WIREMUX_HOST_EVENT_PROTOCOL_COMPATIBILITY:
        capture->compatibility.push_back(event->data.protocol_compatibility);
        break;
    case WIREMUX_HOST_EVENT_BATCH_SUMMARY:
        capture->batch_summaries.push_back(event->data.batch_summary);
        break;
    default:
        break;
    }
}

wiremux_host_session_t InitSession(SessionCapture *capture,
                                   std::vector<uint8_t> *buffer,
                                   std::vector<uint8_t> *scratch,
                                   size_t max_payload_len = 512)
{
    wiremux_host_session_t session = {};
    const wiremux_host_session_config_t config = {
        max_payload_len,
        buffer->data(),
        buffer->size(),
        scratch->data(),
        scratch->size(),
        CaptureEvent,
        capture,
    };
    EXPECT_EQ(wiremux_host_session_init(&session, &config), WIREMUX_STATUS_OK);
    return session;
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

TEST(WiremuxBatchTest, EncodesAndDecodesRecords)
{
    const uint8_t payload1[] = "log line one";
    const uint8_t payload2[] = {0x01, 0x02, 0x03, 0x04};
    const wiremux_record_t records[] = {
        {
            2,
            WIREMUX_DIRECTION_OUTPUT,
            10,
            1000,
            WIREMUX_PAYLOAD_KIND_TEXT,
            nullptr,
            0,
            payload1,
            sizeof(payload1) - 1,
            0,
        },
        {
            3,
            WIREMUX_DIRECTION_OUTPUT,
            11,
            1100,
            WIREMUX_PAYLOAD_KIND_BINARY,
            "wiremux.test.Binary",
            std::strlen("wiremux.test.Binary"),
            payload2,
            sizeof(payload2),
            7,
        },
    };

    std::vector<uint8_t> encoded(wiremux_batch_records_encoded_len(records, 2));
    size_t written = 0;
    ASSERT_EQ(wiremux_batch_records_encode(records, 2, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, encoded.size());

    wiremux_record_t decoded[2] = {};
    size_t decoded_count = 0;
    ASSERT_EQ(wiremux_batch_records_decode(encoded.data(),
                                           encoded.size(),
                                           decoded,
                                           2,
                                           &decoded_count),
              WIREMUX_STATUS_OK);
    ASSERT_EQ(decoded_count, 2u);
    EXPECT_EQ(decoded[0].channel_id, 2u);
    EXPECT_EQ(decoded[0].sequence, 10u);
    EXPECT_EQ(decoded[0].payload_len, sizeof(payload1) - 1);
    EXPECT_EQ(std::memcmp(decoded[0].payload, payload1, decoded[0].payload_len), 0);
    EXPECT_EQ(decoded[1].channel_id, 3u);
    EXPECT_EQ(decoded[1].payload_type_len, std::strlen("wiremux.test.Binary"));
    EXPECT_EQ(std::memcmp(decoded[1].payload_type, "wiremux.test.Binary", decoded[1].payload_type_len), 0);
    EXPECT_EQ(std::memcmp(decoded[1].payload, payload2, decoded[1].payload_len), 0);
}

TEST(WiremuxBatchTest, EncodesAndDecodesBatchMetadata)
{
    const uint8_t records[] = {0x0a, 0x03, 0x08, 0x01, 0x10};
    const wiremux_batch_t batch = {
        WIREMUX_COMPRESSION_HEATSHRINK,
        records,
        sizeof(records),
        128,
    };
    std::vector<uint8_t> encoded(wiremux_batch_encoded_len(&batch));
    size_t written = 0;

    ASSERT_EQ(wiremux_batch_encode(&batch, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);

    wiremux_batch_t decoded = {};
    ASSERT_EQ(wiremux_batch_decode(encoded.data(), written, &decoded), WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded.compression, WIREMUX_COMPRESSION_HEATSHRINK);
    EXPECT_EQ(decoded.records_len, sizeof(records));
    EXPECT_EQ(decoded.uncompressed_len, 128u);
    EXPECT_EQ(std::memcmp(decoded.records, records, decoded.records_len), 0);
}

TEST(WiremuxBatchTest, RejectsInvalidArgumentsAndCapacity)
{
    const uint8_t records[] = {0x01, 0x02};
    const wiremux_batch_t batch = {
        WIREMUX_COMPRESSION_NONE,
        records,
        sizeof(records),
        sizeof(records),
    };
    uint8_t out[8] = {};
    size_t written = 0;
    size_t record_count = 0;

    EXPECT_EQ(wiremux_batch_encode(nullptr, out, sizeof(out), &written), WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_batch_encode(&batch, nullptr, sizeof(out), &written), WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_batch_encode(&batch, out, sizeof(out), nullptr), WIREMUX_STATUS_INVALID_ARG);
    EXPECT_EQ(wiremux_batch_encode(&batch, out, 1, &written), WIREMUX_STATUS_INVALID_SIZE);
    EXPECT_EQ(wiremux_batch_records_decode(records, sizeof(records), nullptr, 1, &record_count),
              WIREMUX_STATUS_INVALID_ARG);
}

TEST(WiremuxCompressionTest, HeatshrinkRoundTripsRepeatedPayload)
{
    const uint8_t input[] = "ESP_LOGI demo demo demo demo demo telemetry telemetry telemetry";
    std::vector<uint8_t> compressed(sizeof(input) * 2);
    std::vector<uint8_t> decoded(sizeof(input));
    size_t compressed_len = 0;
    size_t decoded_len = 0;

    ASSERT_EQ(wiremux_compress(WIREMUX_COMPRESSION_HEATSHRINK,
                               input,
                               sizeof(input) - 1,
                               compressed.data(),
                               compressed.size(),
                               &compressed_len),
              WIREMUX_STATUS_OK);
    ASSERT_EQ(wiremux_decompress(WIREMUX_COMPRESSION_HEATSHRINK,
                                 compressed.data(),
                                 compressed_len,
                                 decoded.data(),
                                 decoded.size(),
                                 &decoded_len),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded_len, sizeof(input) - 1);
    EXPECT_EQ(std::memcmp(decoded.data(), input, decoded_len), 0);
}

TEST(WiremuxCompressionTest, Lz4RoundTripsRepeatedPayload)
{
    const uint8_t input[] = "channel=2 level=info value=42 channel=2 level=info value=43 channel=2 level=info value=44";
    std::vector<uint8_t> compressed(sizeof(input) * 2);
    std::vector<uint8_t> decoded(sizeof(input));
    size_t compressed_len = 0;
    size_t decoded_len = 0;

    ASSERT_EQ(wiremux_compress(WIREMUX_COMPRESSION_LZ4,
                               input,
                               sizeof(input) - 1,
                               compressed.data(),
                               compressed.size(),
                               &compressed_len),
              WIREMUX_STATUS_OK);
    ASSERT_EQ(wiremux_decompress(WIREMUX_COMPRESSION_LZ4,
                                 compressed.data(),
                                 compressed_len,
                                 decoded.data(),
                                 decoded.size(),
                                 &decoded_len),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(decoded_len, sizeof(input) - 1);
    EXPECT_EQ(std::memcmp(decoded.data(), input, decoded_len), 0);
}

TEST(WiremuxCompressionTest, RejectsUnsupportedAlgorithmAndSmallOutput)
{
    const uint8_t input[] = "abcabcabc";
    uint8_t out[4] = {};
    size_t written = 0;

    EXPECT_EQ(wiremux_compress(99, input, sizeof(input) - 1, out, sizeof(out), &written),
              WIREMUX_STATUS_NOT_SUPPORTED);
    EXPECT_EQ(wiremux_decompress(99, input, sizeof(input) - 1, out, sizeof(out), &written),
              WIREMUX_STATUS_NOT_SUPPORTED);
    EXPECT_EQ(wiremux_decompress(WIREMUX_COMPRESSION_NONE,
                                 input,
                                 sizeof(input) - 1,
                                 out,
                                 sizeof(out),
                                 &written),
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

TEST(WiremuxManifestTest, EncodesChannelInteractionMode)
{
    static const uint32_t modes[] = {
        WIREMUX_CHANNEL_INTERACTION_LINE,
        WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH,
    };
    const wiremux_channel_descriptor_t channel = {
        1,
        "console",
        nullptr,
        WIREMUX_DIRECTION_INPUT | WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        modes,
        sizeof(modes) / sizeof(modes[0]),
        WIREMUX_CHANNEL_INTERACTION_LINE,
        {},
    };
    const wiremux_device_manifest_t manifest = {
        nullptr,
        nullptr,
        WIREMUX_FRAME_VERSION,
        8,
        &channel,
        1,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        nullptr,
        WIREMUX_FEATURE_MANIFEST_PROTOBUF | WIREMUX_FEATURE_MANIFEST_REQUEST,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
    const size_t len = wiremux_device_manifest_encoded_len(&manifest);
    std::vector<uint8_t> encoded(len);
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, len);

    const std::vector<uint8_t> expected_channel = {
        0x08, 0x01,
        0x12, 0x07, 'c', 'o', 'n', 's', 'o', 'l', 'e',
        0x20, 0x01,
        0x20, 0x02,
        0x28, 0x01,
        0x40, 0x01,
        0x48, 0x01,
        0x48, 0x02,
        0x50, 0x01,
    };
    auto it = std::search(encoded.begin(), encoded.end(), expected_channel.begin(), expected_channel.end());
    EXPECT_NE(it, encoded.end());
}

TEST(WiremuxManifestTest, EncodesPassthroughPolicy)
{
    const wiremux_channel_descriptor_t channel = {
        1,
        "console",
        nullptr,
        WIREMUX_DIRECTION_INPUT | WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH,
        {
            WIREMUX_NEWLINE_POLICY_CR,
            WIREMUX_NEWLINE_POLICY_PRESERVE,
            WIREMUX_ECHO_POLICY_REMOTE,
            WIREMUX_CONTROL_KEY_POLICY_FORWARDED,
        },
    };
    const wiremux_device_manifest_t manifest = {
        nullptr,
        nullptr,
        WIREMUX_PROTOCOL_API_VERSION_CURRENT,
        8,
        &channel,
        1,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        nullptr,
        WIREMUX_FEATURE_MANIFEST_PROTOBUF | WIREMUX_FEATURE_MANIFEST_REQUEST,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
    const size_t len = wiremux_device_manifest_encoded_len(&manifest);
    std::vector<uint8_t> encoded(len);
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);
    EXPECT_EQ(written, len);

    const std::vector<uint8_t> expected_policy = {
        0x5a, 0x08,
        0x08, 0x03,
        0x10, 0x01,
        0x18, 0x01,
        0x20, 0x02,
    };
    auto it = std::search(encoded.begin(), encoded.end(), expected_policy.begin(), expected_policy.end());
    EXPECT_NE(it, encoded.end());
}

TEST(WiremuxManifestTest, ClampsChannelNameToFifteenAsciiBytes)
{
    const wiremux_channel_descriptor_t channel = {
        1,
        "0123456789abcdef",
        nullptr,
        WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
        {},
    };
    const wiremux_device_manifest_t manifest = {
        nullptr,
        nullptr,
        WIREMUX_FRAME_VERSION,
        8,
        &channel,
        1,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        nullptr,
        WIREMUX_FEATURE_MANIFEST_PROTOBUF,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
    std::vector<uint8_t> encoded(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);

    const std::vector<uint8_t> expected_name = {
        0x12, 0x0f, '0', '1', '2', '3', '4', '5', '6', '7',
        '8', '9', 'a', 'b', 'c', 'd', 'e',
    };
    auto it = std::search(encoded.begin(), encoded.end(), expected_name.begin(), expected_name.end());
    EXPECT_NE(it, encoded.end());
    EXPECT_EQ(std::find(encoded.begin(), encoded.end(), 'f'), encoded.end());
}

TEST(WiremuxManifestTest, ClampsChannelNameAtUtf8Boundary)
{
    const wiremux_channel_descriptor_t channel = {
        4,
        "🚗🎒😄🔥",
        nullptr,
        WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
        {},
    };
    const wiremux_device_manifest_t manifest = {
        nullptr,
        nullptr,
        WIREMUX_FRAME_VERSION,
        8,
        &channel,
        1,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        nullptr,
        WIREMUX_FEATURE_MANIFEST_PROTOBUF,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
    std::vector<uint8_t> encoded(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);

    const std::vector<uint8_t> expected_name = {
        0x12, 0x0c,
        0xf0, 0x9f, 0x9a, 0x97,
        0xf0, 0x9f, 0x8e, 0x92,
        0xf0, 0x9f, 0x98, 0x84,
    };
    auto it = std::search(encoded.begin(), encoded.end(), expected_name.begin(), expected_name.end());
    EXPECT_NE(it, encoded.end());
}

TEST(WiremuxManifestTest, OmitsInvalidUtf8ChannelName)
{
    const char invalid_name[] = {
        (char)0xf0,
        (char)0x9f,
        (char)0x9a,
        '\0',
    };
    const wiremux_channel_descriptor_t channel = {
        1,
        invalid_name,
        nullptr,
        WIREMUX_DIRECTION_OUTPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
        {},
    };
    const wiremux_device_manifest_t manifest = {
        nullptr,
        nullptr,
        WIREMUX_FRAME_VERSION,
        8,
        &channel,
        1,
        WIREMUX_ENDIANNESS_LITTLE,
        128,
        nullptr,
        WIREMUX_FEATURE_MANIFEST_PROTOBUF,
        WIREMUX_SDK_NAME_ESP,
        "0.1.0",
    };
    std::vector<uint8_t> encoded(wiremux_device_manifest_encoded_len(&manifest));
    size_t written = 0;

    ASSERT_EQ(wiremux_device_manifest_encode(&manifest, encoded.data(), encoded.size(), &written),
              WIREMUX_STATUS_OK);

    const std::vector<uint8_t> invalid_field = {
        0x12, 0x03, 0xf0, 0x9f, 0x9a,
    };
    auto it = std::search(encoded.begin(), encoded.end(), invalid_field.begin(), invalid_field.end());
    EXPECT_EQ(it, encoded.end());
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
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
        {},
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
        nullptr,
        0,
        WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED,
        {},
    };
    manifest.channels = &invalid_payload_types;
    EXPECT_EQ(wiremux_device_manifest_encoded_len(&manifest), 0u);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);

    const wiremux_channel_descriptor_t invalid_interaction_modes = {
        1,
        "invalid",
        nullptr,
        WIREMUX_DIRECTION_INPUT,
        nullptr,
        0,
        nullptr,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        0,
        nullptr,
        1,
        WIREMUX_CHANNEL_INTERACTION_LINE,
        {},
    };
    manifest.channels = &invalid_interaction_modes;
    EXPECT_EQ(wiremux_device_manifest_encoded_len(&manifest), 0u);
    EXPECT_EQ(wiremux_device_manifest_encode(&manifest, out, sizeof(out), &written),
              WIREMUX_STATUS_INVALID_ARG);
}

TEST(WiremuxVersionTest, ClassifiesCompileTimeSupportedApiRange)
{
    EXPECT_EQ(WIREMUX_PROTOCOL_API_VERSION_CURRENT, 2u);
    EXPECT_EQ(WIREMUX_PROTOCOL_API_VERSION_MIN_SUPPORTED, 1u);
    EXPECT_EQ(wiremux_protocol_api_compatibility(WIREMUX_PROTOCOL_API_VERSION_CURRENT),
              WIREMUX_PROTOCOL_COMPAT_SUPPORTED);
    EXPECT_EQ(wiremux_protocol_api_compatibility(0), WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_OLD);
    EXPECT_EQ(wiremux_protocol_api_compatibility(WIREMUX_PROTOCOL_API_VERSION_CURRENT + 1),
              WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_NEW);
}

TEST(WiremuxVersionTest, CurrentAndFrozenApiSnapshotsMatchCanonicalProto)
{
    const std::string root = WIREMUX_CORE_SOURCE_DIR;
    const std::string canonical = ReadFile((root + "/../../api/proto/wiremux.proto").c_str());
    ASSERT_FALSE(canonical.empty());
    EXPECT_EQ(ReadFile((root + "/../../api/proto/api/current/wiremux.proto").c_str()), canonical);
    EXPECT_NE(ReadFile((root + "/../../api/proto/api/1/wiremux.proto").c_str()), canonical);
    EXPECT_EQ(ReadFile((root + "/../../api/proto/api/2/wiremux.proto").c_str()), canonical);
}

TEST(WiremuxHostSessionTest, EmitsTerminalAndRecordEventsInOrder)
{
    const uint8_t payload[] = "hello";
    const wiremux_envelope_t envelope = {
        2,
        WIREMUX_DIRECTION_OUTPUT,
        1,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        nullptr,
        0,
        payload,
        sizeof(payload) - 1,
        0,
    };
    const std::vector<uint8_t> frame = EncodeEnvelopeFrame(envelope);
    std::vector<uint8_t> input = {'o', 'k', '\n'};
    input.insert(input.end(), frame.begin(), frame.end());

    SessionCapture capture;
    MockFunction<void(int)> mock;
    capture.mock = &mock;
    EXPECT_CALL(mock, Call(WIREMUX_HOST_EVENT_TERMINAL)).Times(AtLeast(1));
    EXPECT_CALL(mock, Call(WIREMUX_HOST_EVENT_RECORD));
    std::vector<uint8_t> buffer(1024);
    std::vector<uint8_t> scratch(1024);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch);

    ASSERT_EQ(wiremux_host_session_feed(&session, input.data(), input.size()), WIREMUX_STATUS_OK);
    ASSERT_EQ(wiremux_host_session_finish(&session), WIREMUX_STATUS_OK);

    EXPECT_THAT(capture.terminal, ElementsAre('o', 'k', '\n'));
    ASSERT_EQ(capture.records.size(), 1u);
    EXPECT_EQ(capture.records[0].channel_id, 2u);
    EXPECT_EQ(capture.records[0].payload, std::vector<uint8_t>({'h', 'e', 'l', 'l', 'o'}));
}

TEST(WiremuxHostSessionTest, EmitsCrcErrorsWithoutFatalStreamFailure)
{
    const wiremux_frame_header_t header = {
        WIREMUX_FRAME_VERSION,
        3,
    };
    std::vector<uint8_t> frame = EncodeFrame(header, {0x08, 0x01});
    frame[WIREMUX_FRAME_HEADER_LEN] ^= 0xffu;

    SessionCapture capture;
    std::vector<uint8_t> buffer(1024);
    std::vector<uint8_t> scratch(1024);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch);

    ASSERT_EQ(wiremux_host_session_feed(&session, frame.data(), frame.size()), WIREMUX_STATUS_OK);
    ASSERT_EQ(capture.crc_errors.size(), 1u);
    EXPECT_EQ(capture.crc_errors[0].flags, 3u);
    EXPECT_EQ(capture.crc_errors[0].payload_len, 2u);
}

TEST(WiremuxHostSessionTest, ParsesManifestAndReportsSupportedCompatibility)
{
    wiremux_device_manifest_t manifest = SampleManifest();
    wiremux_channel_descriptor_t channels[3];
    std::memcpy(channels, manifest.channels, sizeof(channels));
    channels[1].default_interaction_mode = WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH;
    channels[1].passthrough_policy = {
        WIREMUX_NEWLINE_POLICY_CR,
        WIREMUX_NEWLINE_POLICY_PRESERVE,
        WIREMUX_ECHO_POLICY_REMOTE,
        WIREMUX_CONTROL_KEY_POLICY_FORWARDED,
    };
    manifest.channels = channels;
    manifest.protocol_version = WIREMUX_PROTOCOL_API_VERSION_CURRENT;
    const std::vector<uint8_t> manifest_bytes = EncodeManifestBytes(manifest);
    const wiremux_envelope_t envelope = {
        0,
        WIREMUX_DIRECTION_OUTPUT,
        1,
        0,
        WIREMUX_PAYLOAD_KIND_CONTROL,
        WIREMUX_MANIFEST_PAYLOAD_TYPE,
        std::strlen(WIREMUX_MANIFEST_PAYLOAD_TYPE),
        manifest_bytes.data(),
        manifest_bytes.size(),
        0,
    };
    const std::vector<uint8_t> frame = EncodeEnvelopeFrame(envelope);

    SessionCapture capture;
    std::vector<uint8_t> buffer(2048);
    std::vector<uint8_t> scratch(2048);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch, 2048);

    ASSERT_EQ(wiremux_host_session_feed(&session, frame.data(), frame.size()), WIREMUX_STATUS_OK);
    EXPECT_EQ(capture.manifest.device_name, "test-device");
    EXPECT_EQ(capture.manifest.protocol_version, WIREMUX_PROTOCOL_API_VERSION_CURRENT);
    EXPECT_EQ(capture.manifest.max_payload_len, 512u);
    EXPECT_THAT(capture.manifest.channel_names, ElementsAre("system", "console", "telemetry"));
    ASSERT_EQ(capture.manifest.passthrough_policies.size(), 3u);
    EXPECT_EQ(capture.manifest.passthrough_policies[1].input_newline_policy, WIREMUX_NEWLINE_POLICY_CR);
    EXPECT_EQ(capture.manifest.passthrough_policies[1].output_newline_policy, WIREMUX_NEWLINE_POLICY_PRESERVE);
    EXPECT_EQ(capture.manifest.passthrough_policies[1].echo_policy, WIREMUX_ECHO_POLICY_REMOTE);
    EXPECT_EQ(capture.manifest.passthrough_policies[1].control_key_policy, WIREMUX_CONTROL_KEY_POLICY_FORWARDED);
    ASSERT_EQ(capture.compatibility.size(), 1u);
    EXPECT_EQ(capture.compatibility[0].compatibility, WIREMUX_PROTOCOL_COMPAT_SUPPORTED);
}

TEST(WiremuxHostSessionTest, RejectsNewerDeviceApiWithUpgradeHostDiagnosticState)
{
    wiremux_device_manifest_t manifest = SampleManifest();
    manifest.protocol_version = WIREMUX_PROTOCOL_API_VERSION_CURRENT + 1;
    const std::vector<uint8_t> manifest_bytes = EncodeManifestBytes(manifest);
    const wiremux_envelope_t envelope = {
        0,
        WIREMUX_DIRECTION_OUTPUT,
        1,
        0,
        WIREMUX_PAYLOAD_KIND_CONTROL,
        WIREMUX_MANIFEST_PAYLOAD_TYPE,
        std::strlen(WIREMUX_MANIFEST_PAYLOAD_TYPE),
        manifest_bytes.data(),
        manifest_bytes.size(),
        0,
    };
    const std::vector<uint8_t> frame = EncodeEnvelopeFrame(envelope);

    SessionCapture capture;
    std::vector<uint8_t> buffer(2048);
    std::vector<uint8_t> scratch(2048);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch, 2048);

    ASSERT_EQ(wiremux_host_session_feed(&session, frame.data(), frame.size()), WIREMUX_STATUS_OK);
    ASSERT_EQ(capture.compatibility.size(), 1u);
    EXPECT_EQ(capture.compatibility[0].device_api_version, WIREMUX_PROTOCOL_API_VERSION_CURRENT + 1);
    EXPECT_EQ(capture.compatibility[0].compatibility, WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_NEW);
}

TEST(WiremuxHostSessionTest, ExpandsCompressedBatchesUsingCallerScratch)
{
    const uint8_t payload[] = "batch record";
    const wiremux_record_t record = {
        3,
        WIREMUX_DIRECTION_OUTPUT,
        7,
        0,
        WIREMUX_PAYLOAD_KIND_TEXT,
        nullptr,
        0,
        payload,
        sizeof(payload) - 1,
        0,
    };
    std::vector<uint8_t> records(wiremux_batch_records_encoded_len(&record, 1));
    size_t records_len = 0;
    ASSERT_EQ(wiremux_batch_records_encode(&record, 1, records.data(), records.size(), &records_len),
              WIREMUX_STATUS_OK);
    records.resize(records_len);

    std::vector<uint8_t> compressed(records.size() * 2);
    size_t compressed_len = 0;
    ASSERT_EQ(wiremux_compress(WIREMUX_COMPRESSION_HEATSHRINK,
                               records.data(),
                               records.size(),
                               compressed.data(),
                               compressed.size(),
                               &compressed_len),
              WIREMUX_STATUS_OK);
    compressed.resize(compressed_len);

    const wiremux_batch_t batch = {
        WIREMUX_COMPRESSION_HEATSHRINK,
        compressed.data(),
        compressed.size(),
        static_cast<uint32_t>(records.size()),
    };
    std::vector<uint8_t> batch_bytes(wiremux_batch_encoded_len(&batch));
    size_t batch_len = 0;
    ASSERT_EQ(wiremux_batch_encode(&batch, batch_bytes.data(), batch_bytes.size(), &batch_len),
              WIREMUX_STATUS_OK);
    batch_bytes.resize(batch_len);

    const wiremux_envelope_t envelope = {
        0,
        WIREMUX_DIRECTION_OUTPUT,
        1,
        0,
        WIREMUX_PAYLOAD_KIND_BATCH,
        WIREMUX_BATCH_PAYLOAD_TYPE,
        std::strlen(WIREMUX_BATCH_PAYLOAD_TYPE),
        batch_bytes.data(),
        batch_bytes.size(),
        0,
    };
    const std::vector<uint8_t> frame = EncodeEnvelopeFrame(envelope);

    SessionCapture capture;
    std::vector<uint8_t> buffer(2048);
    std::vector<uint8_t> scratch(2048);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch, 2048);

    ASSERT_EQ(wiremux_host_session_feed(&session, frame.data(), frame.size()), WIREMUX_STATUS_OK);
    ASSERT_EQ(capture.records.size(), 1u);
    EXPECT_EQ(capture.records[0].channel_id, 3u);
    EXPECT_EQ(capture.records[0].payload, std::vector<uint8_t>({'b', 'a', 't', 'c', 'h', ' ', 'r', 'e', 'c', 'o', 'r', 'd'}));
    ASSERT_EQ(capture.batch_summaries.size(), 1u);
    EXPECT_EQ(capture.batch_summaries[0].compression, WIREMUX_COMPRESSION_HEATSHRINK);
    EXPECT_EQ(capture.batch_summaries[0].record_count, 1u);
    EXPECT_EQ(capture.batch_summaries[0].raw_bytes, records.size());
}

TEST(WiremuxHostSessionTest, ReportsScratchExhaustionDeterministically)
{
    const uint8_t fake_compressed[] = {0x00};
    const wiremux_batch_t batch = {
        WIREMUX_COMPRESSION_HEATSHRINK,
        fake_compressed,
        sizeof(fake_compressed),
        512,
    };
    std::vector<uint8_t> batch_bytes(wiremux_batch_encoded_len(&batch));
    size_t batch_len = 0;
    ASSERT_EQ(wiremux_batch_encode(&batch, batch_bytes.data(), batch_bytes.size(), &batch_len),
              WIREMUX_STATUS_OK);
    batch_bytes.resize(batch_len);
    const wiremux_envelope_t envelope = {
        0,
        WIREMUX_DIRECTION_OUTPUT,
        1,
        0,
        WIREMUX_PAYLOAD_KIND_BATCH,
        WIREMUX_BATCH_PAYLOAD_TYPE,
        std::strlen(WIREMUX_BATCH_PAYLOAD_TYPE),
        batch_bytes.data(),
        batch_bytes.size(),
        0,
    };
    const std::vector<uint8_t> frame = EncodeEnvelopeFrame(envelope);

    SessionCapture capture;
    std::vector<uint8_t> buffer(1024);
    std::vector<uint8_t> scratch(8);
    wiremux_host_session_t session = InitSession(&capture, &buffer, &scratch, 1024);

    ASSERT_EQ(wiremux_host_session_feed(&session, frame.data(), frame.size()), WIREMUX_STATUS_OK);
    ASSERT_EQ(capture.decode_errors.size(), 1u);
    EXPECT_EQ(capture.decode_errors[0].stage, WIREMUX_HOST_DECODE_BATCH);
    EXPECT_EQ(capture.decode_errors[0].status, WIREMUX_STATUS_INVALID_SIZE);
}
