# Journal - magicdian (Part 1)

> AI development session journal
> Started: 2026-04-25

---



## Session 1: ESP serial mux one-way milestone

**Date**: 2026-04-25
**Task**: ESP serial mux one-way milestone
**Branch**: `main`

### Summary

Implemented the first ESP serial mux milestone: Rust host listener/decoder with reconnect, channel filtering and CRC diagnostics; ESP-IDF mux component with framed envelope output, log and console adapters, USB Serial/JTAG raw transport, and console_mux_demo; Chinese docs and code-spec updates. Clarified that complete MVP remains bidirectional console operation, created PR5-PR8 follow-up tasks, and archived completed PR1-PR4 milestone tasks.

### Main Changes

- Added project-wide `2604.27.1` version declarations using the
  `YYMM.DD.BuildNumber` convention.
- Added Apache-2.0 release metadata for host Cargo and ESP-IDF component
  manifests.
- Added generated ESP Registry packages for `wiremux-core` and `esp-wiremux`,
  including English/Chinese README templates and release documentation.
- Added GitHub Release CI for trusted ESP Registry uploads from `main`.
- Added GitHub-facing English/Chinese README files with screenshots and device
  integration examples.
- Captured the release/versioning/registry package contract in
  `.trellis/spec/backend/quality-guidelines.md`.

### Git Commits

| Hash | Message |
|------|---------|
| `709fb9a` | (see git log) |
| `8d356dd` | (see git log) |
| `7aef44e` | (see git log) |
| `1bb3749` | (see git log) |
| `adc9372` | (see git log) |
| `9ac534c` | (see git log) |

### Testing

- [OK] `cargo fmt --check`
- [OK] `cargo check`
- [OK] `cargo test`
- [OK] `cmake -S sources/core/c -B sources/core/c/build`
- [OK] `cmake --build sources/core/c/build`
- [OK] `ctest --test-dir sources/core/c/build --output-on-failure`
- [OK] `bash -n tools/esp-registry/generate-packages.sh`
- [OK] `tools/esp-registry/generate-packages.sh`
- [OK] `compote component pack --name wiremux-core`
- [OK] `compote component pack --name esp-wiremux`
- [OK] `idf.py build` in `sources/esp32/examples/esp_wiremux_console_demo`
- [OK] `git diff --check`

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 2: Complete bidirectional mux MVP

**Date**: 2026-04-26
**Task**: Complete bidirectional mux MVP
**Branch**: `main`

### Summary

Implemented PR5-PR8: host serialport send/listen input framing, ESP32 inbound frame parser and channel dispatch, console line-mode MVP, serial backend hardening, docs/spec sync, hardware verification of console/log/telemetry channels, and task archival.

### Main Changes

- Updated `sources/host/src/tui.rs` so the bottom input area renders a dedicated
  subdued passthrough hint: `> passthrough: type in output pane`.
- Kept line-mode input rendering and readonly rendering unchanged.
- Added a TUI render test proving the passthrough hint appears and stale line
  input text is hidden while passthrough mode is active.
- Archived `.trellis/tasks/04-28-passthrough-input-hint` after the feature
  commit.

### Git Commits

| Hash | Message |
|------|---------|
| `addb794` | (see git log) |
| `1f57e59` | (see git log) |
| `0369f00` | (see git log) |
| `7e5fcb3` | (see git log) |
| `3186cc0` | (see git log) |
| `c585e34` | (see git log) |
| `471fbe9` | (see git log) |
| `8aa8d09` | (see git log) |

### Testing

- [OK] `cargo test` in `sources/host`
- [OK] `cargo check` in `sources/host`
- [OK] `cargo fmt --check` in `sources/host`
- [OK] Human TUI validation passed

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 3: Bootstrap project guidelines

**Date**: 2026-04-26
**Task**: Bootstrap project guidelines
**Branch**: `main`

### Summary

Completed Trellis bootstrap guidelines from the existing ESP-IDF/Rust framework: documented backend conventions, no-database boundary, current bidirectional mux contracts, no-frontend boundary, future UI guardrails, and archived the bootstrap task.

### Main Changes

