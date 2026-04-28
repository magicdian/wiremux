# brainstorm: host tui selectable output

## Goal

Update the host-side TUI so text rendered inside ratatui remains practical to select
and copy, especially console output and status content. Also update the project
version to `2604.28.1`.

## What I already know

* The requested version is `2604.28.1`.
* The change is on the host-side TUI implementation.
* Current behavior after entering ratatui prevents selecting internal text.
* Console output and status content should be selectable/copyable.
* Console selection should use the mouse position to decide whether selection
  extends upward or downward, with automatic scrolling while selecting.
* Host TUI lives in `sources/host/src/tui.rs` and uses ratatui + crossterm.
* TUI startup currently enters alternate screen, raw mode, and enables
  crossterm mouse capture.
* The TUI already handles mouse wheel scrollback and scrollbar dragging in
  application code.
* Rendered output rows are built from filtered logical lines and wrapped to the
  output pane width before display.
* Status text is rendered as two ratatui lines in a dedicated status panel.
* Existing TUI unit tests cover scrollback, scrollbar mouse mapping, rendering,
  and cursor behavior.

## Assumptions (temporary)

* The core issue is mouse capture / alternate screen / raw mode interaction
  rather than plain text rendering alone.
* The MVP should preserve existing TUI keyboard input, channel filtering,
  scroll wheel, scrollbar, line-mode, and passthrough behavior.
* Selection should not require replacing ratatui.

## Open Questions

* None.

## Requirements (evolving)

* Bump version metadata to `2604.28.1`.
* Use application-managed selection in the ratatui TUI, not terminal-native
  selection, because mouse capture and ratatui scrollback need to remain under
  app control.
* Allow users to select host TUI console output with a native-terminal-like
  mouse drag interaction.
* Allow users to select host TUI status text with a native-terminal-like mouse
  drag interaction.
* Highlight the active selected range in the TUI.
* Support console selection that can auto-scroll upward or downward based on
  the active mouse position near the output pane edges.
* Do not auto-copy by default after mouse release.
* Keep the selection highlight after scrolling.
* Clear the selection when the user clicks another position or presses `Esc`.
* Reserve the design so auto-copy can become a configurable option later, even
  if the TUI configuration surface is not implemented in this task.
* Prefer using system-native copy expectations when a selection exists, while
  accepting that terminal-reserved shortcuts may not always reach the app.
* When a selection exists, support app-level copy actions:
  * `Command-C` if the terminal forwards it to crossterm.
  * `Ctrl-Shift-C`.
  * `y`.
  * `Enter`.
* Use OSC 52 as the initial low-dependency clipboard write path.
* Preserve the selection after explicit copy unless the user clicks elsewhere or
  presses `Esc`.
* Keep current mouse wheel, scrollbar, line-mode input, passthrough input, and
  channel filter behavior working.

## Acceptance Criteria (evolving)

* [x] Version reports/build metadata use `2604.28.1`.
* [x] Console text displayed in the TUI can be selected and copied.
* [x] Status text displayed in the TUI can be selected and copied.
* [x] Drag selection near the top of console output scrolls upward.
* [x] Drag selection near the bottom of console output scrolls downward.
* [x] Releasing the mouse after selection does not auto-copy by default.
* [x] Selection remains highlighted after scrolling and after copy.
* [x] Clicking another non-selection start position or pressing `Esc` clears the
      selection.
* [x] When a selection exists, `Ctrl-Shift-C`, `y`, and `Enter` copy through the
      app-level clipboard path; `Command-C` is supported when forwarded by the
      terminal.
* [x] The design leaves a clear code path for future `copy_on_select` and
      clipboard backend configuration.
* [x] Existing TUI controls continue to work for normal operation.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green.
* Docs/notes updated if behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Replacing ratatui with a different UI framework.
* Adding a graphical frontend.

## Technical Notes

* Initial task created before Q&A per brainstorm workflow.
* Relevant code:
  * `sources/host/src/tui.rs`: TUI state, render, mouse/key event handling,
    scrollback, status panel.
  * `sources/host/src/interactive.rs`: terminal/serial event backends.
  * `sources/host/Cargo.toml`: crossterm/ratatui versions and crate version.
  * `VERSION`, `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`,
    README badges, and `sources/host/Cargo.lock`: version update sites.
* Relevant specs:
  * `.trellis/spec/backend/directory-structure.md`: host Rust code under
    `sources/host`, protocol parsing remains library-testable.
  * `.trellis/spec/backend/quality-guidelines.md`: run `cargo test`,
    `cargo check`, and `cargo fmt --check` for host changes.
  * `.trellis/spec/backend/error-handling.md`: interactive TUI paths must keep
    interrupted terminal/serial I/O recoverable.

