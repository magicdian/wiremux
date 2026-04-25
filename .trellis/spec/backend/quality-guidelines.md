# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

This project has a cross-language protocol boundary between Rust host code and ESP-IDF C code. Protocol correctness must be protected with unit tests, explicit constants, and byte-level validation.

## Forbidden Patterns

- Do not duplicate frame constants with different values across host and ESP code.
- Do not parse mux frames by magic alone; always validate version, length, and CRC.
- Do not place protocol state machines only in CLI/app entrypoints; keep them unit-testable.
- Do not hard-code `/dev/tty.usbmodem2101` in implementation. It is only a local example path.
- Do not make console mode a compile-time-only behavior. Public config must preserve line-mode and passthrough mode.
- Do not call ESP logging APIs from mux internals after installing the log adapter.

## Required Patterns

### Host Protocol Tests

Required command:

```bash
cd sources/host
cargo test
cargo check
cargo fmt --check
```

Minimum parser cases:

- valid frame
- partial frame
- mixed terminal text and mux frame
- false magic with bad CRC
- unsupported version resync
- oversized payload
- one-byte replay/chunking

### ESP API Stability

Console integration must use mode-configurable config:

```c
typedef enum {
    ESP_SERIAL_MUX_CONSOLE_MODE_DISABLED = 0,
    ESP_SERIAL_MUX_CONSOLE_MODE_LINE = 1,
    ESP_SERIAL_MUX_CONSOLE_MODE_PASSTHROUGH = 2,
} esp_serial_mux_console_mode_t;
```

`PASSTHROUGH` can return `ESP_ERR_NOT_SUPPORTED` until implemented, but the enum and config field must remain.

## Testing Requirements

- Host Rust code must pass `cargo test`, `cargo check`, and `cargo fmt --check`.
- ESP-IDF code must be built with `idf.py build` in `sources/esp32/examples/console_mux_demo` when ESP-IDF is available.
- Any frame layout change must add or update a host parser test.
- Any ESP encoder change must be manually or automatically validated against the host scanner.

## Code Review Checklist

- Are frame constants still byte-compatible between Rust and C?
- Does the frame payload still encode `MuxEnvelope`, not raw text without channel metadata?
- Does mixed-stream parsing preserve ordinary terminal output?
- Are queue/backpressure failures non-fatal?
- Does log redirection avoid recursion?
- Does console API remain future-compatible with passthrough mode?