- Reduced the serial read timeout used by `wiremux tui` and `wiremux passthrough`
  from the passive listener's 100ms timeout to a 5ms interactive timeout.
- Kept `wiremux listen` on the existing longer timeout so passive listening
  behavior remains unchanged.
- Updated backend specs to capture the rule that interactive host loops must not
  gate keyboard polling behind long blocking serial reads.

### Git Commits

| Hash | Message |
|------|---------|
| `8abad12` | (see git log) |
| `9b3c7e9` | (see git log) |

### Testing

- [OK] `git diff --check`
- [OK] `cargo fmt --check`
- [OK] `cargo check`
- [OK] `cargo test` (76 tests)
- [OK] Human passthrough test confirmed noticeably improved typing feel.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 4: Wiremux core migration

**Date**: 2026-04-26
**Task**: Wiremux core migration
**Branch**: `main`

### Summary

(Add summary)

### Main Changes

| Area | Summary |
|------|---------|
| Product rename | Migrated public host crate/binary to `wiremux`, ESP-IDF component to `esp-wiremux`, and ESP C APIs to `esp_wiremux_*`. |
| Core architecture | Added `sources/core` with shared proto schema, portable C frame encode/decode, CRC32, envelope encode/decode, manifest encode, and a C smoke test. |
| ESP adapter | Reworked ESP component to consume core protocol APIs while keeping FreeRTOS tasks, queues, USB Serial/JTAG transport, console adapter, and log adapter platform-specific. |
| Host CLI | Kept `listen`, `send`, and single-handle `listen --line` flows under the `wiremux` crate/bin with `WMUX` framing. |
| Specs/docs | Updated backend/frontend Trellis specs and Chinese docs for the new layered `wiremux` architecture, manifest fields, endian semantics, and roadmap boundaries. |
| Validation | Ran portable C smoke test, `cargo fmt --check`, `cargo check`, `cargo test` with 32 tests, `git diff --check`, stale-name search, and TODO/FIXME search. Human verified ESP32 `help` command over channel 1 after rebuild. |

Notes:
- ESP-IDF build was not run in this shell because `idf.py` was unavailable, but the user rebuilt ESP32 firmware externally and verified channel-1 console output.
- Follow-up roadmap remains host structured manifest decode, broker/service mode, PTY endpoints, and future TUI.


### Git Commits

| Hash | Message |
|------|---------|
| `9c54ea1` | (see git log) |
| `494239a` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 5: Core GoogleTest foundation

**Date**: 2026-04-26
**Task**: Core GoogleTest foundation
**Branch**: `main`

### Summary

Added host-side GoogleTest/GoogleMock infrastructure for the portable C core, migrated smoke coverage into 16 gtest cases, documented the CMake/CTest workflow, and archived the completed Trellis task.

### Main Changes

- Added a backend quality contract for TUI scroll responsiveness under bursty
  terminal input.
- Coalesced queued TUI mouse-wheel events into direction runs so stale
  wheel-down bursts do not starve later wheel-up or quit keys.
- Made wheel-down at live tail a cheap no-op and deferred expensive scroll range
  recomputation to cases that need it.
- Added regression tests for down-to-tail-then-up behavior and quit handling
  after a stale wheel-down burst.

### Git Commits

| Hash | Message |
|------|---------|
| `91b88e3` | (see git log) |
| `75234de` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 6: Wiremux batched compression

**Date**: 2026-04-26
**Task**: Wiremux batched compression
**Branch**: `main`

### Summary

Implemented generic Wiremux batch records and compression support across core C, ESP component, demo diagnostics, and Rust host decoding. Hardware validation showed batched heatshrink and LZ4 working at 115200 baud, with LZ4 giving better compression on matched mock payloads. Archived the completed task set after acceptance.

### Main Changes

- Added `generic-enhanced` host mode and wired build helper/Cargo feature mapping.
- Implemented generic virtual serial broker with Unix PTY endpoints, manifest channel export, bounded output queueing, and TUI input ownership controls.
- Preserved generic host as core-only while generic-enhanced/all-features can default virtual serial on.
- Normalized virtual serial terminal output: non-passthrough text records get terminal line breaks; passthrough channels preserve byte streams.
- Added platform feature matrix and TUI shortcut matrix, updated architecture/docs/specs, and bumped release version to `2604.29.7`.

