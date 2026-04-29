# brainstorm: generic enhanced host virtual serial

## Goal

Design and implement a generic enhanced host virtual serial feature that exposes
wiremux channels as `/dev/tty*`-style PTY devices so external tools such as
`minicom` or `screen` can observe and, when permitted, write channel data.

## What I already know

* The feature should forward channel I/O to virtual serial devices on the host.
* The host mode hierarchy should gain `Generic Enhanced Host` under lunch host
  mode selection.
* `Generic Host` should remain limited to protocol/features defined in
  `wiremux-core`.
* Generic enhanced and vendor enhanced functionality should behave like overlay
  modules: all-feature or vendor-enhanced builds load generic enhanced at
  startup, then load the connected device vendor's enhanced overlay after device
  identification.
* The desired runtime design should avoid loading all vendor enhanced features at
  once.
* Architecture updates are expected in `docs/product-architecture.md` if the
  layer model is accepted.
* Device output may be delivered to both the wiremux host UI and the virtual
  serial PTY.
* Device input needs ownership/gating. By default, if a channel is configured
  for input, the wiremux host owns input and virtual serial writes are discarded.
* The TUI should be able to switch input ownership to the virtual serial PTY; in
  that mode the TUI keeps listening to shortcuts and drops ordinary typed input.
* Future ESP32 vendor enhanced support may expose a special aggregate/enhanced
  PTY, allow external tools such as `idf.py flash` to open it, and then
  conditionally take input ownership for OTA or esptool passthrough flashing.
* Existing host build modes are currently `generic`, `vendor-enhanced`, and
  `all-features`, so `generic-enhanced` is a build/profile contract change.
* Existing `docs/product-architecture.md` already places virtual TTY/port bridge
  behavior in the host enhanced layer and keeps vendor flashing outside core.
* Existing `sources/profiles/pty/README.md` reserves a PTY profile contract and
  explicitly leaves OS PTY implementation to host/vendor adapter layers.
* Existing Rust host flow centralizes decoded device records through
  `HostEvent::Record` and builds all host input frames through
  `build_input_frame`.
* Existing TUI input is manifest-driven: unfiltered mode is read-only; filtered
  input is allowed only for manifest input-capable channels; passthrough vs line
  mode comes from manifest interaction metadata.

## Assumptions (temporary)

* This task is a complex architecture/design task and should finish the
  brainstorm PRD before implementation.
* The initial MVP should implement generic enhanced virtual serial only, not
  ESP32 OTA/esptool vendor enhanced behavior.
* A PTY backend is preferred over creating kernel-level serial devices.
* Host-side virtual serial should be a broker/service attached near decoded
  host-session events and host input-frame writing, not a TUI-only renderer
  feature.

## Open Questions

* Which small-PR implementation slice should be attempted first after
  brainstorming: build/profile/docs scaffolding, config+abstraction, or runtime
  broker+PTY plumbing?

## Requirements (evolving)

* Add a generic enhanced host layer/mode concept separate from generic core host
  behavior.
* Implement the virtual serial feature using a brokered generic enhanced service
  boundary, not as a TUI-only add-on.
* Expose configured channels as host PTY devices.
* Mirror device output to both host UI and PTY readers.
* Gate PTY input writes according to per-channel input ownership.
* Preserve extension points for future vendor enhanced overlays such as ESP32
  aggregate flashing PTYs.
* Update architecture documentation for the layer/overlay model.
* Keep virtual TTY/PTY configuration separate from the physical `SerialProfile`.
* Define a cross-platform virtual serial abstraction. The current runtime
  implementation may support only Unix/Linux/macOS PTYs, while Windows keeps a
  placeholder backend/interface with deterministic unsupported errors.
* Drive virtual serial enablement from the global host config. If the config has
  no virtual serial section, the default behavior is enabled.
* When virtual serial is enabled and no explicit channel export list is
  configured, export all channels from the device manifest.
* Output-only manifest channels should be exported as read-only PTY endpoints.
* Input-capable manifest channels should be exported as PTY endpoints whose
  writes are governed by input ownership.
* If the user disables virtual serial in global config, the TUI can still enable
  it immediately through a shortcut and/or settings action for the current
  session.
