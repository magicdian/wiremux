# Fix TUI Passthrough Wrapped Cursor Placement

## Goal
Keep the TUI cursor aligned with the active passthrough prompt/input echo when a
console output line wraps inside a narrow output pane.

## Requirements
- In filtered TUI passthrough mode, long console output that wraps must place the
  cursor at the visual end of the active prompt/echo, not on the logical row.
- When the output pane is resized narrower, wrapped visual rows must contribute
  to the scrollback range and scrollbar visibility.
- The existing virtual prompt behavior after completed passthrough output must
  continue to work.
- The fix must stay in the host TUI rendering layer and must not change wire
  protocol, ESP behavior, or host frame encoding.
- Add regression coverage for narrow output panes that force wrapping.

## Acceptance Criteria
- [x] Repeated `help` output in passthrough mode keeps cursor placement aligned
      in a narrow TUI.
- [x] Resizing the terminal wider does not become the only way to recover cursor
      placement.
- [x] Shrinking the terminal shows scrollback when wrapped visual rows overflow,
      even if the logical output-line count fits.
- [x] `cargo test`, `cargo check`, and `cargo fmt --check` pass in
      `sources/host`.

## Technical Notes
The likely failure point is `sources/host/src/tui.rs` cursor placement using the
logical rendered line index and clamped x offset without accounting for
`Paragraph::wrap(Wrap { trim: false })` visual row wrapping.
