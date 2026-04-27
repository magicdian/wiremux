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
descriptors. Channel descriptors may include repeated payload kinds, payload
types, and interaction modes in addition to the default payload kind and default
interaction mode. Native endianness is diagnostic metadata for tools and binary
payload interpretation; it does not change the `WMUX` frame layout or protobuf
wire encoding.

`ChannelDescriptor.name` is the host display label. Manifest encoders must emit
at most 15 bytes for this field and must clamp at a UTF-8 codepoint boundary.
C-side buffers that materialize the display name should be 16 bytes to reserve
space for `\0`, but the protobuf string payload itself does not carry the NUL
byte. `ChannelDescriptor.description` remains long-form human metadata and must
not be used as the compact prompt label.

Host tools may request a fresh manifest by sending a system-channel input
envelope:

- `channel_id = 0`
- `direction = input`
- `kind = control`
- `payload_type = "wiremux.v1.DeviceManifestRequest"`
- `payload = wiremux.v1.DeviceManifestRequest` bytes, currently empty

The device replies with the existing `wiremux.v1.DeviceManifest` payload type.
This focused request is the current control-plane contract and may later be
wrapped by a general control request/response protocol.

### Host CLI Contract

Current listener:

```bash
wiremux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id]
wiremux listen --port <path> [--channel output_id] [--send-channel input_id] [--line text]
wiremux send --port <path> --channel <id> --line <text> [--baud 115200] [--max-payload bytes]
wiremux tui --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500]
```

Required behavior:

- At listen startup, create a diagnostics file under
  `std::env::temp_dir()/wiremux/` and print one stdout marker:
  `wiremux> diagnostics: <path>`.
- The diagnostics filename must include a timestamp and sanitized requested port,
  for example `wiremux-1777220326-422658-dev_cu.usbmodem2101.log`.
- Without `--channel`, print ordinary terminal bytes and decoded mux record
  payloads. Each decoded mux record is displayed as `chN> `, or `chN(name)> `
  after the host has learned a non-empty channel name from a manifest, followed
  by raw payload bytes; batch summaries and full record metadata go to
  diagnostics.
- With `--channel <id>`, suppress ordinary terminal bytes and print only raw
  payload bytes for decoded mux records from that channel. Do not add a channel
  prefix or force a trailing newline in filtered mode.
- Payload bytes containing `CRLF`, `CR`, or `LF` must render as real terminal
  line breaks on stdout, not as escaped `\r` or `\n` text. Escaped payload
  summaries belong in diagnostics.
- If unfiltered output switches from one decoded channel to another while the
  previous channel has a partial visible line, the host may end that display
  line and must print a dedicated marker line:
  `wiremux> continued after partial chN line`.
- `listen --line <text>` must write one host-to-device input frame after each successful serial connection, then keep listening on the same serial handle. This is the preferred single-process hardware verification path because most serial devices are exclusively opened.
- `listen --line <text>` defaults to input channel 1. `--send-channel <id>` overrides the input target while `--channel <id>` keeps its output-filter meaning.
- `send --channel <id> --line <text>` is a non-interactive one-shot path for scripts and tests, but it should not be used concurrently with a listener on the same serial device.
- `listen` passively consumes manifest responses when they appear and updates a
  channel-name cache. It must not proactively send `DeviceManifestRequest`; if
  it misses the device boot manifest, it falls back to `chN> ` until another
  manifest is received.
- ESP demos that rely on passive `listen` label discovery should emit an
  immediate manifest and a short delayed manifest after boot, because USB serial
  reset/reconnect can make the host miss the first manifest.
- `tui` owns one serial handle, requests a manifest after connect, displays
  decoded output in a ratatui interface using `chN(name)> ` when manifest names
  are available, and sends bottom-line input through mux input frames. In
  unfiltered mode TUI input targets channel 1; in channel filter mode it targets
  the active channel.
- TUI channel filters use `Ctrl-B 0` for unfiltered mode and `Ctrl-B 1..9` for
  channel filters 1 through 9.
- TUI output scrollback is an in-memory viewport over the existing bounded
  `MAX_LINES` buffer in `sources/host/src/tui.rs`. `scroll_offset = 0` means the
  output pane follows live tail output. Mouse wheel up increases
  `scroll_offset` and freezes the visible window; matching incoming lines must
  increase `scroll_offset` while frozen so the same historical rows stay visible.
- TUI scroll recovery uses explicit user actions only: mouse wheel down to
  `scroll_offset = 0`, dragging the right-side output scrollbar to the bottom,
  or pressing `Enter` twice while the input line is empty. `Enter` with
  non-empty input must preserve the existing send behavior and must not count
  toward the recovery gesture.
- The TUI right-side scrollbar represents scrollable positions, not raw content
  rows: `position = max_scroll_offset - scroll_offset`. At live tail
  (`scroll_offset = 0`) the thumb must render at the bottom; at the oldest
  visible position it must render at the top. Mouse dragging must start on the
  scrollbar column, but once dragging is active, row movement should keep
  updating the offset even if the pointer leaves the column.
- On macOS, prefer `/dev/cu.*` over the paired `/dev/tty.*` device when the user passes a USB serial/JTAG path.
- Use the Rust `serialport` backend for macOS, Linux, and Windows. Do not shell out to `stty` for normal operation.
- Host transmit commands must reuse `encode_envelope()` and `build_frame_payload_with_max()` rather than duplicating protocol constants in `main.rs`.

### Good/Base/Bad Cases

- Good: ordinary text, then valid channel-3 `WMUX` frame, then ordinary text.
  Host emits terminal bytes, `ch3> <payload>` bytes, then terminal bytes in
  unfiltered mode.
- Good: `listen --channel 1 --line help` emits channel-1 payload bytes directly,
  preserving console newlines and adding no `ch1> ` prefix.
