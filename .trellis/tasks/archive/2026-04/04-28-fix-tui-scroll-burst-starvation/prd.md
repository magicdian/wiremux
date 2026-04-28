# Fix TUI Scroll Burst Starvation

## Goal
Fix the host TUI freeze that happens after a large scrollback session when queued
mouse-wheel down events continue to be processed after the view has already
returned to live tail.

## Requirements
- Preserve one wrapped visual row per wheel event for normal scrolling.
- Coalesce stale wheel-down events once the TUI reaches live tail so they do not
  block later wheel-up or quit key handling.
- Keep existing scrollbar drag animation and live-output follow behavior.
- Add focused unit coverage for burst coalescing semantics.

## Acceptance Criteria
- [x] Consecutive wheel-down events at live tail are cheap no-ops.
- [x] A wheel-down burst that reaches live tail can be followed immediately by a
      wheel-up action without processing all remaining stale wheel-down events.
- [x] Existing TUI scrollback tests still pass.
- [x] `cargo fmt --check`, `cargo check`, and `cargo test` pass in
      `sources/host`.

## Technical Notes
- Primary target: `sources/host/src/tui.rs`.
- Prefer a small helper around mouse wheel burst handling that is easy to unit
  test without a real terminal.
- Avoid changing protocol, serial backend, or ESP code.
