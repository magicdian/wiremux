# Directory Structure

> How backend code is organized in this project.

---

## Overview

This project is split into a host-side Rust tool and an ESP-IDF component. Source code lives under `sources/`, not repository root `src/`.

The current committed milestone is a one-way decoder/demo milestone. The complete MVP boundary is bidirectional: the host must be able to send mux input to an ESP channel and operate an ESP console through the mux.

## Directory Layout

```text
sources/
├── host/
│   ├── Cargo.toml
│   ├── proto/esp_serial_mux.proto
│   └── src/
│       ├── crc32.rs
│       ├── frame.rs
│       ├── lib.rs
│       └── main.rs
└── esp32/
    ├── components/esp_serial_mux/
    │   ├── CMakeLists.txt
    │   ├── include/
    │   └── src/
    └── examples/console_mux_demo/
```

## Module Organization

### Host Rust

Keep protocol parsing in the library crate and CLI behavior in `src/main.rs`.

- `src/frame.rs`: binary frame constants, encoder helpers, mixed-stream scanner.
- `src/crc32.rs`: CRC32 implementation used by the frame scanner.
- `src/lib.rs`: public module exports for tests and later tools.
- `proto/esp_serial_mux.proto`: stable envelope and manifest schema.

Do not put parser state machines directly in `main.rs`; they must stay unit-testable without a serial device.

The host CLI may start with a `listen` command, but transmit support must be added as commands or modes under the same crate. Do not create a second host executable for sending channel input.

### ESP-IDF

The reusable ESP component lives under `sources/esp32/components/esp_serial_mux`.

- `include/esp_serial_mux.h`: core init, channel registration, write APIs.
- `include/esp_serial_mux_frame.h`: magic/length/CRC frame encoder contract.
- `include/esp_serial_mux_console.h`: mode-configurable console adapter API.
- `include/esp_serial_mux_log.h`: ESP log adapter API.
- `src/esp_serial_mux.c`: service task, queues, envelope encoding, transport writes.
- `src/esp_serial_mux_frame.c`: C frame encoder and CRC32.
- `src/esp_serial_mux_console.c`: line-mode console adapter.
- `src/esp_serial_mux_log.c`: `esp_log_set_vprintf()` adapter.

Examples belong under `sources/esp32/examples/<name>`.

## Naming Conventions

- Rust modules use snake_case filenames.
- ESP-IDF public symbols use the `esp_serial_mux_` prefix.
- ESP-IDF component folder is `esp_serial_mux`.
- Demo projects should be named by scenario, for example `console_mux_demo`.

## Cross-Layer Protocol Contract

The host frame scanner and ESP frame encoder must remain byte-compatible.

### Signatures

Rust:

```rust
pub const MAGIC: [u8; 4] = *b"ESMX";
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
#define ESP_SERIAL_MUX_MAGIC "ESMX"
#define ESP_SERIAL_MUX_FRAME_VERSION 1
#define ESP_SERIAL_MUX_FRAME_HEADER_LEN 14

esp_err_t esp_serial_mux_frame_encode(const esp_serial_mux_frame_header_t *header,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);
```

### Binary Frame Layout

| Offset | Size | Field | Encoding |
|--------|------|-------|----------|
| 0 | 4 | magic | ASCII `ESMX` |
| 4 | 1 | version | `1` |
| 5 | 1 | flags | low 8 frame flags |
| 6 | 4 | payload length | little-endian `u32` |
| 10 | 4 | CRC32 | little-endian IEEE CRC32 of payload |
| 14 | N | payload | protobuf-compatible `MuxEnvelope` bytes |

### Payload Contract

Frame payload must be `esp_serial_mux.v1.MuxEnvelope` bytes with these required fields for emitted device data:

- `channel_id` field 1
- `direction` field 2
- `sequence` field 3
- `timestamp_us` field 4
- `kind` field 5
- `payload` field 7
- `flags` field 8

For host-to-device input frames, `channel_id`, `direction`, `sequence`, `kind`, and `payload` are required. `direction` must be input, and ESP dispatch must reject frames for unregistered channels or channels without input direction enabled.

### Host CLI Contract

Current listener:

```bash
esp-serial-mux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id]
```

Required behavior:

- Without `--channel`, print ordinary terminal bytes and all decoded mux frames.
- With `--channel <id>`, suppress ordinary terminal bytes and print only decoded mux frames for that channel.
- On macOS, prefer `/dev/cu.*` over the paired `/dev/tty.*` device when the user passes a USB serial/JTAG path.
- Configure serial paths in raw mode before reading so binary mux frames are not transformed by the terminal driver.

Future MVP transmit commands must reuse the same frame/envelope implementation rather than duplicating protocol constants in `main.rs`.

### Good/Base/Bad Cases

- Good: ordinary text, then valid `ESMX` frame, then ordinary text. Host emits terminal, frame, terminal.
- Base: frame arrives one byte at a time. Host emits no partial frame until length and CRC are complete.
- Bad: ordinary text contains `ESMX` with unsupported version or oversized length. Host must resynchronize and preserve bytes as terminal output.
- Bad: a candidate frame has valid magic/version/length but bad CRC. Host emits a `crc_error` diagnostic event, drains the invalid candidate, and continues scanning.

### Tests Required

- Rust tests must cover valid frames, partial frames, false magic, bad CRC, unsupported version, oversized payload, and one-byte chunk replay.
- ESP frame encoder changes must be validated against Rust scanner output before release.
- Bidirectional MVP changes must add host frame-building tests and ESP inbound dispatch tests or demo-level manual verification steps.
