# brainstorm: host ratatui tui channel switching

## Goal

Introduce a cross-platform host-side TUI built with ratatui so debugging can
show muxed serial output interactively and switch channel filters without
restarting commands.

## What I already know

* The feature targets the host side of the project.
* The user wants ratatui for a cross-platform TUI display.
* The TUI should improve debugging efficiency by allowing channel switching
  inside the UI.
* Startup should support specifying the serial port, similar to minicom.
* The TUI should also allow changing port, baud rate, and related settings
  after launch through shortcuts or UI actions.
* A global configuration file is desirable.
* Channel switching should feel similar to tmux window switching.
* Shortcut choices must avoid conflicts with macOS, Ghostty, and common
  terminal/system shortcuts.
* Shortcut plus `0` should enter the unfiltered/default mode.
* Shortcut plus digits should enter the corresponding channel filter mode.
* Current docs describe the host as a non-TUI CLI used for mixed-stream parsing,
  channel filtering, and console line-mode input.
* `docs/zh/host-tool.md` already lists ratatui TUI as a future host-tool plan.
* Current host commands are `wiremux listen` and `wiremux send`; omitting the
  subcommand defaults to `listen`.
* Current `listen` supports `--port`, `--baud`, `--max-payload`,
  `--reconnect-delay-ms`, `--channel`, `--line`, and `--send-channel`.
* Current output behavior is specified: filtered mode prints raw payload bytes;
  unfiltered mode preserves ordinary terminal bytes and prefixes mux records as
  `chN> `.
* Current host code is concentrated in `sources/host/src/main.rs`, with protocol
  parsing/building in library modules.
* `sources/host/Cargo.toml` currently depends only on `serialport = "4"`.
* `sources/core/proto/wiremux.proto` already defines `DeviceManifest` and
  `ChannelDescriptor` with `feature_flags`, channel `directions`,
  `payload_kinds`, `payload_types`, `flags`, and `default_payload_kind`.
* `esp_wiremux_emit_manifest()` currently emits registered channels on system
  channel 0 as `payload_type = "wiremux.v1.DeviceManifest"`.
* The ESP console API already reserves
  `ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH`, but the current implementation returns
  `ESP_ERR_NOT_SUPPORTED` for passthrough mode.
* The current manifest does not explicitly encode console input mode
  (`line` vs `passthrough`) and there is no host-initiated manifest request
  message.
* Core C already defines generic protocol enums such as `WIREMUX_DIRECTION_*`
  and `WIREMUX_PAYLOAD_KIND_*`; interaction/input mode should follow that
  pattern instead of staying ESP-specific.
* Host Rust currently does not appear to decode `DeviceManifest`; manifest
  handling is only logged by payload type/summary today.

## Assumptions (temporary)

* The first implementation should extend the existing Rust host CLI rather
  than introduce a separate binary unless repo inspection suggests otherwise.
* The TUI should coexist with current non-interactive host commands.
* Channel filtering is a host-side output view concern and should not require
  changing device-side protocol framing unless current protocol support is
  insufficient.

## Open Questions

* None. User approved this scope and asked to complete all subtasks before
  acceptance.

## Requirements (evolving)

* Add a ratatui-based host TUI for interactive serial/mux debugging.
* Support unfiltered output mode as the default view.
* Support channel-filtered output modes selected by a shortcut prefix plus a
  digit.
* Use `Ctrl-B` as the MVP shortcut prefix for channel filtering.
* Support `Ctrl-B` then `0` to return to unfiltered mode.
* Support `Ctrl-B` then `1..9` to switch to channel filters 1 through 9.
* Preserve current CLI workflows unless explicitly replaced.
* Keep existing `listen` and `send` behavior available for scripts and
  regression checks.
* Keep host protocol parsing and display-state behavior unit-testable without a
  serial device.
* MVP scope is channel switching first: launch-time port/baud only, TUI output
  display, status/help/exit controls, and prefix-plus-digit channel filtering.
* Runtime port/baud changes and persistent global config are future work, but
  the implementation should leave room for them through separated session,
  config, and UI state modules.
* Include interactive TUI input in the MVP.
* In a specific channel filter mode, TUI input must be sent through that
  channel's mux input path.
* In unfiltered mode, TUI input must be sent through mux input channel 1 by
  default, matching existing `listen --line` behavior.
* TUI input must not raw-write user text directly to the serial stream in this
  MVP.
