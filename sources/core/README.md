# Wiremux Core Boundary

This directory contains portable Wiremux protocol code and documents the
boundary between shared protocol logic and platform adapters. The runnable Rust
host code still lives in `sources/host/wiremux`, and the first device SDK is
`sources/vendor/espressif/generic/components/esp-wiremux`, but new platform SDKs should share this
core instead of inventing platform-specific protocol variants.

Core-owned concepts:

- Frame layout: `WMUX` magic, version, flags, payload length, CRC32, payload.
- Portable C frame/CRC API: `c/include/wiremux_frame.h` and
  `c/src/wiremux_frame.c`.
- Schema: `../api/proto/versions/current/wiremux.proto`, with frozen API
  snapshots under `../api/proto/versions/`.
- Envelope fields: channel ID, direction, sequence, timestamp, payload kind,
  payload type, payload bytes, and flags.
- Portable C envelope API: `c/include/wiremux_envelope.h` and
  `c/src/wiremux_envelope.c`.
- Generic batch fields: repeated channel records, compression algorithm, encoded
  records bytes, and uncompressed record payload length.
- Portable C batch and codec APIs: `c/include/wiremux_batch.h`,
  `c/include/wiremux_compression.h`, `c/src/wiremux_batch.c`, and
  `c/src/wiremux_compression.c`.
- Channel model: system channel, input/output direction validation, payload
  kind semantics, and manifest schema.
- Portable C manifest API: `c/include/wiremux_manifest.h` and
  `c/src/wiremux_manifest.c`.
- Device capabilities: protocol version, max channels, max payload length,
  transport name, native endianness, SDK name/version, and feature flags.
- Parser behavior: mixed-stream resynchronization, length bounds, CRC validation,
  and deterministic handling of invalid candidate frames.
- Protocol API version policy:
  `WIREMUX_PROTOCOL_API_VERSION_CURRENT`,
  `WIREMUX_PROTOCOL_API_VERSION_MIN_SUPPORTED`, and
  `wiremux_protocol_api_compatibility()`.
- Host session behavior: `c/include/wiremux_host_session.h` owns mixed-stream
  scanning, envelope decode, manifest parsing, batch expansion, decompression,
  manifest request frame construction, and protocol API compatibility events.

Platform adapter responsibilities:

- Transport IO, including UART, USB CDC/JTAG, TCP bridges, or custom byte
  streams.
- Runtime integration, including tasks/threads, queues, locks, timers, memory
  policy, and error-code mapping.
- Platform-specific adapters such as ESP-IDF console binding and ESP log
  forwarding.
- Host presentation, including CLI arguments, serial reconnect, TUI state,
  stdout rendering, and diagnostics file formatting.

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
- Batched device output uses a system-channel `MuxEnvelope` with
  `kind = batch`, `payload_type = "wiremux.v1.MuxBatch"`, and payload bytes that
  decode to `wiremux.v1.MuxBatch`.
- ESP runtime code continues to own FreeRTOS tasks, transport setup, timers, and
  ESP console/log bindings.

Current Rust host integration:

- `sources/host/wiremux/build.rs` compiles `wiremux_core_c` into a static library for
  the Rust host crate.
- `sources/host/wiremux/crates/wiremux-cli/src/host_session.rs` wraps the C host session API and copies
  callback-scope C views into Rust-owned events.
- `listen` and `tui` feed serial bytes into `wiremux_host_session_feed()` and
  render returned Rust events. The Rust layer owns transport and UI behavior,
  while core owns protocol decode and compatibility decisions.

Memory model:

- Host session events are callback-scope views.
- Core does not return heap-owned event objects or require a release/free API.
- Callers provide parser buffer and scratch workspace; scratch exhaustion is a
  deterministic decode error.
- Rust copies any manifest, record, terminal, or diagnostic payload it needs
  after a callback returns.

Portable validation:

```bash
cmake -S sources/core/c -B sources/core/c/build
cmake --build sources/core/c/build
ctest --test-dir sources/core/c/build --output-on-failure
```

The host-side core tests use GoogleTest and GoogleMock through CMake
`FetchContent`. Keep build output under `sources/core/c/build/` so fetched
dependencies and generated files stay out of the repository.