### Git Commits

| Hash | Message |
|------|---------|
| `b4d76b2` | (see git log) |
| `b73749b` | (see git log) |
| `d46b35a` | (see git log) |
| `1b86705` | (see git log) |
| `4596b51` | (see git log) |
| `369d363` | (see git log) |

### Testing

- [OK] `tools/wiremux-build check host`
- [OK] `git diff --check`

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 7: Optimize host listen output

**Date**: 2026-04-27
**Task**: Optimize host listen output
**Branch**: `dev`

### Summary

(Add summary)

### Main Changes

| Area | Summary |
|------|---------|
| Host CLI | Split listen stdout display from diagnostics logging. Filtered channel output now writes raw payload bytes without prefixes or forced newlines; unfiltered output uses concise `chN> ` record prefixes. |
| Diagnostics | Added per-run temp diagnostics files under `std::env::temp_dir()/wiremux/`, with startup marker `wiremux> diagnostics: <path>`. Full frame metadata, batch summaries, CRC errors, and decode errors are written there. |
| Display UX | Preserves CRLF/CR/LF as real terminal line breaks and inserts an own-line `wiremux> continued after partial chN line` marker when switching channels from a partial visible line. |
| Tests/Docs/Specs | Added host display and batch diagnostics tests; updated Chinese host docs and backend specs to match the new output contract. |

**Verification**:
- `python3 ./.trellis/scripts/task.py validate .trellis/tasks/04-26-host-output-ux`
- `cargo test`
- `cargo check`
- `cargo fmt --check`

**Commits**:
- `378842d feat(host): simplify listen output`
- `fe1af9f chore(task): archive 04-26-host-output-ux`


### Git Commits

| Hash | Message |
|------|---------|
| `378842d` | (see git log) |
| `fe1af9f` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 8: Host ratatui TUI and manifest discovery

**Date**: 2026-04-27
**Task**: Host ratatui TUI and manifest discovery
**Branch**: `dev`

### Summary

Added ratatui host TUI, host-initiated DeviceManifestRequest discovery, core channel interaction modes, ESP manifest response mapping, host manifest decode/cache support, docs/spec updates, and tests. Human manually validated the ESP-flashed TUI flow before commit.

### Main Changes

- Added `sources/api/host/generic_enhanced/versions/{current,1}` proto
  snapshots for the host-side generic enhanced capability catalog.
- Documented generic enhanced stable/frozen API rules, virtual serial as the
  first generic enhanced capability, and the future
  catalog-to-registry-to-provider resolution flow.
- Updated backend Trellis specs so future API work knows where host-side
  enhanced snapshots live.

### Git Commits

| Hash | Message |
|------|---------|
| `3edebfb` | (see git log) |
| `bc11c69` | (see git log) |
| `2af4677` | (see git log) |
| `c40f7e9` | (see git log) |
| `6aa67ac` | (see git log) |
| `1e6b44f` | (see git log) |
| `ca9359d` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 9: Host TUI Log Scrollback

**Date**: 2026-04-27
**Task**: Host TUI Log Scrollback
**Branch**: `dev`

### Summary

Added host TUI mouse-wheel log scrollback with frozen historical views, right-side draggable scrollbar, empty-Enter live-follow recovery, docs, specs, and tests.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `6454642` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 10: Clarify ESP enum aliases

**Date**: 2026-04-27
**Task**: Clarify ESP enum aliases
**Branch**: `dev`

### Summary

Kept ESP_WIREMUX public aliases and documented that they intentionally mirror core wire-protocol enum values without runtime conversion.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `1de0107` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 11: Manifest channel name labels

**Date**: 2026-04-27
**Task**: Manifest channel name labels
**Branch**: `dev`

### Summary

Implemented manifest-backed channel display labels using the existing
`ChannelDescriptor.name` field. Host `listen` and TUI now render `chN(name)>`
when manifest metadata is available, while filtered listen output remains raw.

### Main Changes

- Added UTF-8-safe 15-byte channel-name truncation in the portable C manifest
  encoder, with C tests for ASCII, emoji, and invalid UTF-8 cases.