* Use a hybrid input design: MVP provides an input-line UI that submits complete
  lines over mux input, while internal state should allow future key-stream or
  passthrough input modes.
* Add core/proto capability discovery in this MVP:
  * define core-level input/interaction mode semantics, including at least line
    mode and passthrough/key-stream mode;
  * expose those semantics through manifest/capability payloads;
  * let host request capabilities instead of relying only on unsolicited
    manifest emission;
  * update ESP adapter code to use core-defined interaction mode values rather
    than owning passthrough as an ESP-only enum concept.
* Use a focused manifest request protocol for this MVP:
  * define `wiremux.v1.DeviceManifestRequest`;
  * host sends the request on system channel 0 with
    `payload_type = "wiremux.v1.DeviceManifestRequest"`;
  * device replies with `payload_type = "wiremux.v1.DeviceManifest"`;
  * extend `ChannelDescriptor` with a core-level input/interaction mode field;
  * leave a future path to replace or wrap this with a general
    `ControlRequest` / `ControlResponse` protocol.

## Acceptance Criteria

* [x] A host TUI command can be launched cross-platform.
* [x] The TUI can display incoming mux output.
* [x] The TUI starts in unfiltered mode.
* [x] `Ctrl-B 0` returns to unfiltered mode.
* [x] `Ctrl-B N` switches to channel `N` filtering for supported channel digits.
* [x] Existing non-TUI host commands continue to work.
* [x] The TUI exposes a visible status/help cue for the active filter and core
      shortcuts.
* [x] TUI serial settings are accepted at startup through CLI arguments.
* [x] The TUI supports interactive input from inside the session.
* [x] In channel-filtered mode, submitted input is wrapped as host-to-device mux
      input for the selected channel.
* [x] In unfiltered mode, submitted input is wrapped as host-to-device mux input
      for channel 1.
* [x] The MVP input implementation is line-based but does not block future
      passthrough/key-stream support.
* [x] Host can send a system-channel capability/manifest request after serial
      connection.
* [x] ESP can respond with a core-defined manifest/capability payload that
      includes per-channel input/interaction mode.
* [x] Host TUI can decode and cache the response for display and future input
      mode selection.
* [x] ESP console mode constants are aligned with core-defined interaction mode
      values instead of defining passthrough only at the ESP layer.
* [x] Host manifest request and device manifest response are covered by
      protocol/core or host/ESP tests where practical.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI checks pass.
* Docs/notes updated if command behavior, config behavior, or shortcuts change.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Replacing all existing host CLI behavior with a TUI.
* Device firmware or wire protocol changes unless required by research.
* Adding a database, migration system, or embedded key-value store for config.
* Runtime port or baud-rate switching in this MVP.
* Global config file creation, editing, or persistence in this MVP.
* Active ESP passthrough behavior implementation unless required only as a
  declared core capability value with no active device support.

## Technical Notes

* Task created for brainstorming before implementation.
* Relevant spec files:
  * `.trellis/spec/backend/directory-structure.md`: host Rust layout, CLI
    boundaries, and protocol/display contracts.
  * `.trellis/spec/backend/quality-guidelines.md`: required host tests and
    bidirectional console invariants.
  * `.trellis/spec/backend/error-handling.md`: mixed-stream scanner errors must
    remain deterministic and non-fatal.
  * `.trellis/spec/backend/database-guidelines.md`: no database; if persistent
    host config is added, define durability boundary and use explicit files.
  * `.trellis/spec/backend/logging-guidelines.md`: host diagnostics stay concise
    on stdout and detailed in diagnostics files.
* Likely files to modify:
  * `sources/host/Cargo.toml`: add ratatui/crossterm and lightweight config
    dependencies if chosen.
  * `sources/host/src/main.rs`: CLI parser and command routing.
  * New host modules under `sources/host/src/`: likely `tui.rs`, `config.rs`,
    and serial/session helpers to avoid keeping UI state in `main.rs`.
  * `docs/zh/host-tool.md`: document TUI launch, shortcuts, config path, and
    preserved non-TUI commands.

## Research Notes

### What similar tools do

* Minicom uses `Ctrl-A` as an escape prefix, then a command key; `Ctrl-A z`
  opens help and `Ctrl-A o` opens configuration. Its prefix can be changed in
  configuration.
* Minicom also has a Meta/ALT command-key mode. Its manual says the Meta mode
  assumes the Meta key sends an ESC prefix, not the high-bit variant.
