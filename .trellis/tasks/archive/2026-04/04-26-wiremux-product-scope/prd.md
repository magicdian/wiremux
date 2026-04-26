# Brainstorm: Wiremux Product Scope

## Goal

Evaluate whether the current serial multiplexer direction has practical product value, identify comparable existing solutions, and broaden the product scope beyond ESP32-only demos toward a general serial multiplexing/tooling product.

## What I Already Know

* The current project has a basic prototype and demo for serial muxing.
* The initial target appears to be ESP32, but the product idea may apply to any serial device.
* The user is considering a host-side multi-window UX, initially imagined with `ratatui`.
* The user also wants to evaluate a virtual-device mode where each logical channel appears as a separate device path, such as `/dev/vmux.cu.usbmodem2101.ch1`, so users can use existing tools like `screen` or `minicom`.
* Repo inspection confirms the project currently has an ESP-IDF component, an ESP32 console mux demo, and a Rust host CLI/library.
* The current protocol already has channel IDs, direction, sequence, timestamps, payload kind, payload type, payload bytes, and flags, which makes it broader than a one-off ESP32 console hack.
* Current host tool is a non-TUI CLI with `listen` and `send`, using the Rust `serialport` crate against macOS/Linux/Windows serial paths.
* Current ESP32 demo maps channel 0 to system/control manifest, channel 1 to console line mode, channel 2 to logs, and channel 3 to telemetry.

## Assumptions (Temporary)

* The current implementation includes both device-side protocol/demo code and a Rust host CLI/library.
* The immediate output of this brainstorm is product and architecture direction, not implementation.
* A useful MVP should preserve the current ESP32 demo while avoiding a product definition that is ESP32-only.

## Open Questions

* Confirm exact rename/refactor implementation scope before code changes start.

## Requirements (Evolving)

* Compare this project with existing serial, terminal, and device-multiplexing tools.
* Evaluate product value and likely user segments.
* Assess `ratatui` versus lower-level terminal control for tmux-like window switching.
* Assess feasibility and trade-offs of exposing logical mux channels as virtual serial devices.
* Propose 2-3 feasible product directions with trade-offs.
* Prioritize a layered architecture refactor rather than implementing TUI or virtual devices immediately.
* Keep short-term SDK support ESP32-focused while designing host-side abstractions so other serial targets can be added later.
* Treat host CLI as both a validation tool and a quick-use product surface.
* Preserve cross-platform intent, but defer Windows virtual-device support.
* Add service/broker mode and virtual per-channel devices to the roadmap, not the immediate implementation scope.
* Rename current product/codebase from ESP-specific `wiremux` naming to `wiremux` public naming.
* Rename ESP SDK/component surface to `esp-wiremux` paths and `esp_wiremux_*` C identifiers.
* Add a visible core boundary scaffold so reusable protocol concepts are not buried inside ESP-specific naming.
* Preserve the current ESP32 demo and host CLI behavior after rename.

## Acceptance Criteria (Evolving)

* [x] Similar/adjacent tools are identified and mapped to this project's differentiation.
* [x] Product scope is broadened beyond ESP32 with concrete use cases.
* [x] Host UI strategy options are compared.
* [x] Virtual-device strategy options are compared.
* [x] A recommended MVP direction is proposed with explicit out-of-scope items.
* [x] Naming direction is chosen before code/directory rename work starts.
* [x] Refactor scope identifies which names are public API, crate/package names, paths, docs, and protocol identifiers.
* [x] Host Rust crate builds and tests pass after rename.
* [x] Host CLI binary is named `wiremux` and help/examples use `wiremux`.
* [x] ESP component directory is named `esp-wiremux`.
* [x] ESP public C identifiers use `esp_wiremux_*`.
* [x] Protocol magic is renamed consistently to `WMUX` if compatibility is intentionally broken in this early phase.
* [x] Docs describe ESP32 as the first reference SDK, not the product boundary.
* [x] Core boundary contains real portable C frame encode/decode, CRC, envelope, and manifest code used by the ESP adapter.
* [x] Roadmap notes include future TUI, service/broker, PTY exposure, and deferred Windows virtual COM support.

## Definition of Done (Team Quality Bar)

* Research notes captured in this PRD.
* Repo constraints and current architecture summarized.
* Product/technical decisions captured as ADR-lite notes if a direction is chosen.
* No implementation is started until the product direction is confirmed.

## Out of Scope (Explicit)

