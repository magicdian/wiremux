# brainstorm: non-polling TUI event loop

## Goal

Optimize the interactive host loops away from fixed polling and sleeping toward
event-driven designs, reducing interactive latency and idle CPU usage while
preserving serial I/O and terminal input responsiveness. The MVP covers both the
ratatui TUI loop and the non-TUI passthrough loop.

## What I already know

* The current TUI is believed to roughly loop through:
  * `port.read()` with up to 5 ms blocking.
  * `crossterm::event::poll(1ms)` for keyboard and mouse input.
  * `terminal.draw(...)`.
  * `sleep(16ms)`.
* The optimization may need platform-specific handling.
* macOS and Linux might be able to share a Unix-oriented approach.
* Windows likely needs a separate implementation strategy.
* Code inspection confirms the TUI loop is in `sources/host/src/tui.rs`.
* `sources/host/src/tui.rs` currently:
  * Opens the serial port with `INTERACTIVE_SERIAL_READ_TIMEOUT` (5 ms).
  * Reads serial data synchronously each loop.
  * Drains crossterm events with `event::poll(Duration::from_millis(1))`.
  * Draws unconditionally with `terminal.draw(...)`.
  * Sleeps for 16 ms after each draw.
* The non-TUI passthrough loop in `sources/host/src/main.rs` has a similar
  serial read plus crossterm poll pattern, but without ratatui drawing.
* Crossterm 0.29 already implements terminal input readiness with OS waits:
  * Unix uses `mio::Poll` over the terminal fd and SIGWINCH.
  * Windows uses `WaitForMultipleObjects` on the console input handle.
* The current serial abstraction returns `Box<dyn serialport::SerialPort>`.
  That trait does not expose raw fd/handle access.
* `serialport::TTYPort` on Unix exposes `AsRawFd`, so a Unix-specific poll
  implementation is feasible if the code keeps the concrete type or adds an
  adapter.
* `serialport::COMPort` on Windows has a raw handle internally, but the current
  cross-platform trait path does not expose a waitable overlapped I/O model.
* The TUI should target 60 fps by default.
* In GPU-accelerated terminals such as Ghostty, the TUI should be able to target
  120 fps.

## Assumptions (temporary)

* The TUI is implemented in the Rust host package.
* The goal is not merely lowering the sleep duration, but making wakeups depend
  on real serial/input/timer events where practical.
* Compatibility and maintainability matter more than using the lowest-level OS
  primitive directly.

## Open Questions

* Whether to implement the MVP now after final scope confirmation.

## Requirements (evolving)

* Understand the current TUI loop and serial/input integration points.
* Understand the current passthrough loop and serial/input integration points.
* Compare feasible non-polling or reduced-polling designs for Unix and Windows.
* Preserve interactive keyboard/mouse behavior.
* Preserve serial read behavior and low-latency output rendering.
* Support a render cadence that can cap at 60 fps by default and 120 fps in
  terminals/configurations where that is desirable.
* Enable 120 fps automatically for Ghostty-like GPU-accelerated terminals when
  confidently detected.
* Provide a CLI override for TUI frame rate so automatic detection can be
  corrected by the user.
* Apply the event-driven I/O architecture to `wiremux passthrough` as well, while
  keeping TUI frame-rate behavior scoped to `wiremux tui`.
* Preserve a unified upper-level event loop shape so TUI and passthrough do not
  fork business logic by platform.
* Design the event backend as an adapter layer that can use a compatibility
  implementation everywhere and optionally switch Unix to a raw-fd `mio` high
  performance implementation.

## Acceptance Criteria (evolving)

* [ ] Existing TUI loop and relevant files are identified.
* [ ] Existing passthrough loop and relevant files are identified.
* [ ] Feasible approaches are compared with platform trade-offs.
* [ ] MVP scope is explicitly agreed.
* [ ] Out-of-scope work is recorded.
* [ ] TUI avoids unconditional redraw/sleep while respecting a 60/120 fps cap.
* [ ] Passthrough avoids fixed short polling while preserving exit-key behavior.

## Definition of Done (team quality bar)

* Tests added/updated if implementation proceeds.
* Lint / typecheck / CI green if implementation proceeds.
* Docs/notes updated if behavior or platform support changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Rewriting the entire TUI architecture unless repo inspection proves it is
  necessary.
* Dropping Windows support.
* Adding frame-rate behavior to non-TUI passthrough.
* Requiring Unix raw-fd `mio` serial readiness in the first pass.
* Implementing Windows overlapped serial I/O in the first pass.

## Technical Notes

