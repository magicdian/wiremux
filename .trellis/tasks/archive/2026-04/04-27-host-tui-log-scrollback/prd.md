# Host TUI Log Scrollback

## Goal

Improve the host-side ratatui TUI so users can inspect previous log/output lines with the mouse wheel without losing the current live debugging workflow. While the user is scrolled away from the bottom, the output pane should stop following new incoming lines; it should resume normal auto-follow when the user scrolls back to the bottom or presses Enter twice.

## What I already know

* The user wants the host TUI to support mouse-wheel scrolling through previous logs.
* Wheel scrolling should pause automatic following of new output.
* Normal automatic following should resume when the log view reaches the bottom or when Enter is pressed twice consecutively.
* The repository is a single-repo WireMux project with ESP-IDF C components, a C core, and a Rust host CLI/TUI.
* Current branch is `dev`, the working tree was clean at session start, and there were no active Trellis tasks before this placeholder task was created.
* Current TUI implementation is in `sources/host/src/tui.rs`.
* `run()` currently enters alternate screen/raw mode but does not enable mouse capture.
* The event loop currently handles only `Event::Key`; mouse wheel events are ignored.
* `render()` currently filters output lines, reverses them, and always takes the latest visible lines, so the output pane always follows the tail.
* TUI usage docs live in `docs/zh/host-tool.md`.

## Assumptions (temporary)

* This is a backend/host task touching Rust TUI code and Chinese host-tool docs.
* No protocol or ESP-side behavior changes are required.

## Open Questions

* None.

## Requirements (evolving)

* Enable mouse-wheel scrollback in `wiremux tui`.
* Show a vertical scrollbar on the right side of the output pane when scrollback is available.
* Allow dragging the output pane scrollbar to inspect scroll progress and jump through history.
* Preserve current live-tail behavior until the user scrolls.
* Pause output auto-follow when the user scrolls away from the bottom.
* Resume output auto-follow when the user scrolls back to the bottom.
* Resume output auto-follow after two consecutive Enter keypresses only when the input line is empty.
* Preserve existing input behavior: Enter sends the bottom input line, Esc clears it, Ctrl-C quits, Ctrl-B filters channels.

## Acceptance Criteria (evolving)

* [x] Mouse wheel up shows older output in the TUI output pane.
* [x] A right-side scrollbar is displayed when the output has scrollable history.
* [x] Dragging the right-side scrollbar changes the visible log position.
* [x] New incoming output does not force the view back to the tail while the user is scrolled up.
* [x] Mouse wheel down to the bottom restores automatic tail-follow.
* [x] Pressing Enter twice consecutively with an empty input line restores automatic tail-follow.
* [x] Pressing Enter with non-empty input sends input and does not count toward the restore gesture.
* [x] Existing input send behavior still works.
* [x] Existing channel filter behavior still works with scrollback.
* [x] Host Rust checks/tests pass.

## Technical Approach

Add explicit scrollback state to the host TUI app model and enable crossterm mouse capture while the alternate-screen TUI is active. Treat `scroll_offset = 0` as live auto-follow mode. Mouse wheel up increases the offset and pauses following new output; wheel down decreases the offset and restores auto-follow when it reaches zero. Empty Enter keypresses increment a restore gesture counter while scrolled away from the tail; the second consecutive empty Enter sets the offset back to zero. Any non-empty input send or unrelated key/mouse interaction resets that counter.

Filtered views should compute scroll limits from the currently visible filtered line set so `Ctrl-B` filters and scrollback remain consistent. Incoming lines continue to be stored in the existing bounded `MAX_LINES` buffer; no persistence or protocol change is needed.

## Decision (ADR-lite)

**Context**: The existing TUI always renders the latest lines and has no mouse input handling, which makes previous logs inaccessible during active output.

**Decision**: Implement in-memory viewport scrollback in the host TUI only. Use mouse wheel events for navigation and restore live-follow via bottom scroll or two consecutive empty Enter keypresses.

**Consequences**: The implementation stays local to host TUI behavior and keeps protocol/ESP code untouched. Scrollback remains limited by the existing `MAX_LINES` buffer, which is acceptable for this MVP.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint, typecheck, or build checks pass for the affected area.
* Docs/notes updated if behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* ESP firmware changes.
* Protocol changes.
* Persisting logs beyond the existing in-memory `MAX_LINES` buffer.
* Keyboard-only scroll shortcuts unless added explicitly.
* Resizing/reworking the overall TUI layout.

## Technical Notes

* Task: `.trellis/tasks/04-27-host-tui-log-scrollback`
* Repo inventory from `rg --files` shows primary code under `sources/core`, `sources/esp32`, and `sources/host`.
* Existing docs include Chinese user docs under `docs/zh`.
* Relevant code: `sources/host/src/tui.rs`
* Relevant docs: `docs/zh/host-tool.md`
* Relevant specs: `.trellis/spec/backend/directory-structure.md`, `.trellis/spec/backend/quality-guidelines.md`, `.trellis/spec/backend/error-handling.md`
* Dependency support: `sources/host/Cargo.toml` already depends on `crossterm = "0.29"` and `ratatui = "0.30"` with crossterm support.
* Likely implementation shape:
  * Add mouse capture in `run()` using crossterm mouse capture commands.
  * Extend `App` with a scroll offset / auto-follow state and an Enter restore counter.
  * Handle `Event::Mouse` with `MouseEventKind::ScrollUp` and `ScrollDown`.
  * Change `render()` to compute filtered lines, apply scroll offset, and title/status-indicate paused scroll state.
  * Add unit-testable helpers for scroll state and visible range where practical.
* User decision: only empty-input Enter keypresses count toward the double-Enter restore gesture.