* Implementing the TUI or virtual-device mode during this brainstorm.
* Finalizing a commercial business model.
* Supporting every OS-specific virtual device backend in the first MVP.
* Implementing Windows native virtual COM support in the short term.
* Implementing service/broker mode in the immediate next step unless explicitly pulled into scope later.
* Implementing every future platform adapter in this task.
* Implementing `ratatui` TUI in this task.
* Implementing PTY/virtual-device exposure in this task.

## Technical Notes

* External research is required because the user asked about comparable existing solutions and established conventions.
* Repo inspection is required to map product options onto current architecture.
* Current docs explicitly keep `ratatui` out of first-phase scope and mention capture/replay as a later host feature.
* Current ESP32 component has a transport abstraction (`esp_wiremux_transport_t`) and default stdout/stdin transport, so it can theoretically be adapted to UART, USB CDC/JTAG, TCP bridges, or custom embedded transports.
* Current host implementation is synchronous and single-port/single-process oriented; virtual-device mode would require a broker process that owns the physical serial port and fans channels out to PTYs or OS-specific virtual serial devices.

## Research Notes

### What Similar Tools Do

* GSM 07.10 / CMUX is a direct precedent: Linux has an `n_gsm` line discipline that multiplexes a modem serial link and exposes resulting channels as `ttygsm*`-style virtual serial devices. This validates the concept of multiple logical serial sessions over one physical serial link.
* ser2net and RFC2217-style tooling solve remote serial access and serial-over-network, including serial parameter control, but they generally expose a serial port remotely rather than adding semantic embedded channels like console/log/telemetry/control.
* pySerial supports URL handlers such as `rfc2217://`, `socket://`, `loop://`, and device discovery helpers. This shows a convention for making serial endpoints more abstract than local `/dev/tty*` paths.
* socat/openpty patterns are commonly used on Unix-like systems to create PTY-backed virtual serial endpoints for testing and bridging, but they do not provide channel semantics by themselves.
* com0com/hub4com on Windows show that virtual COM ports and port sharing are valuable enough to have driver-level ecosystems, but Windows support has a different implementation and signing burden.
* tmux/screen/Zellij solve terminal workspace multiplexing, not serial protocol multiplexing. Their useful lesson is UX vocabulary: sessions/windows/panes, status bars, detach/attach, keybindings, and scriptability.

### Product Value Hypothesis

* The highest-value user pain is not "ESP32 needs mux"; it is "a single serial connection is overloaded with boot logs, runtime logs, REPL/console, telemetry, control commands, test harness traffic, and sometimes binary protocol data."
* This is common across ESP32, Zephyr boards, STM32/MCU projects, Linux SBC UART consoles, modems, radios, lab instruments, factory fixtures, and device farm automation.
* The differentiator should be semantic channels over any byte stream, not another terminal emulator or another generic serial-over-TCP bridge.

### Ratatui / TUI Notes

* `ratatui` is useful for building a foreground TUI with widgets, layouts, status bars, tabs, and keyboard handling.
* Lower-level terminal control through `crossterm` can implement raw mode and alternate-screen behavior directly, but reproducing tmux-like windows/panes manually means owning layout, render diffing, event routing, scrollback, focus, keymaps, resize handling, and cleanup.
* A foreground TUI is inherently one product surface: the user runs `wiremux tui ...` and interacts inside that app. It does not automatically let existing tools open each channel.

### Virtual Device Notes

* On Linux, PTY-backed endpoints are practical for an MVP. The process can allocate `/dev/pts/*` slaves and optionally create stable symlinks outside `/dev`, such as under `/tmp`, `/run/user/$UID`, or `~/.local/state/wiremux/`.
* On macOS, PTYs exist and can be used by many terminal programs, but making a device look like a first-class `/dev/cu.*` serial device visible to all serial-port-scanning apps may require IOKit/driver-level integration. A symlink to a PTY may work for tools that accept explicit paths, but not for every GUI scanner.
* On Windows, real virtual COM ports usually imply driver/ConPTY/com0com-style integration. A first MVP should avoid promising native COM port fanout unless it depends on an existing driver or limits support to a named pipe/TCP bridge.

### Feasible Approaches Here

**Approach A: Protocol/library first, CLI remains simple**

* How it works: define the product as a transport-agnostic mux protocol plus host/device libraries; keep `listen`, `send`, `manifest`, and capture/replay strong.
* Pros: highest reuse across boards and languages; avoids hard OS-specific virtual-device complexity.
* Cons: less immediately impressive UX; users still need this CLI to interact with channels.

