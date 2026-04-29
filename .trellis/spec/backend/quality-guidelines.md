# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

This project has a cross-language protocol boundary between Rust host code and
ESP-IDF C code. Protocol correctness must be protected with unit tests, explicit
constants, and byte-level validation.

The current framework includes framing, decoding, channel filtering, host
transmit, ESP inbound dispatch, console line-mode integration, log capture,
telemetry, and demo packaging. Future changes must preserve bidirectional console
operation.

## Forbidden Patterns

- Do not duplicate frame constants with different values across host and ESP code.
- Do not parse mux frames by magic alone; always validate version, length, and CRC.
- Do not place protocol state machines only in CLI/app entrypoints; keep them unit-testable.
- Do not add new Rust-side protocol parsers for manifest, batch, compression, or
  API compatibility in CLI/TUI paths when the C host session API can own the
  behavior.
- Do not couple `DeviceManifest.protocol_version` to `WIREMUX_FRAME_VERSION`;
  frame version and protocol API version are separate contracts.
- Do not return C heap-owned event objects across the Rust FFI boundary. Host
  session events are callback-scope views and Rust must copy data it keeps.
- Do not hard-code `/dev/tty.usbmodem2101` in implementation. It is only a local example path.
- Do not make console mode a compile-time-only behavior. Public config must preserve line-mode and passthrough mode.
- Do not call ESP logging APIs from mux internals after installing the log adapter.
- Do not implement host-to-device frames with a separate ad-hoc wire format. Use the same `WMUX` frame and `MuxEnvelope` payload contract.
- Do not treat transitional paths as permanent architecture. New specs and
  design docs must use the target layout from `docs/source-layout-build.md` and
  label `sources/host` and `sources/esp32` as current pre-migration paths when
  they are still needed for commands.

## Required Patterns

### Host Protocol Tests

Required command:

```bash
cd sources/host/wiremux
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

### Portable Core Tests

Portable core C behavior must be protected by the host-side GoogleTest suite in
`sources/core/c`.

Required command after any `sources/core/c/include/`,
`sources/core/c/src/`, or `sources/core/c/tests/` change:

```bash
cmake -S sources/core/c -B sources/core/c/build
cmake --build sources/core/c/build
ctest --test-dir sources/core/c/build --output-on-failure
```

Test target and dependency contract:

```cmake
add_library(wiremux_core_c STATIC ...)
add_executable(wiremux_core_tests tests/wiremux_core_test.cpp)
target_link_libraries(wiremux_core_tests PRIVATE wiremux_core_c GTest::gmock_main)
gtest_discover_tests(wiremux_core_tests)
```

Rules:

- Every new portable core feature must add or update related tests in
  `sources/core/c/tests/wiremux_core_test.cpp`.
- Every portable core behavior change must update tests for both the successful
  path and the relevant `wiremux_status_t` error branch.
- Portable batch/compression changes must test uncompressed batch records,
  batch metadata, heatshrink round-trip, LZ4 round-trip, unsupported codec, and
  small output errors.
- Portable host session changes must test callback ordering, callback-scope
  event copying, CRC errors, manifest parsing, batch expansion, compression
  decode failures, scratch exhaustion, and API compatibility classification.
- Protocol API changes update `sources/api/proto/versions/current/`. Freeze a
  numbered API snapshot when shipped, update `wiremux_version.h` constants, and
  keep snapshot tests current.
- Do not add production-only abstractions solely to demonstrate GoogleMock.
  Link `GTest::gmock_main` so real future collaboration boundaries can use
  gmock when they exist.
- Keep test fixtures C++-only and call the production C API through
  `extern "C"` includes; do not change the portable core ABI to satisfy tests.

Minimum portable core cases:

- CRC32 known vector
- frame encode/decode, empty payload, invalid args, undersized output, short
  input, bad magic, bad version, max payload rejection, incomplete full frame,
  and CRC mismatch
- envelope encode/decode, zero-length optional fields, invalid args,
  insufficient output, unknown varint fields ignored, unsupported wire type,
  truncated varint, and truncated length-delimited field
- manifest encoding, optional empty strings omitted, invalid args, insufficient
  output, and invalid channel descriptor pointer/count combinations

### ESP API Stability

Console integration must use mode-configurable config:

```c
typedef enum {
    ESP_WIREMUX_CONSOLE_MODE_DISABLED = 0,
    ESP_WIREMUX_CONSOLE_MODE_LINE = 1,
    ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH = 2,
} esp_wiremux_console_mode_t;
```

`PASSTHROUGH` is implemented through configurable passthrough backends. Core
backend names must remain platform-neutral; ESP-facing names may alias them.

## Scenario: Bidirectional Console Boundary

### 1. Scope / Trigger

Trigger: any change to console operation, host input, ESP inbound dispatch, or
full-duplex mux behavior.

### 2. Signatures

Host:

```bash
wiremux listen [--port <path>] [--baud 115200] [--data-bits 8] [--stop-bits 1] [--parity none|odd|even] [--flow-control none|software|hardware] [--channel id]
wiremux listen [--port <path>] [--channel output_id] [--send-channel input_id] --line <text>
wiremux send [--port <path>] --channel <id> [--line text]
wiremux passthrough [--port <path>] --channel <id> [--interactive-backend auto|compat|mio]
wiremux tui [--port <path>] [--baud 115200] [--data-bits 8] [--stop-bits 1] [--parity none|odd|even] [--flow-control none|software|hardware] [--interactive-backend auto|compat|mio] [--tui-fps 60|120]
```

Rust host interactive backend:

```rust
pub enum InteractiveBackendMode {
    Auto,
    Compat,
    Mio,
}

pub enum InteractiveEvent {
    SerialBytes(Vec<u8>),
    SerialEof,
    SerialError(std::io::Error),
    Terminal(crossterm::event::Event),
    Timeout,
}

pub fn open_interactive_backend(
    profile: &SerialProfile,
    mode: InteractiveBackendMode,
    read_timeout: Duration,
) -> io::Result<(PathBuf, ConnectedInteractiveBackend)>;
```

ESP:

```c
typedef esp_err_t (*esp_wiremux_transport_read_fn)(uint8_t *data,
                                                      size_t capacity,
                                                      size_t *read_len,
                                                      uint32_t timeout_ms,
                                                      void *user_ctx);