## Research Notes

### Constraints from current implementation

* Native terminal mouse selection conflicts with `EnableMouseCapture`; dragging
  is delivered to the TUI instead of the terminal selection engine.
* Disabling mouse capture globally would make text selectable by the terminal,
  but would also break current in-app mouse wheel/scrollbar handling and cannot
  reliably auto-scroll the ratatui scrollback buffer.
* Application-managed selection can reuse existing rendered row calculations and
  scrollback offsets, making top/bottom edge auto-scroll feasible and testable.

### Feasible approaches here

**Approach A: Application-managed selection + copy command** (Recommended)

* How it works: left-drag in output/status starts an in-app selection, ratatui
  highlights the selected range, dragging near output top/bottom changes
  scrollback offset automatically, and a copy action places the selected text on
  the clipboard/terminal clipboard.
* Pros: preserves current mouse controls, can implement selection across hidden
  scrollback rows, supports deterministic tests.
* Cons: needs a clear copy trigger and clipboard mechanism.

**Approach B: Native terminal selection mode**

* How it works: add a TUI mode or flag that disables mouse capture so the
  terminal can select visible text normally.
* Pros: simple mental model and uses terminal's built-in copy behavior.
* Cons: cannot control ratatui scrollback auto-scroll, loses in-app mouse
  handling while active, selection is limited by terminal behavior.

**Approach C: Hybrid selection mode**

* How it works: normal mode keeps existing mouse capture; a key toggles
  selection mode that either uses app-managed selection or temporarily disables
  mouse capture for native selection.
* Pros: reduces accidental conflict with scrollbar/input interactions.
* Cons: adds mode complexity and more user-facing behavior to document/test.

### Copy shortcut and clipboard conventions

* Windows Terminal keeps selections persistent by default, clears selection on a
  single left click or `Esc`, supports explicit copy actions, and has a
  configurable `copyOnSelect` option. It also supports copy without dismissing
  selection via a `dismissSelection` action parameter.
* Windows Terminal copy bindings include `Ctrl+C`, `Ctrl+Shift+C`,
  `Ctrl+Insert`, and `Enter`, but these operate on Windows Terminal's native
  selection. An application-drawn ratatui selection is invisible to that native
  selection engine unless the application copies the selected text itself.
* GNOME Terminal uses `Shift+Ctrl+C` for copy because plain `Ctrl+C` belongs to
  terminal applications. When an application accepts mouse input, users use
  `Shift` so the terminal can catch mouse selection instead of sending mouse
  events to the app.
* iTerm2 exposes `Copy to pasteboard on selection` as a preference, but if it is
  disabled the user must run the terminal's copy action. It also documents OSC
  52 as the cross-terminal clipboard-write sequence, with user consent required.
* WezTerm/iTerm2/tmux-style copy modes commonly keep a selection in an
  application/terminal mode and bind explicit copy actions such as `y`,
  `Ctrl+Shift+C`, or `Enter`.
* Crossterm can represent `KeyModifiers::SUPER` (Command on macOS), but only
  when keyboard enhancement flags are enabled and only if the terminal sends
  the event to the application. macOS terminal emulators often reserve
  `Command-C` for their own copy menu action, so app-level `Command-C` cannot be
  guaranteed.

### Clipboard implementation implications

* Because wiremux draws the selection itself, native terminal copy shortcuts
  cannot reliably copy the selected text directly.
* To make explicit copy work, wiremux should provide an app-level copy action
  that writes the selected text to the clipboard.
* OSC 52 is the lowest-dependency terminal-native write path and works in many
  modern terminals, but support and permission prompts vary. It is also suitable
  as a future configurable clipboard backend.
* Direct system clipboard crates are an alternative but add platform-specific
  dependencies and may behave differently over SSH/remote terminals.

## Decision (ADR-lite)

**Context**: Native terminal selection conflicts with ratatui mouse capture and
cannot control the TUI's internal scrollback.

**Decision**: Use application-managed selection for the host TUI. The TUI owns
drag selection, highlight rendering, and auto-scroll during console selection.

**Consequences**: Existing mouse behavior can be preserved, and selection can
span ratatui scrollback rows. The implementation must provide an app-level copy
path because the terminal's built-in copy command may not see the app's
highlighted selection as a native terminal selection.

**Copy behavior decision**: The MVP does not auto-copy on selection release.
Selection remains highlighted across scrolling and explicit copy. `Esc` or a
new click outside the current selection clears it. Copy uses app-level actions
(`Command-C` when forwarded, `Ctrl-Shift-C`, `y`, `Enter`) and writes through
OSC 52 initially. Future configuration should be possible for
`copy_on_select` and alternate clipboard backends.