* tmux uses a prefix key, `Ctrl-B` by default, followed by command keys.
  `prefix + 0..9` selects windows by number.
* Ratatui supports a crossterm backend. Ratatui's default features enable the
  crossterm backend, which is suitable for Linux, macOS, and Windows terminals.
* Crossterm exposes key events with `KeyCode` and `KeyModifiers`, including
  `CONTROL`, `ALT`, `SHIFT`, and character digits. Advanced modifier
  disambiguation exists, but depending on terminal support for Super/Meta would
  be riskier than plain Ctrl/Alt.
* On macOS, Terminal.app has a profile setting to make Option act as Meta; Apple
  documents this as useful for X11 and some text editors.
* Ghostty documents `macos-option-as-alt`: Option normally may produce Unicode
  characters on macOS, while treating it as Alt makes terminal programs that
  expect Alt work but can break Unicode input through Option.
* Source references:
  * https://man7.org/linux/man-pages/man1/minicom.1.html
  * https://man7.org/linux/man-pages/man1/tmux.1.html
  * https://support.apple.com/guide/terminal/trmlkbrd/mac
  * https://ghostty.org/docs/config/reference

### Constraints from this repo/project

* The current CLI has no async runtime and no TUI/event loop dependency.
* The serial port is exclusive in practice, so the TUI should own one serial
  handle and handle both input/output on that handle.
* Existing `listen --line` single-handle behavior must not regress.
* Config should be a small user config file, not a database-backed feature.
* Channel filtering for batch frames must apply to decoded inner records, not
  the channel-0 batch wrapper.

### Feasible approaches here

**Approach A: Add `wiremux tui` as a sibling command** (Recommended)

* How it works: keep `listen`/`send` unchanged; add `wiremux tui --port ...`
  with the same serial settings and an interactive ratatui session.
* Pros: low regression risk for scripts, clear docs, easy to test parser
  behavior, lets TUI code evolve independently.
* Cons: users must learn a new subcommand; some serial/session code should be
  extracted to avoid duplication.

**Approach B: Make `listen` enter TUI with `--tui`**

* How it works: add `wiremux listen --tui --port ...`; existing `listen`
  remains plain unless the flag is present.
* Pros: reuses the mental model that TUI is an enhanced listener.
* Cons: `listen` parser/behavior gets more overloaded; harder to keep output
  guarantees obvious.

**Approach C: Replace interactive `listen` with TUI by default**

* How it works: `listen` becomes TUI-oriented unless a `--plain` flag is used.
* Pros: most discoverable once the TUI exists.
* Cons: highest compatibility risk and not aligned with existing docs/tests that
  rely on plain stdout output.

### Capability discovery options

**Option A: No proto change in MVP; observe existing manifest if emitted**
(Recommended)

* How it works: TUI uses line input by default, may parse/cache
  `DeviceManifest` if it appears on channel 0, and exposes architecture hooks for
  future input modes. Device mode detection is future work.
* Pros: keeps this task focused on the TUI/channel-switching UX; avoids a
  cross-language protocol update before passthrough exists.
* Cons: TUI cannot automatically choose line vs passthrough based on device
  capability in the MVP.

**Option B: Add manifest channel-mode flags, no request path**

* How it works: define channel flags such as line-input and passthrough-input,
  set them in ESP channel descriptors, and let host use them when manifest is
  observed.
* Pros: small protocol surface; uses existing `ChannelDescriptor.flags`.
* Cons: still relies on device emitting manifest; requires cross-language C/Rust
  updates and tests.

**Option C: Add explicit host-initiated capability request/response**

* How it works: define control request/response messages on system channel 0,
  host asks for manifest/capabilities after connect, and device responds.
* Pros: clean long-term control plane; avoids requiring periodic manifest
  emission.
* Cons: larger cross-layer change: proto messages, C encode/decode or dispatch,
  ESP system-channel input handling, Rust request/response handling, docs and
  tests.
* Decision: chosen. The user wants host-requested capabilities in this MVP and
  wants input/passthrough mode semantics moved into the core protocol layer.

### Control-plane protocol options

**Option 1: Focused `DeviceManifestRequest`** (Chosen)

* How it works: host sends `wiremux.v1.DeviceManifestRequest` on channel 0 and
  device replies with the existing `wiremux.v1.DeviceManifest` payload type,
  extended with per-channel core interaction mode.