- Added Rust host display label handling for passive `listen` manifest frames
  and TUI manifest state.
- Added ESP32 demo channel 4 with an overlong emoji name and UTF-8 payloads,
  plus a delayed manifest emission so passive listen can learn labels after USB
  serial reset/reconnect.
- Updated docs, backend specs, and `.gitignore`; removed the tracked local
  `.vscode/settings.json`.

### Git Commits

| Hash | Message |
|------|---------|
| `4328303` | (see git log) |

### Testing

- [OK] `cargo fmt --check`
- [OK] `cargo check`
- [OK] `cargo test`
- [OK] `ctest --test-dir sources/core/c/build --output-on-failure`
- [OK] Human ESP32 reset/listen verification passed
- [WARN] `idf.py build` not run in Codex shell because `idf.py` was unavailable

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 12: Versioning and ESP Registry release setup

**Date**: 2026-04-27
**Task**: Versioning and ESP Registry release setup
**Branch**: `dev`

### Summary

Added YYMM.DD.BuildNumber versioning, Apache-2.0 metadata, ESP Registry package generation and Release CI, GitHub README/README_CN, ESP Registry README templates, and release packaging specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `9e4dd0b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 13: Core host session and protocol API versioning

**Date**: 2026-04-27
**Task**: Core host session and protocol API versioning
**Branch**: `dev`

### Summary

Moved host protocol parsing/building into the portable C core, added protocol API version snapshots and compatibility checks, and archived the completed Trellis task.

### Main Changes

| Area | Result |
|------|--------|
| Core C | Added `wiremux_host_session_*` API, protocol API version helpers, manifest compatibility events, batch expansion, and gtest/gmock coverage. |
| Rust host | Statically links the C core via `build.rs`; CLI/TUI runtime now use `HostSession`; old Rust-side frame/envelope/manifest/batch/codec/crc modules were removed. |
| ESP/package | ESP manifest reports current protocol API version; ESP registry package generation includes the new core sources. |
| Specs | Updated backend specs for core host session contracts, protocol version policy, memory model, and required tests. |

Testing completed:
- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cmake --build sources/core/c/build`
- `ctest --test-dir sources/core/c/build --output-on-failure`
- `git diff --check`


### Git Commits

| Hash | Message |
|------|---------|
| `e41a832` | (see git log) |
| `d769242` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 14: Registry example packaging release

**Date**: 2026-04-27
**Task**: Registry example packaging release
**Branch**: `dev`

### Summary

Recorded completion of the 2604.27.2 ESP Registry patch that packages the ESP-IDF console demo as an esp-wiremux registry example and archives the finished task.

### Main Changes

| Area | Result |
|------|--------|
| Release | `fec186f` bumped declarations to `2604.27.2` and updated release documentation. |
| ESP Registry | Package generation now copies `esp_wiremux_console_demo` into `esp-wiremux/examples/`, so the Registry examples tab can show the demo. |
| Docs/specs | Trusted Uploader tag-ref behavior and package-generation expectations are documented. |
| Task state | Archived `04-27-04-27-registry-example-2604-27-2` after confirming the work was already committed. |

