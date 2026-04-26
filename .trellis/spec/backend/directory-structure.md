# Directory Structure

> How backend code is organized in this project.

---

## Overview

This project is split into a host-side Rust tool and an ESP-IDF component.
Source code lives under `sources/`, not repository root `src/`.

The current framework is bidirectional: the host can decode ESP mux output and
send input frames, while the ESP component can parse inbound mux frames and
dispatch them to registered channel handlers.

## Directory Layout

```text
sources/
├── core/
│   ├── README.md
│   ├── proto/wiremux.proto
│   └── c/
│       ├── CMakeLists.txt
│       ├── include/
│       │   ├── wiremux_envelope.h
│       │   ├── wiremux_frame.h
│       │   ├── wiremux_manifest.h
│       │   ├── wiremux_batch.h
│       │   ├── wiremux_compression.h
│       │   └── wiremux_status.h
│       ├── src/
│           ├── wiremux_batch.c
│           ├── wiremux_compression.c
│           ├── wiremux_proto_internal.h
│           ├── wiremux_envelope.c
│           ├── wiremux_frame.c
│           └── wiremux_manifest.c
│       └── tests/
│           └── wiremux_core_test.cpp
├── host/
│   ├── Cargo.toml
│   └── src/
│       ├── crc32.rs
│       ├── frame.rs
│       ├── lib.rs
│       └── main.rs
└── esp32/
    ├── components/esp-wiremux/
    │   ├── CMakeLists.txt
    │   ├── include/
    │   └── src/
    └── examples/esp_wiremux_console_demo/
```

## Module Organization

### Host Rust

Keep protocol parsing in the library crate and CLI behavior in `src/main.rs`.

- `src/frame.rs`: binary frame constants, encoder helpers, mixed-stream scanner.
- `src/crc32.rs`: CRC32 implementation used by the frame scanner.
- `src/lib.rs`: public module exports for tests and later tools.
- `sources/core/proto/wiremux.proto`: stable envelope and manifest schema.

Do not put parser state machines directly in `main.rs`; they must stay
unit-testable without a serial device.

Host transmit support belongs in the same crate as the listener. Keep the
existing `listen --line` single-handle path and `send` one-shot path in
`src/main.rs`; do not create a second executable for channel input.

### Portable Core

The portable C core lives under `sources/core/c`.

- `include/wiremux_frame.h`: shared `WMUX` frame constants, frame header type,
  status enum, encoded-length helper, frame encoder, single-frame decoder, and
  CRC32 contract.
- `include/wiremux_envelope.h`: shared `wiremux.v1.MuxEnvelope` field model,
  direction/payload-kind enums, encoded-length helper, encoder, and decoder.
- `include/wiremux_manifest.h`: shared `wiremux.v1.DeviceManifest` and
  `ChannelDescriptor` encoder contract, including native endianness and device
  capability fields.
- `include/wiremux_batch.h`: shared `wiremux.v1.MuxBatch` and record container
  contract.
- `include/wiremux_compression.h`: shared compression/decompression contract for
  Wiremux batch payloads.
- `include/wiremux_status.h`: portable status codes used by core APIs before
  platform adapters map them to runtime-specific error types.
- `src/wiremux_frame.c`: platform-independent frame encode/decode
  implementation with no ESP-IDF dependency.
- `src/wiremux_envelope.c`: platform-independent protobuf-compatible envelope
  encode/decode implementation.
- `src/wiremux_manifest.c`: platform-independent protobuf-compatible manifest
  encoder, including repeated payload kind/type descriptor fields.
- `src/wiremux_batch.c`: platform-independent protobuf-compatible batch and
  batch-record encode/decode implementation.
- `src/wiremux_compression.c`: platform-independent heatshrink-style and LZ4
  codec implementation used by ESP and host-compatible tests.
- `CMakeLists.txt`: host-side GoogleTest/GoogleMock test project for the
  portable core.
- `tests/wiremux_core_test.cpp`: host-side GoogleTest coverage for CRC, frame,
  envelope, manifest, and representative error/status branches.