* Pros: explicit, small, and aligned with the immediate capability discovery
  need.
* Cons: future non-manifest control messages may need a more general wrapper.

**Option 2: General `ControlRequest` / `ControlResponse`** (Future)

* How it works: introduce request IDs, request kinds, status codes, and response
  payloads for all control-plane operations.
* Pros: best long-term control-plane abstraction.
* Cons: larger protocol and implementation surface than this TUI/capability MVP
  needs.

**Option 3: Special flag or empty control payload**

* How it works: host uses a control flag or empty system-channel payload to
  trigger manifest emission.
* Pros: smallest implementation.
* Cons: weak protocol semantics and less self-describing diagnostics.

### Shortcut candidates

**Candidate A: `Ctrl-B` prefix, then digit** (tmux-like)

* Pros: directly matches tmux `prefix + 0..9`; uncommon at the OS/window-manager
  level; simple to document.
* Cons: conflicts with users running inside tmux unless tmux prefix is changed
  or sent through.
* Decision: chosen for MVP default.

**Candidate B: `Ctrl-A` prefix, then digit** (minicom-like)

* Pros: familiar for serial terminal users; aligns with minicom-style config and
  help flows.
* Cons: conflicts with shell/readline line-start muscle memory if input mode
  later becomes richer; conflicts with minicom/screen habits when nested.

**Candidate C: `Alt-0..9` direct switching**

* Pros: no two-step prefix state; visually easy to map to channels.
* Cons: Alt handling varies by terminal and shell settings; macOS terminal apps
  may map Option/Alt to text input or system shortcuts, making it less reliable.

**Candidate D: `Esc`, then digit**

* Pros: works naturally with terminals that encode Meta as ESC prefix; mirrors
  the macOS minicom behavior the user observed (`Esc`, then command key).
* Cons: ESC is also a universal cancel/back-out key in TUIs; using it as the
  primary channel prefix can make modal interactions feel less standard unless
  the TUI carefully disambiguates timeout/cancel behavior.

## Decision (ADR-lite)

**Context**: The TUI needs channel switching that is fast, terminal-portable, and
unlikely to conflict with macOS/Ghostty/system shortcuts. The user wants
tmux-like channel switching and has observed minicom's macOS Meta/ESC behavior.

**Decision**: Use `Ctrl-B` as the MVP prefix. `Ctrl-B 0` selects unfiltered mode,
and `Ctrl-B 1..9` selects channel filters 1 through 9.

**Consequences**: This matches tmux-style window selection and avoids
Alt/Option portability issues on macOS. Users running wiremux inside tmux may
have a prefix conflict; the MVP accepts this, and later config support should
allow changing the prefix.

## Decision (ADR-lite): TUI input routing

**Context**: The current project contract requires host-to-device input to use
the same `WMUX` frame and `MuxEnvelope(direction=input)` format as existing
`listen --line` and `send`. Raw serial writes would bypass mux routing and may
not reach the ESP console path.

**Decision**: The MVP TUI never raw-writes user text. In unfiltered mode,
submitted input goes to mux channel 1. In channel-filtered mode, submitted input
goes to the active filtered channel.

**Consequences**: This preserves the protocol contract and gives predictable
single-handle behavior. Future passthrough mode can be added deliberately if the
ESP side supports it.

## Decision (ADR-lite): TUI input interaction

**Context**: The user wants complete interactive input, but the current ESP
console adapter is line-mode and passthrough mode is only reserved in the public
API.

**Decision**: Use a hybrid design. The MVP UI provides line-based interactive
input because that matches current device behavior. The implementation should
separate input-mode state from rendering/serial transport so a future key-stream
or passthrough mode can be selected from device capabilities.

**Consequences**: The MVP remains usable for current console commands while
preserving a path toward richer terminal-style interaction.

## Decision (ADR-lite): Capability discovery and input mode ownership

**Context**: TUI input behavior should eventually follow device capabilities.
The current ESP API reserves passthrough mode, but the core protocol/manifest
does not yet describe channel interaction mode and the host cannot request
capabilities on demand.

**Decision**: Include explicit capability discovery in this MVP. Define
line-mode and passthrough/key-stream mode at the core/proto layer, surface it in
manifest/capability responses, and add a host-initiated system-channel request.
ESP console mode should align to core-defined values rather than owning
passthrough as an ESP-only concept.