**Approach B: Ratatui foreground workspace**

* How it works: add `tui` mode with windows/tabs for console, logs, telemetry, manifest, and raw frames.
* Pros: fastest way to make the demo feel like a product; Rust ecosystem fit; good for live debugging.
* Cons: users must adopt this UI; not compatible with existing `screen`, `minicom`, IDE serial monitors as first-class clients.

**Approach C: Host broker with virtual per-channel endpoints**

* How it works: one daemon owns the real serial port, demuxes frames, and exposes each channel as a PTY/TCP/named endpoint.
* Pros: strongest "works with your existing tools" story; matches proven GSM CMUX-style virtual serial model.
* Cons: OS-specific complexity; macOS and Windows expectations differ; lifecycle, permissions, discovery, and cleanup become core product problems.

**Approach D: Layered product: broker core, multiple frontends**

* How it works: build a host broker abstraction first, then attach CLI, TUI, PTY endpoints, TCP/RFC2217-ish endpoints, and capture/replay as frontends.
* Pros: best long-term architecture; supports both product surfaces without duplicating serial ownership.
* Cons: more design work; may be too much for the immediate next implementation if not sliced carefully.

### Current Direction Chosen by User

* Use the layered product route.
* Short-term scope: ESP32-side SDK/library remains the first-class device implementation; host side remains CLI-first.
* Cross-platform support matters, but Windows virtual-device support can be deferred.
* Virtual per-channel devices should likely live behind a future service/broker mode.
* CLI should remain useful for validation and quick manual usage, while SDK users can integrate directly.

### Naming Research Notes

* `vmux` is concise and relevant if "virtual mux" becomes the long-term identity, but it already appears in multiple public contexts: a visionOS SSH terminal workspace, a Rust crate, and industrial/measurement software. This creates search/discovery ambiguity.
* `serialmux` is descriptive, but similar names already exist, including a documented `serialMux` project and a recent `pySerialMux` package.
* `smux` and `sermux` are compact but already used in networking/serial contexts and are less self-explanatory.
* A stronger name should communicate the durable product idea: multiplexing logical channels over one byte stream or physical wire, not only ESP32 and not only virtual devices.
* Candidate naming directions:
  * `vmux`: shortest; good if virtual endpoints become the headline, weaker for current SDK/CLI phase and has naming collisions.
  * `wiremux`: clear "one wire/link, many channels" positioning; less tied to serial-only or virtual-device-only semantics.
  * `chanmux`: accurately describes channel multiplexing, but feels more internal/technical.
  * `portmux`: clear for users thinking in serial ports, but may imply OS port fanout rather than protocol channels.
  * `muxline`: compact and terminal/serial flavored, but less obvious at first glance.

### Naming Decision

* Public project name: `wiremux`.
* Rationale: `wiremux` communicates the durable product concept, "multiple logical channels over one physical wire/link", without binding the project to ESP32, serial-only transports, or virtual-device-only positioning.
* Consequences:
  * User-facing CLI should become `wiremux`.
  * Rust host crate/library naming should move from the previous ESP-specific name toward `wiremux`.
  * ESP platform directory/package naming should use `esp-wiremux`.
  * ESP C code and public API names should use underscore form `esp_wiremux_*`.
  * Protocol magic can be renamed to `WMUX` if breaking compatibility is acceptable at this early stage.
  * Documentation and examples should avoid describing the product as ESP32-only; ESP32 becomes the first reference device SDK.

### SDK Layering Direction

* SDK design should split common protocol/core code from platform-specific integration code.
* Common code should own wire protocol primitives, frame encode/decode, envelope/channel metadata types, validation rules, and reusable mux routing semantics.
* Platform adapters should own transport IO, task/thread scheduling, queues, locks, timers, logging integration, memory policy, and console/REPL binding.
* ESP32 remains the first supported device SDK, but should be structured as `esp-wiremux` built on top of reusable core concepts.
* Future platform implementations should not need to re-design the public protocol/API model independently; they should bind the shared core to their platform transport and runtime.

### Current Code Split Observed

