# Brainstorm: ESP32 Serial Multiplexer Library and Host Tool

## Goal

Design an ESP32 library and host-side tool that provide software multiplexing over a single USB/JTAG/serial connection. The ESP32 library should capture terminal-style output such as `printf`, `vprintf`, C++ streams, and system logs, encode records into framed protobuf/nanopb messages, and let a PC-side tool decode, filter, and switch between logical channels such as `system`, `channel1`, `channel2`, and `channel3`.

## What I Already Know

* The motivation is hardware-constrained: the ESP32 module exposes only one USB/JTAG interface, and changing hardware is not desired.
* The intended value is to emulate multi-channel terminal/data streams in software.
* The embedded side is expected to be an ESP32 library/module, likely for ESP-IDF projects.
* The host side could be Python, Go, or Rust.
* The user is weighing two protocol models:
  * Fixed built-in proto schema and fixed behavior.
  * Larger framework that allows users to provide custom `.proto` schemas.
* Refined proposal: keep the architecture dynamic but the firmware implementation static. The proto describes channel data and bindings, while the firmware only supports a few fixed channel-count variants such as 2, 4, or 8 channels.
* Each logical channel may expose only two conceptual directions/keys: input and output.
* Users explicitly register bindings, for example ESP console on channel 1 and log printing on channel 2.
* To avoid incomplete terminal-output interception, mux protobuf records may be prefixed with a magic marker; the host parses only records with the magic marker and treats all other bytes as ordinary terminal output.
* The mux module likely needs a continuously running internal service/task. Producer APIs enqueue messages, while the service owns framing, batching, backpressure, and transport writes.
* The service should support configurable send policies:
  * Immediate send when a message arrives.
  * Periodic/batched send, also flushing when the buffer is near full based on baud rate or transport throughput.
* Another project currently follows ESP-IDF v5.4 `examples/system/console/advanced`. Integration should be simple for this style of project.
* The project should create a `docs/` directory for Chinese-first usage documentation. Documentation can be completed after the core implementation, but API shape should be designed for doc simplicity from the start.
* Host-side software should not use Python for the primary user tool. The preferred distribution model is a single executable file.
* Rust with `ratatui` is a candidate for the host TUI. Go is also a candidate if it better fits serial I/O, protobuf reflection, and distribution.
* Decision: use Rust for the host tool.
* First host release should not implement `ratatui`; it should focus on CLI/protocol core.
* Initial host MVP should be able to open `/dev/tty.usbmodem2101`, while allowing the device path to vary at runtime.
* Console integration should not be hard-coded to one mode. MVP may implement line-mode first, but API/config must preserve a path to transparent passthrough.
* Source code should live under `sources/`, split into `sources/host` and `sources/esp32`.
* The repository is currently greenfield: no source code exists yet beyond Trellis scaffolding and `AGENTS.md`.

## Assumptions

* The first implementation should target ESP-IDF rather than Arduino-only projects, because ESP-IDF exposes lower-level logging and app tracing hooks.
* The transport should start with one physical byte stream and support resynchronization after corruption or dropped bytes.
* The ESP32 side should avoid dynamic schema loading. Any custom proto support should be compile-time generated through nanopb or an adapter API.
* The host tool can afford dynamic descriptor loading more easily than the ESP32 target.

## Requirements (Evolving)

* Provide logical channel multiplexing over one physical serial/JTAG/USB byte stream.
* Capture at least explicit library writes in MVP; interception of all `printf`, `vprintf`, `cout`, and ESP logging may be staged because global interception has integration risk.
* Define a small stable core envelope for framing, channel metadata, timestamps, severity, sequence numbers, and payload kind.
* Support host-side channel filtering/switching.
* Evaluate whether user-defined proto should be a host-side/custom-payload extension instead of replacing the core protocol.
* Support compile-time fixed channel capacity variants, initially considering 2, 4, and 8 channels.
* Support explicit user registration of channel bindings rather than requiring full global interception.
* Support mixed streams where non-mux terminal bytes can coexist with mux frames.
* Provide an internal mux service/task that decouples producers from transport writes.
* Support at least two flush policies: immediate and buffered/periodic with high-watermark flush.
* Define per-channel or global buffer limits and drop/backpressure behavior.
* Provide an ESP-IDF console integration path with minimal changes to projects based on the official advanced console example.
* Provide Chinese user-facing docs after implementation covering installation, channel binding, console integration, log integration, host tool usage, and troubleshooting.
* Provide a host-side tool as a single executable, not a Python script as the primary UX.
* Host tool should support serial port selection, mux frame extraction from mixed streams, channel list/manifest display, channel switching, raw capture, and basic decode of built-in payloads.
* Host MVP should be a non-TUI Rust CLI that can open a configured serial device path, initially tested with `/dev/tty.usbmodem2101`.
* Console adapter configuration must include a mode field so line-mode and transparent passthrough can share the same public integration surface.
* Repository source layout:
  * `sources/host`: Rust host CLI and protocol decoder.
  * `sources/esp32`: ESP-IDF component/library for firmware integration and example demo projects.
