# brainstorm: improve TUI status navigation

## Goal

Improve the Wiremux TUI status area so important runtime information remains
accessible when the terminal is not wide enough to show every status field at
once.

## What I already know

* The status area now contains enough information that narrow terminal windows
  can hide important fields.
* The user wants to compare `minicom` and GNU `screen` before choosing key
  behavior.
* If comparable tools do not reserve left/right arrows for a stronger
  convention, the user is open to using left/right to switch status content.
* The current Wiremux TUI has a fixed `status` block with two content rows and
  height 4.
* The first status row currently includes filter, input mode, backend, FPS, and
  transient status text.
* The second status row includes connected port, target port, manifest/device
  details, virtual serial summary, and ESP enhanced summary.
* Output rows already wrap to fit the available width; status rows currently
  render as long single rows inside a fixed-height block.
* Mouse selection and copy already support selecting text from the status pane.
* The settings UI already uses left/right for text cursor movement and confirm
  button focus, so status navigation must not interfere while settings or
  settings popups are open.
* Passthrough input maps arrow keys to terminal escape sequences for the device,
  so bare left/right must not be stolen while passthrough input is active.

## Assumptions (temporary)

* This round should improve discoverability and accessibility of status
  information without redesigning the whole TUI layout.
* The MVP should keep status visible in the existing lower panel rather than
  adding a separate full-screen diagnostics view.
* Status navigation can be implemented in the TUI app state without changing
  the Wiremux device protocol.

## Open Questions

* None.

## Requirements (evolving)

* Preserve all existing status information somewhere in the TUI.
* Avoid taking left/right arrow input away from the remote device during
  passthrough mode.
* Preserve existing settings-popup left/right behavior.
* Keep status text selectable/copyable.
* Add focused unit/render tests for narrow-width status behavior and key
  handling.
* Define status fields through priority metadata so earlier dynamic pages keep
  the most useful fields visible as the terminal narrows.
* Priority `0` is the highest priority. Larger numbers are lower priority.
* Multiple status fields may share the same priority.
* Fields with the same priority sort by stable field id in `a-z` order. Display
  labels are not used for ordering, so label text can change without changing
  layout behavior.
* Priority values come from a checked-in built-in TOML file for this MVP. The
  TOML is read at compile time and converted into the compiled-in default truth
  for that build.
* The MVP must not read user status-priority config at runtime, but the TOML
  schema should be compatible with future user overrides.
* Compute status pages dynamically from the sorted field list and current
  status panel width. A wide terminal can collapse all fields into `1/1`; a
  narrow terminal can expand the same fields into `1/N`, `2/N`, etc.
* Status page content must be recalculated from the current terminal/status
  panel size during rendering. Resizing from narrow to wide must allow the
  status view to reduce page count and fill newly available width immediately,
  instead of preserving a stale narrow layout with empty space.
* In non-passthrough modes, bare `Left` / `Right` switches status pages.
* In passthrough mode, bare `Left` / `Right` continues to be forwarded to the
  device; status pages can be switched through the Wiremux prefix with
  `Ctrl-B Left` / `Ctrl-B Right` and `Ctrl-B [` / `Ctrl-B ]`.
* Show the active dynamic status page in the status panel title so page navigation is
  discoverable.
* Page navigation must default to non-wrapping behavior: pressing previous on
  the first page or next on the last page keeps the current page and reports the
  boundary.
* Keep the page-navigation implementation extensible so a future option can
  switch between non-wrapping and cyclic page behavior.

## Acceptance Criteria (evolving)

* [x] At 80 columns, the status area still shows the core live state without
      overlapping text.
* [x] At narrower practical widths, secondary status fields remain reachable by
      keyboard instead of being permanently clipped.
* [x] Left/right behavior does not break passthrough arrow-key forwarding.
* [x] Left/right behavior does not break settings text-input or confirm popup
      navigation.
* [x] Tests cover status page/segment navigation and rendering.
* [x] `Ctrl-B Left` / `Ctrl-B Right` and `Ctrl-B [` / `Ctrl-B ]` switch status
      pages in passthrough and non-passthrough modes.
* [x] Dynamic status-page rendering keeps higher-priority status fields before
      lower-priority fields at constrained widths.