**Consequences**: The task becomes cross-layer: proto, portable C core, ESP
adapter, Rust host, TUI, docs, and tests all need coordinated updates. The MVP
can still run line-mode only while accurately declaring passthrough unsupported
or future-capable.

## Decision (ADR-lite): Manifest request protocol

**Context**: The host needs a way to request capabilities without relying on
unsolicited device manifest emission, but a full control-plane framework would
increase this task's scope.

**Decision**: Add a focused `wiremux.v1.DeviceManifestRequest` payload for this
MVP. The host sends it on system channel 0 and the device replies with
`wiremux.v1.DeviceManifest`. Keep field numbering and message design compatible
with a future general `ControlRequest` / `ControlResponse` wrapper.

**Consequences**: Capability discovery is explicit and testable now, while the
long-term control-plane design remains open.

## Implementation Plan

### PR1: Core/proto capability model

Goal: make input interaction mode a core protocol concept.

Tasks:

* Add a core/proto enum for channel interaction/input mode with at least:
  unspecified, line, and passthrough/key-stream.
* Extend `ChannelDescriptor` with repeated or declared interaction mode support.
* Add `DeviceManifestRequest` to the proto schema.
* Add matching C core constants/types in `wiremux_manifest.h`.
* Update manifest encoding length/write paths and C core tests.
* Keep field numbers additive and backward compatible.

Acceptance:

* C core manifest test covers interaction mode encoding.
* Existing manifest encoding tests continue to pass.
* Docs/specs identify the new manifest request and interaction mode contract.

### PR2: ESP manifest request/response and mode mapping

Goal: let the device respond to a host-requested manifest and align console mode
ownership with core mode values.

Tasks:

* Replace or alias ESP console mode values to core-defined interaction modes.
* Record each registered channel's interaction mode in the ESP channel config or
  adapter registration path.
* Include interaction mode in emitted channel descriptors.
* Decode `DeviceManifestRequest` received on system channel 0.
* Respond by emitting `DeviceManifest`.
* Preserve existing unsolicited startup/demo manifest behavior.

Acceptance:

* Existing line-mode console behavior still works.
* System-channel manifest request produces a manifest response.
* Passthrough can remain unsupported, but the declared mode is core-owned.

### PR3: Rust host manifest protocol support

Goal: teach host code to request and understand device capabilities.

Tasks:

* Add Rust encode/decode support for `DeviceManifestRequest` and
  `DeviceManifest`/`ChannelDescriptor` interaction mode fields.
* Build a host-to-device manifest request frame for system channel 0.
* Decode manifest responses from normal and batched stream paths.
* Cache the latest manifest in host session state for TUI display and future
  input-mode selection.
* Add parser support for `wiremux tui`.

Acceptance:

* Host tests cover manifest request frame construction.
* Host tests cover manifest decode, including channel interaction mode.
* Existing `listen` / `send` parser tests still pass.

### PR4: Ratatui TUI

Goal: provide the usable interactive debugging experience.

Tasks:

* Add ratatui/crossterm dependencies.
* Implement `wiremux tui --port <path> [--baud ...] [--max-payload ...]
  [--reconnect-delay-ms ...]`.
* Build a serial session loop that owns one serial handle, reads decoded output,
  sends manifest requests after connect, and writes input frames.
* Build a TUI with output viewport, status/help bar, active filter indicator,
  manifest/capability summary, and bottom input line.
* Implement `Ctrl-B 0` and `Ctrl-B 1..9` channel filter switching.
* Submit input lines to channel 1 in unfiltered mode and to the active channel
  in filtered mode.
* Keep UI state and serial/protocol state separated so future config and
  passthrough work has clear insertion points.

Acceptance:

* TUI starts and exits cleanly.
* Filter switching updates visible state and output filtering.
* Input line submits mux input frames to the expected channel.
* Non-TUI `listen` / `send` behavior remains unchanged.

### PR5: Docs, specs, and final verification

Goal: make the new behavior documented and checked.

Tasks:

* Update `docs/zh/host-tool.md` and any getting-started/troubleshooting sections
  impacted by `wiremux tui`.
* Update backend specs if command contracts, manifest contracts, or test
  requirements changed.
* Run `cargo fmt --check`, `cargo check`, and `cargo test` in `sources/host`.
* Run C core configure/build/test after core C changes.
* Run ESP demo build if ESP-IDF environment is available.

Acceptance:

* Required automated checks pass or any unavailable environment is clearly
  reported.
* PRD acceptance criteria are satisfied.