Platform adapters must prefer this core for shared protocol primitives instead
of duplicating frame constants, length checks, CRC implementations, or
single-frame decode rules.

### ESP-IDF

The reusable ESP component lives under `sources/esp32/components/esp-wiremux`.

- `include/esp_wiremux.h`: core init, channel registration, input handler,
  receive, and write APIs.
- `include/esp_wiremux_frame.h`: ESP-facing wrapper around the portable
  `wiremux_frame.h` contract.
- `include/esp_wiremux_console.h`: mode-configurable console adapter API.
- `include/esp_wiremux_log.h`: ESP log adapter API.
- `src/esp_wiremux.c`: service tasks, queues, inbound parsing, core envelope
  encode/decode integration, protobuf manifest emission, transport reads/writes.
- `src/esp_wiremux_frame.c`: maps portable `wiremux_status_t` results to
  `esp_err_t`.
- `src/esp_wiremux_console.c`: line-mode console adapter.
- `src/esp_wiremux_log.c`: `esp_log_set_vprintf()` adapter.

Examples belong under `sources/esp32/examples/<name>`.

## Naming Conventions

- Rust modules use snake_case filenames.
- ESP-IDF public symbols use the `esp_wiremux_` prefix.
- ESP-IDF component folder is `esp-wiremux`.
- Demo projects should be named by scenario, for example `esp_wiremux_console_demo`.

## Cross-Layer Protocol Contract

The host frame scanner and ESP frame encoder must remain byte-compatible.

### Signatures

Rust:

```rust
pub const MAGIC: [u8; 4] = *b"WMUX";
pub const SUPPORTED_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 14;

pub struct FrameScanner;
impl FrameScanner {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<StreamEvent>;
    pub fn finish(&mut self) -> Vec<StreamEvent>;
}
```

C:

```c
#define WIREMUX_MAGIC "WMUX"
#define WIREMUX_FRAME_VERSION 1
#define WIREMUX_FRAME_HEADER_LEN 14

wiremux_status_t wiremux_frame_encode(const wiremux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);

wiremux_status_t wiremux_frame_decode(const uint8_t *data,
                                      size_t len,
                                      size_t max_payload_len,
                                      wiremux_frame_view_t *frame);

#define ESP_WIREMUX_MAGIC "WMUX"
#define ESP_WIREMUX_FRAME_VERSION 1
#define ESP_WIREMUX_FRAME_HEADER_LEN 14

esp_err_t esp_wiremux_frame_encode(const esp_wiremux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);
```

### Binary Frame Layout

| Offset | Size | Field | Encoding |
|--------|------|-------|----------|
| 0 | 4 | magic | ASCII `WMUX` |
| 4 | 1 | version | `1` |
| 5 | 1 | flags | low 8 frame flags |
| 6 | 4 | payload length | little-endian `u32` |
| 10 | 4 | CRC32 | little-endian IEEE CRC32 of payload |
| 14 | N | payload | protobuf-compatible `MuxEnvelope` bytes |

### Payload Contract

Frame payload must be `wiremux.v1.MuxEnvelope` bytes with these required fields for emitted device data:

- `channel_id` field 1
- `direction` field 2
- `sequence` field 3
- `timestamp_us` field 4
- `kind` field 5
- `payload` field 7
- `flags` field 8

Batched output remains wrapped in a `MuxEnvelope`. The outer envelope uses:

- `channel_id = 0`
- `direction = output`
- `kind = batch`
- `payload_type = "wiremux.v1.MuxBatch"`
- `payload = wiremux.v1.MuxBatch` bytes

`MuxBatch.records` contains encoded `MuxBatchRecords` bytes. If
`MuxBatch.compression` is not `none`, `records` contains compressed
`MuxBatchRecords` bytes and `uncompressed_len` must be the decoded record byte
length. Host tools must decode the batch and apply channel filtering to the
inner records, not to the system-channel wrapper.

### Batch and Compression Signatures

C core:

```c
#define WIREMUX_BATCH_PAYLOAD_TYPE "wiremux.v1.MuxBatch"

typedef enum {
    WIREMUX_COMPRESSION_NONE = 0,
    WIREMUX_COMPRESSION_HEATSHRINK = 1,
    WIREMUX_COMPRESSION_LZ4 = 2,
} wiremux_compression_algorithm_t;

size_t wiremux_batch_records_encoded_len(const wiremux_record_t *records,
                                         size_t record_count);

wiremux_status_t wiremux_batch_records_encode(const wiremux_record_t *records,
                                              size_t record_count,
                                              uint8_t *out,
                                              size_t out_capacity,
                                              size_t *written);

wiremux_status_t wiremux_batch_records_decode(const uint8_t *data,
                                              size_t len,
                                              wiremux_record_t *records,
                                              size_t record_capacity,
                                              size_t *record_count);

wiremux_status_t wiremux_compress(uint32_t algorithm,
                                  const uint8_t *input,
                                  size_t input_len,
                                  uint8_t *out,
                                  size_t out_capacity,
                                  size_t *written);

wiremux_status_t wiremux_decompress(uint32_t algorithm,
                                    const uint8_t *input,
                                    size_t input_len,
                                    uint8_t *out,
                                    size_t out_capacity,
                                    size_t *written);
```

ESP policy:

```c
typedef struct {
    esp_wiremux_send_mode_t send_mode;
    esp_wiremux_compression_algorithm_t compression;
    uint32_t batch_interval_ms;
    size_t batch_max_bytes;
    bool force_compression;
} esp_wiremux_direction_policy_t;

typedef struct {
    esp_wiremux_codec_stats_t compression[3];
} esp_wiremux_diagnostics_t;

esp_err_t esp_wiremux_get_diagnostics(esp_wiremux_diagnostics_t *diagnostics);
```

Validation matrix:

| Case | Required behavior |
|------|-------------------|
| immediate policy | one `esp_wiremux_write*()` emits one single-record `MuxEnvelope` frame |
| batched policy, buffer reaches `batch_max_bytes` | flush one `wiremux.v1.MuxBatch` wrapper frame |
| batched policy, interval expires with buffered data | flush one `wiremux.v1.MuxBatch` wrapper frame |
| batched policy, interval expires with no buffered data | emit no frame |
| compression result is larger and `force_compression = false` | fall back to `WIREMUX_COMPRESSION_NONE` and increment fallback count |
| unsupported compression ID | normalize or reject deterministically before decode/dispatch |
| host channel filter with batch wrapper | apply filter to inner records, not channel 0 wrapper |

Good/Base/Bad cases:

- Good: two log records on channel 2 with heatshrink compression decode as two
  channel-2 records on host.
- Base: console channel remains immediate and uncompressed while log/telemetry
  output batches.
- Bad: compressed batch with unsupported codec produces a visible host decode
  error in unfiltered mode and does not display corrupted payload as text.

For host-to-device input frames, `channel_id`, `direction`, `sequence`, `kind`, and `payload` are required. `direction` must be input, and ESP dispatch must reject frames for unregistered channels or channels without input direction enabled.

System channel manifest frames must use:

- `channel_id = 0`
- `direction = output`
- `kind = control`
- `payload_type = "wiremux.v1.DeviceManifest"`
- `payload = wiremux.v1.DeviceManifest` bytes

`DeviceManifest` must include protocol version, max channels, max payload length,
native endianness, transport name, SDK name/version, feature flags, and channel
descriptors. Channel descriptors may include repeated payload kinds and payload
types in addition to the default payload kind. Native endianness is diagnostic
metadata for tools and binary payload interpretation; it does not change the
`WMUX` frame layout or protobuf wire encoding.

### Host CLI Contract

Current listener:

```bash
wiremux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id]
wiremux listen --port <path> [--channel output_id] [--send-channel input_id] [--line text]
wiremux send --port <path> --channel <id> --line <text> [--baud 115200] [--max-payload bytes]
```

Required behavior:

- Without `--channel`, print ordinary terminal bytes and all decoded mux frames.
- With `--channel <id>`, suppress ordinary terminal bytes and print only decoded mux frames for that channel.
- `listen --line <text>` must write one host-to-device input frame after each successful serial connection, then keep listening on the same serial handle. This is the preferred single-process hardware verification path because most serial devices are exclusively opened.
- `listen --line <text>` defaults to input channel 1. `--send-channel <id>` overrides the input target while `--channel <id>` keeps its output-filter meaning.
- `send --channel <id> --line <text>` is a non-interactive one-shot path for scripts and tests, but it should not be used concurrently with a listener on the same serial device.
- On macOS, prefer `/dev/cu.*` over the paired `/dev/tty.*` device when the user passes a USB serial/JTAG path.
- Use the Rust `serialport` backend for macOS, Linux, and Windows. Do not shell out to `stty` for normal operation.
- Host transmit commands must reuse `encode_envelope()` and `build_frame_payload_with_max()` rather than duplicating protocol constants in `main.rs`.

### Good/Base/Bad Cases

- Good: ordinary text, then valid `WMUX` frame, then ordinary text. Host emits terminal, frame, terminal.
- Base: frame arrives one byte at a time. Host emits no partial frame until length and CRC are complete.
- Bad: ordinary text contains `WMUX` with unsupported version or oversized length. Host must resynchronize and preserve bytes as terminal output.
- Bad: a candidate frame has valid magic/version/length but bad CRC. Host emits a `crc_error` diagnostic event, drains the invalid candidate, and continues scanning.

### Tests Required

- Rust tests must cover valid frames, partial frames, false magic, bad CRC, unsupported version, oversized payload, and one-byte chunk replay.
- ESP frame encoder changes must be validated against Rust scanner output before release.
- Portable C core changes must pass `ctest --test-dir sources/core/c/build
  --output-on-failure` after configuring and building `sources/core/c`.
- Bidirectional changes must keep the existing host frame-building and CLI parser
  tests current, and should add ESP inbound dispatch tests or demo-level manual
  verification steps when ESP behavior changes.

## Scenario: Single-Process Console Verification

### 1. Scope / Trigger

Trigger: validating a command that needs both host input and decoded output on the same physical serial device.

### 2. Signatures

```bash
wiremux listen --port <path> --channel 1 --line help
wiremux listen --port <path> --send-channel 1 --channel 2 --line mux_log
wiremux listen --port <path> --send-channel 1 --channel 3 --line mux_hello
```

### 3. Contracts

- `--line` sends exactly one input frame per successful connection.
- `--send-channel` selects the input channel. If omitted, `--channel` is reused as the input target; if both are omitted, input defaults to channel 1.
- `--channel` filters decoded output only; it must not be required to send input.
- The listener must continue decoding output after sending the input frame.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| `listen --channel 1 --line help` | send input to channel 1 and print console output channel 1 |
| `listen --send-channel 1 --channel 2 --line mux_log` | send console command to channel 1 and print log output channel 2 |
| `listen --line mux_log` | send input to channel 1 and print all decoded mux frames plus ordinary terminal bytes |
| invalid channel value | return a clear CLI parse error before opening serial |
| payload exceeds max payload | return a clear input-frame size error |

### 5. Good/Base/Bad Cases

- Good: `listen --channel 1 --line help` prints a channel-1 command response.
- Base: `listen --send-channel 1 --channel 3 --line mux_hello` prints telemetry output without a second serial process.
- Bad: running `send` from a second process while `listen` owns the port may fail or starve output; use `listen --line` for hardware checks.

### 6. Tests Required

- Host parser test for `listen --line` defaulting to send channel 1.
- Host parser test for `--send-channel` differing from output `--channel`.
- Host frame round-trip test that builds an input frame and decodes it through `FrameScanner`.

### 7. Wrong vs Correct

#### Wrong

```bash
wiremux listen --port /dev/cu.usbmodem2101 --channel 1
wiremux send --port /dev/cu.usbmodem2101 --channel 1 --line help
```

This assumes two processes can reliably own the same serial port.

#### Correct

```bash
wiremux listen --port /dev/cu.usbmodem2101 --channel 1 --line help
```
