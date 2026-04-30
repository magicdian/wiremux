# brainstorm: enhanced overlay api stability

## Goal

Define the architecture requirements for generic enhanced APIs and vendor overlay
activation so Wiremux can support forward-compatible vendor enhanced features,
including future closed-source overlay packages that are not tightly bound to
the host application binary.

## What I already know

* Vendor enhanced features are expected to depend on generic enhanced features.
* Generic enhanced APIs may need a stable API plus frozen API mechanism to give
  vendor overlays a forward-compatible base.
* Future vendor overlays may be distributed as closed-source packages, similar
  to plugin packages, and should not require being linked directly into the host
  application.
* The preferred distribution shape may be a zip/custom plugin package rather
  than asking users to manage standalone executable files directly.
* Users may install overlay packages through the TUI or by passing a package
  path to the CLI.
* The host should install overlay packages into a user-scoped Wiremux directory,
  such as a future `~/wiremux/overlay` or platform-appropriate config/data
  directory.
* On startup, the host can scan installed overlay packages, load their manifests
  into memory, validate compatibility/trust metadata, and activate matching
  overlay runtimes when a device manifest requests them.
* Each overlay may need a package-like identity.
* Official Wiremux overlay package names could use a reserved namespace such as
  `wiremux.vendor_name.xxx`.
* Third-party overlays should be blocked from using the `wiremux` package
  prefix.
* Overlay manifests may need signature metadata.
* Core protocol may need to carry enhanced package name information.
* Host-side overlay activation may need an `OverlayManager` that reads a
  manifest, resolves the enhanced package name to an installed plugin, and
  starts the matching overlay.
* `docs/product-architecture.md` already models generic enhanced as the
  vendor-neutral host tooling layer and vendor enhanced as adapters layered on
  top of it.
* `sources/api/proto/versions/README.md` already defines `current/` plus
  numbered frozen protocol API snapshots, including host support for older
  frozen versions.
* `sources/api/proto/versions/current/wiremux.proto` currently has
  `DeviceManifest` fields for device identity, protocol version, feature flags,
  SDK name/version, and channels, but no overlay package requirements.
* `build/wiremux-hosts.toml` and host Cargo features currently model overlays
  as compile-time host modes/features, not independently installed packages.

## Assumptions

* The first implementable scope updates product/architecture docs and host-side
  generic enhanced API schemas before adding runtime plugin loading code.
* "Stable API" means an API surface that preserves compatibility within a
  declared version range.
* "Frozen API" means an immutable compatibility profile that a shipped overlay
  can target even if newer generic enhanced APIs are introduced later.

## Open Questions

* None currently blocking. User confirmed the MVP may proceed to implementation
  on 2026-04-30.

## Requirements (evolving)

* The current MVP should be documentation plus protocol schema work.
* The MVP should define the generic enhanced stable/frozen mechanism before
  committing to a final plugin package/runtime shape.
* Generic enhanced v1 should expose only the virtual serial capability.
* Generic enhanced API names should use the `wiremux.generic.enhanced.*`
  namespace, for example `wiremux.generic.enhanced.virtual_serial`, to avoid
  confusion with core protocol or generic profile APIs.
* Generic enhanced proto structure must preserve forward compatibility for later
  overlay packages and TUI contribution APIs without requiring breaking schema
  changes.
* Generic enhanced proto is a host-side capability catalog, not a replacement
  runtime for feature implementations.
* Host resolution should follow `host core/session state -> enhanced proto
  catalog -> implementation registry -> virtual serial provider`.
* Future vendor enhanced features should be able to depend on
  `wiremux.generic.enhanced.virtual_serial` through this catalog and resolver
  instead of importing private virtual serial internals.
* Generic enhanced compatibility uses a single `frozen_version` field. The host
  can support multiple frozen versions concurrently so an overlay/API consumer
  targeting version 1 can keep working when the host current API reaches version
  3.
* Generic enhanced host-side protobuf definitions should live under a host API
  directory such as `sources/api/host/generic_enhanced/versions/current` and
  `sources/api/host/generic_enhanced/versions/1`, making clear that this is a
  host-side enhanced API contract rather than a device/host shared core protocol
  schema.
* Future overlay package declarations and TUI contribution messages should not
  be added to proto in the MVP; document the direction only.
* Future closed-source overlays should default toward an out-of-process runtime
  communicating with the host through a stable local protocol.
* In-process dynamic libraries may be revisited later as a higher-risk optional
  mode, but they are not part of the stable generic enhanced ABI commitment.
