# Wiremux host global config and TUI target switching

## Goal

Add host-side global configuration and TUI runtime switching for the real wiremux physical serial profile. This round should improve the day-to-day host/TUI workflow and prepare the connection-target model needed by later broker or virtual serial work, without implementing virtual TTY features yet.

## What I already know

* Candidate 1: global configuration plus TUI runtime switching for serial target and baud rate.
* Candidate 2: virtual serial device support so external programs can open channel paths, with data flowing device -> wiremux core -> virtual tty -> minicom/screen.
* Candidate 2 also needs a transmit-control mechanism: when TUI input is enabled, external input is discarded; when external input is allowed by TUI config, TUI does not listen for channel text input and only listens for internal shortcuts.
* Only one task should be selected for this round.
* The host workspace is split into `host-session`, `interactive`, `tui`, and `cli` crates.
* Current commands require `--port`; `--baud` defaults to 115200 in CLI parsing.
* TUI currently owns one interactive backend connection at a time and reconnects to the original `TuiArgs { port, baud }`.
* TUI already supports channel filtering and input state switching (`ReadOnly`, `Line`, `Passthrough`) based on manifest channel capabilities.
* Project docs list "TUI global config file, runtime port/baud switching, configurable shortcuts" before service/broker and Unix PTY exposure.
* Project docs describe virtual TTY as a later feature built on service/broker mode, where one host process owns the real serial port and distributes channels to frontends.

## Assumptions (temporary)

* The project currently has a Rust host tool and TUI.
* Serial target and baud-rate selection are likely foundational for later virtual TTY work.
* Virtual TTY support is likely broader in scope because it involves OS-specific PTY behavior, channel ownership, and input arbitration.
* A first virtual TTY MVP should probably be Unix-only, with Windows virtual COM explicitly deferred.

## Requirements

* Add a host global config source for real device target defaults.
* Store physical transport settings only in this MVP, at minimum:
  * serial port path
  * baud rate
  * data bits
  * stop bits
  * parity
  * flow control
* Keep explicit CLI arguments as the highest-priority override.
* Allow `wiremux tui` to start from config when `--port` is not provided, while still accepting `--port`, `--baud`, and serial option overrides.
* In TUI, show the current configured/requested target and the connected resolved path.
* In TUI, allow runtime switching of serial target, baud rate, data bits, stop bits, parity, and flow control.
* Add a complete settings panel for editing the physical serial profile, using an OpenWrt/menuconfig-inspired single-column style.
* Copy/adapt the referenced menuconfig style matrix into this repository's `docs/` as a Wiremux-specific TUI settings style guide.
* Runtime target changes should disconnect the current backend and reconnect using the new target settings.
* Follow a minicom-inspired configuration model: interactive target settings, explicit save action, and CLI one-shot overrides.
* Do not store virtual channel baud settings in this MVP.
* Preserve a path toward future broker/virtual TTY work without overbuilding it in this round.

## Acceptance Criteria

* [x] Current host/TUI/core structure has been inspected.
* [x] Both candidate tasks are compared by dependency order, risk, and user value.
* [x] One task is recommended with a clear rationale.
* [x] MVP scope and explicit out-of-scope items are documented.
* [x] A default config file can provide port, baud, data bits, stop bits, parity, and flow control for `wiremux tui`.
* [x] CLI `--port`, `--baud`, and serial option flags override config values for the current run.
* [x] TUI can change port, baud, data bits, stop bits, parity, and flow control at runtime and reconnect without restarting the process.
* [x] TUI can save the selected physical serial profile to global config through an explicit action.
* [x] TUI settings panel follows the referenced menuconfig style constraints for row grammar, popup behavior, dirty tracking, and minimum viewport handling where applicable.
* [x] Wiremux-specific menuconfig style documentation exists under `docs/`.
* [x] Unit tests cover config precedence, serial option mapping, and target-switch state transitions where practical.

## Definition of Done (team quality bar)

