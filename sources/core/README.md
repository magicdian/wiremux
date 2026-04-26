# Wiremux Core Boundary

This directory contains portable Wiremux protocol code and documents the
boundary between shared protocol logic and platform adapters. The runnable Rust
host code still lives in `sources/host`, and the first device SDK is
`sources/esp32/components/esp-wiremux`, but new platform SDKs should share this
core instead of inventing platform-specific protocol variants.

Core-owned concepts:

- Frame layout: `WMUX` magic, version, flags, payload length, CRC32, payload.
- Portable C frame/CRC API: `c/include/wiremux_frame.h` and
  `c/src/wiremux_frame.c`.
- Schema: `proto/wiremux.proto`.
- Envelope fields: channel ID, direction, sequence, timestamp, payload kind,
  payload type, payload bytes, and flags.
- Portable C envelope API: `c/include/wiremux_envelope.h` and
  `c/src/wiremux_envelope.c`.
- Channel model: system channel, input/output direction validation, payload
  kind semantics, and manifest schema.
- Portable C manifest API: `c/include/wiremux_manifest.h` and
  `c/src/wiremux_manifest.c`.
- Device capabilities: protocol version, max channels, max payload length,
  transport name, native endianness, SDK name/version, and feature flags.
- Parser behavior: mixed-stream resynchronization, length bounds, CRC validation,
  and deterministic handling of invalid candidate frames.

Platform adapter responsibilities:

- Transport IO, including UART, USB CDC/JTAG, TCP bridges, or custom byte
  streams.
- Runtime integration, including tasks/threads, queues, locks, timers, memory
  policy, and error-code mapping.
- Platform-specific adapters such as ESP-IDF console binding and ESP log
  forwarding.

The ESP-IDF adapter is named `esp-wiremux` on disk and uses public C identifiers
with the `esp_wiremux_*` prefix. The host Rust crate and CLI use the product name
`wiremux`.

Current ESP adapter integration:

- `esp_wiremux_frame.h` aliases shared frame constants and `wiremux_frame_header_t`.
- `esp_wiremux_frame.c` maps `wiremux_status_t` into `esp_err_t`.
- `esp_wiremux.c` uses `wiremux_envelope_encode()` and
  `wiremux_envelope_decode()`, and delegates single-frame validation to
  `wiremux_frame_decode()`, instead of owning private protocol helpers.
- `esp_wiremux_emit_manifest()` emits a `wiremux.v1.DeviceManifest` protobuf
  payload with `payload_type = "wiremux.v1.DeviceManifest"`.
- ESP runtime code continues to own FreeRTOS tasks, transport setup, timers, and
  ESP console/log bindings.

Portable validation:

```bash
cc -std=c99 -Wall -Wextra -Werror -I sources/core/c/include \
  sources/core/c/tests/wiremux_core_smoke_test.c \
  sources/core/c/src/wiremux_frame.c \
  sources/core/c/src/wiremux_envelope.c \
  sources/core/c/src/wiremux_manifest.c \
  -o /tmp/wiremux_core_smoke_test
/tmp/wiremux_core_smoke_test
```
