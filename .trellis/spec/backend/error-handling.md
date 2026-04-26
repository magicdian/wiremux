# Error Handling

> How errors are handled in this project.

---

## Overview

Error handling must keep protocol parsing deterministic. Host scanner errors are not fatal for mixed streams; invalid candidate frames are treated as terminal bytes and parsing continues. ESP APIs use `esp_err_t` and must avoid logging from mux internals to prevent recursive log capture.

## Error Types

Rust:

```rust
pub enum BuildFrameError {
    PayloadTooLarge { len: usize, max: usize },
}

pub enum FrameError {
    CrcMismatch {
        version: u8,
        flags: u8,
        payload_len: usize,
        expected_crc: u32,
        actual_crc: u32,
    },
}
```

ESP-IDF:

```c
ESP_ERR_INVALID_ARG   // null pointer, invalid channel, invalid direction
ESP_ERR_INVALID_SIZE  // payload or output buffer too small/large
ESP_ERR_INVALID_STATE // mux not initialized/started, adapter not bound
ESP_ERR_NOT_FOUND     // write to unregistered channel or unsupported direction
ESP_ERR_NOT_SUPPORTED // reserved future mode such as passthrough
ESP_ERR_TIMEOUT       // queue full or transport timeout
ESP_ERR_NO_MEM        // allocation or RTOS object creation failed
```

## Error Handling Patterns

### Host Scanner

The scanner must not fail the whole stream on invalid mux candidates.

| Condition | Behavior |
|-----------|----------|
| no magic present | emit safe terminal bytes and retain possible magic prefix suffix |
| incomplete header/frame | buffer until more bytes arrive |
| unsupported version | emit one byte as terminal and rescan |
| payload length exceeds max | emit one byte as terminal and rescan |
| CRC mismatch | drain the full candidate frame and emit `StreamEvent::FrameError(FrameError::CrcMismatch)` |
| valid frame | emit `StreamEvent::Frame` |

CLI reporting rules:

- Without a channel filter, `FrameError::CrcMismatch` must print a visible `crc_error` line with version, flags, payload length, expected CRC, and actual CRC.
- With `--channel <id>`, CRC errors cannot be attributed to a channel because the envelope is untrusted; suppress them to keep filtered channel output clean.
- Envelope decode failures for CRC-valid frames are printed only in unfiltered mode unless a future decoder can safely extract a channel ID from a partially decoded envelope.

### ESP Producer APIs

`esp_serial_mux_write()` must validate before enqueueing:

- mux is initialized and started
- channel ID is below `ESP_SERIAL_MUX_MAX_CHANNELS`
- channel is registered
- direction is allowed by the channel config
- payload pointer is non-null when length is non-zero
- payload length is at or below configured `max_payload_len`

Queue-full behavior follows channel backpressure policy:

- `DROP_NEWEST`: return `ESP_ERR_TIMEOUT` and increment dropped counter.
- `DROP_OLDEST`: remove one queued item, enqueue the new item if possible.
- `BLOCK_WITH_TIMEOUT`: wait up to caller timeout.

### ESP Inbound APIs

Bidirectional MVP must add an inbound parser/dispatch path before claiming console operation over mux.

Required validation:

| Condition | Result |
|-----------|--------|
| frame magic/version invalid | resynchronize, do not dispatch |
| CRC mismatch | drop candidate frame, increment/drop-report later, do not dispatch |
| envelope direction is not input | reject with no channel callback |
| channel is unregistered | reject with no channel callback |
| channel does not allow input | reject with no channel callback |
| input payload exceeds configured max | reject before invoking callback |
| callback returns error | report through system/control channel when implemented |

### ESP Default USB Serial/JTAG Transport

When `esp_serial_mux_config_init()` leaves the default USB Serial/JTAG transport installed, `esp_serial_mux_init()` must prepare the transport before `esp_serial_mux_start()` creates the RX task.

Required behavior:

| Condition | Result |
|-----------|--------|
| default USB Serial/JTAG read or write transport is used and driver is not installed | call `usb_serial_jtag_driver_install()` before mux start |
| USB Serial/JTAG driver is already installed | reuse it, do not reinstall |
| custom read and write transport are both provided | do not install USB Serial/JTAG driver |
| driver install fails | return the install error from `esp_serial_mux_init()` and do not start tasks |

Assertion point: no task may call `usb_serial_jtag_read_bytes()` unless `usb_serial_jtag_is_driver_installed()` was true or driver install just succeeded.

## Common Mistakes

### Logging from mux internals

Do not call `ESP_LOGx` from mux service, transport, or log adapter internals. The log adapter hooks `esp_log_set_vprintf()`; internal logging can recurse.

### Treating false magic as fatal

The host runs on mixed terminal streams. A bad frame candidate must not terminate the listener.

### Claiming MVP before bidirectional console works

The listener-only milestone is useful, but it is not the complete MVP. Do not mark MVP complete until a host command or stdin-forwarding mode can send channel input and the ESP console channel can execute commands through mux.

### Calling USB Serial/JTAG read before driver install

`usb_serial_jtag_read_bytes()` dereferences the driver object internally. If the mux RX task starts before `usb_serial_jtag_driver_install()`, ESP32 can panic with `LoadProhibited` at boot.

Fix: keep driver preparation inside the default transport path in `sources/esp32/components/esp_serial_mux/src/esp_serial_mux.c`, before task creation. If an application supplies custom transport callbacks, that application owns its transport driver initialization.