Finish-work checks rerun:
- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cmake --build sources/core/c/build`
- `ctest --test-dir sources/core/c/build --output-on-failure`
- `tools/esp-registry/generate-packages.sh`
- `git diff --check`
- verified `dist/esp-registry/esp-wiremux/examples/esp_wiremux_console_demo` exists

External tool gaps in this shell:
- `compote --version` failed: `command not found`
- `idf.py --version` failed: `command not found`


### Git Commits

| Hash | Message |
|------|---------|
| `fec186f` | (see git log) |
| `a94bfc9` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 15: Console passthrough mode 2604.27.3

**Date**: 2026-04-27
**Task**: Console passthrough mode 2604.27.3
**Branch**: `dev`

### Summary

(Add summary)

### Main Changes

| Area | Summary |
|------|---------|
| Protocol/API | Added API v2 passthrough policy metadata and froze `sources/core/proto/api/2/wiremux.proto`; bumped protocol current version to 2. |
| Host | Added `wiremux passthrough`, manifest-driven TUI passthrough input, `Ctrl-]`/`Esc x` attach exit handling, and TUI per-channel passthrough stream rendering across interleaved logs. |
| ESP SDK | Implemented passthrough console binding with configurable backend aliases and passthrough policy emission. |
| Demo/Docs | Updated ESP demo with `mux_console_mode line|passthrough`, docs, release metadata, and version `2604.27.3`. |
| Specs | Captured passthrough stream contracts and tests in backend code-specs. |

**Verification**:
- `cargo fmt --check` in `sources/host`
- `cargo check` in `sources/host`
- `cargo test` in `sources/host` (`58 passed`)
- `cmake --build sources/core/c/build`
- `ctest --test-dir sources/core/c/build --output-on-failure` (`35 passed`)
- `idf.py build` attempted for ESP demo, but `idf.py` is not installed in this environment.


### Git Commits

| Hash | Message |
|------|---------|
| `a89bfcd` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 16: Polish TUI passthrough terminal experience

**Date**: 2026-04-27
**Task**: Polish TUI passthrough terminal experience
**Branch**: `dev`

### Summary

Improved TUI passthrough UX with Esc-x exit parity, terminal-native cursor placement, shell-like prompt rendering, docs, specs, and focused host tests.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `7562092` | (see git log) |
| `1816fd8` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 17: TUI unclassified input read-only

**Date**: 2026-04-28
**Task**: TUI unclassified input read-only
**Branch**: `dev`

### Summary

Made the TUI all-channel view read-only, gated channel input on manifest DIRECTION_INPUT, kept passthrough active only for explicit input-capable channels, updated host docs/specs, and archived the task after user acceptance.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `25d38df` | (see git log) |
| `d83c7a4` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 18: Fix TUI passthrough wrapped output

**Date**: 2026-04-28
**Task**: Fix TUI passthrough wrapped output
**Branch**: `dev`

### Summary

Fixed TUI passthrough rendering so wrapped output rows drive visible scrollback, scrollbar range, and cursor placement; added narrow-pane regression tests and archived the task.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `5c57630` | (see git log) |
| `ec53ce5` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 19: Passthrough input hint

**Date**: 2026-04-28
**Task**: Passthrough input hint
**Branch**: `dev`

### Summary

Added a subdued passthrough hint to the host TUI bottom input area so users can see that typing belongs in the output pane; archived the completed task.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `ee52e2b` | (see git log) |
| `9dcffe1` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 20: Reduce TUI passthrough input latency

**Date**: 2026-04-28
**Task**: Reduce TUI passthrough input latency
**Branch**: `dev`

### Summary

Reduced interactive host serial read timeout for TUI and passthrough so keyboard polling is no longer gated by the passive listener timeout; documented the latency rule in backend specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `d8c807c` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 21: Event-driven interactive backend

**Date**: 2026-04-28
**Task**: Event-driven interactive backend
**Branch**: `dev`

### Summary

Added shared interactive backends for TUI and passthrough, Unix mio support, compat fallback, TUI FPS/status display, docs, tests, and backend spec contracts.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `77e5f0e` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 22: Fix TUI scroll smoothness

**Date**: 2026-04-28
**Task**: Fix TUI scroll smoothness
**Branch**: `dev`

### Summary

Improved host TUI scrollback smoothness by reducing wheel scroll granularity, using viewport-aware scrollbar state, animating coarse scrollbar drag targets across frames, fixing live scrollback status labels, and documenting the TUI scrollbar behavior in backend specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `a61b094` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 23: Fix TUI Scroll Burst Starvation

**Date**: 2026-04-28
**Task**: Fix TUI Scroll Burst Starvation
**Branch**: `dev`

### Summary

Captured the TUI scroll responsiveness contract, then fixed host TUI wheel burst starvation by coalescing queued scroll events, making live-tail wheel-down cheap, and adding regression tests for wheel-up and quit responsiveness.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `7921a22` | docs: capture tui scroll responsiveness contract |
| `10f3346` | fix(tui): coalesce scroll wheel bursts |

### Testing

- [OK] `cargo fmt --check`
- [OK] `cargo check`
- [OK] `cargo test`

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 24: TUI resize EINTR fix

**Date**: 2026-04-28
**Task**: TUI resize EINTR fix
**Branch**: `dev`

### Summary

Fixed TUI window resize exits caused by Interrupted system call. Added retry handling for interactive terminal/serial operations, documented the recoverable EINTR contract in backend specs, and verified with cargo fmt --check, cargo check, cargo test, plus manual resize testing.

### Main Changes

- Added shared `retry_interrupted()` handling for host interactive terminal operations.
- Kept TUI and passthrough interactive loops alive when resize-driven `SIGWINCH`
  interrupts `poll`, terminal reads, terminal size queries, or serial reads.
- Captured the recoverable `ErrorKind::Interrupted` contract in backend error
  handling and quality specs.

### Git Commits

| Hash | Message |
|------|---------|
| `725c6dd` | (see git log) |

### Testing

- [OK] Human verified `wiremux tui` no longer exits during window resize.
- [OK] `cargo fmt --check`
- [OK] `cargo check`
- [OK] `cargo test` - 86 tests passed.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 25: TUI scrollbar button live-follow fix

**Date**: 2026-04-28
**Task**: TUI scrollbar button live-follow fix
**Branch**: `dev`

### Summary

(Add summary)

### Main Changes

| Area | Summary |
|------|---------|
| Spec | Captured the TUI scrollbar button contract: up/down buttons are direct jump commands, while drag targets may animate. |
| TUI | Fixed scrollbar button mouse handling so the up button jumps to oldest visible scrollback and the down button immediately restores live-following output. |
| Tests | Added coverage for button hit mapping, direct jump behavior, and live output appending after the down-button action. |

**Verification**:
- `cargo fmt --check` in `sources/host`
- `cargo check` in `sources/host`
- `cargo test` in `sources/host` (88 tests passed)
- `git diff --check`

**Updated Files**:
- `.trellis/spec/backend/directory-structure.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `sources/host/src/tui.rs`
- `.trellis/tasks/archive/2026-04/04-28-fix-tui-scrollbar-button-live-follow/`