* ESP32 work should include both the reusable component/library and at least one example demo project for testing and demonstration.

## Acceptance Criteria (Evolving)

* [x] A concrete MVP protocol boundary is chosen.
* [x] The fixed-vs-custom-proto decision is documented with trade-offs.
* [x] Major risks are documented: framing, backpressure, ISR/thread safety, logging recursion, memory limits, schema compatibility, host UX.
* [x] Implementation can be split into small PRs.
* [x] Magic-prefixed mux frames can be extracted from a mixed byte stream without corrupting ordinary terminal output.
* [x] Fixed channel-capacity builds document memory and behavior trade-offs.
* [x] Producer APIs are non-recursive and can avoid long blocking on slow transports.
* [x] Service flush policies are configurable and documented.
* [x] An ESP-IDF advanced-console-style project can bind console I/O to a mux channel with a small, explicit integration patch.
* [x] `docs/` contains Chinese-first getting started and integration guides before public release.
* [x] Host-side implementation language is selected with explicit trade-offs.
* [x] Host tool has a path to single-file releases for macOS/Linux/Windows.
* [x] Rust host CLI can open the configured serial device and read a mixed stream.
* [x] Console integration API is mode-configurable and does not require a breaking API change to add transparent passthrough later.
* [x] Implementation uses `sources/host` and `sources/esp32` as the top-level source roots.
* [x] ESP32 implementation includes an example demo project that shows mux setup and channel binding.

## Definition of Done

* Tests added/updated where implementation exists.
* Lint/typecheck/build passes for host and embedded targets.
* Protocol docs and examples are updated.
* Rollback path considered if global output interception proves unstable.

## Research Notes

### What Similar Tools and Protocols Suggest

* ESP-IDF already has Application Level Tracing, which can transfer arbitrary data between host and ESP32 via JTAG, UART, or USB with low overhead and supports host-to-target data as well as target-to-host tracing. This confirms the idea has precedent, but app_trace is tracing-oriented rather than a developer-friendly logical mux protocol.
* Nanopb is designed for small embedded C targets and generates static C descriptors from `.proto` definitions. It supports fixed-size options such as `max_size` and `max_count`, which matters for predictable ESP32 RAM use.
* Nanopb documentation explicitly says protobuf does not define message framing. Serial-like streams need a separate frame format with length, message type, synchronization, and error checking.
* Protobuf descriptors and dynamic message factories exist on host platforms, so host-side loading of arbitrary `.proto`/descriptor sets is feasible.
* Protobuf unknown fields and reserved fields support schema evolution when binary protobuf is kept as the exchange format.
* COBS and SLIP-style framing are established approaches for byte-stream packet boundaries; COBS gives bounded worst-case overhead, while SLIP is simpler but provides no addressing, type ID, or error checking by itself.

### Constraints From This Repo

* No current implementation exists, so the project can choose a clean protocol boundary now.
* Trellis backend/frontend guidelines are not yet project-specific; future code should also update those specs once the architecture is chosen.

### Feasible Approaches

**Approach A: Fixed Core Proto With Typed Extension Payloads (Recommended)**

* How it works: Define a stable built-in `MuxFrame` envelope and a few built-in payloads, such as `LogRecord`, `RawBytes`, `Control`, and `Heartbeat`. Users can add custom protobuf payloads behind a `payload_type` string or numeric type ID, while the mux core still owns framing, channel routing, sequencing, and control behavior.
* Pros: Keeps ESP32 implementation small and predictable, gives users extension points, allows host-side custom decoding, and avoids turning the device-side mux into a dynamic framework.
* Cons: Requires careful design of the extension registry and host CLI UX.