typedef esp_err_t (*esp_wiremux_input_handler_t)(uint8_t channel_id,
                                                    const uint8_t *payload,
                                                    size_t payload_len,
                                                    void *user_ctx);

esp_err_t esp_wiremux_register_input_handler(uint8_t channel_id,
                                                esp_wiremux_input_handler_t handler,
                                                void *user_ctx);

esp_err_t esp_wiremux_receive_bytes(const uint8_t *data, size_t len);
```

These names are the current public boundary. If they change, update this spec,
the demo, and host verification commands in the same task.

### 3. Contracts

- Host input frames use the same magic/version/length/CRC wrapper as device output frames.
- Host input envelopes set `direction = input`.
- Console line-mode sends complete command lines to the console channel.
- Host physical serial configuration is modeled as a `SerialProfile` with
  `port`, `baud`, `data_bits`, `stop_bits`, `parity`, and `flow_control`.
  CLI overrides have priority over global config, and global config has
  priority over built-in defaults. If neither CLI nor config supplies `port`,
  commands requiring a device must fail clearly before opening a backend.
- TUI settings edit only the physical serial profile. Applying a changed profile
  must drop the current backend, reset the host session, reconnect with the new
  profile, and request the manifest again. Saving defaults must be an explicit
  action and must not happen simply because CLI flags were used.
- TUI input mode is manifest-driven. Unfiltered TUI input is read-only and must
  not fall back to channel 1. Filtered TUI input targets the active channel only
  when the manifest descriptor includes `DIRECTION_INPUT`; `LINE` channels send
  complete command lines on Enter and `PASSTHROUGH` channels send key bytes
  promptly. TUI must not raw-write user text to the serial stream outside
  `WMUX` frames.
- Interactive host loops must not block keyboard handling behind the passive
  listener's serial read timeout. `tui` and `passthrough` must consume
  `InteractiveEvent` values from the shared interactive backend rather than each
  owning an ad-hoc serial-read plus terminal-poll loop.
- `--interactive-backend` is optional and defaults to `auto`. On Unix, `auto`
  must prefer the `mio` backend and fall back to `compat` with a visible backend
  label if `mio` cannot open. On non-Unix platforms, `auto` uses `compat`.
  Explicit `mio` is Unix-only and must fail clearly when unsupported.
- The compat backend may use serial/input reader threads and channels. The Unix
  `mio` backend must keep the upper `tui`/`passthrough` business logic identical
  by emitting the same `InteractiveEvent` variants.
- TUI rendering is dirty-driven and capped by target FPS. `--tui-fps` accepts
  only `60` or `120`; absent an override, the host defaults to 60 fps and may
  select 120 fps for confidently detected Ghostty terminals. Scroll input must
  not use large fixed row jumps that visually defeat the configured frame rate;
  prefer one wrapped visual row per wheel event unless a dedicated smooth-scroll
  accumulator renders intermediate positions across frames.
- TUI scroll handling must preserve input responsiveness under bursty terminal
  input. Mouse-wheel bursts and scrollbar drags may be coalesced, but they must
  not require the event loop to process every stale scroll event before handling
  `Ctrl-C`, `Ctrl-]`, or `Esc x`. Avoid doing full wrapped-row recomputation once
  per queued wheel event; cache, coalesce, or defer expensive scroll range work so
  reverse scrolling and quit keys remain responsive while live serial output is
  arriving.
- TUI scrollbar buttons are explicit jump commands, not smooth-scroll deltas.
  The down button must immediately re-enter live-following output by setting
  `scroll_offset = 0` using the latest rendered-row range, even if serial rows
  arrive during the same event burst. The up button must jump directly to the
  oldest visible position. Do not animate button clicks through a long backlog;
  reserve frame-by-frame animation for coarse scrollbar drag targets.
- TUI text selection is application-managed because crossterm mouse capture
  prevents terminal-native selection from seeing ratatui's internal scrollback.
  In `sources/host/wiremux/crates/tui/src/lib.rs`, selection state must track pane
  (`Output`/`Status`), anchor row/column, cursor row/column, active drag state,
  pending clipboard text, and any edge auto-scroll direction. Rendering must
  highlight selected spans from the same wrapped visual rows used by scrollback
  and scrollbar math. Copy actions must operate on the selected application text
  and write through OSC 52 initially; do not assume terminal-native
  `Command-C` or `Ctrl-Shift-C` can copy an app-drawn highlight.
- TUI output selection edge scrolling must be frame-driven after the pointer
  reaches the output pane's top or bottom content row. A single
  `MouseEventKind::Drag` may start `selection_auto_scroll`, but continued
  scrolling must not require additional mouse movement; the main loop should
  schedule the next render deadline while auto-scroll is active and advance the
  selection cursor as the scrollback window moves. Stop edge auto-scroll on
  mouse release, clearing the selection, or reaching the scroll limit.
- Interactive host loops must tolerate recoverable OS interruptions. On Unix,
  terminal resize can deliver `SIGWINCH` while the TUI is blocked in readiness
  polling, terminal event reads, terminal size queries, or serial reads. These
  paths must retry or continue on `std::io::ErrorKind::Interrupted` and must not
  exit with `Interrupted system call`.
- TUI status must continue to show device manifest metadata including
  `DeviceManifest.protocol_version` as the device proto API version. Backend and
  FPS status belong in the existing status area, not a separate debug panel.
  TUI status must also distinguish the requested physical target from the
  resolved connected path.
- TUI settings panels must follow `docs/wiremux-tui-menuconfig-style.md` for
  row grammar, popup behavior, dirty tracking, `Esc` behavior, and the `80x24`
  minimum viewport overlay.
- TUI passthrough display is channel-local stream editing. In
  `sources/host/wiremux/crates/tui/src/lib.rs`, `complete_stream_line()`,
  `backspace_stream_line()`, and `append_stream_segment()` must operate on the
  latest incomplete `OutputLine` for the same channel. Do not use only
  `lines.back_mut()` for passthrough stream editing, because interleaved log or
  telemetry records from other channels can otherwise split a console echo line.
- TUI passthrough prompt rendering must preserve terminal semantics. Empty
  `CR`, `LF`, or `CRLF` echoes are completed prompt history rows, not reusable
  input buffers. If the latest active-channel row is complete and the view is at
  live tail, `sources/host/wiremux/crates/tui/src/lib.rs` may append a virtual current prompt row
  during rendering; this row must not mutate `App::lines` or scrollback history.
  In passthrough mode, place the terminal cursor in the output pane after the
  active channel prompt/echo. Cursor placement must account for visual wrapping
  inside the output pane: previous wrapped rows and the active prompt/echo's
  wrapped offset both affect the terminal row/column. Output visibility and
  scrollbar range must use the same wrapped visual row count, not only logical
  `OutputLine` count, so resizing the TUI narrower cannot hide overflow without
  a scrollbar. The scrollbar renderer must use total rendered rows, output
  content height, and the visible window's first rendered row; a one-cell
  viewport over only `max_scroll_offset + 1` makes the thumb size and motion
  misrepresent the real viewport. Keep the scrollbar thumb visually solid; tiny
  fractional block glyph changes can make terminal scrollbars feel more stalled
  and jittery. Dragging can still be coarse because terminal mouse events report
  character-cell rows, so drag handlers should animate the visible scroll offset
  toward the coarse target across frames instead of applying the full target
  jump in one render. In line mode, place the cursor in the bottom input box.
- Host manifest requests use system channel 0 with
  `payload_type = "wiremux.v1.DeviceManifestRequest"` and empty request payload.
- Device manifest responses use `payload_type = "wiremux.v1.DeviceManifest"`
  and include core-defined channel interaction modes.
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
| device write uses combined input/output direction flags | ESP rejects with `ESP_ERR_INVALID_ARG` before enqueueing |
| host sends oversized input payload | ESP rejects before allocation-heavy work |
| console command succeeds | host can observe response on console channel |
| console command fails | host can observe command error text or return status |
| default USB Serial/JTAG driver missing | mux init installs driver before RX task starts |
| serial disconnects during send/listen | host reconnect behavior remains deterministic |
| config supplies `serial.port` and CLI omits `--port` | host resolves the physical serial profile from config |
| CLI supplies serial profile flags | CLI values override config values for this run only |
| invalid `--data-bits`, `--stop-bits`, `--parity`, or `--flow-control` | CLI parse fails before opening a serial backend |
| TUI applies a changed serial profile | current backend is closed, host session is reset, and reconnect uses the new profile |
| TUI saves defaults | `[serial]` config is written explicitly and not as a side effect of temporary CLI overrides |
| host requests manifest on channel 0 | ESP emits a DeviceManifest response |
| TUI submits input in unfiltered mode | host treats the view as read-only and sends no mux input frame |
| TUI submits input in channel filter mode for an output-only channel | host treats the channel as read-only and sends no mux input frame |
| TUI submits input in channel filter mode for an input-capable channel | host sends mux input frame to active channel |
| `--interactive-backend auto` on Unix and mio opens | active backend label is `mio` |
| `--interactive-backend auto` on Unix and mio fails but compat opens | active backend label reports compat fallback and interactive use continues |
| `--interactive-backend mio` on non-Unix | command fails clearly before entering the interactive loop |
| `--tui-fps 144` | CLI parse fails with allowed values `60` or `120` |
| TUI/passthrough waits for serial data while the user types | keyboard handling is not gated by a long passive-listener read timeout |
| window resize occurs while `wiremux tui` is running | TUI redraws/resizes and does not exit with `Interrupted system call` |
| TUI receives manifest with protocol API version | status displays the device API version from `DeviceManifest.protocol_version` |
| passthrough ch1 echo is interrupted by ch2/ch3/ch4 output before CR/LF | TUI appends later ch1 bytes/backspace edits to the existing incomplete ch1 stream line |
| passthrough command output ends with non-empty line | live-tail render shows the next `chN(name)> ` prompt row and cursor without storing that row in history |
| passthrough command output wraps inside a narrow output pane | scrollbar and cursor row/column follow visual wrapped rows, not the logical `OutputLine` index |
| passthrough empty Enter echoes `CRLF` | TUI stores a completed empty prompt history row and renders the following current prompt row |
| non-passthrough channel emits partial text then another channel emits output | TUI keeps ordinary line-oriented record display; per-channel stream editing is not applied |
| user generates many mouse-wheel events, reaches live tail, then immediately scrolls upward or quits | TUI handles the latest direction/quit key promptly instead of draining stale wheel events first |
| user clicks the scrollbar down button while new output is still arriving | TUI snaps to `scroll_offset = 0` and follows live output on the next render |
| user drags output selection to the top content row and stops moving the mouse | TUI continues scrolling upward on render deadlines until the mouse is released or oldest visible output is reached |
| user drags output selection to the bottom content row and stops moving the mouse | TUI continues scrolling downward on render deadlines until the mouse is released or live tail is reached |
| user releases the mouse after selecting output/status text | TUI keeps the highlight and does not auto-copy by default |
| user presses `Esc` while a selection exists | TUI clears the selection before treating `Esc` as the exit/input-clear prefix |
| user presses `Ctrl-Shift-C`, `y`, `Enter`, or forwarded `Command-C` while a selection exists | TUI copies the selected application text through OSC 52 and keeps the highlight |
| user presses terminal-native copy but the terminal intercepts the key before crossterm sees it | no app event is generated; document/use app-level copy keys instead of relying on native terminal selection |

### 5. Good/Base/Bad Cases

- Good: `listen --channel 1 --line help` executes the ESP console help command and returns console text through channel 1.
- Good: in TUI passthrough mode, device echo `h e l p`, interleaved telemetry,
  backspace echo, `p`, and `CRLF` renders as one ch1 `help` line.
- Good: in TUI passthrough mode, a completed `available commands...\n` response
  is followed visually by a current `ch1(console)> ` prompt row. Empty Enter
  creates a completed empty prompt history row and advances to the next current
  prompt, matching terminal behavior.
- Good: `wiremux tui --interactive-backend auto` on Unix shows `backend mio` in
  status when raw-fd readiness is available, while Windows shows `backend
  compat`.
- Good: TUI status shows `api=<version>` from the received device manifest so
  users can see which proto API the device is using.
- Good: `wiremux tui` can start without `--port` when the global config contains
  `[serial].port`; passing `--port` or `--baud` overrides the config only for
  the current run.
- Good: `Ctrl-B s` opens a menuconfig-style settings panel. Editing data bits
  from `8` to `7` and applying reconnects the current TUI session with the new
  physical serial profile.
- Good: after a long scrollback session, a burst of wheel-down events that
  reaches live tail can be followed immediately by wheel-up or `Ctrl-C`; the TUI
  coalesces stale scroll events and preserves quit-key responsiveness.
- Good: clicking the TUI scrollbar down button during active output immediately
  returns to following live output. Clicking the up button jumps to the oldest
  visible scrollback position without spending many frames animating through a
  large buffer.
- Good: dragging an output selection to the top or bottom content row starts
  continuous auto-scroll that keeps extending the highlighted range even if the
  mouse stays still.
- Good: selecting status text copies exactly the visible status row text through
  the same app-level copy path as output selection.
- Base: telemetry and log channels continue emitting while console input is used.
- Base: `wiremux passthrough --interactive-backend compat` works on every
  platform supported by `serialport`.
- Bad: TUI passthrough append logic edits only the global last line, causing an
  interleaved telemetry record to split a single console input echo into two
  ch1 rows.
- Bad: adding a Unix `mio` implementation by forking the whole TUI or
  passthrough business loop instead of keeping the shared `InteractiveEvent`
  boundary.
- Bad: placing backend/FPS information in a separate TUI panel that hides or
  displaces the existing device manifest/version status.
- Bad: storing virtual channel baud in the physical serial profile. Virtual TTY
  termios compatibility metadata, broker behavior, and channel QoS are separate
  future concerns.
- Bad: saving CLI override values into global config implicitly; save defaults
  must remain an explicit TUI/settings action.
- Bad: processing every queued mouse-wheel event with a fresh full scroll-range
  recomputation while keyboard quit events wait behind the mouse backlog.
- Bad: treating the scrollbar down button as an animated target from a stale
  row range, so new output arrives during catch-up and the TUI remains several
  rows above live tail instead of entering live-following mode.
- Bad: implementing selection edge scroll only inside `MouseEventKind::Drag`,
  which stalls auto-scroll whenever the pointer is held still at the pane edge.
- Bad: relying on terminal-native selection/copy to read ratatui output while
  `EnableMouseCapture` is active; the terminal selection engine cannot see
  application-managed scrollback rows or highlights.
- Bad: treating empty `CRLF` as a reusable incomplete prompt suppresses terminal
  Enter semantics and makes prompt history diverge from shell-like behavior.
- Bad: corrupt host input frame does not call the console handler and does not crash the mux task.
- Bad: `listen` in one process and `send` in another process race on the same serial device; use `listen --line` for single-device verification.

### 6. Tests Required

- Host unit test builds an input frame and verifies the scanner decodes it back into the expected envelope fields.
- Host unit tests cover `listen --line`, `--send-channel`, invalid channel, missing line for one-shot `send`, and macOS `tty` to `cu` preference.
- Host unit tests cover `tui` parser behavior, manifest request frame
  construction, manifest decode with channel interaction modes,
  `--interactive-backend` parsing, invalid backend values, `--tui-fps 60|120`,
  and invalid FPS values.
- Host unit tests cover serial profile config resolution, config-vs-CLI
  precedence, TOML round-trip, valid and invalid serial option values, and
  mapping the resolved profile into serial backend builders.
- Host TUI tests cover opening the settings panel, applying a changed serial
  profile and requesting reconnect, rendering the small-viewport settings
  overlay, and status display for requested target vs connected path.
- Host TUI render tests must assert that the status area includes backend, FPS,
  and device proto API version from `DeviceManifest.protocol_version`.
- Host interactive event-loop tests must cover retry behavior for
  `std::io::ErrorKind::Interrupted`, because unit tests cannot reliably deliver
  real terminal resize signals in CI.
- Host unit tests cover TUI scrollback behavior: live-tail visible-window math,
  mouse wheel pause/resume, append-while-frozen stability, filtered scroll
  counts, empty-input double-Enter recovery, scrollbar row-to-offset mapping,
  drag continuation when the pointer leaves the scrollbar column, and scrollbar
  bottom alignment at `scroll_offset = 0`. Add coverage for scrollbar up/down
  buttons as immediate jumps, including the case where the down button is clicked
  while live output appends. Responsiveness coverage for future event-loop
  changes must include burst coalescing or equivalent behavior where stale
  wheel-down events do not block a later wheel-up or quit key after live tail is
  reached.
- Host unit tests cover TUI application-managed selection: output selection
  highlights and copies visible text, status selection copies status text,
  `Esc` clears selection before exit-prefix handling, `Ctrl-Shift-C` without a
  selection does not quit, explicit copy keeps the selection, OSC 52 output is
  correctly encoded, edge drag scrolls up/down, and edge auto-scroll continues
  on render/frame advancement without requiring another mouse event.
- Host unit tests cover TUI passthrough stream behavior: append until newline,
  split backspace echo, active passthrough output restoring live tail, and
  continuation of an incomplete passthrough channel line across interleaved
  records from other channels. Prompt behavior tests must cover empty `CRLF`
  completing a history row, repeated empty newlines stacking prompt history,
  virtual prompt rendering after completed output, virtual prompt rendering after
  empty Enter, passthrough cursor placement in the output pane, and cursor
  placement after narrow-pane wrapping of completed output plus active echo.
  Scrollback tests must include a narrow-pane case where logical lines fit but
  wrapped visual rows overflow and therefore require a scrollbar.
- Portable C tests cover manifest encoding of channel interaction modes and
  channel-name UTF-8-safe truncation.
- ESP inbound parser test or demo verification covers a valid input frame and bad CRC.
- ESP unit or review-level validation covers `esp_wiremux_write()` rejecting
  combined direction flags and input callbacks receiving payload data that does
  not alias the shared RX buffer.
- Demo-level verification documents the exact commands used to run `help`
  through channel 1, trigger `mux_log` on channel 2, trigger `mux_hello` on
  channel 3, and trigger `mux_utf8` on channel 4.

### 7. Wrong vs Correct

#### Wrong

```text
Host writes raw "help\n" to the serial port and assumes ESP console receives it.
```

#### Correct

```text
Host wraps "help\n" in a channel-1 input MuxEnvelope, then in a WMUX frame with CRC32.
ESP validates the frame and dispatches the payload to the registered console input handler.
```

Correct single-process hardware check:

```text
Host opens the serial port once, sends the input frame with `listen --line`, then keeps decoding output on the same handle.
```

#### Wrong

```text
Unix raw-fd readiness is implemented by maintaining a separate TUI loop from the
compat backend, so passthrough escape handling and TUI status drift by platform.
```

#### Correct

```text
Unix mio and compat both emit `InteractiveEvent` values. TUI and passthrough own
the protocol/session/status behavior above that shared backend boundary.
```

## Scenario: Generic Enhanced Virtual Serial Overlay

### 1. Scope / Trigger

Trigger: changing generic enhanced host mode, virtual serial endpoints, TUI
input ownership, host config handling, or virtual serial output formatting.

This is a host overlay boundary spanning build selection, CLI config resolution,
`interactive` PTY I/O, and TUI controls.

### 2. Signatures

Commands and feature modes:

```bash
cd sources/host/wiremux
cargo run --features generic-enhanced -- tui
cargo run --features all-features -- tui
cargo test --features generic
cargo test --features generic-enhanced
cargo test --features all-features
```

Configuration:

```toml
[virtual_serial]
enabled = true
export = "all-manifest-channels"
name_template = "wiremux-{device}-{channel}"
```

TUI shortcuts:

```text
Ctrl-B v  toggle virtual serial when the host build supports generic enhanced
Ctrl-B o  toggle active filtered channel input owner between host and virtual serial
```

Core implementation points:

```text
sources/host/wiremux/crates/interactive/src/lib.rs
  VirtualSerialBroker
  VirtualSerialEndpointIo::write_output(&mut self, bytes: &[u8]) -> io::Result<usize>
  terminal_text_output_bytes(payload, previous_was_cr, record_delimited)
