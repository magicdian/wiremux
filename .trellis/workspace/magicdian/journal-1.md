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

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `8abad12` | (see git log) |
| `9b3c7e9` | (see git log) |

### Testing

- [OK] (Add test results)

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

(Add details)

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

(Add details)

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

- [OK] (Add test results)

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

(Add details)

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
