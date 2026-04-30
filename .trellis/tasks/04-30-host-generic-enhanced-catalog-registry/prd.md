# brainstorm: host generic enhanced catalog registry

## Goal

Design a lightweight Rust host-side catalog/registry layer for generic enhanced
capabilities so the host can expose capabilities from the latest
`generic_enhanced` proto contract and resolve
`wiremux.generic.enhanced.virtual_serial` to the existing virtual serial broker
without vendor enhanced code importing virtual serial internals directly.

## What I already know

* The previous task added host-side generic enhanced proto snapshots under
  `sources/api/host/generic_enhanced/versions/{current,1}`.
* Generic enhanced v1 declares virtual serial as the first capability:
  `wiremux.generic.enhanced.virtual_serial`.
* The desired host flow is:
  `host core/session state -> generic enhanced proto catalog -> implementation registry -> virtual serial provider`.
* The user prefers the built-in virtual serial capability declaration to come
  from the imported/latest proto contract, not from scattered Rust string
  constants.
* The Rust host should gain a small, future-extensible catalog/registry.
* Vendor enhanced code should later depend on capability name/version and a
  resolver instead of directly importing virtual serial internals.
* This should not be a large host runtime rewrite.
* Existing Rust host crates are `host-session`, `interactive`, `tui`, and `cli`.
* Current dependency direction is effectively `cli -> tui -> interactive ->
  host-session`; `interactive` owns `VirtualSerialBroker`.
* `tui::App` directly owns `VirtualSerialBroker`, `VirtualSerialConfig`, and a
  `virtual_serial_supported` boolean.
* CLI support checks are currently feature-gated through
  `host_supports_virtual_serial() -> cfg!(feature = "generic-enhanced")`.
* There is no existing Rust protobuf codegen flow. `host-session/build.rs`
  compiles the portable C core; core protocol parsing is hand-written C.
* A `.proto` schema defines message types, but it does not by itself contain the
  built-in `wiremux.generic.enhanced.virtual_serial` catalog entry as data.

## Assumptions (temporary)

* This task is design/brainstorm first; do not implement until the final
  requirements are confirmed.
* The MVP should preserve current virtual serial behavior and only add a
  resolver layer around the existing implementation.
* The host should default to the latest generic enhanced proto snapshot
  (`versions/current`) for built-in capability declarations.
* Runtime package loading and third-party overlay installation remain out of
  scope for this task.

## Open Questions

* How should Rust consume the host-side proto contract and catalog data:
  generated Rust types plus a proto/textproto catalog, descriptor metadata, or
  a lightweight checked-in generated Rust module?

## Requirements (evolving)

* Add a lightweight generic enhanced catalog/registry on the Rust host side.
* Default Rust host tooling should reference the latest generic enhanced proto
  version.
* Avoid scattering hard-coded capability declaration data in Rust code.
* The registry should resolve `wiremux.generic.enhanced.virtual_serial` at
  frozen version 1 to the existing virtual serial broker/provider.
* Future vendor enhanced features should depend on capability name/version, not
  virtual serial internal modules.
* Keep virtual serial runtime behavior unchanged for the MVP.
* Preserve the existing `generic-enhanced` Cargo feature as the build-time gate
  for whether the built-in virtual serial provider is registered.
* Use a full Rust protobuf generation/decoding path for the generic enhanced
  host API rather than Rust-only constants or manually generated declarations.
* Treat `generic_enhanced` proto/catalog consumption as a long-term host API
  capability because future vendor enhanced features will depend on generic
  enhanced capabilities.
* Add a new independent Rust crate under
  `sources/host/wiremux/crates/generic-enhanced` for host-side generic enhanced
  proto types, decoded catalog data, registry, and resolver contracts.
* Keep `host-session` below the enhanced layer. It should continue to own core
  protocol/session bindings and should not know about generic enhanced tooling.
* Keep `interactive` as the virtual serial implementation owner. It may depend
  on `generic-enhanced` to register/resolve the virtual serial provider, but
  generic enhanced consumers should not need to import virtual serial internals.

