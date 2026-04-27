# PR4 Ratatui host TUI

## Goal

Add a usable ratatui-based host TUI for channel-filtered mux debugging.

## Requirements

* Add `wiremux tui --port <path> [--baud ...] [--max-payload ...]
  [--reconnect-delay-ms ...]`.
* Render output, status/help, active filter, manifest summary, and input line.
* Implement `Ctrl-B 0` unfiltered and `Ctrl-B 1..9` channel filtering.
* Submit input lines to channel 1 in unfiltered mode and active channel in
  filtered mode.
* Keep UI state separated from serial/protocol state.

## Acceptance Criteria

* [x] TUI starts and exits cleanly.
* [x] Filter switching changes active filter.
* [x] Submitted input frames target the expected channel.
* [x] Non-TUI commands remain unchanged.

## Technical Notes

Parent task: `.trellis/tasks/04-27-host-ratatui-tui`.