* [x] Resizing the terminal wider recomputes the status-page layout and fills
      newly available status width with eligible fields.
* [x] Page navigation does not wrap by default.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green for touched code.
* Docs/notes updated if behavior or platform support changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* A full TUI redesign.
* Changing Wiremux protocol fields or device manifests.
* Replacing the existing output scrollback and selection model.
* Reworking settings-panel layout beyond avoiding key conflicts.

## Research Notes

### What similar tools do

* `minicom` uses `Ctrl-A` as a command prefix and uses arrow keys inside its
  menus. Its status line is a compact terminal affordance; when a bottom status
  line cannot fit, the status is shown when pressing the command prefix. It also
  has a configuration option to enable/disable the status line.
* GNU `screen` sends normal input to the active program by default and uses
  `Ctrl-A` as the command prefix. In copy/scrollback mode, left/right arrows
  move the cursor horizontally. Its copy-mode keymap can be customized through
  `markkeys`.
* Neither tool suggests stealing bare left/right arrows from an active terminal
  passthrough session. Both reserve richer navigation for command/menu/copy
  contexts rather than raw terminal input.

### Constraints from our repo/project

* `sources/host/wiremux/crates/tui/src/lib.rs` owns the App state, key handling,
  rendering, status row construction, and tests.
* `main_layout()` fixes the status panel at height 4, leaving two content rows.
* `status_rows(app)` currently returns two `StatusRow` values with many
  segments each.
* `handle_key_with_areas()` handles global keys, settings, selection copy,
  prefix commands, passthrough input, and line input in one place.
* `interactive::passthrough_key_payload()` maps `KeyCode::Left` and
  `KeyCode::Right` to ANSI escape sequences, so bare arrow keys are data in
  passthrough mode.
* `docs/wiremux-tui-menuconfig-style.md` documents existing settings
  left/right contracts and should be updated if any TUI key contract changes
  touch settings-style expectations.

### Feasible approaches here

**Approach A: Priority-aware dynamic status pages with safe Left/Right** (Recommended)

* How it works: define status fields with priority metadata. At render time,
  sort fields by `(priority, id)` and pack them into two-row status pages using
  the current status panel width. Fields that do not fit on the current page
  move to the next dynamic page. Bare left/right switches pages only when the
  TUI is not in passthrough mode and no settings popup is active. In
  passthrough, arrows keep going to the remote channel and prefix navigation
  remains available.
* Pros: keeps the most useful status visible on small windows, preserves
  passthrough correctness, and gives a natural path to future configurable
  status layout.
* Cons: slightly more implementation structure than fixed status rows; priority
  choices must be tested so important fields do not disappear unexpectedly.

**Approach B: Status pages through `Ctrl-B` prefix only**

* How it works: keep bare arrows entirely reserved for input contexts and add
  `Ctrl-B [` / `Ctrl-B ]` or `Ctrl-B Left` / `Ctrl-B Right` to switch status
  pages.
* Pros: no ambiguity with passthrough or future line-input cursor movement.
* Cons: less ergonomic and less discoverable for a frequently inspected status
  panel.

**Approach C: Compact rotating status row**

* How it works: keep two rows but show only high-priority summary fields, with
  a rotating detail slot that can be switched by key or automatically cycled.
* Pros: preserves the current panel footprint and gives a dashboard-like feel.
* Cons: automatic cycling can be distracting, and manual cycling becomes less
  explicit than named pages.

## Technical Notes

* Files inspected:
  * `sources/host/wiremux/crates/tui/src/lib.rs`
  * `sources/host/wiremux/crates/interactive/src/lib.rs`
  * `docs/wiremux-tui-menuconfig-style.md`
  * `README.md`
* External references:
  * GNU Screen manual, copy-mode movement and key binding behavior:
    https://www.gnu.org/software/screen/manual/html_node/Movement.html
  * GNU Screen manual, command prefix and key-binding model:
    https://www.gnu.org/software/screen/manual/screen.html
  * minicom man page, menu arrow keys and status line behavior:
    https://man7.org/linux/man-pages/man1/minicom.1.html

## Technical Approach

Implement dynamic status pages in the TUI app state. Render only the active
computed page inside the existing fixed-height status block, and put the
`current/total` page indicator in the block title. Keep all status data
generated from existing app state.