* Tests added/updated if implementation proceeds.
* Lint / typecheck / CI green if implementation proceeds.
* Docs/notes updated if CLI/TUI behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Implementing both candidate tasks in this round.
* Virtual TTY / PTY device creation.
* Broker/service mode.
* External program input arbitration.
* Virtual channel baud configuration.
* Channel QoS policy.
* Windows native virtual COM support.
* Full minicom feature parity, including dialing directory, modem init strings, scripts, capture management, and file transfer UI.

## Technical Notes

* Initial PRD seeded before repository inspection, per brainstorm workflow.
* Relevant files inspected:
  * `sources/host/wiremux/crates/cli/src/args.rs`: command shapes, required `--port`, baud default.
  * `sources/host/wiremux/crates/cli/src/serial.rs`: serial open helper and macOS tty/cu candidate logic.
  * `sources/host/wiremux/crates/interactive/src/lib.rs`: shared interactive backend, port candidate resolution, terminal/serial event loop.
  * `sources/host/wiremux/crates/tui/src/args.rs`: TUI args currently carry static port/baud.
  * `sources/host/wiremux/crates/tui/src/lib.rs`: TUI reconnect loop, input state routing, channel filtering, tests.
  * `docs/zh/host-tool.md`: explicit roadmap ordering.
  * `docs/product-architecture.md`: virtual TTY is part of host enhanced responsibilities and profile-driven future architecture.
  * `sources/profiles/README.md`: `pty/` is documentation-only skeleton; no runtime profile implementation yet.
* Design note from discussion: physical serial ports, OS virtual TTY endpoints, and logical channel QoS should be modeled as separate concepts. Physical serial baud configures the real transport. Virtual TTY baud is usually terminal metadata or compatibility surface, not actual link capacity. QoS should be expressed as explicit channel scheduling/backpressure policy, not inferred from a TTY baud setting unless a future compatibility mode deliberately maps baud-like hints into QoS weights.
* Virtual TTY baud design note: PTY data transfer does not use UART-style bit timing, so an external `screen`/`minicom` baud value normally does not prevent or cause garbled bytes. Garbled output on PTY is more likely caused by line discipline, raw/cooked mode, echo, CR/LF handling, terminal encoding, or the real device-side serial baud being wrong. A virtual baud may still be useful as default termios metadata or compatibility/display config for tools that expect a baud field.

## Research Notes

### Constraints from current repo

* Candidate 1 can mostly stay inside existing CLI/TUI/interactive boundaries: add config resolution, surface current target in TUI state, and teach the reconnect loop to reopen with updated target settings.
* Candidate 2 introduces new process/IO topology: real serial owner, virtual PTY endpoints, per-channel routing, external input arbitration, and likely a new command or broker mode.
* Existing TUI input routing is already explicit enough to support a future ownership mode, but no shared broker abstraction exists yet.
* Current dependencies do not include config parsing (`serde`, `toml`, directories) or PTY libraries. Either task may add deps, but PTY support is more OS-specific and operationally risky.

### Feasible approaches here

**Approach A: Global config + TUI runtime target switching** (Recommended)

* How it works: add a host config source for default port/baud, keep CLI overrides highest priority, then add TUI commands/menu to switch target and force reconnect using the selected port/baud.
* Pros: fixes daily workflow friction, aligns with documented roadmap, creates reusable connection-target state for later broker/PTY work.
* Cons: does not immediately let external tools like `minicom` attach to channels.

**Approach B: Virtual TTY MVP**

* How it works: add a Unix PTY endpoint for one or more channels, route device output into PTY master, and gate PTY input vs TUI input with an ownership/control policy.
* Pros: high product value for terminal-native workflows and external tool compatibility.
* Cons: likely needs broker/service ownership first, has OS-specific PTY semantics, introduces contention rules, and is harder to test without integration harnesses.

**Approach C: Broker/input-ownership foundation only**

* How it works: skip user-facing virtual TTY for now and first refactor host/TUI around a shared serial owner plus explicit channel input ownership.
* Pros: reduces future virtual TTY risk.
* Cons: less visible user value than Approach A and may be too much infrastructure without an immediate feature.

## Recommendation

Prioritize Candidate 1 / Approach A for this round.

Rationale:

* It is already listed earlier in the project roadmap.
* It improves the current TUI workflow immediately.
* It creates the target-selection state that virtual TTY/broker mode will need anyway.
* It has a smaller, more testable blast radius than virtual PTY input arbitration.
* Virtual TTY should follow after the host has a clearer connection ownership model.

## Decision (ADR-lite)

**Context**: The initial choice was between global config/TUI target switching and virtual serial devices. Further discussion clarified that physical serial baud, virtual TTY baud, and future channel QoS should remain separate concepts.

**Decision**: Implement global config and TUI runtime switching for the physical serial profile first. Do not implement or persist virtual channel baud settings in this MVP.

**Consequences**: The first implementation delivers immediate usability improvements and creates a reusable target model. Virtual TTY remains a later feature, likely after a broker/connection-owner design exists. Future virtual TTY endpoints may expose termios compatibility settings, but those settings should not be confused with real transport baud.

## Minicom Notes

* Minicom exposes one-shot command-line device and baud overrides, for example `--device` and `--baudrate`.
* Minicom also supports entering setup mode and editing "Serial port setup"; common setup flows edit the serial device and communication parameters, then save a default profile.
* For wiremux, the relevant pattern is: runtime edit target settings, save defaults explicitly, and let CLI overrides remain temporary unless saved.
* Wiremux should not copy unrelated minicom features in this MVP.
* Physical serial option modeling is in scope because some devices require non-default data bits, stop bits, parity, or flow control.
* Additional minicom features considered:
  * Capture/logging exists in minicom, but wiremux already has diagnostics and should not add general capture in this config MVP.
  * Lock-file behavior is important for physical serial ownership, but should be deferred until broker/virtual TTY ownership is designed.
  * Multiple candidate devices are useful; wiremux already has macOS `tty.*`/`cu.*` candidate logic and can expose this as a chooser later.
  * Setup-at-start is useful; wiremux can support this later as a CLI flag, but the MVP can start with in-TUI target editing.
  * Macros, scripting, dialing directory, modem init strings, and file transfer protocols are not relevant to this MVP.

## Technical Approach

* Introduce a small host config module, likely in the CLI crate or a new shared host config crate if needed by both CLI parsing and TUI.
* Define config precedence as: CLI args > global config > built-in defaults.
* Make `--port` optional only where config can supply it; commands that need a target should still fail clearly when neither CLI nor config provides a port.
* Keep `baud` defaulting to 115200 when neither CLI nor config supplies it.
* Add a physical serial profile model with explicit defaults:
  * data bits: 8
  * stop bits: 1
  * parity: none
  * flow control: none
* Map the serial profile into the `serialport` builder for CLI listen/send and interactive/TUI backends.
* Change TUI runtime state from static `TuiArgs { port, baud }` to a mutable serial profile target that drives reconnect attempts.
* Add a minicom-like target/settings interaction in TUI using existing prefix/modal patterns rather than adding a large settings system.
* Settings panel style should be adapted from `/Users/magicdian/Documents/personal_project/bridging-io/docs/matrix/MENUCONFIG_STYLE_MATRIX.md`:
  * single-column, centered menu layout
  * semantic row templates instead of handcrafted row strings
  * `field-entry-row` grammar for editable serial fields: `Label (value) --->`
  * `choice-list-modal` for enum settings such as data bits, stop bits, parity, and flow control
  * `text-input-modal` for port and baud
  * `confirm-modal` for save/discard/exit decisions
  * `Esc` closes popup first, backs out one level next, then exits settings
  * dirty marker is based on current draft vs loaded baseline, not edit history
  * below `80x24`, show a resize-required overlay instead of rendering a broken settings panel
* On target change, close current backend, reset host session, request manifest again after reconnect, and update status/diagnostics.

## Implementation Plan

* PR1: Add serial profile config data model, load/save path, config precedence tests, serial option mapping, and CLI parsing changes.
* PR2: Add menuconfig-style TUI settings panel and draft/baseline dirty tracking.
* PR3: Add TUI mutable serial profile target and runtime switch/reconnect behavior.
* PR4: Add explicit save action, docs update, and final regression tests.