* Generic enhanced virtual serial v1 should not define a dedicated
  `VirtualSerialV1` config message yet. It should declare the API and derive
  behavior from existing manifest channel descriptors.
* The declaration schema should reserve an additive typed config position so a
  future `VirtualSerialV1` or other typed config can be added without breaking
  existing consumers.
* Generic enhanced APIs must be designed as a forward-compatible base for vendor
  enhanced overlays.
* Stable generic enhanced APIs should be distinguished from in-development
  generic enhanced APIs.
* Frozen generic enhanced API snapshots should be available for released vendor
  overlays to target.
* Overlay activation should be based on an explicit package identity rather than
  only compile-time host integration.
* Official and third-party overlay namespaces must be distinguishable.
* Manifest/signature metadata must be considered in the architecture.
* Host overlay resolution should be manifest-driven: read device manifest,
  inspect enhanced overlay/package declarations, resolve to installed or
  built-in overlay providers, then activate compatible providers.
* Future closed-source overlays should default to separate out-of-process
  providers behind a stable host protocol, not direct dependencies on host
  internals.
* Overlay package installation should be separated from overlay execution:
  a package is the install/update/signing unit; an executable, WASM module, or
  shared library inside the package is the runtime unit.
* TUI extensions should be declaration/event based. Overlays should publish
  typed UI contributions such as status rows, badges, actions, progress items,
  diagnostics, or panels; the host TUI owns rendering and layout.

## Acceptance Criteria (evolving)

* [x] The PRD records a clear compatibility model for stable and frozen generic
  enhanced APIs.
* [x] The PRD records that generic enhanced v1 only includes virtual serial.
* [x] The proto design reserves an additive path for future overlay package
  identity and TUI contribution APIs.
* [x] The proto design reserves an additive typed config position without
  requiring a virtual serial config message in v1.
* [x] The PRD records a package naming and namespace reservation model for
  overlays.
* [x] The PRD records the protocol and host-side components likely affected by
  overlay activation.
* [x] Out-of-scope runtime work is explicit if the first task is docs-only.

## Definition of Done (team quality bar)

* Tests added/updated where implementation changes are made.
* Lint / typecheck / CI green for implementation work.
* Docs/notes updated if behavior or architecture changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Actual closed-source plugin packaging format until the architecture scope is
  confirmed.
* Runtime dynamic loading implementation until the compatibility and trust
  boundaries are agreed.
* Final overlay package/runtime format for this MVP; package shape remains a
  follow-up design topic.
* Stable ABI commitment for in-process dynamic library overlays.

## Technical Notes

* Initial notes captured from the user request on generic enhanced API stability
  and overlay package activation.
* Repo files inspected:
  * `docs/product-architecture.md`
  * `docs/source-layout-build.md`
  * `sources/api/proto/versions/README.md`
  * `sources/api/proto/versions/current/wiremux.proto`
  * `sources/core/c/include/wiremux_manifest.h`
  * `sources/host/wiremux/crates/host-session/src/lib.rs`
  * `sources/host/wiremux/crates/cli/Cargo.toml`
  * `build/wiremux-hosts.toml`
* Current TUI state is compiled into `sources/host/wiremux/crates/tui/src/lib.rs`.
  `App` owns manifest, status, virtual serial, and rendering state directly.
  `status_rows(app)` currently constructs fixed status rows, so plugin-provided
  TUI content would need an explicit extension model rather than direct access
  to TUI internals.

## Research Notes

### What similar systems do

* Android Stable AIDL keeps a current development API and frozen versions, and
  enforces compatibility checks between frozen versions and the current API.
  It also treats vendor interfaces as a stability boundary.
  Reference: https://source.android.com/docs/core/architecture/aidl/stable-aidl
* Kubernetes API groups are independently versioned and have separate
  alpha/beta/stable tracks with deprecation rules, which maps well to generic
  enhanced APIs evolving independently from vendor overlays.
  Reference: https://kubernetes.io/docs/reference/using-api/deprecation-policy/
* VS Code extensions require a manifest identity, publisher, semantic version,
  and host engine compatibility range, which maps well to overlay package
  manifests.
  Reference: https://code.visualstudio.com/api/references/extension-manifest

### Constraints from this repo

* Wiremux already has a frozen protocol API snapshot mechanism, so generic
  enhanced API freezing should reuse the same mental model instead of inventing
  a separate policy.
* Current host overlays are selected by build features/host modes. Supporting
  closed-source overlays requires a later runtime/plugin registry boundary.