sources/host/wiremux/crates/tui/src/lib.rs
  handle_virtual_serial_input
  handle_stream_event
```

### 3. Contracts

- `generic` host builds are core-only. They must not activate virtual serial
  from default config, explicit `[virtual_serial]`, or `Ctrl-B v`.
- `generic-enhanced`, vendor enhanced, and `all-features` builds support the
  generic virtual serial overlay. If config omits `[virtual_serial]`, virtual
  serial defaults to enabled in these builds.
- The default export policy is `all-manifest-channels`. Each manifest channel
  gets one endpoint when virtual serial is enabled and the backend supports it.
- Output-only endpoints are read-only. Input-capable endpoints start with
  `VirtualSerialInputOwner::Host`; TUI `Ctrl-B o` can hand ownership to the
  virtual endpoint for the active filtered channel.
- Unix/macOS use PTY endpoints opened through `VirtualSerialEndpointIo`.
  Windows may compile the interface but returns unsupported until a virtual COM
  backend is implemented.
- Text payloads mirrored to terminal clients normalize LF to CRLF. Non-empty
  non-passthrough text mux records that do not already end in CR or LF receive a
  synthetic CRLF record break. Channels advertising
  `CHANNEL_INTERACTION_PASSTHROUGH` preserve byte-stream semantics and must not
  receive synthetic record breaks.
- PTY output backpressure is bounded per endpoint. `WouldBlock`, `TimedOut`, and
  `Interrupted` write results keep pending bytes queued for later flush; queue
  overflow drops only overflow bytes and should not spam TUI status.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| `generic` build with `[virtual_serial] enabled = true` | TUI reports virtual serial unsupported and creates no endpoints |
| `generic-enhanced` build with config omitting `[virtual_serial]` | virtual serial starts enabled after manifest sync |
| User presses `Ctrl-B v` before manifest | broker toggles state and waits for manifest |
| Manifest has output-only channel | virtual endpoint mirrors output and discards writes without host input frames |
| Input-capable channel, owner is host | virtual serial writes are discarded with reason `host owns input` |
| Input-capable channel, owner is virtual serial | virtual serial writes become mux input frames |
| Non-passthrough text payload `mock stress 1` | endpoint receives `mock stress 1\r\n` |
| Non-passthrough text payload ending `\n` | endpoint receives exactly one terminal CRLF ending |
| Passthrough text payload `partial` | endpoint receives `partial` with no synthetic record break |
| PTY write returns `WouldBlock` | unwritten bytes remain queued and later flush without diagnostics noise |
| Platform has no backend | endpoint status is unsupported; TUI continues running |

### 5. Good/Base/Bad Cases

- Good: `cargo run --features generic-enhanced -- tui` exports manifest channels
  and `minicom` shows bursty non-passthrough text records as separate lines.
- Good: passthrough console echo split across mux records remains a byte stream
  in the virtual endpoint, preserving future flashing/tool passthrough behavior.
- Base: `cargo run -- tui` in a generic build keeps virtual serial unavailable
  even if the user saved `[virtual_serial] enabled = true`.
- Bad: appending CRLF to every text payload, including passthrough channels,
  corrupts byte-oriented tools such as future esptool passthrough.
- Bad: treating PTY `EAGAIN` as a user-visible output error makes minicom/screen
  look unreliable during normal terminal backpressure.

### 6. Tests Required

- `interactive` tests must cover LF-to-CRLF normalization, synthetic record
  breaks for non-passthrough text records, passthrough stream preservation, and
  output queue retry after backpressure.
- `tui` tests must cover unsupported generic builds, toggling virtual serial,
  and toggling active channel input ownership.
- `cli` tests must cover default virtual serial config behavior in supported
  and unsupported host modes.
- `tools/wiremux-build check host` must pass because it exercises generic,
  generic-enhanced, vendor, and all-feature host modes.

### 7. Wrong vs Correct

#### Wrong

```text
Virtual serial mirrors text payload bytes exactly for every channel, so multiple
non-passthrough mux records without trailing newline collapse into one minicom row.
```

#### Correct

```text
Virtual serial uses terminal line semantics only for non-passthrough text records
and preserves passthrough channels as byte streams.
```

#### Wrong

```text
Generic host mode reads `[virtual_serial] enabled = true` and creates PTYs.
```

#### Correct

```text
Only generic-enhanced or higher host builds can activate the virtual serial overlay.
```

## Scenario: Release Versioning and ESP Registry Packaging

### 1. Scope / Trigger

Trigger: changing release versions, ESP Component Registry manifests, release
automation, or generated ESP-IDF component package layout.

This is an infra boundary. The source tree keeps `sources/core/c` platform-neutral
while release automation generates ESP Registry packages under `dist/`.

### 2. Signatures

Version files and declarations:

```text
VERSION
sources/host/wiremux/crates/cli/Cargo.toml
sources/host/wiremux/Cargo.lock
sources/vendor/espressif/generic/components/esp-wiremux/idf_component.yml
sources/vendor/espressif/generic/components/esp-wiremux/include/esp_wiremux.h
```

Generator:

```bash
tools/esp-registry/generate-packages.sh

