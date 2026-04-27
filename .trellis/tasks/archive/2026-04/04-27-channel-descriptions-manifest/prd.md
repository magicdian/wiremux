# brainstorm: channel descriptions in manifest

## Goal

Improve host-side usability by using the SDK/device side's existing short
channel `name` metadata as a host display label, while preserving wiremux's
channel-agnostic routing model. The host can display this optional name when it
reads or requests the device manifest.

## What I already know

* wiremux currently treats every channel as an opaque channel and does not
  assign built-in semantics such as console, log, or telemetry.
* The feature uses the existing channel `name` as a short prompt label.
* The name should be carried in manifest information.
* Host prompt display should include the name when present, for example
  `ch1(console)>`, and keep the current display when absent, for example `ch1>`.
* `sources/core/proto/wiremux.proto` already defines
  `ChannelDescriptor.description = 3`.
* Portable C manifest structs and encoder already include
  `wiremux_channel_descriptor_t.description`.
* ESP SDK channel config already includes `esp_wiremux_channel_config_t.description`,
  and `esp_wiremux_emit_manifest()` copies it into manifest channel descriptors.
* Host Rust manifest decode already reads channel `description`.
* Current demo/adapters use longer descriptive text such as
  `ESP-IDF console line-mode adapter`, `ESP-IDF log adapter`, and
  `Demo application text output`.
* Current host display still renders channel prefixes as `chN>`, and TUI status
  only summarizes channel count/max payload.

## Assumptions (temporary)

* Channel descriptions are optional metadata, not protocol routing semantics.
* Descriptions or display labels should be validated at the SDK boundary before
  being exposed in a manifest if they affect terminal prompt rendering.
* Backward compatibility matters for hosts and devices that do not know about
  the new field.

## Open Questions

* None.

## Requirements (evolving)

* Use existing per-channel `name` metadata as the short host display label.
* Treat channel `name` as short display metadata with a maximum of 15 bytes on
  the wire, using a 16-byte C buffer internally where needed to reserve one byte
  for `\0`.
* Core/SDK-side handling should clamp overlong channel names before they affect
  host display.
* Allow UTF-8 channel names. If a name exceeds 15 bytes, clamp to the longest
  valid UTF-8 prefix within 15 bytes to avoid mojibake or invalid manifest
  strings.
* Keep `description` as long-form channel metadata.
* Include configured names/descriptions in manifest data visible to the host.
* Display `chN(name)>` in TUI output when a channel name exists.
* Display `chN(name)>` in non-TUI `wiremux listen` unfiltered output when a
  channel name exists.
* Non-TUI `listen` should passively learn labels only from manifest frames it
  receives; it must not add a proactive manifest request.
* Preserve the current `chN>` display behavior when absent.
* Add an ESP32 demo UTF-8/emoji output channel to exercise UTF-8 channel names
  and UTF-8 payload rendering end to end.
* The demo UTF-8 channel should intentionally configure an overlong emoji name
  such as `🚗🎒😄🔥`, so manifest output exercises UTF-8-safe truncation to the
  longest valid prefix under the 15-byte cap.

## Acceptance Criteria (evolving)

* [ ] Existing channels without names behave and display as they do today.
* [ ] A channel with name `console` is exposed through the manifest and
  displayed by TUI and unfiltered `listen` as `chN(console)>`.
* [ ] Empty/missing names keep the current `chN>` display behavior.
* [ ] Names longer than 15 bytes are clamped by core/SDK-side handling before
  manifest emission.
* [ ] UTF-8 names are clamped without splitting a multi-byte codepoint.
* [ ] Unsafe prompt labels cannot break host terminal display.
* [ ] Non-TUI `listen` uses channel names only after receiving a manifest, and
  falls back to `chN>` if no manifest has been seen.
* [ ] ESP32 demo configures an overlong UTF-8/emoji channel name and exposes the
  truncated valid prefix without mojibake in host prefixes.
* [ ] ESP32 demo emits UTF-8 text and emoji payloads on that channel, and host
  output preserves valid UTF-8.
* [ ] Existing tests/build checks pass.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / build checks pass for affected Rust and ESP-IDF code.
* Docs/notes updated if protocol or SDK behavior changes.
* Compatibility and rollback considered.

## Out of Scope (explicit)

* Assigning built-in meanings to channel numbers inside wiremux.
* Reserving fixed channel roles such as console/log/telemetry.
* Host-side auto-routing based on descriptions.

## Technical Notes

* `sources/core/proto/wiremux.proto`: existing `ChannelDescriptor` includes
  `name`, `description`, directions, payload kinds/types, and interaction modes.
* `sources/core/c/src/wiremux_manifest.c`: optional string fields are omitted
  when NULL/empty; no length validation currently exists for `description`.
* `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`: public
  `esp_wiremux_channel_config_t` already exposes `const char *description`.
* `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`:
  `esp_wiremux_emit_manifest()` copies channel config descriptions into
  manifest descriptors.
* `sources/host/src/manifest.rs`: host decode already stores
  `ChannelDescriptor.description`.
* `sources/host/src/main.rs`: non-TUI unfiltered output currently formats
  `ch{}> ` and does not cache manifests.
