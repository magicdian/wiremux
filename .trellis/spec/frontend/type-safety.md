# Type Safety

> Type safety conventions for frontend-facing protocol work.

---

## Overview

There is no TypeScript frontend in this repository. Current type safety comes
from Rust types, C structs/enums, ESP-IDF `esp_err_t`, and the protobuf-compatible
wire schema.

Important existing type definitions:

- `MuxEnvelope`, `DecodeError`, `FrameScanner`, and `StreamEvent` in
  `sources/host/src/`.
- `esp_wiremux_config_t`, `esp_wiremux_channel_config_t`, and payload enums
  in `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`.
- `esp_wiremux_frame_header_t` and frame constants in
  `sources/esp32/components/esp-wiremux/include/esp_wiremux_frame.h`.
- `MuxEnvelope`, `ChannelDescriptor`, and `DeviceManifest` in
  `sources/core/proto/wiremux.proto`.

## Type Organization

Current organization:

- Rust public protocol modules are exported from `sources/host/src/lib.rs`.
- Rust CLI-only argument structs stay private in `sources/host/src/main.rs`.
- ESP public API types live in component headers under `include/`.
- ESP private implementation structs stay in `src/*.c`.
- Cross-language field numbers live in the proto file and must remain stable.

Future frontend types should be generated from or manually checked against the
protocol schema and backend constants. Do not make a separate incompatible
frontend model.

## Validation

Current runtime validation examples:

```rust
u8::try_from(channel).map_err(|_| format!("invalid --channel value: {value}; must be 0..255"))
```

```c
if (channel_id >= ESP_WIREMUX_MAX_CHANNELS || (payload_len > 0 && payload == NULL)) {
    return ESP_ERR_INVALID_ARG;
}
```

```rust
if payload.len() > max_payload_len {
    return Err(BuildFrameError::PayloadTooLarge {
        len: payload.len(),
        max: max_payload_len,
    });
}
```

Future frontend validation must cover:

- Channel IDs fit the ESP channel range.
- Payload size is at or below configured max payload.
- Direction values are valid input/output values.
- Binary payloads are not forced through UTF-8.
- Frame versions and protocol versions are displayed or rejected explicitly.

## Common Patterns

- Use small enums for constrained behavior (`StreamEvent`, `FrameError`,
  `BuildFrameError`, ESP mode enums).
- Keep CLI parse errors as strings until `run()` maps them to process output.
- Keep ESP API errors as `esp_err_t`.
- Keep binary payloads as bytes until a rendering function chooses escaped text
  or hex.

## Forbidden Patterns

- Do not use TypeScript `any` for mux frames if a frontend is added.
- Do not represent channel IDs as unbounded numbers at protocol boundaries.
- Do not convert binary payload bytes to strings as part of decoding.
- Do not change protobuf field numbers to match frontend naming preferences.
- Do not duplicate C/Rust constants in frontend code without a test that catches
  drift.

## Common Mistakes

- Treating the proto field name as less important than the field number. Field
  numbers are the compatibility contract.
- Assuming every frame payload is a valid UTF-8 string.
- Losing integer bounds when converting between UI numbers, Rust integers, and C
  integer types.