**Approach A2: Dynamic Architecture With Static Channel Slots (Refined Recommendation)**

* How it works: Keep a stable proto-defined channel/control model, but firmware is compiled with a fixed maximum channel count such as 2, 4, or 8. Each channel has an input side and output side. Users explicitly register producers/consumers such as console, logs, telemetry, or command handlers to channel slots. The transport emits magic-prefixed framed protobuf records so the host can parse mux packets from a mixed terminal stream.
* Pros: Matches embedded constraints, avoids dynamic allocation-heavy abstractions, keeps UX extensible, supports partial adoption, and avoids requiring total takeover of all terminal output.
* Cons: Channel count is not runtime-unbounded, binding mistakes become user-visible, and mixed-stream parsing must be robust against false magic matches and truncated frames.

### Service Runtime Model

**Recommended internal architecture**

* Producer side: `mux_write()`, log adapter, console adapter, and telemetry adapters convert input into records and enqueue them into bounded buffers.
* Service side: one mux task drains buffers, serializes protobuf payloads, wraps them in magic/length/CRC frames, and writes to the selected transport.
* Control side: host-to-device frames are decoded by the same service and dispatched to registered channel input handlers.

**Flush policy options**

* Immediate: low latency, simplest mental model, good for console interaction and sparse logs. Higher overhead and worse throughput when many small records are produced.
* Periodic/batched: better throughput and lower framing overhead, good for high-rate telemetry and logs. Adds latency and requires careful high-watermark flushing.
* Hybrid recommendation: per-channel policy with global safety limits. Console/control channels default to immediate or short timeout; logs/telemetry default to batch with flush interval and high-watermark threshold.

**Backpressure decisions to define**

* Bounded queue size per channel or shared global pool.
* Drop policy: drop newest, drop oldest, block with timeout, or escalate with a dropped-record counter.
* Priority policy: control and console input should outrank bulk telemetry/log output.
* ISR policy: ISR contexts should only use a minimal non-blocking enqueue path or be explicitly unsupported.

### ESP-IDF Console Integration Model

**Target project shape**

* ESP-IDF v5.4 advanced console examples initialize the console peripheral, initialize linenoise/console support, register commands, then run a loop around `linenoise(prompt)` and `esp_console_run(line, &ret)`.
* ESP-IDF console APIs also support command registration via `esp_console_cmd_register()` and dispatch via `esp_console_run()`.
* ESP-IDF logging supports `esp_log_set_vprintf()` to redirect log output, but the callback must be re-entrant because logs can be emitted from multiple thread contexts.

**Recommended adapter strategy**

* Console channel adapter: provide a mux-backed stdin/stdout or line-source adapter that lets existing console loops keep using `linenoise()`/`esp_console_run()` where possible.
* Log channel adapter: provide `mux_install_esp_log_sink(channel_id, options)` that wraps `esp_log_set_vprintf()` and forwards formatted log text to a mux channel.
* Explicit binding API: user code should look like `mux_bind_console_channel(MUX_CHANNEL_CONSOLE, &config)` and `mux_bind_esp_log_channel(MUX_CHANNEL_LOG, &config)` rather than requiring users to understand frame/protobuf internals.
* Console mode should be configured through init/bind parameters, not hard-coded. Proposed modes:
  * `MUX_CONSOLE_MODE_LINE`: host sends complete lines; ESP dispatches with `esp_console_run()`. MVP implementation target.
  * `MUX_CONSOLE_MODE_PASSTHROUGH`: host and ESP exchange terminal bytes; later target for linenoise/ANSI transparent behavior.
  * `MUX_CONSOLE_MODE_DISABLED`: reserve the channel but do not install a console adapter.
* The public config should keep common fields stable across modes: channel ID, prompt/name metadata, flush policy, input queue size, output queue size, and optional callbacks.

**Expected integration effort**

* Low effort if the project has a centralized console initialization function and a single REPL loop.
* Medium effort if console I/O is spread across multiple files or if the project depends heavily on raw terminal ANSI behavior.
* Higher risk if the project requires all early boot, panic, or ROM output to be captured, because those happen before the mux service is fully available.
* Line-mode can be implemented first, but internals should split "console input source", "console output sink", and "command dispatch" so passthrough can replace only the I/O layer later.

