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
- Protocol API changes must update `sources/core/proto/api/current/`, freeze a
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
wiremux listen --port <path> [--channel id]
wiremux listen --port <path> [--channel output_id] [--send-channel input_id] --line <text>
wiremux send --port <path> --channel <id> [--line text]
wiremux passthrough --port <path> --channel <id> [--interactive-backend auto|compat|mio]
wiremux tui --port <path> [--interactive-backend auto|compat|mio] [--tui-fps 60|120]
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
    requested: &Path,
    baud: u32,
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
  select 120 fps for confidently detected Ghostty terminals.
- TUI status must continue to show device manifest metadata including
  `DeviceManifest.protocol_version` as the device proto API version. Backend and
  FPS status belong in the existing status area, not a separate debug panel.
- TUI passthrough display is channel-local stream editing. In
  `sources/host/src/tui.rs`, `complete_stream_line()`,
  `backspace_stream_line()`, and `append_stream_segment()` must operate on the
  latest incomplete `OutputLine` for the same channel. Do not use only
  `lines.back_mut()` for passthrough stream editing, because interleaved log or
  telemetry records from other channels can otherwise split a console echo line.
- TUI passthrough prompt rendering must preserve terminal semantics. Empty
  `CR`, `LF`, or `CRLF` echoes are completed prompt history rows, not reusable
  input buffers. If the latest active-channel row is complete and the view is at
  live tail, `sources/host/src/tui.rs` may append a virtual current prompt row
  during rendering; this row must not mutate `App::lines` or scrollback history.
  In passthrough mode, place the terminal cursor in the output pane after the
  active channel prompt/echo. Cursor placement must account for visual wrapping
  inside the output pane: previous wrapped rows and the active prompt/echo's
  wrapped offset both affect the terminal row/column. Output visibility and
  scrollbar range must use the same wrapped visual row count, not only logical
  `OutputLine` count, so resizing the TUI narrower cannot hide overflow without
  a scrollbar. In line mode, place the cursor in the bottom input box.
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
| host requests manifest on channel 0 | ESP emits a DeviceManifest response |
| TUI submits input in unfiltered mode | host treats the view as read-only and sends no mux input frame |
| TUI submits input in channel filter mode for an output-only channel | host treats the channel as read-only and sends no mux input frame |
| TUI submits input in channel filter mode for an input-capable channel | host sends mux input frame to active channel |
| `--interactive-backend auto` on Unix and mio opens | active backend label is `mio` |
| `--interactive-backend auto` on Unix and mio fails but compat opens | active backend label reports compat fallback and interactive use continues |
| `--interactive-backend mio` on non-Unix | command fails clearly before entering the interactive loop |
| `--tui-fps 144` | CLI parse fails with allowed values `60` or `120` |
| TUI/passthrough waits for serial data while the user types | keyboard handling is not gated by a long passive-listener read timeout |
| TUI receives manifest with protocol API version | status displays the device API version from `DeviceManifest.protocol_version` |
| passthrough ch1 echo is interrupted by ch2/ch3/ch4 output before CR/LF | TUI appends later ch1 bytes/backspace edits to the existing incomplete ch1 stream line |
| passthrough command output ends with non-empty line | live-tail render shows the next `chN(name)> ` prompt row and cursor without storing that row in history |
| passthrough command output wraps inside a narrow output pane | scrollbar and cursor row/column follow visual wrapped rows, not the logical `OutputLine` index |
| passthrough empty Enter echoes `CRLF` | TUI stores a completed empty prompt history row and renders the following current prompt row |
| non-passthrough channel emits partial text then another channel emits output | TUI keeps ordinary line-oriented record display; per-channel stream editing is not applied |

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
- Host TUI render tests must assert that the status area includes backend, FPS,
  and device proto API version from `DeviceManifest.protocol_version`.
- Host unit tests cover TUI scrollback behavior: live-tail visible-window math,
  mouse wheel pause/resume, append-while-frozen stability, filtered scroll
  counts, empty-input double-Enter recovery, scrollbar row-to-offset mapping,
  drag continuation when the pointer leaves the scrollbar column, and scrollbar
  bottom alignment at `scroll_offset = 0`.
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
sources/host/Cargo.toml
sources/host/Cargo.lock
sources/esp32/components/esp-wiremux/idf_component.yml
sources/esp32/components/esp-wiremux/include/esp_wiremux.h
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
- Base: local ESP example still builds from `sources/esp32/examples/...` using
  the source-tree component and parent-relative local core reference.
- Bad: editing `sources/core/c/CMakeLists.txt` to use
  `idf_component_register()` makes future maintainers think the portable core is
  ESP-only.
- Bad: root README introduces Wiremux as ESP32-only even though the core is
  platform-neutral.

### 6. Tests Required

- `bash -n tools/esp-registry/generate-packages.sh`
- `tools/esp-registry/generate-packages.sh`
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
  `sources/host`.
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

#### Wrong

```text
Release host v2604.27.3 while ESP component manifest still says 2604.27.2.
```

#### Correct

```text
Update `VERSION`, Cargo files, ESP manifest, and `ESP_WIREMUX_VERSION` together.
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
- ESP-IDF code must be built with `idf.py build` in `sources/esp32/examples/esp_wiremux_console_demo` when ESP-IDF is available.
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