* Core protocol should avoid vendor-specific behavior, but it can carry generic
  declarations such as profile IDs, overlay package IDs, compatibility versions,
  and signature metadata.

### Feasible approaches here

**Approach A: Docs-first stable/frozen overlay contract** (recommended first PR)

* How it works: define generic enhanced API states, overlay package identity,
  namespace rules, manifest fields, and the intended `OverlayManager` flow in
  architecture/spec docs. Do not implement runtime plugin loading yet.
* Pros: captures the compatibility boundary now; low implementation risk; keeps
  future proto/runtime work focused.
* Cons: does not prove runtime loading or signing behavior yet.

**Approach B: Proto-first activation metadata**

* How it works: add proto fields/messages for enhanced overlay package IDs,
  generic enhanced API versions, compatibility versions, and trust metadata,
  then update C/Rust manifest parsing enough to preserve those fields.
* Pros: makes device-to-host activation data concrete early.
* Cons: freezes wire protocol shape before the package/signing model is fully
  tested.

Chosen MVP direction: combine docs-first contract with minimal proto-first
metadata for generic enhanced API identity/versioning. Leave package runtime
shape and dynamic loading out of scope.

Generic enhanced v1 scope: virtual serial only. The schema should still define
generic API identity/version declarations and an additive extension point so
future overlay package metadata and TUI contribution messages can be added
without changing existing field meanings.

Naming decision: use `wiremux.generic.enhanced.virtual_serial` for the first
generic enhanced API instead of `wiremux.generic.virtual_serial`, because the
shorter name could be mistaken for core Wiremux functionality rather than host
generic enhanced behavior.

Versioning decision: use one `frozen_version` field rather than a device-declared
version range. Compatibility ranges are a host/plugin responsibility. A host
compiled with generic enhanced API current version 3 can still expose frozen
version 1 to overlays or consumers that target version 1.

Proto organization decision: create a host-side generic enhanced API tree such
as `sources/api/host/generic_enhanced/versions/current` and
`sources/api/host/generic_enhanced/versions/1`, rather than placing these
messages beside the shared core device/host protocol. This avoids implying that
generic enhanced host APIs are wiremux-core features.

MVP exclusion decision: do not add overlay package declarations or TUI
contribution messages to proto yet. Keep those in docs until the plugin/runtime
shape is revisited.

Virtual serial config decision: do not add a dedicated `VirtualSerialV1`
message in MVP. The first frozen generic enhanced API only declares
`wiremux.generic.enhanced.virtual_serial` at `frozen_version = 1` and derives
endpoints from the existing manifest channel descriptors. Reserve a future
typed config field so a later `VirtualSerialV1` message can be added
additively.

Final runtime direction confirmed on 2026-04-30: future closed-source overlays
should default to out-of-process providers. Dynamic library loading is a
higher-risk optional mode and should not be treated as the stable generic
enhanced ABI.

**Approach C: Host OverlayManager MVP**

* How it works: add an in-process `OverlayManager` and built-in provider
  registry that resolves manifest package IDs to compiled providers. Reserve
  dynamic closed-source plugin loading for a later PR.
* Pros: validates activation flow while avoiding dynamic loading complexity.
* Cons: still changes runtime architecture and may force early API choices.

**Approach D: Out-of-process overlay runtime**

* How it works: the host starts overlay packages as separate executables or
  services and communicates over a stable local RPC/stdio protocol. The overlay
  receives manifest/session events and returns commands, diagnostics, and UI
  contribution messages.
* Pros: best fit for closed-source overlays, crash isolation, language
  flexibility, and no host relink requirement.
* Cons: requires lifecycle, security, version negotiation, and IPC protocol
  design.

Package shape: a `.wiremux-overlay` zip/custom archive can contain
`overlay.toml`, signature metadata, platform executables, optional resources,
and compatibility declarations. Installing the package unpacks or stores it
under the user Wiremux data directory. Starting the overlay executable is not
dynamic linking; the host loads package metadata into memory and spawns a
separate process when needed.

**Approach E: In-process dynamic library ABI**

* How it works: the host loads overlay shared libraries and calls a stable C ABI
  or Rust ABI wrapper.
* Pros: lower latency and simpler data sharing than a process boundary.
* Cons: much riskier for Rust ABI stability, host crashes, dependency conflicts,
  platform loading differences, and closed-source trust boundaries.

Package shape: the same package format could contain `.dylib`, `.so`, or `.dll`
files, but loading those into the host process is dynamic linking and should be
treated as a separate, higher-risk runtime mode.