* `sources/host/src/tui.rs`: TUI requests/decodes manifest and currently formats
  rendered records as `ch{channel}> `.
* `docs/zh/channel-binding.md` already documents that channel description
  metadata is emitted in the channel 0 `DeviceManifest`, not per data frame.

## Research Notes

### Constraints from the repo

* No new protobuf field is required for a long-form description; the field
  already exists and is backward-compatible for protobuf readers.
* Existing descriptions are human-readable sentences, not short prompt labels.
  Reusing them directly would produce noisy prefixes like
  `ch1(ESP-IDF console line-mode adapter)>`.
* CLI `listen` currently does not request/cache manifest, while TUI does. A
  prompt label feature can be TUI-first unless listen mode also gains manifest
  awareness.

### Feasible approaches here

**Approach A: Reuse `description` as the prompt label**

* How it works: enforce a 16-byte limit on `description`, update built-in
  adapters/examples to short values (`console`, `log`, `telemetry`), and render
  `chN(description)>` when host has manifest data.
* Pros: smallest protocol/API change; matches the field already present in proto,
  C, ESP SDK, docs, and Rust decode.
* Cons: changes the meaning of `description` from long description to compact
  display label; existing docs/examples with longer descriptions must change.

**Approach B: Add a separate short display label field** (recommended if we want
to preserve long descriptions)

* How it works: keep `description` as long-form metadata, add a new optional
  `display_label`/`label` field with a 16-byte limit, expose it in SDK config,
  encode/decode it in manifest, and use it for `chN(label)>`.
* Pros: cleaner semantics; preserves current long descriptions and docs; future
  host views can show both label and description.
* Cons: larger cross-layer change across proto, C core, ESP SDK, host decode, and
  docs.

**Approach C: Use `name` for prompt labels**

* How it works: treat existing `name` as the short user-facing label and render
  `chN(name)>`; leave `description` long-form.
* Pros: likely closest to current data shape because built-ins already use names
  like `console`, `log`, `telemetry`; fewer changes than adding a new field.
* Cons: it slightly shifts the user's proposal from “description” to “name”, and
  `name` may already be expected to identify the channel in other manifest UI.

## Decision (ADR-lite)

**Context**: The wire manifest already separates `name` and `description`.
Existing `description` values are sentence-like long descriptions, while `name`
values are short identifiers suitable for prompts.

**Decision**: Use existing `ChannelDescriptor.name` as the host prompt label.
Keep `description` as long-form metadata.

**Consequences**: No protobuf schema expansion is needed. Host prompt rendering
can become clearer without redefining `description`. Both TUI and non-TUI
unfiltered `listen` should use channel names when manifest metadata is available.

## Display Name Policy Notes

* A 15-byte wire cap is reasonable because channel names are intended to be
  compact prompt labels, not descriptions.
* C-side handling may use a 16-byte buffer to reserve one byte for NUL
  termination; protobuf string payloads themselves do not carry the NUL byte.
* Byte length is easier and more deterministic than character count across C,
  protobuf, and Rust.
* UTF-8 is allowed, but truncation must keep only the longest valid UTF-8 prefix
  within 15 bytes. Invalid source bytes from C strings should not be emitted as
  invalid protobuf strings.
* Host rendering should still sanitize or ignore unsafe names defensively, even
  if SDK registration validates them.
* Emoji names are allowed when they fit the 15-byte UTF-8-safe cap. For example,
  `🚗🎒😄` is 12 bytes. Four 4-byte emoji such as `🚗🎒😄🔥` exceed the cap and
  should clamp to the longest valid prefix that fits, usually the first three
  emoji.

## Display Name Policy Decision

**Context**: Host prompt labels should stay compact, and C-side code needs a
deterministic bound when preparing display names.

**Decision**: Clamp channel `name` to at most 15 bytes in core/SDK-side handling,
with any C buffer sized to 16 bytes to include `\0`.

**Consequences**: Overlong names remain usable but display deterministically.
Implementation must avoid cutting a multi-byte UTF-8 sequence into invalid UTF-8.

## Scope Decision

**Context**: TUI already requests and caches manifest metadata. Non-TUI
`wiremux listen` currently displays unfiltered mux records as `chN>`, but does
not request/cache manifest metadata.

**Decision**: Support `chN(name)>` in both TUI and non-TUI unfiltered listen
output. Filtered `listen --channel N` continues writing raw payload without
prefix, preserving script-friendly behavior. Non-TUI listen should passively
consume manifest frames when they appear; it should not send a manifest request.

**Consequences**: Non-TUI listen must decode manifest responses and update a
channel-label cache. If it starts after the device boot manifest or otherwise
misses manifest metadata, it falls back to the current `chN>` behavior. It should
not print manifest payload as regular channel 0 data in unfiltered mode.

## Demo UTF-8 Decision

**Context**: UTF-8-safe truncation and host rendering need a concrete end-to-end
demo path, not only unit tests.

**Decision**: Add a demo-only UTF-8/emoji output channel, likely channel 4, with
an intentionally overlong emoji channel name such as `🚗🎒😄🔥` and UTF-8/emoji
payload text.

**Consequences**: The demo validates both manifest label handling and terminal
payload display, including UTF-8-safe name truncation. This remains demo
metadata and does not make wiremux assign semantic meaning to the channel.
