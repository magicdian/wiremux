# Brainstorm: vendor enhanced API architecture

## Goal

Optimize the recently implemented `vendor_enhanced` feature so it has a long-term architecture instead of existing only as Rust host implementation details. The expected direction is to introduce stable/frozen API mechanics for vendor enhanced capabilities, place Espressif-specific protocol declarations under `sources/api/host/vendor_enhanced/espressif`, and connect those declarations to the corresponding host implementation.

## What I already know

* The previous round implemented enhanced functionality in Rust host code.
* The current concern is architectural longevity: vendor enhanced features may need the same stable/frozen API treatment as other protocol surfaces.
* A proposed API location is `sources/api/host/vendor_enhanced/espressif`.
* The implementation should be associated with the protocol declaration rather than remaining implicit.
* `sources/api/host/generic_enhanced/versions` already uses `current/` plus frozen numbered snapshots, with `generic_enhanced.proto` and `catalog.textproto` as source-of-truth inputs.
* `sources/host/wiremux/crates/generic-enhanced/build.rs` compiles the current proto and encodes the textproto catalog with `protoc`; `src/lib.rs` decodes that catalog and provides registry/provider resolution.
* The current ESP enhanced MVP lives in `sources/host/wiremux/crates/tui/src/esp_enhanced.rs`; it hard-codes endpoint name, Espressif manifest detection, esptool SLIP classification, DTR/RTS reset behavior, raw bridge transition, baud tracking, and summary strings.
* `interactive::host_supports_virtual_serial_provider()` already demonstrates feature-gated support checks through the generic enhanced registry.
* Host feature wiring is currently `cli/esp32 -> generic-enhanced + tui/esp32`, while `tui` has only an `esp32` feature and no vendor enhanced catalog dependency.

## Assumptions (temporary)

* The MVP should preserve current runtime behavior while moving/declaring contracts.
* Espressif is the first vendor enhanced implementation, but the layout should leave room for other vendors.
* The API declaration should be source-controlled and versionable, not generated ad hoc from Rust code.

## Open Questions

* None.

## Requirements (evolving)

* Identify the existing enhanced implementation and API/versioning conventions.
* Define a future-proof location and structure for Espressif vendor enhanced API declarations.
* Define how the host implementation links to the declaration.
* Use Approach A: add vendor enhanced API declarations and a Rust registry/catalog association for the ESP implementation.
* Keep `generic_enhanced` and `vendor_enhanced` as separate API families. `vendor_enhanced` may declare requirements on generic enhanced capabilities, but it must not statically depend on or import generic enhanced proto files.
* Use the official Wiremux vendor enhanced namespace `wiremux.vendor.enhanced.espressif.*`.
* Declare the first Espressif vendor enhanced capability as `wiremux.vendor.enhanced.espressif.esptool_bridge`.
* Express generic enhanced dependencies as capability requirements by stable API name and frozen version, so the host registry/resolver can find the matching built-in or installed implementation.
* Keep registry/provider/resolve types in a shared host enhanced crate, not inside `vendor_enhanced`. Generic and vendor enhanced crates translate their API catalogs into shared capability declarations and register built-in providers through the shared registry.

## Acceptance Criteria (evolving)

* [x] The chosen API layout is documented in this PRD.
* [x] The implementation plan identifies files to modify and validation points.
* [x] Out-of-scope items are explicit before implementation starts.
* [x] The chosen approach keeps vendor enhanced API versioning separate from `DeviceManifest.protocol_version`.
* [x] The chosen approach avoids leaving ESP enhanced capability identity as only hard-coded TUI constants.
* [x] The first catalog declares `wiremux.vendor.enhanced.espressif.esptool_bridge` at frozen version 1.
* [x] Rust tests prove the ESP esptool bridge provider resolves from the vendor enhanced catalog.
* [x] The vendor enhanced proto/catalog does not import the generic enhanced proto solely to express generic capability requirements.
* [x] Shared registry/provider/resolve types live outside `generic-enhanced` and `vendor-enhanced`.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / CI green.
* Docs/spec notes updated if behavior or architecture changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Runtime behavior changes to flashing unless intentionally included after research.
* Adding non-Espressif vendor enhanced implementations in this task.