**Documentation plan**

* Add `docs/` after implementation or near the first public example.
* Chinese-first docs:
  * `docs/zh/getting-started.md`
  * `docs/zh/esp-idf-console-integration.md`
  * `docs/zh/channel-binding.md`
  * `docs/zh/host-tool.md`
  * `docs/zh/troubleshooting.md`
* Keep API examples short enough that a user can integrate console/log channels without reading protocol internals.

### Host Tool Language Options

**Option H1: Rust + ratatui (Recommended if protocol correctness and TUI quality are priorities)**

* How it works: Build a Rust CLI/TUI around `ratatui` for panes, channel lists, logs, and command input. Use `serialport` or an equivalent serial crate for transport. Use generated `prost` types for core mux frames and `prost-reflect` for optional descriptor-driven custom payload decode.
* Pros: Strong type safety, good fit for binary protocol parsing, single executable distribution, high confidence around framing/CRC/state-machine correctness, `ratatui` is purpose-built for rich terminal UIs.
* Cons: Slower iteration than Go, async/terminal/event-loop design requires discipline, cross-compilation can be more work.

**Option H2: Go + Bubble Tea**

* How it works: Build a Go CLI/TUI around Bubble Tea for the terminal UI. Use `go.bug.st/serial` for cross-platform serial ports. Use generated Go protobuf for core frames and `google.golang.org/protobuf/types/dynamicpb` plus `protodesc` for descriptor-driven custom payload decode.
* Pros: Very simple single-binary distribution, fast build times, straightforward concurrency model, mature terminal UX ecosystem, protobuf reflection is official in the Go protobuf module.
* Cons: Binary protocol/state-machine invariants rely more on tests and discipline than type system guarantees; TUI layout can feel more framework-opinionated.

**Option H3: Split CLI Core + Optional TUI**

* How it works: First build a non-interactive CLI that can list ports, connect, dump channels, capture raw frames, replay captures, and send console lines. Add TUI after protocol and capture/replay are stable.
* Pros: Best debugging path, fastest validation of ESP-side protocol, easier automated tests, avoids designing the TUI before the protocol is proven.
* Cons: Delays the polished user experience.

**Host recommendation**

* Start with a host core that is testable without a terminal UI: frame scanner, decoder, manifest model, capture/replay, serial transport abstraction.
* Decision: choose Rust because the project values protocol robustness, future maintainability, and a future `ratatui` path more than fastest initial implementation speed.
* Defer `ratatui` until the protocol core, capture/replay, and basic CLI are stable.
* In either language, keep the TUI as a thin layer over a protocol/session core.

### Host MVP

* Binary: tentative name `esp-serial-mux`.
* Command shape: `esp-serial-mux --port /dev/tty.usbmodem2101 --baud 115200`.
* Default behavior: open the port, read bytes, print ordinary non-mux terminal output, decode and print mux frames that pass magic/length/CRC validation.
* Early subcommands to consider:
  * `listen`: read and display mixed stream.
  * `capture`: save raw bytes to a file.
  * `replay`: replay a capture through the decoder without hardware.
  * `send`: send a line or bytes to a channel.
* No TUI in the first release.

**Approach B: Fully Fixed Proto and Fixed Behavior**

* How it works: Ship one canonical schema with fixed channel concepts and fixed payload types. Users can only send logs/raw bytes/known records.
* Pros: Easiest to implement, easiest to document, most reliable for embedded constraints.
* Cons: Less attractive as a reusable library if users want domain-specific telemetry or structured records.

**Approach C: User-Defined Proto Framework**

* How it works: Users define arbitrary proto files; the host tool loads descriptors; the ESP32 side includes generated nanopb code for each custom schema.
* Pros: Maximum flexibility and attractive for advanced telemetry use cases.
* Cons: Highest complexity. Device-side registration, message IDs, generated code integration, memory bounds, schema compatibility, and debugging become the product, not just the mux.

## Expansion Sweep

### Future Evolution

* The project could evolve from "better logs over one cable" into a structured telemetry/control plane for ESP32 apps.
* Extension points worth preserving now: stable frame envelope, payload type IDs, version negotiation, and feature bits.
* Fixed channel-count variants can remain source-compatible if the proto model uses channel IDs and capability negotiation.

