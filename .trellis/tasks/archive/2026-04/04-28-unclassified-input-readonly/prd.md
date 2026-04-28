# brainstorm: unclassified input should be read-only

## Goal

Make TUI input behavior respect the distinction between unclassified output and
channel-scoped sessions. When no channel is selected or specified, the view
should be read-only and must not route input to channel 1 by default. When a
specific channel is selected, input availability and input mode should follow
that channel's configuration.

## What I already know

* The current behavior appears to route input without an explicit channel to
  channel 1.
* Wiremux allows each channel to configure whether it has input and/or output.
* Users may configure all channels as log-output-only, so defaulting input to
  channel 1 can violate the configured channel model.
* If channel 1 is in passthrough mode, unclassified input can cause cursor
  movement or terminal control behavior in the wrong view.
* Code inspection confirmed that `App::active_input_channel()` currently falls
  back to channel 1 when `filter` is `None`.
* Both TUI line input and TUI passthrough input call the active input channel
  helper, so the fallback affects both modes.
* Manifest channel descriptors already expose channel directions and interaction
  modes, so this likely does not require a protocol/schema change.
* Existing Chinese host-tool documentation explicitly says no-filter TUI input
  defaults to mux channel 1; docs must be updated with the new behavior.

## Assumptions (temporary)

* "Unclassified" means the aggregate or no-channel view, not a concrete channel
  session.
* Channel-scoped input mode is already represented somewhere in the manifest or
  runtime channel config.
* This should be a TUI/runtime behavior change, not a protocol redesign.

## Open Questions

* None.

## Requirements (evolving)

* Unclassified/no-channel context is read-only.
* Unclassified/no-channel context does not route typed input to channel 1 or any
  fallback channel.
* Explicit channel context uses that channel's configured input capability.
* Explicit channel context uses that channel's configured input mode, including
  line and passthrough behavior.
* Channel input capability should be derived from the manifest `directions`
  field when a manifest is available.
* Read-only state should be visible in the TUI, including status/input labeling.
* Read-only state should not accumulate line input, send passthrough bytes, or
  show an editable input cursor.

## Acceptance Criteria (evolving)

* [x] Typing while no channel is selected sends no input bytes to any channel.
* [x] Typing while channel 1 is selected only sends input if channel 1 allows
  input.
* [x] Typing while another channel is selected only sends input if that channel
  allows input.
* [x] Passthrough mode is only active for an explicitly selected passthrough
  channel.
* [x] A channel with no manifest input direction is treated as read-only.
* [x] The TUI status/input panel clearly shows read-only for unclassified view
  and non-input channels.
* [x] Read-only contexts do not accumulate typed characters in the input buffer.
* [x] Read-only contexts do not place the cursor as though input were editable.
* [x] Tests or focused verification cover the no-channel read-only case.
* [x] Documentation no longer says no-filter TUI input defaults to channel 1.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI-relevant checks pass.
* Docs/notes updated if behavior or user-facing semantics change.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Changing channel manifest schema unless inspection shows the existing schema
  cannot express the desired behavior.
* Adding new UI views unrelated to input routing.
* Changing the CLI `listen --line` / `--send-channel` behavior.
* Supporting channels above 9 in the existing `Ctrl-B` numeric filter shortcut.

## Technical Approach

Model the TUI input target as optional: all/unclassified view has no input
target, while filtered channel views have a concrete channel. Then derive an
input state from the active target and manifest metadata:

* `ReadOnly`: no active input target, or manifest says the active channel does
  not support `DIRECTION_INPUT`.
* `Line`: active channel supports input and does not advertise passthrough as
  the active/default interaction mode.
* `Passthrough`: active channel supports input and advertises passthrough.

Use this derived state consistently in key handling, prompt rendering, status
text, passthrough prompt/cursor rendering, and tests.

## Decision (ADR-lite)

**Context**: The previous TUI behavior treated the all-channel view as an
implicit channel 1 input session. That conflicts with per-channel
input/output capability declarations and causes passthrough cursor behavior to
appear in an unclassified output view.

**Decision**: Implement option 3: all/unclassified is read-only, explicit
channel input follows manifest direction and interaction metadata, and the TUI
visibly marks read-only contexts.

**Consequences**: This removes the implicit channel 1 convenience from TUI all
view. Users who want to type into channel 1 must explicitly select channel 1
with the existing shortcut.

## Technical Notes

* Initial task created from user brainstorm request on 2026-04-28.
* Likely impacted implementation: `sources/host/src/tui.rs`.
* Likely impacted docs: `docs/zh/host-tool.md`; possibly README TUI summaries if
  they mention routing semantics.
* Existing helper behavior:
  * `App::active_input_channel()` returns `filter.unwrap_or(1)`.
  * `App::active_input_is_passthrough()` uses that helper, so an all-channel view
    can accidentally enter passthrough if channel 1 advertises passthrough.
  * `handle_key()` sends line input on Enter to `active_input_channel()`.
  * `send_tui_passthrough_key()` sends per-key payloads to
    `active_input_channel()`.
* Existing manifest fields:
  * `ChannelDescriptor.directions` contains `DIRECTION_INPUT` and/or
    `DIRECTION_OUTPUT`.
  * `ChannelDescriptor.default_interaction_mode` and `interaction_modes` drive
    line vs passthrough behavior.
* Implementation completed in `sources/host/src/tui.rs`.
* Behavior/spec docs updated in `docs/zh/host-tool.md`,
  `.trellis/spec/backend/directory-structure.md`, and
  `.trellis/spec/backend/quality-guidelines.md`.
* Verification passed: `cargo fmt --check`, `cargo check`, and `cargo test` in
  `sources/host`.