## Expansion Sweep

### Future evolution

* More generic enhanced capabilities may be added beside virtual serial, such
  as TCP bridge or capture/replay.
* Vendor enhanced overlays should eventually declare dependencies against the
  catalog and ask a resolver for providers instead of importing provider modules.

### Related scenarios

* CLI/TUI support checks should move from direct
  `host_supports_virtual_serial()` style booleans toward asking the registry
  whether the capability is registered.
* The registry should live low enough for future vendor code to reuse, but not
  below the crate that owns provider implementations.

### Failure and edge cases

* Catalog declares a capability but the provider is not registered because the
  Cargo feature is disabled.
* Provider exists but only supports a different frozen version.
* Duplicate provider registrations for the same `api_name` and
  `frozen_version`.

## Feasible Approaches

### Approach A: Proto-adjacent catalog data plus Rust codegen

How it works:

* Add a proto-adjacent catalog data file under
  `sources/api/host/generic_enhanced/versions/current`, for example
  `catalog.textproto`, that contains the built-in
  `wiremux.generic.enhanced.virtual_serial` declaration using
  `GenericEnhancedApiCatalog`.
* Add a small host API crate or build step that generates Rust types from
  `generic_enhanced.proto` and embeds/decodes the latest catalog.
* `interactive` registers the virtual serial provider when the
  `generic-enhanced` feature path is active.

Pros:

* Closest to the user's preference: declaration data lives with the proto
  version, not as scattered Rust constants.
* Scales to future catalog entries and frozen snapshots.

Cons:

* Requires introducing a Rust proto toolchain/dependencies or a build helper.
* More moving parts for the first registry PR.

Decision: selected. The user prefers this direction because generic enhanced is
a long-term host capability layer and future vendor enhanced providers will
depend on generic enhanced features.

### Approach B: Proto-adjacent catalog data plus lightweight generated Rust

How it works:

* Keep the authoritative declaration beside the proto, but use a checked-in or
  build-generated Rust module with `CapabilityDeclaration` values derived from
  that catalog.
* Add a test that validates the generated Rust declaration against the
  proto-adjacent catalog file.
* Runtime registry uses plain Rust structs and does not need protobuf decoding.

Pros:

* Avoids bringing protobuf runtime dependencies into the host immediately.
* Still avoids hand-writing capability declarations in implementation code.

Cons:

* Needs a generation/validation convention so generated Rust cannot drift.
* Less pure than decoding proto data at runtime.

Decision: not selected for the MVP. It avoids protobuf runtime dependencies but
is less appropriate for the long-term generic enhanced dependency layer.

### Approach C: Rust-only registry constants with proto schema tests

How it works:

* Define `const VIRTUAL_SERIAL_API_NAME` and `const VIRTUAL_SERIAL_FROZEN_VERSION`
  in Rust.
* Add tests that compare these constants to a proto-adjacent catalog or README
  expectation.

Pros:

* Smallest implementation.
* Minimal dependencies.

Cons:

* Conflicts with the user's preference to avoid hard-coding built-in capability
  declarations in Rust.
* Easy for future capabilities to drift across docs/proto/Rust.

Decision: rejected for the MVP because vendor enhanced should depend on a real
generic enhanced proto contract, not Rust-only constants.

## Decision (ADR-lite)

Context: The host needs a stable catalog/registry boundary so future vendor
enhanced features can depend on generic enhanced capabilities such as virtual
serial without importing provider internals.

Decision: Use Rust protobuf codegen/decoding for the latest
`generic_enhanced` host API and store the built-in virtual serial declaration as
proto-adjacent catalog data. The host registry resolves decoded
`api_name + frozen_version` declarations to registered Rust providers.

