# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

This project has a cross-language protocol boundary between Rust host code and ESP-IDF C code. Protocol correctness must be protected with unit tests, explicit constants, and byte-level validation.

Do not call the listener-only state an MVP. The current milestone proves framing, decoding, channel filtering, log capture, telemetry, and demo packaging. The MVP requires bidirectional channel input and a console that can be operated from the host through mux.

## Forbidden Patterns

- Do not duplicate frame constants with different values across host and ESP code.
- Do not parse mux frames by magic alone; always validate version, length, and CRC.
- Do not place protocol state machines only in CLI/app entrypoints; keep them unit-testable.
- Do not hard-code `/dev/tty.usbmodem2101` in implementation. It is only a local example path.
- Do not make console mode a compile-time-only behavior. Public config must preserve line-mode and passthrough mode.
- Do not call ESP logging APIs from mux internals after installing the log adapter.
- Do not implement host-to-device frames with a separate ad-hoc wire format. Use the same `ESMX` frame and `MuxEnvelope` payload contract.

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

## Scenario: Bidirectional Console MVP Boundary

### 1. Scope / Trigger

Trigger: any change that claims MVP completeness, console operation, host input, or full-duplex mux behavior.

### 2. Signatures

Host:

```bash
esp-serial-mux listen --port <path> [--channel id]
esp-serial-mux listen --port <path> [--channel output_id] [--send-channel input_id] --line <text>
esp-serial-mux send --port <path> --channel <id> [--line text]
```

ESP:

```c
typedef esp_err_t (*esp_serial_mux_transport_read_fn)(uint8_t *data,
                                                      size_t capacity,
                                                      size_t *read_len,
                                                      uint32_t timeout_ms,
                                                      void *user_ctx);

typedef esp_err_t (*esp_serial_mux_input_handler_t)(uint8_t channel_id,
                                                    const uint8_t *payload,
                                                    size_t payload_len,
                                                    void *user_ctx);

esp_err_t esp_serial_mux_register_input_handler(uint8_t channel_id,
                                                esp_serial_mux_input_handler_t handler,
                                                void *user_ctx);

esp_err_t esp_serial_mux_receive_bytes(const uint8_t *data, size_t len);
```

Exact names may change during implementation, but the capability must exist: host builds an input envelope, ESP decodes it, and the registered channel handler receives bounded bytes.

### 3. Contracts

- Host input frames use the same magic/version/length/CRC wrapper as device output frames.
- Host input envelopes set `direction = input`.
- Console line-mode sends complete command lines to the console channel.
- Hardware manual verification should use `listen --line` to send and receive through one serial handle. Most serial devices do not support a separate `listen` process and `send` process at the same time.
- `--send-channel` selects the input channel independently from the output filter `--channel`.
- ESP line-mode console dispatch calls `esp_console_run()` or an equivalent registered dispatcher, not a hard-coded demo command table in the mux core.
- Output from command execution is emitted on the console output channel.
- ESP inbound dispatch must validate magic, version, length, CRC, envelope direction, channel registration, and channel input capability before invoking callbacks.
- Default USB Serial/JTAG transport must install or reuse the USB Serial/JTAG driver before creating an RX task.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| host sends to unregistered channel | ESP rejects without callback |
| host sends output-direction frame | ESP rejects without callback |
| host sends oversized input payload | ESP rejects before allocation-heavy work |
| console command succeeds | host can observe response on console channel |
| console command fails | host can observe command error text or return status |
| default USB Serial/JTAG driver missing | mux init installs driver before RX task starts |
| serial disconnects during send/listen | host reconnect behavior remains deterministic |

### 5. Good/Base/Bad Cases

- Good: `listen --channel 1 --line help` executes the ESP console help command and returns console text through channel 1.
- Base: telemetry and log channels continue emitting while console input is used.
- Bad: corrupt host input frame does not call the console handler and does not crash the mux task.
- Bad: `listen` in one process and `send` in another process race on the same serial device; use `listen --line` for single-device verification.

### 6. Tests Required

- Host unit test builds an input frame and verifies the scanner decodes it back into the expected envelope fields.
- Host unit tests cover `listen --line`, `--send-channel`, invalid channel, missing line for one-shot `send`, and macOS `tty` to `cu` preference.
- ESP inbound parser test or demo verification covers a valid input frame and bad CRC.
- Demo-level verification documents the exact commands used to run `help` through channel 1, trigger `mux_log` on channel 2, and trigger `mux_hello` on channel 3.

### 7. Wrong vs Correct

#### Wrong

```text
Host writes raw "help\n" to the serial port and assumes ESP console receives it.
```

#### Correct

```text
Host wraps "help\n" in a channel-1 input MuxEnvelope, then in an ESMX frame with CRC32.
ESP validates the frame and dispatches the payload to the registered console input handler.
```

Correct single-process hardware check:

```text
Host opens the serial port once, sends the input frame with `listen --line`, then keeps decoding output on the same handle.
```

## Testing Requirements

- Host Rust code must pass `cargo test`, `cargo check`, and `cargo fmt --check`.
- ESP-IDF code must be built with `idf.py build` in `sources/esp32/examples/console_mux_demo` when ESP-IDF is available.
- Any frame layout change must add or update a host parser test.
- Any ESP encoder change must be manually or automatically validated against the host scanner.
- Any MVP-completeness claim must include at least one bidirectional console verification path.

## Code Review Checklist

- Are frame constants still byte-compatible between Rust and C?
- Does the frame payload still encode `MuxEnvelope`, not raw text without channel metadata?
- Does mixed-stream parsing preserve ordinary terminal output?
- Are queue/backpressure failures non-fatal?
- Does log redirection avoid recursion?
- Does console API remain future-compatible with passthrough mode?