## Technical Notes

* Initial user-proposed path: `sources/api/host/vendor_enhanced/espressif`.
* Relevant specs read:
  * `.trellis/spec/backend/directory-structure.md`: host APIs belong under `sources/api/host/<api-family>/versions`; host Rust workspace dependency direction is constrained.
  * `.trellis/spec/backend/quality-guidelines.md`: host-side enhanced API changes update `sources/api/host/<api-family>/versions/current/`; freeze numbered snapshots when shipping stable contracts; keep catalog and Rust decode/registry tests in sync.
  * `.trellis/spec/backend/error-handling.md`: interactive paths should keep errors deterministic and diagnostics-oriented.
  * `.trellis/spec/guides/cross-layer-thinking-guide.md`: this change crosses API declaration, build/codegen, Rust registry, feature gating, and TUI runtime behavior.
  * `.trellis/spec/guides/code-reuse-thinking-guide.md`: prefer following the existing generic enhanced catalog/registry pattern instead of creating a parallel ad hoc mechanism.
* Existing generic enhanced files:
  * `sources/api/host/generic_enhanced/versions/current/generic_enhanced.proto`
  * `sources/api/host/generic_enhanced/versions/current/catalog.textproto`
  * `sources/api/host/generic_enhanced/versions/1/*`
  * `sources/host/wiremux/crates/generic-enhanced/{build.rs,src/lib.rs}`
* Current ESP enhanced implementation file:
  * `sources/host/wiremux/crates/tui/src/esp_enhanced.rs`

## Expansion Sweep

### Future evolution

* Other vendors may need their own enhanced APIs under the same host API umbrella, so the layout should not bake Espressif into the whole vendor enhanced model.
* ESP enhanced likely grows from esptool passthrough into OTA, device reset, native virtual serial, or flasher metadata; the proto should reserve additive extension room.

### Related scenarios

* Generic enhanced and vendor enhanced should share stability vocabulary and validation behavior where practical.
* Host mode selection (`generic`, `generic-enhanced`, `vendor-enhanced`, `all-features`) should remain consistent with catalog/provider support checks.

### Failure and edge cases

* Catalog drift: Rust implementation says it supports an ESP capability, but the API catalog does not declare it.
* Version drift: frozen API snapshot changes accidentally after release.
* Feature drift: `esp32` feature compiles TUI behavior without a matching vendor enhanced registry check.

## Research Notes

### Constraints from existing repo

* Host-side enhanced APIs are explicitly separate from the core Wiremux device/host protocol under `sources/api/proto`.
* The existing generic enhanced contract uses `sources/api/host/<api-family>/versions/{current,N}` and a textproto catalog decoded by Rust tests.
* `crates/generic-enhanced` must not depend on concrete provider crates. By analogy, a vendor enhanced API crate should own declarations and neutral registry types, while concrete behavior can remain in `tui` or later move to a provider crate.
* Vendor enhanced dependencies on generic enhanced should be represented as capability references, not protobuf imports or static Rust crate dependencies from the API schema.
* Provider registry and resolve behavior are host enhanced infrastructure, not vendor-specific behavior.
* Current ESP behavior is implementation-heavy and TUI-local; the MVP should not overfit the proto to every internal state machine detail.

### Feasible approaches here

**Approach A: Mirror generic enhanced with one vendor-enhanced API family** (recommended)

* How it works: create `sources/api/host/vendor_enhanced/espressif/versions/{current,1}` with an Espressif vendor enhanced proto and catalog. Add a Rust crate such as `crates/vendor-enhanced` or `crates/espressif-enhanced` that compiles/decodes the catalog and exposes `latest_esptool_bridge_capability_id()` plus registration helpers. Wire TUI/interactive support checks through this registry.
* Pros: matches current architecture, gives stable/frozen semantics immediately, links declaration to implementation through build/test gates, and leaves room for other vendor subtrees later.
* Cons: adds a new crate and some boilerplate that may feel large for one current capability.

**Approach B: API files only, no Rust registry yet**

* How it works: add proto/catalog snapshots and documentation under `sources/api/host/vendor_enhanced/espressif`, but keep current TUI implementation hard-coded except for doc references.
* Pros: fast and low-risk; records the architectural intent.
* Cons: does not really solve drift because implementation support can still diverge from the declaration.