### Related Scenarios

* Host-to-target control may be needed later for changing log levels, subscribing to channels, pausing streams, or sending commands.
* Binary capture/replay should be considered because it makes host-tool debugging and regression testing much easier.
* Explicit channel registration creates a clean path for optional adapters: ESP console adapter, ESP log adapter, printf adapter, telemetry adapter.
* Service flush policy should be part of channel configuration or device manifest so the host can explain latency/throughput behavior.
* Docs should be written primarily in Chinese for the initial target users, with English protocol/API references optional later.
* Host tool should support raw capture/replay early because it decouples host UI testing from real ESP hardware.

### Failure and Edge Cases

* Serial/JTAG streams can drop or corrupt bytes, so resync and checksum/CRC matter.
* Backpressure matters: logging must not block critical tasks indefinitely or recursively log from the mux transport itself.
* Global interception of `printf`/`cout`/ESP logs can conflict with existing logging backends and bootloader/early startup output.
* Magic-prefix parsing needs a complete frame structure, not just a magic value. Recommended frame shape: magic, version, header length or payload length, flags/type, CRC, protobuf payload.
* False magic bytes can appear in ordinary terminal output. The host must validate length, version, and CRC before treating bytes as mux data.
* Immediate send from producer context risks blocking application tasks on slow UART/JTAG writes. A service task avoids that but introduces queue memory and scheduling behavior.
* Buffered sending risks losing the last buffered logs during crash/panic unless panic flush or unbuffered critical-path mode is supported.
* Redirecting ESP logs via `esp_log_set_vprintf()` requires a re-entrant callback; the mux log adapter must avoid shared mutable state without locking and must not recursively log internally.
* Linenoise and ANSI escape handling may be fragile when console bytes are wrapped into mux frames; host terminal emulation must preserve interactive behavior for the console channel.
* If the host TUI is built first, protocol bugs may be hidden behind UI behavior. A CLI/capture core should come before or alongside the TUI.
* If line-mode details leak into the public API, transparent passthrough will require breaking changes. Keep mode-specific behavior behind config and adapter internals.

## Open Questions

* None for MVP planning. Remaining details should be resolved during protocol/API implementation with tests and examples.

## Technical Approach

The project will use a dynamic architecture with a static embedded implementation.

* ESP32 firmware is compiled with a fixed channel capacity, initially targeting 2/4/8 channel variants.
* The protocol uses a fixed binary frame wrapper: magic, version, flags, payload length, CRC, and protobuf payload.
* The protobuf model contains a stable core envelope plus channel/device descriptors.
* Channel metadata is sent through a manifest/descriptor message, not repeated in every data frame.
* Each channel supports input and output directions.
* Users explicitly bind adapters to channels, such as console, logs, telemetry, or custom output.
* ESP32 does not dynamically load arbitrary proto schemas. Custom payloads may be supported through compile-time/generated code and host-side descriptor decoding later.
* A mux service task owns queue draining, protobuf encoding, framing, flush policy, and transport writes.
* The first host tool is a Rust non-TUI CLI. `ratatui` is deferred until the protocol core is stable.

## Decision (ADR-lite)

**Context**: The project needs to multiplex console/log/data streams over one ESP32 serial-like connection without changing hardware. The design must remain simple for users but predictable enough for embedded constraints.

**Decision**: Use a fixed core protocol with static channel slots on ESP32 and explicit channel binding APIs. Support mixed streams by prefixing mux frames with a robust binary magic/length/CRC frame. Use Rust for the host CLI. Implement console line-mode first, but keep console mode configurable so transparent passthrough can be added later without breaking the public API.

**Consequences**:

* The ESP32 side stays memory-bounded and testable.
* The host can decode mux frames from a normal terminal stream without requiring total output takeover.
* Some flexibility is intentionally deferred: no runtime-unbounded channels, no arbitrary device-side proto loading, no first-release TUI.
* API design must preserve extension points from day one, especially for console mode, payload type IDs, manifest capabilities, and flush policies.

## Implementation Plan

### PR1: Protocol and Host Decoder Core