WIREMUX_RELEASE_VERSION=<YYMM.DD.BuildNumber>
WIREMUX_ESP_REGISTRY_NAMESPACE=<namespace>
WIREMUX_ESP_REGISTRY_URL=<registry-url>
WIREMUX_REPOSITORY_URL=<repository-url>
WIREMUX_ESP_REGISTRY_OUTPUT_DIR=<dist/esp-registry path>
```

Generated package roots:

```text
dist/esp-registry/wiremux-core/
dist/esp-registry/esp-wiremux/
```

CI:

```text
.github/workflows/esp-registry-release.yml
```

### 3. Contracts

- Release versions use `YYMM.DD.BuildNumber`, for example `2604.27.2`.
- Same-day patch releases increment `BuildNumber`; a different release date
  updates `YYMM.DD` and resets `BuildNumber` to `1`.
- `VERSION`, host Cargo package version, host lockfile version, ESP component
  manifest version, and `ESP_WIREMUX_VERSION` must match.
- Host crate and ESP component license declarations must be `Apache-2.0`.
- `sources/core/c/CMakeLists.txt` must remain a host-side portable C test/build
  project. Do not convert the core source tree into an ESP-IDF component.
- Registry packages are generated into ignored `dist/esp-registry/` directories.
- `wiremux-core` package contains copied portable core headers/sources, its own
  generated `CMakeLists.txt`, `idf_component.yml`, `README.md`,
  `README_CN.md`, and `LICENSE`.
- `esp-wiremux` package contains copied ESP adapter headers/sources, its own
  generated `CMakeLists.txt`, `idf_component.yml`, `README.md`,
  `README_CN.md`, and `LICENSE`.
- `esp-wiremux` registry manifest depends on
  `<namespace>/wiremux-core` at the same version with `require: public`.
- `esp-wiremux` package includes `examples/esp_wiremux_console_demo` with a
  registry-friendly project `CMakeLists.txt`; do not copy the source-tree
  example's `EXTRA_COMPONENT_DIRS` into the generated package.
- Generated example `main/idf_component.yml` depends on
  `<namespace>/esp-wiremux` at the same version and includes
  `override_path: "../../../"` so local packaged-example builds use the package
  being validated. ESP Registry strips `override_path` when users download the
  example.
- Trusted Uploader entries for the release workflow must leave Branch empty.
  GitHub Release events use tag refs, while the workflow enforces main ancestry
  before upload.
- Root GitHub `README.md` should describe Wiremux as a platform-neutral
  serial-style byte-stream multiplexer; ESP-IDF is the current reference
  integration, not the whole project boundary.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| version does not match `^[0-9]{4}\.[0-9]{2}\.[0-9]+$` | generator exits non-zero |
| `WIREMUX_ESP_REGISTRY_OUTPUT_DIR` points outside `dist/esp-registry` | generator refuses to write |
| source-tree ESP component references `../../../core/c` for local dev | allowed only in source tree |
| generated `esp-wiremux` package references parent-relative core paths | fail review; package must depend on registry `wiremux-core` |
| generated package missing README or LICENSE | fail package validation |
| generated `esp-wiremux` package missing `examples/esp_wiremux_console_demo` | fail package validation |
| generated example keeps source-tree `EXTRA_COMPONENT_DIRS` | fail review; downloaded examples must use registry dependencies |
| Trusted Uploader Branch is `main` for the release workflow | registry OIDC auth fails because release events use tag refs |
| release workflow runs from a non-main commit | workflow must fail before upload |
| release tag version differs from `VERSION` after stripping leading `v` | workflow must fail before upload |
| namespace is pending or unavailable | do not publish production release with that namespace |

### 5. Good/Base/Bad Cases

- Good: `VERSION` is `2604.27.2`, Cargo and ESP declarations match, generated
  packages pack with `compote component pack`, and both tarballs include README,
  README_CN, LICENSE, and `idf_component.yml`.
- Good: `magicdian/esp-wiremux` Registry page shows one example after the patch
  upload because the generated package includes `examples/esp_wiremux_console_demo`.
- Base: local ESP example still builds from `sources/vendor/espressif/generic/examples/...` using
  the source-tree component and parent-relative local core reference.
- Bad: editing `sources/core/c/CMakeLists.txt` to use
  `idf_component_register()` makes future maintainers think the portable core is
  ESP-only.
- Bad: root README introduces Wiremux as ESP32-only even though the core is
  platform-neutral.

### 6. Tests Required

- `tools/wiremux-build doctor`
- `tools/wiremux-build check all`
- `tools/wiremux-build package esp-registry`
- `rg` check that release declarations use the same version.
- `rg` check that generated packages do not contain parent-relative core paths.
- `compote component pack --name wiremux-core` in
  `dist/esp-registry/wiremux-core`.
- `compote component pack --name esp-wiremux` in
  `dist/esp-registry/esp-wiremux`.
- `tar -tzf` check that each package archive includes README, README_CN,
  LICENSE, and `idf_component.yml`.
- `tar -tzf` check that the `esp-wiremux` archive includes
  `examples/esp_wiremux_console_demo/CMakeLists.txt`,
  `examples/esp_wiremux_console_demo/main/idf_component.yml`, and demo source.
- Build the generated registry example under
  `dist/esp-registry/esp-wiremux/examples/esp_wiremux_console_demo` after
  package generation.
- Host checks: `cargo fmt --check`, `cargo check`, and `cargo test` in
  `sources/host/wiremux`.
- Portable core checks when core files changed: configure, build, and run
  `ctest` for `sources/core/c`.
- ESP example build with ESP-IDF when ESP component or packaging behavior
  changed.

### 7. Wrong vs Correct

#### Wrong

```text
Change `sources/core/c/CMakeLists.txt` into an ESP-IDF component and publish it
directly from the source tree.
```

#### Correct

```text
Keep `sources/core/c` portable. Generate `dist/esp-registry/wiremux-core` at
release time with a registry-specific `CMakeLists.txt` and manifest.
```

## Scenario: Source Layout and Build Orchestration

### 1. Scope / Trigger

Trigger: changing source layout, build product definitions, lunch/select
behavior, reproducibility policy, generated build output, or the future
`wiremux-build` tool.

This is a product architecture boundary. Runtime source moves are staged across
PR2 through PR8; documentation and specs should use the target layout even while
commands still reference current paths.

### 2. Signatures

Target source roots:

```text
sources/api/proto
sources/core/c
sources/profiles
sources/host/wiremux
sources/vendor/espressif/generic/components
sources/vendor/espressif/generic/examples
sources/vendor/espressif/s3/README.md
sources/vendor/espressif/p4/README.md
build
build/wiremux-build.toml
build/wiremux-vendors.toml
build/wiremux-hosts.toml
tools/wiremux-build
tools/wiremux-build-helper
.wiremux/build/selected.toml
```

Build configuration files use TOML.

Command signatures:

```bash
tools/wiremux-build lunch
tools/wiremux-build lunch --vendor <skip|all|model-id> --host <generic|generic-enhanced|vendor-enhanced|all-features>
tools/wiremux-build env --shell bash|zsh
tools/wiremux-build check [core|host|vendor|all]
tools/wiremux-build build [core|host|vendor]
```

Selected state payload:

```toml
vendor = "esp32-s3"
host = "vendor-enhanced"
vendor_kind = "model"
vendor_label = "Espressif ESP32-S3"
host_profile = "vendor-enhanced"
vendor_family = "espressif"
vendor_idf_target = "esp32s3"
vendor_example_path = "sources/vendor/espressif/generic/examples/esp_wiremux_console_demo"
selected_at_unix = 1777448133
```

Environment export keys:

```text
WIREMUX_VENDOR
WIREMUX_VENDOR_KIND
WIREMUX_HOST
WIREMUX_HOST_PROFILE
WIREMUX_VENDOR_FAMILY
WIREMUX_IDF_TARGET
WIREMUX_VENDOR_EXAMPLE
```

### 3. Contracts

- `wiremux-build` is a product orchestrator. It may select products, validate
  tools, call Cargo/CMake/`idf.py`, derive environment exports, and collect build
  metadata. It must not replace those underlying tools.
- Planned implementation split: `tools/wiremux-build` is the Python bootstrap;
  `tools/wiremux-build-helper` is the Rust helper.
- Lunch selected state source of truth:
  `.wiremux/build/selected.toml`.
- Optional environment exports from `env --shell bash|zsh` are derived state.
- Configuration priority is `CLI args > selected.toml > product defaults`.
- Environment variables do not normally override selected config.
- `build/wiremux-vendors.toml` owns vendor scopes/models. Built-in scope ids are
  `skip` and `all`; concrete models use `kind = "model"`.
- `build/wiremux-hosts.toml` owns host modes. Valid mode ids are `generic`,
  `generic-enhanced`, `vendor-enhanced`, and `all-features`.
- `tools/wiremux-build lunch` is the primary interactive flow. Non-interactive
  selection uses `lunch --vendor <id> --host <id>`.
- Positional `lunch <device> <host-preset>` arguments are not supported.
- `generic-enhanced` contains vendor-neutral host overlays such as virtual
  serial endpoints. `vendor-enhanced` requires a single concrete vendor model
  and composes generic enhanced behavior with the selected vendor adapter.
  Vendor scopes `skip` and `all` allow `generic`, `generic-enhanced`, or
  `all-features`.
- Initial vendor build/check dispatch supports ESP32-S3. Other listed models may
  remain placeholders but must fail clearly if execution is requested.
- ESP32-S3 vendor dispatch runs in
  `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo` and must
  call `idf.py set-target esp32s3` before `idf.py build`.
- The Python bootstrap must not print the internal `cargo run` helper command
  during normal operation.
- `check` defaults to `all` and is a developer gate. It must not narrow coverage
  based on the selected lunch profile.
- `check host` validates the configured host feature matrix.
- `check vendor` validates every implemented vendor model.
- `build` defaults to the selected project: core, selected host mode, and
  selected vendor target when vendor builds are enabled.
- `build host` and `build vendor` use `.wiremux/build/selected.toml` to resolve
  concrete build variants.
- `vendor-espressif` is not a public or accepted selector; use `vendor`.
- CI is strict per tool and configurable. Local builds are tolerant by default,
  warn on dirty/deviated inputs, and record metadata for diagnostics.
- Future generated paths must be ignored: `/.wiremux/`, `/build/out/`, and
  `/tools/wiremux-build-helper/target/`.

### 4. Validation & Error Matrix

| Case | Required behavior |
|------|-------------------|
| docs mention `sources/core/proto` as target | fail review; target is `sources/api/proto` |
| docs mention `sources/esp32` as target | fail review; target is `sources/vendor/espressif/generic` |
| docs mention `sources/host` as final crate root | fail review; target crate root is `sources/host/wiremux` |
| command docs before migration use current paths | allowed if labeled current/pre-migration |
| selected config differs from env var | selected config wins unless command explicitly documents debug override |
| `lunch --vendor skip --host vendor-enhanced` requested | command fails with deterministic validation error |
| positional `lunch <device> <host-preset>` requested | command fails with migration guidance to `--vendor` and `--host` |
| placeholder vendor model selected for build/check | command fails with deterministic "not implemented yet" style error |
| `env --shell zsh` output is redirected to a file | file contains only `export WIREMUX_*=` lines, no bootstrap trace |
| `tools/wiremux-build check` has no selector | default to `all` |
| `tools/wiremux-build build` has no selector | build core, selected host, and selected vendor scope |
| `vendor-espressif` is used as a selector | fail with a deterministic target error |
| selected vendor is `esp32-s3` and host is `vendor-enhanced` | host build uses Cargo feature `esp32` |
| selected host is `generic-enhanced` | host build uses Cargo feature `generic-enhanced` |
| selected vendor is `all` | build vendor dispatches implemented model entries with `include_in_all = true` |
| selected vendor is `skip` | build vendor prints a warning and does not invoke `idf.py` |
| selected vendor is `esp32-s3` and `idf.py` is available | build vendor runs `idf.py set-target esp32s3` before `idf.py build` |
| `check vendor` runs in CI with `idf.py` available | dispatch every implemented vendor model |
| CI detects dirty/deviated generated output under strict policy | command fails |
| local build detects dirty/deviated input | command warns and records build metadata |

### 5. Tests Required

- Documentation-only PRs must run stale-path `rg` checks across docs/specs and
  `git diff --stat`.
- Runtime layout PRs must additionally run the commands affected by the moved
  paths, such as host Cargo checks, portable C CMake/CTest checks, ESP-IDF
  builds, and release packaging validation.
- Build selector changes must run:
  - `cargo test --manifest-path tools/wiremux-build-helper/Cargo.toml`
  - `cargo fmt --check --manifest-path tools/wiremux-build-helper/Cargo.toml`
  - `cargo check --manifest-path tools/wiremux-build-helper/Cargo.toml`
  - `tools/wiremux-build lunch --vendor esp32-s3 --host vendor-enhanced`
  - `tools/wiremux-build env --shell zsh > /tmp/wiremux-env.out` and assert the
    redirected file contains only shell exports.
  - `tools/wiremux-build lunch esp32-s3 vendor-enhanced` and assert it fails
    with positional migration guidance.
  - `tools/wiremux-build lunch --vendor skip --host vendor-enhanced` and assert
    it fails deterministic validation.
  - `tools/wiremux-build check` and verify it defaults to `all`.
  - `tools/wiremux-build build` after selecting `esp32-s3 + vendor-enhanced`.
  - `tools/wiremux-build check vendor` when `idf.py` is available. If local
    ESP-IDF is not installed, record the local skip and rely on CI/ESP shell for
    the full vendor assertion.
  - `tools/wiremux-build check vendor-espressif` and
    `tools/wiremux-build build vendor-espressif` fail with target errors.

### 6. Good/Base/Bad Cases

- Good: `tools/wiremux-build lunch` in a terminal shows vendor choices first and
  filters host choices after the vendor selection.
- Good: `tools/wiremux-build lunch --vendor esp32-s3 --host vendor-enhanced`
  writes selected state with `vendor_kind = "model"` and `vendor_idf_target =
  "esp32s3"`.
- Good: `tools/wiremux-build env --shell zsh` can be safely used in command
  substitution because stdout contains exports only.
- Good: `tools/wiremux-build check` defaults to the full developer gate rather
  than the selected lunch profile.
- Good: `tools/wiremux-build build` builds core, the selected host mode, and
  selected vendor scope.
- Base: `tools/wiremux-build lunch --vendor all --host generic` is valid and
  vendor build selects implemented `include_in_all` model entries while vendor
  check validates implemented vendor targets.
- Base: `tools/wiremux-build lunch --vendor skip --host all-features` is valid
  and vendor build skips firmware work with a warning.
- Bad: allowing `vendor-enhanced` with `skip` or `all`, because there is no
  single vendor model whose enhanced host feature can be selected.
- Bad: keeping positional `lunch <device> <host-preset>` as a compatibility
  alias, because the build system is still in development and that shape
  preserves the wrong mental model.
- Bad: printing the Python bootstrap `+ cargo run ...` trace during normal
  operation, because it exposes an implementation detail and can break command
  substitution.
- Bad: accepting `vendor-espressif` as a selector, because it leaks the current
  vendor family implementation into the product CLI.

### 7. Wrong vs Correct

#### Wrong

```text
tools/wiremux-build lunch esp32-s3 vendor-enhanced
```

#### Correct

```text
tools/wiremux-build lunch --vendor esp32-s3 --host vendor-enhanced
```

## Testing Requirements

- Host Rust code must pass `cargo test`, `cargo check`, and `cargo fmt --check`.
- Portable C core changes must compile and pass the host-side GoogleTest suite:
  `cmake -S sources/core/c -B sources/core/c/build`, `cmake --build
  sources/core/c/build`, and `ctest --test-dir sources/core/c/build
  --output-on-failure`.
- New portable C core functionality must include related GoogleTest coverage in
  `sources/core/c/tests/wiremux_core_test.cpp` before the change is considered
  complete.
- ESP-IDF code must be built with `idf.py build` in `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo`.
  In CI release validation, `idf.py` presence/version is strict and must not be
  skipped.
- For release validation and packaging, run orchestrator entrypoints (`doctor`,
  `check all`, `package esp-registry`) through `tools/wiremux-build`; direct
  script invocation remains optional for focused packaging diagnostics.
- Any frame layout change must add or update a host parser test.
- Any portable C frame validation change must keep ESP inbound dispatch using `wiremux_frame_decode()`.
- Any ESP encoder change must be manually or automatically validated against the host scanner.
- Any console or full-duplex change must include at least one bidirectional
  console verification path.

## Code Review Checklist

- Are frame constants still byte-compatible between Rust and C?
- Does the frame payload still encode `MuxEnvelope`, not raw text without channel metadata?
- Does mixed-stream parsing preserve ordinary terminal output?
- Are queue/backpressure failures non-fatal?
- Does log redirection avoid recursion?
- Does console API remain future-compatible with passthrough mode?