### Git Commits

| Hash | Message |
|------|---------|
| `a3d9df0` | (see git log) |
| `4a05402` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 26: Host TUI selectable output

**Date**: 2026-04-28
**Task**: Host TUI selectable output
**Branch**: `dev`

### Summary

Implemented app-managed selection for host TUI output/status text with OSC52 copy actions, continuous edge auto-scroll while dragging, version 2604.28.1 metadata updates, and backend spec coverage for the TUI selection contract.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `af9f2e0` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 27: Productize source layout and build orchestration

**Date**: 2026-04-29
**Task**: Productize source layout and build orchestration
**Branch**: `dev`

### Summary

(Add summary)

### Main Changes

Implemented and committed a productization refactor for Wiremux.

| Area | Summary |
|------|---------|
| Product architecture | Added product architecture and source-layout/build orchestration docs. |
| API boundary | Moved protobuf API schema from `sources/core/proto` to `sources/api/proto`. |
| Vendor layout | Moved Espressif implementation to `sources/vendor/espressif/generic`, with `s3` and `p4` placeholder directories. |
| Host layout | Moved host tool to `sources/host/wiremux` and added a Cargo workspace skeleton with `crates/wiremux-cli`. |
| Profiles | Added `sources/profiles` skeleton docs for transfer, console, and pty profile contracts. |
| Build orchestration | Added `tools/wiremux-build` Python bootstrap, Rust helper, TOML product config, lunch/env/doctor/check/build/package commands, JSONL metadata, and CI strict idf.py policy. |
| CI/release | Updated ESP registry release workflow and docs to use `wiremux-build`, final paths, and ESP-IDF v5.4.1 installation. |
| Specs | Updated Trellis backend/frontend specs with executable source-layout, build, validation, and release contracts. |