- Good: `wiremux tui` is scrolled up while new channel-2 log rows arrive. The
  same historical rows stay visible until the user scrolls back to the bottom,
  drags the scrollbar to the bottom, or presses empty `Enter` twice.
- Base: frame arrives one byte at a time. Host emits no partial frame until length and CRC are complete.
- Bad: ordinary text contains `WMUX` with unsupported version or oversized length. Host must resynchronize and preserve bytes as terminal output.
- Bad: a TUI scrollbar uses the raw first visible row as its position; at live
  tail the thumb appears above the bottom and misrepresents scroll progress.
- Bad: a candidate frame has valid magic/version/length but bad CRC. Host writes
  a full `crc_error` diagnostic event to the diagnostics file, emits only a
  concise stdout marker in unfiltered mode, drains the invalid candidate, and
  continues scanning.

### Tests Required

- Rust tests must cover valid frames, partial frames, false magic, bad CRC, unsupported version, oversized payload, and one-byte chunk replay.
- ESP frame encoder changes must be validated against Rust scanner output before release.
- Portable C core changes must pass `ctest --test-dir sources/core/c/build
  --output-on-failure` after configuring and building `sources/core/c`.
- Bidirectional changes must keep the existing host frame-building and CLI parser
  tests current, and should add ESP inbound dispatch tests or demo-level manual
  verification steps when ESP behavior changes.
- Host display changes must test filtered raw payload output, unfiltered `chN> `
  and `chN(name)> ` display, CRLF/CR/LF preservation, partial-line channel
  switch markers, passive manifest label caching for `listen`, and batch
  summary routing to diagnostics.
- Host TUI scrollback changes must test visible window calculation, frozen view
  behavior when matching lines are appended, empty-Enter recovery, filtered-line
  scroll counts, scrollbar row-to-offset mapping, drag behavior after leaving
  the scrollbar column, and scrollbar position at live tail.

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
| `listen --line mux_log` | send input to channel 1 and print ordinary terminal bytes plus concise `chN> ` decoded record payloads |
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

## Scenario: Manifest Channel Name Display Labels

### 1. Scope / Trigger

Trigger: changing manifest channel metadata, host output prefixes, or ESP demo
channel registration. This spans portable C manifest encoding, ESP component
manifest emission, Rust manifest decoding, non-TUI display, and TUI rendering.

### 2. Signatures

C core:

```c
#define WIREMUX_CHANNEL_NAME_MAX_BYTES 15u

typedef struct {
    uint32_t channel_id;
    const char *name;
    const char *description;
    /* remaining descriptor fields */
} wiremux_channel_descriptor_t;
```

Rust host:

```rust
pub const CHANNEL_NAME_MAX_BYTES: usize = 15;
pub fn display_channel_name(name: &str) -> Option<String>;
```

CLI/TUI output:

```text
chN> payload
chN(name)> payload
```

### 3. Contracts

- `wiremux_device_manifest_encode()` is the source of truth for the wire
  `ChannelDescriptor.name` bound. It writes the longest valid UTF-8 prefix that
  fits within `WIREMUX_CHANNEL_NAME_MAX_BYTES`.
- Invalid UTF-8 source bytes in a C string must not be emitted as invalid
  protobuf strings. Emit the valid prefix before the invalid sequence, or omit
  the name if no valid prefix exists.
- Host display must treat channel names as optional. Empty, control-only, or
  missing names fall back to `chN> `.
- Host display must remove control characters from names before rendering to a
  terminal. Valid non-control UTF-8, including emoji, is allowed.
- `listen --channel N` remains raw payload output and must not add channel
  prefixes or manifest labels.
- Non-TUI `listen` only learns labels from manifest frames it receives. TUI
  actively requests manifest after connect and then renders names when present.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| ASCII name `console` | manifest carries `console`; host displays `ch1(console)> ` |
| ASCII name longer than 15 bytes | manifest carries first 15 bytes |
| UTF-8 name `🚗🎒😄🔥` | manifest carries `🚗🎒😄`, not a partial fourth emoji |
| invalid UTF-8 bytes before any valid prefix | manifest omits channel name |
| host receives empty/control-only name | host displays `chN> ` |
| unfiltered listen receives manifest then channel record | manifest is cached and hidden; later record displays `chN(name)> ` |
| unfiltered listen misses manifest | records continue displaying `chN> ` |
| filtered listen receives matching record | output remains raw payload bytes only |
| USB serial reset makes host miss immediate boot manifest | demo delayed manifest lets passive listen learn labels after reconnect |

### 5. Good/Base/Bad Cases

- Good: ESP demo configures channel 4 name as `🚗🎒😄🔥`; host sees
  `ch4(🚗🎒😄)> UTF-8 ...`.
- Base: older firmware emits no names; host output remains `chN> `.
- Bad: core emits 15 raw bytes that split a UTF-8 codepoint; Rust manifest
  decode fails or terminal output shows replacement characters.

### 6. Tests Required

- Portable C tests cover ASCII 15-byte clamp, UTF-8 boundary clamp, and invalid
  UTF-8 omission for manifest channel names.
- Rust manifest tests cover `display_channel_name()` UTF-8 boundary clamp and
  control character removal.
- Rust CLI display tests cover manifest label cache, hidden manifest payload,
  `chN(name)> ` unfiltered output, and raw filtered output preservation.
- TUI tests cover `App::channel_prefix()` with manifest names and fallback.

### 7. Wrong vs Correct

#### Wrong

```text
Encode the first 15 bytes of "🚗🎒😄🔥" directly, producing invalid UTF-8.
```

#### Correct

```text
Encode only "🚗🎒😄" because it is the longest valid UTF-8 prefix within 15 bytes.
```
