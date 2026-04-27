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

- Without a channel filter, `FrameError::CrcMismatch` must write a full
  `crc_error` line to the diagnostics file with version, flags, payload length,
  expected CRC, and actual CRC. Stdout should stay concise and may print only
  `wiremux> crc error; details in diagnostics`.
- With `--channel <id>`, CRC errors cannot be attributed to a channel because the
  envelope is untrusted; suppress stdout reporting to keep filtered channel
  output clean, but still write the full diagnostics line.
- Envelope decode failures for CRC-valid frames must be written to diagnostics.
  In unfiltered mode stdout may print a concise `wiremux>` marker; in filtered
  mode stdout stays clean unless a future decoder can safely extract a channel ID
  from a partially decoded envelope.
- When the Rust host uses the C `wiremux_host_session_*` API, the same reporting
  rules apply to `WIREMUX_HOST_EVENT_CRC_ERROR` and
  `WIREMUX_HOST_EVENT_DECODE_ERROR`.
- `WIREMUX_HOST_EVENT_PROTOCOL_COMPATIBILITY` with
  `WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_NEW` must be deterministic and actionable:
  diagnostics record the device and host API versions, and unfiltered UI should
  tell the user to upgrade the host SDK/tool.
- Scratch workspace exhaustion during batch decompression is a decode error, not
  undefined behavior. It must not emit partial decoded records.

### ESP Producer APIs

`esp_wiremux_write()` must validate before enqueueing:

- mux is initialized and started
- channel ID is below `ESP_WIREMUX_MAX_CHANNELS`
- channel is registered
- direction is exactly one supported envelope direction:
  `ESP_WIREMUX_DIRECTION_INPUT` or `ESP_WIREMUX_DIRECTION_OUTPUT`
- direction is allowed by the channel config
- payload pointer is non-null when length is non-zero
- payload length is at or below configured `max_payload_len`

Queue-full behavior follows channel backpressure policy:

- `DROP_NEWEST`: return `ESP_ERR_TIMEOUT` and increment dropped counter.
- `DROP_OLDEST`: remove one queued item, enqueue the new item if possible.
- `BLOCK_WITH_TIMEOUT`: wait up to caller timeout.

### ESP Inbound APIs

The ESP component has an inbound parser/dispatch path through
`esp_wiremux_receive_bytes()` and registered input handlers. Keep this path
bounded and deterministic.

Required validation:

| Condition | Result |
|-----------|--------|
| frame magic/version invalid | resynchronize, do not dispatch |
| CRC mismatch | drop candidate frame, increment/drop-report later, do not dispatch |
| envelope direction is not input | reject with no channel callback |
| channel is unregistered | reject with no channel callback |
| channel does not allow input | reject with no channel callback |
| input payload exceeds configured max | reject before invoking callback |
| input payload is valid | copy payload out of the shared RX buffer before releasing the mux lock and invoking callback |
| callback returns error | propagate from the handler path when directly invoked; future system/control reporting must not run inside the lock |

The `esp_wiremux_input_handler_t` payload pointer is valid only for the duration
of the callback. The mux core must not pass a pointer into `s_mux.rx_buffer`
after releasing the lock, because other callers of `esp_wiremux_receive_bytes()`
could reuse that buffer before the handler finishes.

### ESP Default USB Serial/JTAG Transport

When `esp_wiremux_config_init()` leaves the default USB Serial/JTAG transport installed, `esp_wiremux_init()` must prepare the transport before `esp_wiremux_start()` creates the RX task.

Required behavior:

| Condition | Result |
|-----------|--------|
| default USB Serial/JTAG read or write transport is used and driver is not installed | call `usb_serial_jtag_driver_install()` before mux start |
| USB Serial/JTAG driver is already installed | reuse it, do not reinstall |
| custom read and write transport are both provided | do not install USB Serial/JTAG driver |
| driver install fails | return the install error from `esp_wiremux_init()` and do not start tasks |

Assertion point: no task may call `usb_serial_jtag_read_bytes()` unless `usb_serial_jtag_is_driver_installed()` was true or driver install just succeeded.

## Common Mistakes

### Logging from mux internals

Do not call `ESP_LOGx` from mux service, transport, or log adapter internals. The log adapter hooks `esp_log_set_vprintf()`; internal logging can recurse.

### Treating false magic as fatal

The host runs on mixed terminal streams. A bad frame candidate must not terminate the listener.

### Regressing bidirectional console flow

Do not regress the current single-handle console path. `listen --line` must be
able to send a channel input frame after connecting and then keep decoding output
on the same serial handle.

### Calling USB Serial/JTAG read before driver install

`usb_serial_jtag_read_bytes()` dereferences the driver object internally. If the mux RX task starts before `usb_serial_jtag_driver_install()`, ESP32 can panic with `LoadProhibited` at boot.

Fix: keep driver preparation inside the default transport path in `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`, before task creation. If an application supplies custom transport callbacks, that application owns its transport driver initialization.

### Passing combined direction flags as envelope direction

Channel configs may use direction flags such as
`ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT`, but a
`MuxEnvelope.direction` value must be a single protobuf enum. `esp_wiremux_write()`
must reject combined or unknown direction values with `ESP_ERR_INVALID_ARG`
before enqueueing, otherwise the device can emit an invalid `direction = 3`
envelope that host tools cannot interpret as a defined enum.