Validation performed:
- User manually verified ESP32 example project build and host build.
- `tools/wiremux-build check all` passed locally: core CMake/CTest passed 35/35, host cargo fmt/check/test passed 97 tests, local vendor check skipped because `idf.py` is not available in this shell.
- `tools/wiremux-build package esp-registry` passed and generated ignored `dist/esp-registry` output.
- `CI=true tools/wiremux-build check vendor-espressif` failed as expected when `idf.py` is missing, confirming CI strict vendor enforcement.
- `cargo check --features generic`, `esp32`, `all-vendors`, and `all-features` passed from `sources/host/wiremux`.
- `tools/wiremux-build lunch core-only device-only` failed clearly; `lunch core-only generic-only` succeeded; `env --shell bash|zsh` emitted selected exports.
- `git diff --check` passed before commit.
- Generated artifacts are ignored: `.wiremux/`, `build/out/`, `dist/`, host/helper targets, and ESP-IDF local outputs.

Commit:
- `505ea91 refactor: productize source layout and build orchestration`

Follow-up note:
- Future host behavior refactoring can now happen inside `sources/host/wiremux/crates/` without blocking this layout migration.


### Git Commits

| Hash | Message |
|------|---------|
| `505ea91` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 28: Proto API schema version path cleanup

**Date**: 2026-04-29
**Task**: Proto API schema version path cleanup
**Branch**: `dev`

### Summary

Removed the duplicate top-level proto schema, moved API snapshots to versions/current and numbered snapshot paths, updated specs/docs/tests, and bumped release metadata to 2604.29.1.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `faa436e` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 29: Release workflow split and 2604.29.2 bump

**Date**: 2026-04-29
**Task**: Release workflow split and 2604.29.2 bump
**Branch**: `dev`

### Summary