* Common/core code now present:
  * `sources/core/proto/wiremux.proto`: shared `MuxEnvelope`, `ChannelDescriptor`, `DeviceManifest`, and capability schema.
  * `sources/core/c/include/wiremux_frame.h` and `sources/core/c/src/wiremux_frame.c`: shared magic, header length, frame encode/decode, CRC32, and portable `wiremux_status_t`.
  * `sources/core/c/include/wiremux_envelope.h` and `sources/core/c/src/wiremux_envelope.c`: shared protobuf-compatible envelope encode/decode.
  * `sources/core/c/include/wiremux_manifest.h` and `sources/core/c/src/wiremux_manifest.c`: shared protobuf-compatible `DeviceManifest` encoder, including native endianness, repeated payload kind/type descriptors, and feature/capability fields.
  * `sources/core/c/tests/wiremux_core_smoke_test.c`: portable C smoke coverage for CRC, envelope encode/decode, manifest encode, and frame decode.
  * `sources/esp32/components/esp-wiremux/src/esp_wiremux_frame.c`: ESP-facing adapter that maps portable status codes to `esp_err_t`.
  * `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`: channel metadata, direction validation, input dispatch rules, FreeRTOS integration, and manifest emission through core encoders.
  * `sources/host/src/frame.rs` and `sources/host/src/envelope.rs`: host-side equivalents of frame scanning and envelope encode/decode.
* ESP-specific code currently mixed into core paths:
  * FreeRTOS queue/task/lock usage in `esp_wiremux.c`.
  * `esp_timer_get_time()` for timestamps.
  * default stdin/stdout and USB Serial/JTAG transport setup.
  * `esp_err_t` error model throughout the public C API.
  * ESP-IDF console adapter in `esp_wiremux_console.c`.
  * ESP-IDF log adapter in `esp_wiremux_log.c`.
* Refactor implication: frame encode/decode/CRC, envelope encode/decode, proto schema, and manifest encoding are now extracted; remaining future work is host structured manifest decode and additional platform adapters.

## Decision (ADR-lite)

**Context**: The current repository name and APIs are ESP32-specific, but the product direction is a general channel multiplexer over serial-like byte streams.

**Decision**: Adopt `wiremux` as the public product name and pursue a layered architecture: protocol/core first, ESP32 SDK as first device adapter, host CLI as first product surface, service/virtual devices and TUI as roadmap items.

**Consequences**: A repository-wide rename is expected. The next implementation should separate durable protocol/host core names from ESP adapter names, and should decide whether early protocol identifiers are renamed now while compatibility cost is low. ESP directory/package naming should use `esp-wiremux`; C identifiers should use `esp_wiremux_*`.

## Technical Approach

Implement the next step as a scoped rename and boundary scaffold, not a feature expansion:

* Rename host package/library/binary from the previous ESP-specific name to `wiremux`.
* Rename ESP component and examples to `esp_wiremux` identifiers and `esp-wiremux` paths.
* Rename protocol magic to `WMUX` while compatibility cost is still low.
* Extract portable C frame encode/decode/CRC, envelope, and manifest code into `sources/core/c` and keep ESP-specific runtime/error mapping in `esp-wiremux`.
* Add/adjust documentation that describes `wiremux` as the general product and `esp-wiremux` as the first reference SDK.
* Keep existing listen/send behavior, channel semantics, CRC validation, line-mode console, log channel, and telemetry demo intact.

## Implementation Plan

* PR1/current task: naming refactor across host, ESP, docs, and tests.
* PR2/future: add host-side structured `DeviceManifest` decode and present capabilities in CLI/TUI.
* PR3/future: host broker/service mode with PTY exposure.
* PR4/future: `ratatui` TUI frontend.

### Research Sources

* Linux kernel GSM 0710 tty multiplexor HOWTO: https://www.kernel.org/doc/html/v6.6/driver-api/tty/n_gsm.html
* pySerial URL handlers: https://pyserial.readthedocs.io/en/stable/url_handlers.html
* ser2net man page: https://manpages.debian.org/experimental/ser2net/ser2net.8.en.html
* Linux pseudoterminal man page: https://man7.org/linux/man-pages/man7/pty.7.html
* socat PTY link option: https://manpages.debian.org/stable/socat
* Ratatui docs: https://docs.rs/ratatui/
* Ratatui backend concepts: https://ratatui.rs/concepts/backends/
* Crossterm terminal docs: https://docs.rs/crossterm/latest/crossterm/terminal/
* GNU Screen manual: https://www.gnu.org/software/screen/manual/screen.html
* vmux visionOS SSH terminal: https://vmux.app/
* vmux Rust crate: https://crates.io/crates/vmux
* pySerialMux package: https://pypi.org/project/pySerialMux/
