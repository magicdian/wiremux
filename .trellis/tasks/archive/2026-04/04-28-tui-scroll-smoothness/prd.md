# Fix TUI Scroll Smoothness

## Goal
Make host TUI vertical scrolling feel smooth at the configured 60/120 FPS target, with special attention to scrollbar movement.

## Requirements
- Investigate TUI event loop, render cadence, mouse wheel handling, and scrollbar mapping.
- Reduce visible jumpiness during vertical scrolling without changing Wiremux protocol behavior.
- Keep scrollbar position consistent with the visible output window and live-tail state.
- Preserve existing keyboard, mouse, filtering, passthrough, and manifest behavior.

## Acceptance Criteria
- [ ] Scrolling updates can be coalesced and rendered at the configured frame cadence instead of moving only in large visible jumps.
- [ ] Scrollbar thumb movement is based on the same scroll state as the output pane.
- [ ] Existing host TUI scrollback tests pass, with new or updated tests for smoother scroll behavior if needed.
- [ ] `cargo fmt --check`, `cargo check`, and relevant `cargo test` pass under `sources/host`.

## Technical Notes
This is host Rust/TUI work only. Avoid protocol, ESP, or portable C changes unless investigation proves they are required.
