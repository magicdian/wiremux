# brainstorm: polish TUI product experience

## Goal

Improve the host TUI product experience around exit shortcuts, text input
cursor feedback, and passthrough prompt behavior so the interface feels closer
to established terminal tools and remains comfortable on macOS terminals.

## What I already know

* The current TUI exits with `Ctrl-C` or `Ctrl-]`.
* macOS terminal behavior makes an additional host-platform-friendly shortcut
  desirable.
* The listen mode already uses a `Meta-X`-style exit pattern comparable to
  minicom. On macOS, Meta is expected to correspond to `Esc`.
* Line mode input and passthrough text entry currently lack a visible cursor,
  making it harder for users to locate the insertion point.
* Passthrough mode currently appears to show `ch1(console) >` only when text is
  present; pressing Enter on empty input can leave a blank line.
* The user provided a local passthrough log path for investigation:
  `/private/var/folders/pt/4b60vbw532sfnr8x_vfqg14c0000gn/T/wiremux/wiremux-1777296832-365606-dev_tty.usbmodem2101.log`.
* The repository already documents standalone `wiremux passthrough` as using
  `Ctrl-]` when supported and `Esc` then `x` as the portable exit sequence.
* TUI currently shows a bottom input box rendered by ratatui and hides the
  terminal cursor because no cursor position is set during draw.
* TUI currently suppresses the channel prefix for empty passthrough output
  lines, which explains the blank-looking line after an empty prompt submission.

## Assumptions (temporary)

* This is backend/host work in the Rust TUI layer rather than a frontend task.
* The preferred macOS-friendly shortcut should be added without removing the
  existing `Ctrl-C` and `Ctrl-]` exits unless code inspection reveals a conflict.
* Cursor visibility should be enabled by default and configurable through an
  existing configuration surface if one exists.
* Passthrough prompt behavior should match shell-like expectations: an empty
  submission should still preserve a visible prompt line rather than producing
  visually blank output.

## Open Questions

* None for MVP.

## Requirements (evolving)

* Add a `Meta-X` / `Esc` then `x` exit shortcut for TUI behavior, aligned with
  listen mode/minicom conventions.
* Add a visible blinking cursor to line mode and passthrough text input, default
  enabled. In line mode the cursor belongs in the bottom input box; in
  passthrough mode the cursor should be rendered in the upper terminal/output
  pane after the active channel prompt/echo to make the UI feel like focus is
  in the terminal.
* Do not add a new cursor configuration surface in this MVP.
* Make passthrough prompt rendering stable when submitting empty input.

## Acceptance Criteria (evolving)

* [x] Existing TUI exit shortcuts continue to work unless explicitly changed.
* [x] `Meta-X` / macOS `Esc` then `x` exits the relevant TUI flow.
* [x] Line mode input displays a blinking cursor at the insertion position by
  default.
* [x] Passthrough text input displays a blinking cursor in the upper terminal
  pane at the active channel prompt/echo position by default.
* [x] Pressing Enter on empty passthrough input does not render as an
  unintuitive blank prompt area.
* [x] When passthrough command output completes with a non-empty line, the TUI
  renders a current prompt row at live tail so the cursor does not sit at the
  end of the previous response.
* [x] Empty passthrough `CRLF` echoes behave like a terminal Enter: they create
  completed prompt history rows, and the TUI render layer still shows the next
  current prompt at live tail.
* [x] Behavior is covered by focused tests where the codebase has testable TUI
  state/rendering seams.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI-equivalent commands pass.
* Docs/notes updated if behavior changes user-facing shortcuts or configuration.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* A full TUI redesign.
* Replacing the existing terminal backend unless required by repo constraints.
* Adding a new frontend application.

## Technical Notes

* Initial PRD created before code inspection as required by the brainstorm
  workflow.
* Relevant files inspected:
  * `sources/host/src/tui.rs`: TUI state, key handling, rendering, passthrough
    stream rendering, unit tests.
  * `sources/host/src/main.rs`: standalone passthrough exit handling and helper
    functions.
  * `docs/zh/host-tool.md` and `README.md`: documented passthrough exit
    sequence.
  * Provided diagnostics log: confirms normal channel output/reconnect behavior;
    the reported prompt issue is in TUI render/state rather than diagnostics.