* Initial PRD seeded before code inspection per brainstorm workflow.
* Relevant files inspected:
  * `sources/host/src/tui.rs`
  * `sources/host/src/main.rs`
  * `sources/host/Cargo.toml`
  * local `serialport-4.9.0` source
  * local `crossterm-0.29.0` source
* User decision: choose automatic Ghostty detection with CLI override for 120
  fps behavior.
* Official Ghostty docs state Ghostty uses `TERM=xterm-ghostty` when its
  terminfo entry is available.
* Official Ghostty shell-integration docs state SSH integration can preserve
  `COLORTERM`, `TERM_PROGRAM`, and `TERM_PROGRAM_VERSION` while falling back to
  `TERM=xterm-256color` on remote hosts.
* Existing CLI parser is hand-written in `sources/host/src/main.rs`; adding a
  TUI-only frame-rate option means extending `TuiArgs`, `parse_args`, `usage`,
  and parser tests.
* `serialport::SerialPortBuilder::open_native()` returns concrete `TTYPort` on
  Unix, so a raw-fd mio backend can own a pollable serial port without relying
  on `Box<dyn SerialPort>`.
* Crossterm exposes public `event::poll/read`, but not its internal Unix event
  source or `tty_fd()` helper. A Unix mio backend should use terminal fd
  readiness only as a wake signal, then let crossterm parse events through its
  public API.
* User decision: implement the full Unix mio backend in this task, not only an
  adapter placeholder or prototype.
* User decision: `--interactive-backend` is optional and defaults to `auto`.
  On Unix, `auto` should prefer the mio backend and fall back to compat only
  when mio is unavailable.
* User decision: entering the TUI should show the current backend mode in the
  existing status area rather than a separate debug HUD.

## Research Notes

### What similar tools/patterns do

* Terminal UI loops commonly separate "event collection" from "rendering":
  input/serial/timer events mark the app dirty, and rendering happens only when
  state changes or a periodic timer fires.
* Cross-platform Rust terminal apps often avoid fully unifying OS handles by
  using worker threads/channels for blocking I/O, then a main UI loop waits on a
  channel with a timeout for scheduled timers.
* Lower-level Unix designs can register terminal fd, serial fd, signal fd/waker,
  and timer deadlines in a single `poll`/`mio` wait. This is efficient but less
  portable and requires concrete fd ownership.
* Windows serial I/O usually needs its own design if the goal is true waitable
  serial readiness. A practical implementation uses a blocking reader thread or
  overlapped I/O; reusing the current `serialport` trait does not directly give a
  unified wait set.

### Constraints from our repo/project

* The host package currently has minimal dependencies:
  `crossterm`, `ratatui`, and `serialport`.
* The current serial open helper returns `Box<dyn serialport::SerialPort>`,
  hiding Unix `AsRawFd` and Windows handle details.
* Ratatui rendering is currently unconditional, so even a better serial wait
  still benefits from introducing a dirty-render gate.
* The TUI has timer-like behavior for reconnect attempts and passthrough escape
  timeout; any event loop must preserve those deadlines.

### Feasible approaches here

**Approach A: event-driven main loop with serial reader thread** (Recommended)

* How it works: move blocking serial reads into a dedicated thread, send decoded
  serial data or raw byte chunks to the TUI via `std::sync::mpsc`, and have the
  main thread wait on the channel with a deadline based on reconnect,
  passthrough escape timeout, and render throttle. Terminal input can still be
  drained with crossterm after each wake, or handled by a second input thread if
  needed. Rendering is dirty-driven but capped by a frame interval such as
  16.67 ms for 60 fps or 8.33 ms for 120 fps.
* Pros: portable across macOS, Linux, and Windows; avoids fd/handle abstraction
  leaks; can remove unconditional 16 ms redraws; simpler to stage and test;
  frame-rate policy is independent of platform-specific serial readiness.
* Cons: not a literal single-kernel `select` over serial + terminal; requires
  careful port ownership and shutdown coordination.

**Approach B: Unix-native `mio`/`poll` wait set plus Windows fallback**

* How it works: on Unix, keep/open `serialport::TTYPort`, register its raw fd
  and terminal input fd with `mio` or `poll`, and wait until serial/input/timer
  readiness. The timer deadline includes the next render deadline, so a dirty UI
  can render at 60/120 fps without continuous sleeping. On Windows, either keep
  the current loop or add a separate reader thread.
* Pros: closest to `select/epoll/kqueue`; efficient on macOS/Linux; no serial
  reader thread on Unix; can model serial/input/render deadlines in one wait
  loop on Unix.
* Cons: bigger platform split; current `Box<dyn SerialPort>` helpers need
  refactoring; interacting with crossterm's own internal terminal event source
  may duplicate fd handling unless we avoid crossterm's global `poll/read`
  abstraction; Windows still needs a separate answer for parity.