* Add `docs/matrix/` and maintain two matrix documents in this task:
  `docs/matrix/feature-support.md` for feature/platform support and
  `docs/matrix/tui-shortcuts.md` for TUI shortcut coverage.
* The feature support matrix must list all tracked Wiremux features and show
  Linux, macOS, and Windows support status.

## Acceptance Criteria (evolving)

* [ ] Generic host mode remains core-only.
* [ ] Generic enhanced host mode can expose channel output through PTY devices.
* [ ] Virtual serial writes do not reach the device unless the PTY owns input.
* [ ] TUI can hand channel input ownership between host input and virtual serial
      input.
* [ ] Documentation describes generic enhanced and vendor enhanced overlay
      loading behavior.
* [ ] Virtual serial code is organized behind an abstraction that can later add a
      Windows virtual-port backend without changing channel routing semantics.
* [ ] Non-Unix builds fail virtual serial activation clearly instead of silently
      pretending PTY support exists.
* [ ] Global config can disable virtual serial, and omitted config uses the
      default enabled behavior.
* [ ] With virtual serial enabled and no explicit channel list, every manifest
      channel has an exported endpoint or a clear unsupported/error status.
* [ ] Writes to output-only channel PTYs are rejected or discarded without
      sending host-to-device frames.
* [ ] TUI exposes an immediate way to enable virtual serial for the running
      session when config disabled it.
* [ ] `docs/matrix/feature-support.md` exists and records Linux/macOS/Windows
      support for current and planned Wiremux features.
* [ ] `docs/matrix/tui-shortcuts.md` exists and records current TUI shortcuts,
      contexts, and effects.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint/typecheck pass.
* Docs updated if behavior or architecture changes.
* Rollout/rollback risks considered for host mode/profile changes.

## Out of Scope (explicit)

* ESP32 OTA implementation.
* ESP32 esptool passthrough implementation.
* Device-side protocol changes unless required for generic PTY forwarding.
* Kernel driver or real hardware serial device emulation.

## Technical Notes

### Repository inspection

* `build/wiremux-hosts.toml` owns lunch host mode definitions.
* `tools/wiremux-build-helper/src/main.rs` hard-codes host mode constants,
  usage text, validation, feature mapping, host gate features, and unit tests.
* `sources/host/wiremux/crates/cli/Cargo.toml` currently defines features:
  `generic`, `esp32`, `all-vendors`, and `all-features`.
* `sources/host/wiremux/crates/interactive/src/lib.rs` owns serial config,
  interactive backend selection, terminal event polling, and existing Unix
  `mio` integration.
* `sources/host/wiremux/crates/tui/src/lib.rs` owns current input state and
  keyboard handling. It already separates read-only, line input, and passthrough
  input states.
* `sources/host/wiremux/crates/host-session/src/lib.rs` owns frame decode events
  and input frame construction.
* `docs/source-layout-build.md` and `.trellis/spec/backend/quality-guidelines.md`
  document current host mode ids and must be updated if a new host mode is added.
* Existing docs describe current TUI shortcuts in `docs/zh/host-tool.md` and
  README files, but there is no authoritative matrix directory yet.
* Project guidelines require documentation in English, so new `docs/matrix/*`
  documents should be English unless a later task adds localized mirrors.

### Research Notes

#### What similar tools/libraries do

* Rust `portable-pty` provides a cross-platform PTY abstraction with master and
  slave traits and runtime implementation selection. It is broader than this
  MVP and is part of WezTerm. Reference:
  https://docs.rs/portable-pty/
* Rust `nix::pty` exposes Unix PTY primitives such as `openpty` and `ptsname`,
  but requires enabling the `term` feature and is Unix-only. Reference:
  https://docs.rs/nix/latest/nix/pty/
* The existing host runtime already uses Unix `mio` for serial + terminal event
  polling and already has `nix` in the transitive lockfile through `serialport`,
  but not as a direct dependency.

#### Constraints from this repo/project

* The first useful target is macOS/Linux because `/dev/tty*` PTY behavior is
  Unix-centric; Windows would need a different compatibility surface.
* Existing CLI/TUI command loops own the physical serial handle directly. A
  multi-consumer virtual serial feature needs a host-side broker abstraction so
  output can fan out and input can be arbitrated.
