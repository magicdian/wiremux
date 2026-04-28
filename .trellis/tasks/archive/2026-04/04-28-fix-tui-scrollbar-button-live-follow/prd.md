# Fix TUI Scrollbar Button Live Follow

## Goal
Fix TUI scrollbar up/down button behavior so button clicks jump immediately
instead of animating through large scrollback ranges. The down button must
return to following live output even when new serial data arrives during the
same interaction.

## Requirements
- Treat scrollbar down-button clicks as a live-follow command that sets
  `scroll_offset = 0`.
- Treat scrollbar up-button clicks as a direct jump to the oldest visible
  scrollback position.
- Preserve existing drag animation behavior for coarse scrollbar drag targets.
- Add or update host TUI tests covering button jumps and live output arriving
  around the down-button action.

## Acceptance Criteria
- [x] Clicking the scrollbar down button snaps to `scroll_offset = 0`.
- [x] New matching output arriving while or after the down-button action does not
      leave the viewport above live tail.
- [x] Clicking the scrollbar up button jumps to the oldest visible position
      without long animation.
- [x] Existing wheel and drag behavior remains covered.
- [x] `cargo fmt --check`, `cargo check`, and relevant host tests pass.

## Technical Notes
- Backend-only Rust host/TUI change.
- Follow `.trellis/spec/backend/directory-structure.md` and
  `.trellis/spec/backend/quality-guidelines.md` TUI scroll contracts.