* Create Rust workspace/tool skeleton under `sources/host`.
* Define binary frame format constants and parser.
* Define core `.proto` schema and generated Rust types.
* Implement mixed-stream scanner: ordinary bytes pass through, valid mux frames decode.
* Add raw capture/replay support for decoder tests.
* Support opening a runtime-configured serial path, initially tested with `/dev/tty.usbmodem2101`.

### PR2: ESP-IDF Component Skeleton

* Create ESP-IDF component structure under `sources/esp32`.
* Add nanopb/protobuf generation path or initial checked-in generated code.
* Implement mux init/start/stop APIs.
* Implement static channel registry and device manifest.
* Implement service task, queues, flush policy, and frame encoder.
* Implement basic output API: write text/raw/protobuf payload to a channel.
* Add an ESP-IDF example demo project under `sources/esp32/examples`.

### PR3: Console and Log Adapters

* Implement `mux_bind_console()` with mode-configurable config.
* Support `MUX_CONSOLE_MODE_LINE` in MVP using `esp_console_run()`.
* Reserve `MUX_CONSOLE_MODE_PASSTHROUGH` in public API but return not-supported until implemented.
* Implement ESP log adapter using `esp_log_set_vprintf()`.
* Add example showing console channel and log channel bindings.

### PR4: Documentation and Polish

* Create `docs/zh/`.
* Add getting started, ESP-IDF console integration, channel binding, host CLI, and troubleshooting guides.
* Add protocol reference and examples.
* Add release notes for single-binary host usage.

## Out of Scope (Draft)

* Full arbitrary user proto runtime loading on the ESP32 device.
* Replacing ESP-IDF app_trace itself.
* Guaranteed capture of bootloader output before the library initializes.
* Multi-transport support beyond one initial physical stream.
* Runtime creation of unlimited channels.
* ISR-safe high-throughput telemetry in the first MVP, unless explicitly scoped later.
* Ratatui/TUI host UI in the first release.

## Technical Notes

* Sources used for initial research:
  * ESP-IDF Application Level Tracing documentation: https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/app_trace.html
  * ESP-IDF app_trace API reference: https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_trace.html
  * Nanopb overview and concepts: https://jpa.kapsi.fi/nanopb/docs/ and https://jpa.kapsi.fi/nanopb/docs/concepts.html
  * Nanopb security model: https://jpa.kapsi.fi/nanopb/docs/security.html
  * Protobuf dynamic descriptors: https://protobuf.dev/reference/cpp/api-docs/google.protobuf.descriptor/ and https://protobuf.dev/reference/cpp/api-docs/google.protobuf.dynamic_message/
  * Protobuf unknown fields and reserved fields: https://protobuf.dev/programming-guides/editions/ and https://protobuf.dev/reference/protobuf/proto3-spec/
  * Python protobuf DescriptorPool: https://googleapis.dev/python/protobuf/latest/google/protobuf/descriptor_pool.html
  * Go protobuf dynamicpb/protodesc: https://pkg.go.dev/google.golang.org/protobuf/types/dynamicpb and https://pkg.go.dev/google.golang.org/protobuf/reflect/protodesc
  * COBS paper: https://www.stuartcheshire.org/papers/COBSforSIGCOMM/
  * RFC 1055 SLIP: https://datatracker.ietf.org/doc/html/rfc1055
  * ESP-IDF v5.4 console documentation: https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32/api-reference/system/console.html
  * ESP-IDF v5.4 logging documentation: https://docs.espressif.com/projects/esp-idf/en/v5.4/esp32/api-reference/system/log.html
  * ESP-IDF v5.4 advanced console example: https://github.com/espressif/esp-idf/tree/release/v5.4/examples/system/console/advanced
  * Ratatui documentation: https://ratatui.rs/ and https://docs.rs/ratatui/
  * Rust protobuf reflection via prost-reflect: https://docs.rs/prost-reflect/
  * Go Bubble Tea documentation: https://github.com/charmbracelet/bubbletea and https://pkg.go.dev/github.com/charmbracelet/bubbletea
  * Go serial library documentation: https://pkg.go.dev/go.bug.st/serial
  * Go protobuf dynamic descriptors: https://pkg.go.dev/google.golang.org/protobuf/types/dynamicpb and https://pkg.go.dev/google.golang.org/protobuf/reflect/protodesc
