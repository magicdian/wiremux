# brainstorm: ESP demo enum conversion cleanup

## Goal

Understand why the ESP demo/component exposes `ESP_WIREMUX_*` enum values that
mirror core `WIREMUX_*` enum values, and decide whether the ESP demo and/or ESP
component should switch to the raw core enum values.

## What I already know

* The user noticed conversion enums such as `ESP_WIREMUX_DIRECTION_OUTPUT`.
* The user suspects this may be compatibility layering from an earlier design.
* The repository contains a core C implementation under `sources/core/c` and an
  ESP-IDF wrapper/demo under `sources/esp32`.
* `sources/esp32/components/esp-wiremux/include/esp_wiremux.h` defines ESP enum
  constants as aliases to core constants, for example
  `ESP_WIREMUX_DIRECTION_OUTPUT = WIREMUX_DIRECTION_OUTPUT`.
* The ESP component public API uses ESP-prefixed types and names, while the
  implementation serializes core `wiremux_*` protocol structs.

## Assumptions (temporary)

* The visible enum conversion may be an ESP-IDF public API wrapper over the
  portable core C API.
* Changing public ESP enum names could affect demo code, docs, and user-facing
  ESP component compatibility.

## Open Questions

* None.

## Requirements (evolving)

* Identify whether `ESP_WIREMUX_*` enum values provide compatibility, API
  isolation, type separation, or no practical benefit.
* Evaluate whether replacing them with raw `WIREMUX_*` values is safe.
* Preserve direction validation: channel `directions` may be an input/output
  bitmask, but envelope/write `direction` must be a single input or output value.

## Acceptance Criteria (evolving)

* [x] Existing enum definitions and conversion call sites are identified.
* [x] Trade-offs of keeping vs removing ESP wrapper enums are documented.
* [x] A recommended direction is proposed with clear scope.
* [x] ESP public header documents why ESP aliases intentionally mirror core
  wire-protocol enum values.

## Definition of Done (team quality bar)

* Tests added/updated if implementation proceeds.
* Lint / typecheck / build checks green if implementation proceeds.
* Docs/notes updated if public behavior or examples change.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Renaming public ESP enum constants to raw `WIREMUX_*` names.
* Changing enum numeric values or wire protocol encoding.
* Changing demo/docs usage from `ESP_WIREMUX_*` to `WIREMUX_*`.

## Technical Notes

* Task created during `$brainstorm`.
* Initial inspection should focus on `sources/esp32/components/esp-wiremux`,
  `sources/esp32/examples/esp_wiremux_console_demo`, and `sources/core/c`.

## Research Notes

### Files inspected

* `sources/esp32/components/esp-wiremux/include/esp_wiremux.h`
* `sources/core/c/include/wiremux_envelope.h`
* `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`
* `sources/esp32/components/esp-wiremux/src/esp_wiremux_console.c`
* `sources/esp32/components/esp-wiremux/src/esp_wiremux_log.c`
* `sources/esp32/examples/esp_wiremux_console_demo/main/esp_wiremux_console_demo_main.c`
* `docs/zh/channel-binding.md`

### What the current design does

* ESP direction, payload kind, channel interaction, and compression constants are
  alias wrappers over core `WIREMUX_*` constants.
* The aliases do not perform runtime conversion; they preserve the exact wire
  protocol numeric values.
* `esp_wiremux_write()` accepts `uint32_t direction`, so raw
  `WIREMUX_DIRECTION_OUTPUT` would compile for direction arguments in C.
* `esp_wiremux_channel_config_t.default_payload_kind` is typed as
  `esp_wiremux_payload_kind_t`, so replacing payload-kind constants with raw core
  enum constants is more of an API/type-surface change than direction alone.
* Direction has two meanings:
  * channel config `directions` is a bitmask and can use
    input-or-output or input-and-output.
  * envelope/write `direction` must be exactly one value, input or output.
    `esp_wiremux.c` validates this with `is_valid_direction()`.

### Practical benefit of ESP aliases

* Public API namespacing: user-facing ESP-IDF examples consistently use
  `esp_wiremux_*` types and `ESP_WIREMUX_*` constants.
* API stability: the ESP component can remain stable even if the portable core
  headers are reorganized later.
* Semantic narrowing: ESP direction aliases intentionally expose only input and
  output, not `WIREMUX_DIRECTION_UNSPECIFIED`.
* Documentation consistency: current Chinese docs and demo snippets use the ESP
  names.

### Cost of ESP aliases

* They duplicate core names and can feel like unnecessary conversion enums.
* Because `esp_wiremux.h` includes core headers directly, the aliases do not
  currently hide the core dependency.
* Internal implementation has many ESP-prefixed references even when it is
  constructing core protocol objects.

### Feasible approaches here

**Approach A: Keep ESP public aliases, add clarity** (Recommended)

* How it works: keep demo/docs using `ESP_WIREMUX_*`; optionally add comments in
  `esp_wiremux.h` explaining aliases intentionally match wire protocol values.
* Pros: no API churn, docs stay consistent, preserves ESP-IDF facade.
* Cons: duplicate names remain.

**Approach B: Use raw `WIREMUX_*` only in ESP internals**

* How it works: keep public API aliases and demo code, but internal protocol
  construction/validation can compare against core `WIREMUX_*` values where that
  better expresses the boundary.
* Pros: clarifies core/ESP boundary without breaking users.
* Cons: mixed constants may feel inconsistent unless documented.

**Approach C: Switch demo/docs/public API to raw core constants**

* How it works: remove or de-emphasize ESP aliases and update demo/docs to use
  raw `WIREMUX_*` constants.
* Pros: fewer duplicated enum names.
* Cons: public API churn, weaker ESP namespace, leaks portable core API into
  ESP-IDF examples, likely needs docs/spec/test updates.

## Decision (ADR-lite)

**Context**: ESP aliases such as `ESP_WIREMUX_DIRECTION_OUTPUT` duplicate core
`WIREMUX_*` names, but they preserve an ESP-IDF-facing public API while mapping
directly to wire-protocol numeric values.

**Decision**: Choose Approach A. Keep ESP public aliases and add a clarifying
comment in `esp_wiremux.h`.

**Consequences**: Existing demo/docs/API names stay stable. The duplicate names
remain, but the header now states that this is intentional namespacing rather
than a runtime conversion layer.