**Approach C: Generalize generic-enhanced crate to cover vendor enhanced**

* How it works: expand `crates/generic-enhanced` into a broader host enhanced API crate that can decode both generic and vendor catalogs.
* Pros: avoids a second registry implementation and can unify catalog validation.
* Cons: rejected for this design because it muddies the ownership model. `vendor_enhanced` depends on `generic_enhanced`; it is not a sibling implementation inside the same generic abstraction.

## Decision (ADR-lite)

**Context**: The current ESP enhanced esptool passthrough exists as TUI-local Rust behavior. Generic enhanced already has a stable/frozen API catalog model, but vendor enhanced should remain a distinct overlay layer. Vendor enhanced capabilities may require generic enhanced capabilities, but that relationship is a resolver-level capability requirement, not a static proto dependency.

**Decision**: Use Approach A. Add Espressif vendor enhanced API declarations under `sources/api/host/vendor_enhanced/espressif/versions`, with `current/` and frozen `1/` snapshots for the MVP. Add Rust catalog code that decodes those declarations into shared enhanced capability declarations, then register the implementation through a shared enhanced registry. Use capability names in the `wiremux.vendor.enhanced.espressif.*` namespace.

**Consequences**: This adds a small amount of crate/build/test boilerplate, but prevents declaration/implementation drift and preserves the intended dependency direction: vendor enhanced can require generic enhanced capabilities by name/version, while generic enhanced remains vendor-neutral and independent of vendor overlays. Registry/resolve behavior remains reusable for generic enhanced, vendor enhanced, and future private overlay plugins.

## Layering Decision

```text
core device/host API
  -> generic_enhanced host API
      -> capability resolver
          -> vendor_enhanced/espressif host API
          -> ESP enhanced TUI/provider implementation
```

`generic_enhanced` and `vendor_enhanced` are both overlay-like host-side APIs, but they are not the same abstraction. `vendor_enhanced` may require generic enhanced capabilities such as virtual serial by declaring a stable capability reference. The shared host-side registry/resolver is responsible for mapping that reference to a built-in or installed implementation. `generic_enhanced` must not depend on vendor-specific declarations or implementations.

Future private or closed-source overlay plugins should use the same model: plugin metadata declares required generic enhanced capabilities by API name and frozen version, then the host resolver validates compatibility and locates implementations. The vendor enhanced proto should therefore define a generic capability-reference shape rather than importing generic enhanced proto messages.

## Preliminary Recommendation

Keep the proto catalog small: declare only the Espressif esptool bridge capability, its stable name, frozen version, stability, description, dependency on the generic enhanced virtual serial API, and minimal typed config/metadata extension points. Keep runtime behavior unchanged except for replacing hard-coded capability support identity with a registry-backed check.

## Technical Approach

* Add `sources/api/host/vendor_enhanced/espressif/versions/{current,1}` with:
  * `espressif_vendor_enhanced.proto`
  * `catalog.textproto`
  * `README.md` under `versions/`
* Model stability with the same development/stable/frozen vocabulary as generic enhanced.
* Include capability requirement declarations so `esptool_bridge` can state it requires `wiremux.generic.enhanced.virtual_serial@1` without importing the generic enhanced proto.
* Add `crates/enhanced-registry` for shared capability IDs, declarations, requirements, provider registrations, registry, and resolve errors.
* Add `crates/vendor-enhanced` for vendor enhanced catalog/codegen/validation and built-in Espressif provider registration helpers.
* Keep `crates/generic-enhanced` responsible for generic enhanced catalog/codegen/validation and built-in generic provider registration helpers.
* Wire ESP enhanced support through that crate under the existing `esp32` feature path, without changing flashing runtime behavior.
* Update host API documentation and product architecture notes so the new API family is discoverable.

## Implementation Plan

* PR1: Add Espressif vendor enhanced API files and frozen v1 snapshot.
* PR2: Add shared `enhanced-registry` crate and refactor generic/vendor enhanced catalogs to use it.
* PR3: Wire TUI ESP enhanced support to the registry, update docs/spec notes, and run host validation.