* Core protocol should not learn about host-only PTY implementation details.
* Vendor enhanced ESP32 flashing later needs a way to request or force input
  ownership for a special PTY, but the generic MVP can model ownership without
  implementing flashing policy.

#### Feasible approaches here

**Approach A: brokered generic enhanced service (recommended)**

* How it works: add a generic enhanced host broker that owns decoded output
  fan-out, PTY endpoint lifecycle, and input ownership. TUI/listen subscribe to
  output; PTY readers receive mirrored channel bytes; PTY writers submit input
  through the broker only when ownership permits.
* Pros: clean extension point for future ESP32 aggregate/flashing PTY; avoids
  duplicating routing in TUI/listen/passthrough; keeps core untouched.
* Cons: larger first PR because command loops need to route through a shared
  service boundary.

**Approach B: TUI-attached PTY bridge**

* How it works: create PTYs only inside `wiremux tui`; TUI continues to own
  serial I/O and additionally mirrors active/configured channel output to PTYs.
* Pros: smaller initial runtime change; easiest way to add the user-facing input
  toggle.
* Cons: virtual serial only exists while TUI is running and future `idf.py flash`
  special PTY would have to couple to TUI state or be reworked later.

**Approach C: standalone bridge command**

* How it works: add a new command such as `wiremux bridge-pty` that owns the
  physical serial port, exports PTYs, and optionally runs without TUI.
* Pros: good for automation and external tools; minimizes impact on current TUI.
* Cons: does not satisfy "wiremux host and virtual serial both receive output"
  inside the current TUI unless a broker is still introduced.

### Preliminary architecture recommendation

The layer split is sound if `Generic Enhanced Host` is defined as a host overlay
that contains vendor-neutral host conveniences: virtual serial/PTY, TCP bridge,
broker, capture/replay, and generic transfer orchestration. `Vendor Enhanced`
should compose on top of generic enhanced instead of replacing it. Build-time
features can include the code for selected overlays, while runtime should
instantiate only generic enhanced plus the one vendor adapter selected by the
connected device manifest/profile. That keeps runtime memory bounded by active
services and connected-device adapters rather than all compiled vendor code.

For the future ESP32 flow, the generic broker should expose an input-ownership
API with policy hooks:

* default owner: host/TUI for input-capable channels;
* manual handoff: TUI shortcut grants PTY input ownership for a channel;
* future force/claim: vendor enhanced adapter can claim ownership for a special
  PTY after recognizing a tool/protocol flow, with clear diagnostics and release
  behavior.

### Configuration recommendation

Virtual serial should be modeled as a separate host config section rather than
part of `[serial]`, because `[serial]` describes the physical device transport.
The default should be enabled when `[virtual_serial]` is absent so generic
enhanced builds expose the capability out of the box.

Possible config shape:

```toml
[virtual_serial]
enabled = true
export = "all-manifest-channels"
name_template = "wiremux-{device}-{channel}"
```

Default export behavior:

* omitted `[virtual_serial]` means enabled;
* omitted explicit channel list means export all manifest channels;
* output-only channels are read-only PTYs;
* input-capable channels accept PTY writes only when ownership allows them.

Open detail still needs convergence: how PTY paths are displayed and persisted
in TUI status/settings.

## Decision (ADR-lite)

**Context**: Virtual serial needs to serve multiple host consumers, arbitrate
host-vs-PTY input, and leave a path for future vendor enhanced flows such as an
ESP32 aggregate/flashing PTY.

**Decision**: Use Approach A, a brokered generic enhanced service. The broker
owns decoded channel output fan-out, virtual endpoint lifecycle, and input
ownership. Generic enhanced is a vendor-neutral overlay. Vendor enhanced
adapters compose on top of generic enhanced and may later request ownership for
special PTYs through explicit policy hooks.

**Consequences**: The MVP is larger than a TUI-local bridge, but it avoids
coupling virtual serial to a single UI and creates the right extension point for
standalone bridge commands, TUI integration, and ESP32 flashing flows. The
implementation must define a cross-platform virtual serial trait/interface now:
Unix/macOS/Linux can implement PTY endpoints first; Windows keeps an interface
placeholder and deterministic unsupported behavior until a virtual COM-port
backend is designed.