Represent status fields as structured items with stable id, label, and priority.
The renderer should lay out fields in priority order and move lower-priority
fields to later dynamic pages when the available content width is too small.

Store the default status-priority configuration in a checked-in TOML file owned
by the TUI crate. A build script should parse/validate that TOML at compile time
and generate Rust constants included by the TUI. That keeps the priorities easy
to edit without making runtime behavior depend on a mutable user file.

Proposed built-in TOML shape:

```toml
version = 1
navigation = "clamp" # future: "cycle"

[[field]]
id = "status.current"
label = "status"
priority = 0
summary = true

[[field]]
id = "filter.active"
label = "filter"
priority = 1
summary = true
```

Validation rules:

* `version` must be supported.
* `navigation` must be `clamp` for the MVP; `cycle` can be reserved but not
  enabled until implemented.
* `id` values must be unique.
* `priority` values must be non-negative integers.
* Fields sort by `(priority, id)`.
* Fields that do not fit on the active dynamic page must remain reachable on a
  later page.

Render/layout rules:

* Status rows and page count are derived during render from the current status
  content width.
* Do not cache the rendered field list only by active page; terminal resize must
  naturally change which fields fit and how many pages exist.
* Prefer placing fields in priority order across the two status content rows.
* If no remaining field fits a row, stop adding lower-priority fields to that
  row instead of truncating labels into unreadable fragments.

Recommended default priority model:

| Field id | Display label | Priority | Rationale |
| --- | --- | --- | --- |
| `status.current` | `status` | 0 | Most actionable transient state, including errors, copy results, prefix state, and scrollback state. |
| `input.mode` | `input` | 1 | Determines whether the user can type, and whether input is line or passthrough. |
| `filter.active` | `filter` | 1 | Determines which channel is visible and which channel receives input. |
| `connection.connected` | `conn` | 2 | Shows the resolved active device path or disconnect state. |
| `connection.target` | `target` | 2 | Shows requested physical serial target; useful when config/auto-resolution differs from connected path. |
| `device.api` | `api` | 3 | Device protocol API version is important compatibility metadata. |
| `enhanced.esp` | `esp` | 4 | ESP enhanced endpoint/flashing state is important when active but secondary to core connection/input state. |
| `enhanced.vtty` | `vtty` | 4 | Generic virtual serial endpoint state is useful but not always active. |
| `runtime.backend` | `backend` | 5 | Useful diagnostic detail, less important during normal operation. |
| `runtime.fps` | `fps` | 5 | Mostly rendering diagnostics. |
| `device.channels` | `channels` | 6 | Manifest detail for inspection/debugging. |
| `device.firmware` | `firmware` | 6 | Useful context but often long and less immediately actionable. |
| `device.max_payload` | `max_payload` | 6 | Low-frequency protocol detail. |
| `device.name` | `device` | 6 | Useful identity field, but low priority if width is constrained. |

Use bare `Left` / `Right` only when the active input state is not passthrough.
Use `Ctrl-B Left` / `Ctrl-B Right` plus `Ctrl-B [` / `Ctrl-B ]` as the
always-available status-page navigation path. Preserve settings popup handling
by keeping settings key dispatch before global status navigation.

Model page navigation with an explicit mode, defaulting to non-wrapping. The
initial implementation should clamp at the first/last page, while leaving a
small enum or equivalent extension point for future cyclic behavior.

## Decision (ADR-lite)

**Context**: The status panel now contains more fields than a narrow terminal
can display. Comparable terminal tools reserve raw keys for active terminal
input and use command/menu contexts for UI navigation.

**Decision**: Use priority-aware dynamic status pages with safe left/right navigation:
bare arrows in non-passthrough modes, prefix-based arrows/brackets in
passthrough and all other modes. Default page navigation is non-wrapping, with a
reserved navigation-mode extension point for cyclic behavior later. Status
priority is defined by a checked-in built-in TOML file parsed at compile time,
and uses stable field ids so future user configuration can override priorities
without changing the renderer contract.

**Consequences**: Status details remain reachable without increasing panel
height, and earlier pages remain useful as width decreases. Passthrough remains
faithful to the remote terminal. Users need the status title or docs to discover
the prefix fallback. Priority defaults become part of the TUI UX contract and
need focused render tests.