* Existing code patterns:
  * Standalone passthrough keeps `Esc` pending for 750 ms, exits if the next key
    is `x`/`X`, and otherwise forwards the original `Esc` before processing the
    following key.
  * TUI uses `handle_key` as the central input dispatch and has focused unit
    tests for passthrough input and stream rendering.
  * TUI renders empty passthrough lines as `Line::from("")`, intentionally
    hiding the `chN(name)> ` prefix for empty text.
* Local dependency check:
  * `ratatui` supports `frame.set_cursor_position`, which should show the
    terminal-native blinking cursor when the terminal supports blinking.
  * `cargo test --manifest-path sources/host/Cargo.toml --no-run` already
    succeeds before implementation.
* Implementation notes:
  * `sources/host/src/tui.rs` now accepts `Alt-x` and `Esc` then `x` as TUI
    exit shortcuts.
  * TUI keeps `Esc` pending briefly so single `Esc` can still clear line input
    or be forwarded in passthrough mode when it is not part of the exit
    sequence.
  * TUI uses `frame.set_cursor_position` for the terminal-native cursor. Line
    mode places it in the bottom input box; passthrough mode places it in the
    upper output pane after the active channel prompt/echo.
  * Empty passthrough stream lines now render the channel prompt instead of an
    empty display row.
  * TUI render now adds a virtual passthrough prompt row at live tail when the
    active channel's latest line is complete and non-empty. This row is not
    stored in history, so the next echoed key naturally replaces it without
    forcing the user to press Enter to create a prompt.
  * Empty passthrough newlines preserve terminal semantics: each empty `CRLF`
    becomes a completed prompt history row. The current prompt remains a render
    overlay at live tail, so empty Enter advances to the next prompt without
    requiring extra Enter presses.
  * User-facing shortcut docs were updated in `docs/zh/host-tool.md`,
    `README.md`, and `README_CN.md`.

## Research Notes

### What similar tools do

* Minicom-like tools use a portable escape-prefix workflow because raw terminal
  control-key behavior differs across terminals and platforms.
* Shell-like prompts keep a visible prompt even for empty command submissions;
  an empty command still advances the prompt rather than producing visually
  ambiguous blank UI.
* TUIs commonly use the terminal-native cursor for text entry instead of
  drawing a custom cursor, so cursor blinking remains controlled by terminal
  settings.

### Constraints from our repo/project

* Standalone passthrough already implements the desired `Esc` then `x` fallback.
* TUI currently has no persisted config surface and `TuiArgs` has only port,
  baud, max payload, and reconnect delay.
* Adding CLI flags changes command help/docs/tests but is still a narrow host
  change.
* A custom drawn cursor would be more testable in buffer snapshots, but it
  risks conflicting with text layout and terminal cursor behavior.

### Feasible approaches here

**Approach A: Native cursor + CLI opt-out** (Recommended)

* How it works: set cursor position in the bottom input area on each draw,
  default enabled, add a TUI flag such as `--no-cursor` only if we want
  immediate configurability.
* Pros: terminal-native blinking, small implementation, matches ratatui
  conventions.
* Cons: blink timing is controlled by terminal settings, not our app.

**Approach B: Native cursor, no public config yet**

* How it works: always set cursor position in the input area, document the
  behavior, defer configurability until there is a broader host config story.
* Pros: smallest scope, no CLI surface churn.
* Cons: does not satisfy the "configurable" part immediately.

**Approach C: Draw a synthetic blinking cursor in the buffer**

* How it works: maintain a blink timer and render a styled block/pipe inside
  the input text.
* Pros: app controls blink cadence and it is easy to snapshot test.
* Cons: more state, more layout edge cases, less terminal-native.

## Decision (ADR-lite)

**Context**: TUI input currently lacks a visible cursor. `ratatui` supports
placing the terminal cursor directly, and terminals provide native blinking.

**Decision**: Use the terminal-native cursor in the TUI input area, enabled by
default, without adding a new CLI flag or config field in this MVP.

**Consequences**: The implementation stays small and terminal-native. Blink
cadence remains controlled by the user's terminal. A public cursor setting can
be added later when the host tool has a broader configuration story.
