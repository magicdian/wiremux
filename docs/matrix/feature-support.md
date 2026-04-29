# Wiremux Feature Support Matrix

This matrix tracks product-level feature support across host platforms. It is
the source of truth for user-facing platform claims.

Legend:

- `Supported`: implemented and covered by normal checks.
- `Partial`: implemented with documented limitations.
- `Planned`: designed or reserved, but not implemented yet.
- `Unsupported`: intentionally unavailable on this platform.

| Feature | Linux | macOS | Windows | Notes |
| --- | --- | --- | --- | --- |
| Core WMUX frame decode/encode | Supported | Supported | Supported | Portable core and Rust host session wrappers. |
| Host `listen` decoded output | Supported | Supported | Supported | Uses `serialport`; hardware availability still depends on local drivers. |
| Host `send` channel input | Supported | Supported | Supported | Sends `WMUX` input frames through the physical serial transport. |
| Host `listen --line` single-handle verification | Supported | Supported | Supported | Preferred manual verification path for full-duplex serial devices. |
| Host `passthrough` command | Supported | Supported | Supported | `mio` backend is Unix-only; Windows uses compat backend. |
| Host TUI | Supported | Supported | Supported | Terminal behavior can vary by terminal emulator. |
| TUI settings for physical serial profile | Supported | Supported | Supported | Edits `[serial]` config only. |
| TUI channel filtering | Supported | Supported | Supported | `Ctrl-B 0..9`. |
| TUI manifest-driven line input | Supported | Supported | Supported | Requires manifest `DIRECTION_INPUT`. |
| TUI manifest-driven passthrough input | Supported | Supported | Supported | Requires passthrough interaction metadata. |
| TUI application-managed text selection | Supported | Supported | Supported | Uses OSC 52 for copy where supported by the terminal. |
| Generic enhanced host mode | Supported | Supported | Partial | Windows compiles the interface but virtual serial activation is unsupported for now. |
| Generic virtual serial endpoints | Supported | Supported | Planned | Requires generic enhanced or higher host build; generic builds cannot enable it from config or TUI. Unix PTY backend first; Windows virtual COM backend is future work. |
| Virtual serial output mirroring | Supported | Supported | Planned | Mirrors every manifest channel to its endpoint when virtual serial is enabled; non-passthrough text records are line-delimited for terminal clients. |
| Virtual serial input ownership gate | Supported | Supported | Planned | Host owns input by default; TUI can hand ownership to virtual serial. |
| Output-only channel read-only virtual endpoint | Supported | Supported | Planned | Writes are rejected or discarded without sending device frames. |
| Vendor enhanced host mode | Partial | Partial | Partial | Build selection exists; device-specific runtime adapters are incremental. |
| ESP32 OTA enhanced flow | Planned | Planned | Planned | Future vendor enhanced feature. |
| ESP32 esptool passthrough enhanced flow | Planned | Planned | Planned | Future vendor enhanced feature using a special virtual endpoint. |
| TCP bridge | Planned | Planned | Planned | Reserved host enhanced capability. |
| Capture/replay | Planned | Planned | Planned | Reserved host enhanced capability. |
| Reliable transfer profile | Planned | Planned | Planned | Profile skeleton exists; runtime protocol is future work. |

## Maintenance

Any feature that changes platform behavior, build modes, host commands, virtual
serial support, or TUI capabilities must update this matrix in the same change.
