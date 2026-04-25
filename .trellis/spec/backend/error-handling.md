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
| CRC mismatch | emit one byte as terminal and rescan |
| valid frame | emit `StreamEvent::Frame` |

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

## Common Mistakes

### Logging from mux internals

Do not call `ESP_LOGx` from mux service, transport, or log adapter internals. The log adapter hooks `esp_log_set_vprintf()`; internal logging can recurse.

### Treating false magic as fatal

The host runs on mixed terminal streams. A bad frame candidate must not terminate the listener.