Consequences: This introduces a protobuf build/runtime path into the Rust host
workspace, but it gives the generic enhanced layer a durable contract suitable
for future vendor enhanced dependencies. The first provider mapping remains
small: `wiremux.generic.enhanced.virtual_serial@1` maps to the existing
`VirtualSerialBroker` path when the `generic-enhanced` feature is enabled.

## Crate Boundary Decision (ADR-lite)

Context: The resolver must be reusable by future vendor enhanced code without
forcing vendor enhanced providers to depend on `interactive` implementation
details or pushing enhanced concerns into the low-level `host-session` crate.

Decision: Add a new `sources/host/wiremux/crates/generic-enhanced` crate. This
crate owns generated generic enhanced proto types, catalog loading/decoding,
capability identity/version types, registry data structures, resolver errors,
and tests for catalog/provider matching. `interactive` remains the owner of
`VirtualSerialBroker` and uses the registry to register the virtual serial
provider when the `generic-enhanced` Cargo feature is enabled.

Consequences: The workspace gains one crate and dependency edge
`interactive -> generic-enhanced`. Future vendor enhanced crates can depend on
`generic-enhanced` for capability resolution without importing `interactive`
unless they specifically need a concrete provider.

## Acceptance Criteria (evolving)

* [x] PRD defines how Rust consumes the generic enhanced proto contract.
* [x] PRD defines the catalog/registry boundary and provider mapping.
* [x] PRD records that virtual serial behavior remains unchanged.
* [x] PRD records how future vendor enhanced code should resolve dependencies.
* [x] Out-of-scope package loading/runtime plugin work is explicit.
* [x] New `generic-enhanced` crate joins the host workspace.
* [x] Current generic enhanced proto can generate Rust types.
* [x] Catalog data decodes `wiremux.generic.enhanced.virtual_serial@1`.
* [x] Registry resolves that capability to a virtual serial provider key.
* [x] Missing provider and duplicate registration have deterministic errors.
* [x] Focused tests cover catalog decode and registry resolve.

## Definition of Done (team quality bar)

* Tests added/updated where implementation changes are made.
* Lint / typecheck / CI green for implementation work.
* Docs/specs updated if behavior or architecture changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Runtime third-party overlay package loading.
* Device manifest overlay package declarations.
* TUI contribution APIs.
* Changing the current virtual serial endpoint behavior.

## Technical Notes

* Initial PRD seeded from the user request on 2026-04-30.
* Files inspected:
  * `sources/api/host/generic_enhanced/versions/current/generic_enhanced.proto`
  * `sources/host/wiremux/Cargo.toml`
  * `sources/host/wiremux/crates/host-session/build.rs`
  * `sources/host/wiremux/crates/interactive/src/lib.rs`
  * `sources/host/wiremux/crates/tui/src/lib.rs`
  * `sources/host/wiremux/crates/cli/src/args.rs`
  * `sources/host/wiremux/crates/cli/src/main.rs`
* Current virtual serial implementation point:
  `sources/host/wiremux/crates/interactive/src/lib.rs` owns
  `VirtualSerialBroker`, `VirtualSerialConfig`, endpoint sync, input polling,
  and output mirroring.
* Current TUI integration point:
  `sources/host/wiremux/crates/tui/src/lib.rs` stores `VirtualSerialBroker`
  directly in `App` and calls `sync_manifest`, `write_output`, `poll_input`,
  and `toggle_input_owner`.
* Current CLI support point:
  `sources/host/wiremux/crates/cli/src/args.rs` has
  `host_supports_virtual_serial() -> cfg!(feature = "generic-enhanced")`.
* Implementation notes:
  * Added `sources/api/host/generic_enhanced/versions/current/catalog.textproto`
    and a frozen v1 copy.
  * Added `sources/host/wiremux/crates/generic-enhanced` with `prost` and
    `prost-build`.
  * `generic-enhanced/build.rs` compiles the latest proto and encodes the
    textproto catalog to protobuf bytes using `protoc`.
  * `interactive::host_supports_virtual_serial_provider()` now checks the
    generic enhanced registry rather than a local `cfg!` expression.