**Approach D: unified adapter with selectable backends** (Preferred design shape)

* How it works: TUI and passthrough consume a shared `InteractiveEvent` stream
  from an adapter-like backend. A compatibility backend uses serial/input reader
  threads and channels on all platforms. A Unix high-performance backend can be
  added behind the same interface using `TTYPort` raw fd plus terminal/input
  readiness through `mio`/`poll`. Windows keeps the compatibility backend until
  a Windows-specific high-performance backend exists.
* Pros: keeps business logic platform-neutral; allows Unix to switch between
  high-performance and compatibility modes; reduces risk by making the
  compatibility backend the baseline; provides a clean seam for future Windows
  overlapped I/O without rewriting TUI/passthrough.
* Cons: requires designing the adapter boundary carefully; a too-generic
  interface could hide useful control flow such as reconnect deadlines,
  passthrough escape timeout, or render scheduling.

**Approach C: minimal reduced-polling loop**

* How it works: keep the current serial and crossterm calls, but render only
  when dirty and replace fixed `sleep(16ms)` with computed deadlines. Possibly
  increase the serial read timeout when idle.
* Pros: smallest change; low risk; likely reduces idle CPU and unnecessary
  terminal draws.
* Cons: still polling/timeout-based; does not deliver the "select/epoll-like"
  architecture the user is evaluating.

## Decision (ADR-lite, evolving)

**Context**: The TUI should reduce idle polling while supporting a render cap of
60 fps normally and 120 fps in terminals such as Ghostty.

**Decision**: Prefer event-driven main loops built on a shared interactive
event backend interface. The baseline backend uses cross-platform serial/input
reader threads. Unix raw-fd `mio` support should be designed as an optional
backend overlay rather than a separate upper-level loop. Implement the full
Unix mio backend in this task, while keeping the compatibility backend available
for all platforms and for Unix fallback. Apply the architecture to both
`wiremux tui` and `wiremux passthrough`. Add TUI frame-rate detection that uses
120 fps for confidently detected Ghostty terminals and allows explicit CLI
override.

**Consequences**: This keeps macOS/Linux/Windows on one upper-level
architecture and lets TUI render cadence evolve independently of serial
readiness. Unix gains a high-performance backend now, but TUI/passthrough
business logic must stay backend-neutral. Passthrough benefits from the same
serial/input event model but does not need frame scheduling.

### Adapter boundary sketch

The backend should emit input/I/O events, not own UI policy:

* `SerialBytes(Vec<u8>)`
* `SerialDisconnected(io::Error or EOF marker)`
* `TerminalEvent(crossterm::event::Event)`
* `Wake` or timeout return for scheduled deadlines

The upper loop should own:

* `HostSession::feed(...)` and domain event handling.
* Reconnect attempts and manifest request behavior.
* Passthrough escape timeout behavior.
* TUI dirty tracking and render frame caps.
* Exit behavior.

Backend selection should likely support:

* `auto`: Unix may use high-performance backend when available; Windows uses
  compatibility backend.
* `compat`: force thread/channel backend on every platform.
* `mio`: require Unix high-performance backend and fail or fall back with a
  clear diagnostic if unavailable.

### Unix mio backend implementation notes

* Add direct dependencies needed by our own code rather than relying on
  crossterm transitive dependencies.
* Open serial with concrete Unix `TTYPort` through `open_native()` so the backend
  can register `AsRawFd` with `mio::unix::SourceFd`.
* Keep serial parsing in the upper loop by emitting `SerialBytes`.
* Register terminal stdin fd as a readiness wake source. On wake, drain terminal
  events through crossterm public `event::poll(Duration::ZERO)` and
  `event::read()`.
* Preserve resize handling. If crossterm SIGWINCH events are not guaranteed to
  wake the mio backend, add an explicit Unix signal wake source or a bounded
  fallback deadline.
* Computed deadlines must include reconnect, passthrough escape timeout, and TUI
  render frame deadlines.
* If `auto` cannot build or open the mio backend, log/diagnose fallback to
  `compat` instead of failing interactive use.

### Frame-rate detection policy

* CLI override takes precedence.
* If no override is provided and `TERM=xterm-ghostty`, use 120 fps.
* If no override is provided and Ghostty is confidently identified through
  `TERM_PROGRAM`/`TERM_PROGRAM_VERSION`, use 120 fps.
* Otherwise use 60 fps.
* Accepted explicit values should be constrained to known-good targets at first:
  `60` and `120`.
* TUI status area must show at least the active interactive backend label
  (`mio`, `compat`, or fallback detail) and target FPS.
