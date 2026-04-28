# brainstorm: passthrough input hint

## Goal

Improve the host-side TUI input experience when the channel input mode is
`passthrough`, so the bottom input area clearly communicates that text should be
typed directly into the remote channel rather than into the host command input.

## What I already know

* The user wants a hint similar to the existing readonly hint.
* The target behavior is only for channel input mode `passthrough`.
* The motivation is to avoid users thinking there are two active input places.
* `sources/host/src/tui.rs` already distinguishes `InputState::ReadOnly`,
  `InputState::Line`, and `InputState::Passthrough`.
* In passthrough mode, the TUI already places the cursor in the output pane via
  `set_passthrough_cursor`; the bottom input area only needs clearer rendering.
* The readonly bottom input hint currently renders a dark gray `> read-only`.

## Assumptions (temporary)

* The host-side UI is the Rust TUI under the host package.
* There is already readonly-state rendering logic that can be reused or mirrored.
* The MVP should be a visual hint/state change, not a change to input routing.

## Open Questions

* None.

## Requirements (evolving)

* Show an explicit hint in the bottom input area when channel input mode is
  `passthrough`.
* Use the exact passthrough hint text: `passthrough: type in output pane`.
* Render the passthrough hint with readonly-like subdued styling.
* Keep existing passthrough input behavior unchanged.
* Keep the active passthrough cursor in the output pane.

## Acceptance Criteria

* [x] When channel input mode is `passthrough`, the bottom input area shows a
  clear hint that host input is not the active typing target.
* [x] The hint is visually consistent with the existing readonly hint pattern.
* [x] Normal and readonly input modes continue to render as before unless the
  final design intentionally changes them.
* [x] Existing passthrough cursor and prompt rendering tests continue to pass.

## Implementation Summary

* Updated bottom input rendering to match on `InputState`, with a dedicated
  passthrough hint branch.
* Added a `ratatui::backend::TestBackend` render test that verifies the
  passthrough hint appears and stale line input text is hidden.
* Verified with `cargo test`, `cargo check`, and `cargo fmt --check` in
  `sources/host`.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green.
* Docs/notes updated if behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Changing the passthrough routing semantics.
* Adding a new settings UI or persistent configuration.
* Redesigning the full host TUI layout.

## Technical Notes

* TUI state and rendering live in `sources/host/src/tui.rs`.
* `App::active_input_state()` derives passthrough from manifest metadata and the
  active channel filter.
* `render()` currently has a binary bottom input rendering branch:
  `ReadOnly` renders `> read-only`; all other states render green `>` plus
  `app.input`.
* `set_cursor_position()` already sends the cursor to the output pane for
  `InputState::Passthrough`.
* Existing render tests use `ratatui::backend::TestBackend` and `buffer_row()`.

## Feasible Approaches

**Approach A: Distinct passthrough hint in bottom input (recommended)**

* How it works: render passthrough with a readonly-like gray hint, for example
  `> passthrough: type in output pane`.
* Pros: directly solves the confusion, keeps line/readonly/passthrough visually
  distinct, low implementation risk.
* Cons: adds one more English UI string to maintain.

**Approach B: Reuse readonly copy/style exactly**

* How it works: render `> read-only` for both readonly and passthrough.
* Pros: minimal change and visually consistent.
* Cons: misleading because passthrough input is not read-only; typing still
  sends keys to the channel.

**Approach C: Status/title only**

* How it works: leave the bottom line empty, rely on the existing input title
  `input: passthrough...` or update that title.
* Pros: smallest visual change.
* Cons: less discoverable; does not address the confusing empty input line.

## Decision (ADR-lite)

**Context**: The TUI already routes passthrough typing to the output pane and
places the cursor there, but the bottom input area still looks like an empty
active input.

**Decision**: Render passthrough mode in the bottom input area with the subdued
hint `> passthrough: type in output pane`.

**Consequences**: The UI distinguishes readonly from passthrough while reusing
the existing visual pattern for inactive bottom input states. Passthrough routing
and cursor behavior remain unchanged.

## Expansion Notes

* Future evolution: the TUI could later centralize all input-state hints if more
  interaction modes are added.
* Related scenarios: readonly, line, and passthrough states should remain
  visually distinct in both the status line and bottom input area.
* Failure/edge cases: narrow terminals may truncate long hints naturally through
  the existing paragraph rendering; this task does not add horizontal scrolling
  or wrapping behavior for the one-line input box.