Split esp-registry release workflow into validate/publish jobs, gated publish on validate with artifact handoff and matrix upload, bumped version to 2604.29.2, and ignored local esp example .clangd.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f7f6d16` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 30: Interactive wiremux-build lunch

**Date**: 2026-04-29
**Task**: Interactive wiremux-build lunch
**Branch**: `dev`

### Summary

Implemented the new `wiremux-build lunch` UX with interactive vendor/host
selection, TOML-maintained vendor and host dimensions, selected-state exports,
and build/check dispatch integration for ESP32-S3.

### Main Changes

- Added `build/wiremux-vendors.toml` and `build/wiremux-hosts.toml`.
- Replaced positional `lunch <device> <host-preset>` with interactive lunch and
  explicit `--vendor/--host` flags.
- Updated `.wiremux/build/selected.toml` payload shape and `env` exports.
- Routed host check/build through selected Cargo features and vendor check/build
  through selected vendor scope.
- Documented the executable build selector contract in backend code-specs.

### Git Commits

| Hash | Message |
|------|---------|
| `4753201` | (see git log) |

### Testing

- [OK] `cargo fmt --check --manifest-path tools/wiremux-build-helper/Cargo.toml`
- [OK] `cargo check --manifest-path tools/wiremux-build-helper/Cargo.toml`
- [OK] `cargo test --manifest-path tools/wiremux-build-helper/Cargo.toml`
- [OK] `./tools/wiremux-build check host`
- [OK] CLI smoke tests for valid lunch, positional rejection, invalid host/vendor
  validation, env stdout exports, and local vendor-check skip without `idf.py`.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 31: Optimize wiremux-build targets

**Date**: 2026-04-29
**Task**: Optimize wiremux-build targets
**Branch**: `dev`

### Summary

Refined wiremux-build check/build command targets, removed bootstrap cargo trace, updated CI/docs/specs, and validated helper plus product gate behavior.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `c9e674c` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 32: Split host Rust workspace crates

**Date**: 2026-04-29
**Task**: Split host Rust workspace crates
**Branch**: `dev`

### Summary

Split the host Rust workspace into host-session, interactive, tui, and cli crates; preserved the public wiremux binary, bumped version to 2604.29.5, updated host workspace specs, and validated the host build/test matrix.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `1036384` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 33: Host serial config and TUI settings

**Date**: 2026-04-29
**Task**: Host serial config and TUI settings
**Branch**: `dev`

### Summary

Implemented host global physical serial profile config, menuconfig-style TUI settings with runtime reconnect, version bump to 2604.29.6, docs/spec updates, and validation via cargo fmt, cargo test, and tools/wiremux-build check host.

### Main Changes

- Added global host physical serial profile config with CLI override precedence.
- Added TUI settings panel for port, baud, data bits, stop bits, parity, and flow control.
- Added runtime apply/reconnect and explicit save-defaults behavior.
- Adapted the menuconfig-style settings guide into `docs/wiremux-tui-menuconfig-style.md`.
- Updated host docs, Trellis specs, release docs, and version metadata to `2604.29.6`.

### Git Commits

| Hash | Message |
|------|---------|
| `32f3435` | feat(host): add serial profile config and TUI settings |

### Testing

- [OK] User manually verified TUI settings persist to `~/Library/Application Support/wiremux/config.toml`.
- [OK] `cargo fmt --check`
- [OK] `cargo test`
- [OK] `tools/wiremux-build check host`
- [OK] `git diff --check`

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 34: Generic enhanced virtual serial

**Date**: 2026-04-29
**Task**: Generic enhanced virtual serial
**Branch**: `dev`

### Summary

Added generic-enhanced host mode and virtual serial PTY overlay, including manifest channel export, TUI enable/input ownership controls, Unix PTY backpressure handling, terminal line-delimited non-passthrough text output, platform/shortcut matrices, code-spec updates, and version bump to 2604.29.7.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `6d8b98e` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 35: Stable virtual serial aliases

**Date**: 2026-04-29
**Task**: Stable virtual serial aliases
**Branch**: `dev`

### Summary

(Add summary)

### Main Changes

Implemented stable virtual serial aliases and lifecycle cleanup for the generic-enhanced host TUI.

| Area | Summary |
|------|---------|
| Virtual serial aliases | Added stable `tty.wiremux-*` aliases for Unix PTYs, preferring `/dev` and falling back to `/tmp/wiremux/tty` unless `WIREMUX_VIRTUAL_SERIAL_DIR` is set. |
| Reconnect lifecycle | Drops virtual endpoints on physical serial disconnect and normal TUI/profile reconnect so aliases disappear and are recreated after the next manifest sync. |
| macOS client behavior | Added best-effort `revoke(2)` on real PTY slaves during endpoint shutdown so clients like minicom can observe disconnect and reconnect to the stable alias. |
| Endpoint stability | Reuses matching endpoints on duplicate unchanged manifests instead of recreating PTYs. |
| Client close handling | Treats Unix PTY `EIO` after a terminal client exits as nonfatal endpoint disconnect/backpressure so the TUI keeps running. |
| Version | Bumped release version to `2604.29.8` across host crates, ESP component metadata, top-level version files, and release docs. |

Validation:
- Human hardware/minicom validation passed, including wiremux restart reconnect and minicom-first-exit behavior.
- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo check --features generic`
- `cargo check --features generic-enhanced`
- `cargo check --features esp32`
- `cargo check --features all-vendors`
- `cargo check --features all-features`
- `cargo test --features generic`
- `cargo test --features generic-enhanced`
- `cargo test --features all-features`


### Git Commits

| Hash | Message |
|------|---------|
| `15ae983` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 36: Define generic enhanced host contract

**Date**: 2026-04-30
**Task**: Define generic enhanced host contract
**Branch**: `dev`

### Summary

Documented the host-side generic enhanced API stability model, added current and frozen v1 proto snapshots for the generic enhanced capability catalog, and recorded virtual serial as the first generic enhanced capability for future vendor overlay resolution.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `341e1b5` | docs(api): define generic enhanced host contract |

### Testing

- [OK] `protoc` descriptor generation for current generic enhanced proto.
- [OK] `protoc` descriptor generation for frozen v1 generic enhanced proto.
- [OK] `git diff --check`.
- [OK] `python3 ./.trellis/scripts/task.py validate .trellis/tasks/04-30-enhanced-overlay-api-stability`.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 37: Host generic enhanced capability registry

**Date**: 2026-04-30
**Task**: Host generic enhanced capability registry
**Branch**: `dev`

### Summary

Added Rust host generic-enhanced crate with protobuf catalog decoding and registry/provider resolution for virtual serial; wired CLI/interactive support checks through registry; updated API catalog docs and backend specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f4c9d11` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete
