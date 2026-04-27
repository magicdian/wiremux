# brainstorm: proto api versioning and shared core host sdk

## Goal

Explore a forward-compatible protocol/API versioning model for WireMux so the
host can communicate with older device firmware when proto definitions evolve,
and evaluate whether the core SDK should become a shared C boundary used by both
device-side and host-side integrations.

## What I already know

* The project already has an initial working effect, but future major protocol
  changes are expected.
* The host-side protocol layer may need to support multiple API versions.
* The device-side SDK should likely default to the latest API, similar to an
  Android AIDL "current API" model.
* The host should not fail simply because it connects to an older device.
* One possible direction is to move common host Rust logic into core and expose
  a C SDK that host applications, including the existing Rust host, can link
  statically.
* The repository already has a portable C protocol core under `sources/core/c`.
  `sources/core/README.md` explicitly says new platform SDKs should share this
  core instead of inventing platform-specific protocol variants.
* The current schema is `sources/core/proto/wiremux.proto` with package
  `wiremux.v1`.
* Current version fields are split across frame and manifest:
  `WIREMUX_FRAME_VERSION = 1` in `wiremux_frame.h`, Rust host
  `SUPPORTED_VERSION = 1` in `sources/host/src/frame.rs`, and
  `DeviceManifest.protocol_version`.
* ESP manifest emission currently sets `protocol_version =
  ESP_WIREMUX_FRAME_VERSION`, so the manifest protocol version is coupled to the
  frame version.
* Rust host currently reimplements frame/envelope/manifest/batch logic in Rust
  instead of linking or binding to `wiremux_core_c`.

## Assumptions (temporary)

* Protocol evolution may include both additive message fields and incompatible
  request/response shape changes.
* Version negotiation should happen before feature-specific protocol traffic is
  interpreted.
* Compatibility policy should be explicit and testable, rather than relying only
  on protobuf field compatibility.

## Open Questions

* Final confirmation of the complete MVP scope and implementation plan.

## Requirements (evolving)

* Define how protocol/API versions are represented and advanced.
* Define how host chooses a compatible protocol when connected to older device
  firmware.
* Define the boundary between device SDK, core SDK, and Rust host.
* Refactor toward `sources/core/c` as the shared protocol/SDK authority for both
  device and host integrations.
* Make the Rust host consume the shared core through a static C library or
  equivalent FFI boundary instead of maintaining separate protocol encoders and
  decoders long term.
* Expose a high-level host session SDK from core. Core should own protocol
  session behavior such as frame scanning, envelope decoding, manifest request
  construction, manifest parsing, protocol/API version negotiation, batch
  expansion, and compression decode. Rust host should primarily own transport,
  UI, CLI argument parsing, and diagnostics presentation.
* Add detailed GoogleTest/GoogleMock coverage for the new C session layer before
  or alongside Rust host migration. Memory ownership and lifetime behavior must
  be tested explicitly because C has no compile-time ownership checker.
* Use a pure caller-owned memory model for host session events:
  callback-scope views only, no core-owned heap objects returned across the C
  API boundary, and no release/free API required for events.
* Use caller-provided scratch workspace for temporary core session work such as
  compressed batch expansion. The scratch buffer capacity should be explicit and
  failures should return/emit deterministic size errors.
* Include compatibility strategy and frozen API snapshots in the MVP. The task
  should establish a `current`/frozen proto API mechanism or equivalent tracked
  contract, with checks/tests that prevent accidental incompatible edits.
* Define core-owned current/min-supported protocol API constants and host
  compatibility decisions for older device `protocol_version` values.
* Host-side compatibility is compile-time bounded: a host build supports its
  compiled `current` API plus every older frozen API version compiled into that
  host SDK. If a device SDK reports a newer protocol API version than the host
  supports, the host may reject the connection with a clear unsupported-version
  diagnostic that tells the user to upgrade the host SDK/tool.

## Acceptance Criteria (evolving)

* [ ] PRD records the chosen compatibility scope and protocol versioning model.
* [ ] PRD records whether shared host SDK extraction is in scope for the first
      implementation task.
* [ ] PRD identifies impacted modules and key migration risks.
* [ ] PRD defines a migration plan for moving Rust host protocol logic behind
      the shared core boundary.
* [ ] PRD defines C ownership/lifetime rules for session input buffers, decoded
      event views, callback payloads, and any heap allocation.
* [ ] Core C tests cover version negotiation, old-device compatibility, malformed
      frames, truncated inputs, CRC failures, manifest decode failures, batch
      expansion, compression decode failures, and callback ordering.
* [ ] Tests prove event view pointers are callback-scoped and no event requires
      caller-side release/free.
* [ ] MVP includes a tracked current/frozen proto API contract and a documented
      rule for when API version must be bumped.
* [ ] Host session treats older supported `protocol_version` devices as
      compatible or degraded, not fatal, and emits deterministic diagnostics for
      unsupported versions.
* [ ] Host session rejects device API versions newer than host compiled current
      with a clear "upgrade host SDK/tool" diagnostic.

## Definition of Done (team quality bar)

* Tests added/updated where implementation changes behavior.
* Lint / typecheck / CI green.
* Docs/spec notes updated if protocol behavior changes.
* Rollout/rollback considered if compatibility behavior is risky.

## Out of Scope (explicit)

* Implementation is out of scope until the brainstorm converges and the PRD is
  approved.
* New transport backends beyond the existing serial-style host flow.
* Rewriting CLI/TUI presentation beyond what is required to consume core session
  events.
* Supporting unknown future API versions beyond deterministic unsupported-version
  diagnostics.

## Technical Notes

### Repo Inspection

* `sources/core/proto/wiremux.proto`: canonical proto schema, package
  `wiremux.v1`; includes `DeviceManifest.protocol_version`.
* `sources/core/c/include/wiremux_frame.h`: frame layout and
  `WIREMUX_FRAME_VERSION`.
* `sources/core/c/include/wiremux_manifest.h`: manifest payload type constants,
  feature flags, and C manifest struct.
* `sources/core/c/include/wiremux_envelope.h`: public C envelope API and numeric
  payload/direction enums.
* `sources/core/c/CMakeLists.txt`: builds `wiremux_core_c` as a static library
  with GoogleTest-based tests.
* `sources/host/src/frame.rs`, `envelope.rs`, `manifest.rs`, `batch.rs`: host
  owns Rust implementations parallel to the C core.
* `sources/host/src/codec.rs`: host also owns Rust heatshrink-profile and LZ4
  block compression/decompression logic parallel to C core.
* `sources/esp32/components/esp-wiremux/src/esp_wiremux.c`: ESP adapter already
  uses C core for envelope/frame/manifest encoding and emits manifest responses.
* Current C core can encode/decode individual complete frames, envelopes,
  manifests, batches, and compression payloads. It does not yet expose a
  mixed-stream scanner equivalent to host `FrameScanner`.
* Host `listen` and TUI flows both decode frames, envelopes, manifest payloads,
  batches, and compressed batch records, then apply UI/diagnostics behavior.
  Those UI/diagnostics layers should remain Rust-owned.

### Research Notes

#### What similar tools do

* Android Stable AIDL keeps a tracked `current` API and frozen historical API
  versions. Its build system checks compatibility between current and frozen
  versions, and clients may depend on the latest or a specific frozen version.
* Stable AIDL allows only compatible changes in stable interfaces, such as
  appending methods/fields with defaults, adding constants, and adding enum
  values, while disallowing unsafe edits.
* Protobuf's own guidance is complementary: do not reuse tag numbers, reserve
  deleted tags/names, avoid changing field types, and do not assume clients and
  servers are updated together.

#### Constraints from our repo/project

* The wire format is intentionally small and manually encoded in C/Rust, so any
  compatibility mechanism should remain cheap for ESP targets.
* Protobuf binary compatibility already helps with additive fields, but it does
  not solve feature negotiation, incompatible message families, or host behavior
  when a device lacks newer features.
* Frame `version` and proto/API `version` should likely be separated:
  frame version describes byte framing, while API version describes schema and
  semantics above the frame.
* Host should be the compatibility superset because it can be updated more often
  and can afford more code size than device firmware.

#### Feasible approaches here

**Approach A: Manifest-negotiated API versions with frozen proto snapshots**

* How it works: introduce explicit API version constants in core, keep
  `proto/wiremux/vN` or frozen API snapshots, maintain `current`, and have host
  choose behavior from `DeviceManifest.protocol_version` plus feature flags.
* Pros: closest to the AIDL mental model; small runtime cost; works with current
  manifest request path; avoids a large host SDK refactor as the first step.
* Cons: requires discipline and tooling/tests so `current` does not drift
  silently.

**Approach B: Full handshake/negotiation control message before normal traffic**

* How it works: define a dedicated host/device negotiation exchange listing
  supported API versions, features, max payloads, and selected API version.
* Pros: strongest semantics; supports host downgrade and future optional
  capabilities cleanly.
* Cons: bigger protocol change; old devices without handshake still need a
  fallback path, so manifest compatibility remains necessary.

**Approach C: Shared core host SDK first, then versioning on top** (Chosen)

* How it works: expand `wiremux_core_c` into a host-capable SDK, expose C APIs
  for scanning/encoding/manifest handling, bind Rust host to the C static
  library, then centralize API version handling there.
* Pros: long-term single source of truth; useful for custom host integrations.
* Cons: larger migration; FFI/build distribution risk; does not by itself define
  version policy.

## Decision (ADR-lite)

**Context**: Protocol/API compatibility will become harder if the ESP adapter,
Rust host, and future host integrations each own separate protocol logic.

**Decision**: Use the large refactor direction. `sources/core/c` should become
the shared SDK/protocol authority for both device-side and host-side code.
Protocol API versioning should be owned by core, and Rust host should migrate
toward statically linking or binding to that core rather than duplicating
encoders/decoders.

**Consequences**: The first implementation is larger and needs careful staging.
It should preserve current CLI/TUI behavior while replacing internals
incrementally. Build integration and FFI ownership/lifetime rules become part of
the design, not an afterthought.

## Refactor Impact Notes

### Rust host logic that duplicates C core today

* Frame encode/decode and CRC32: `sources/host/src/frame.rs` duplicates
  `wiremux_frame.h/.c`, with extra mixed-stream scanning behavior.
* Envelope encode/decode and varint helpers: `sources/host/src/envelope.rs`
  duplicates `wiremux_envelope.h/.c`.
* Manifest decode and payload type constants: `sources/host/src/manifest.rs`
  overlaps `wiremux_manifest.h/.c`, but C core currently only encodes
  manifests and does not expose manifest decode.
* Batch encode/decode and compression: `sources/host/src/batch.rs` and
  `codec.rs` overlap `wiremux_batch.h/.c` and `wiremux_compression.h/.c`.

### Core API gaps for host parity

* A reusable mixed-stream frame scanner API that preserves terminal bytes,
  validates candidate frames, and emits frame/error/terminal events.
* Manifest decode API, unless host keeps Rust-side manifest decode temporarily.
* FFI-safe ownership rules for decoded strings/byte slices, especially when C
  decoders return views into caller-owned input buffers.
* Version policy constants and helpers, separate from frame version:
  current API version, min host-supported API version, feature compatibility
  checks, and manifest negotiation result.
* High-level host session API that can be fed transport bytes and emit typed
  protocol events for Rust host and future custom host integrations.

### Likely implementation staging

* Stage 1: establish current/frozen proto API layout or equivalent tracked
  contract, add API version policy constants, and document version bump rules.
* Stage 2: design and implement manifest decode and high-level core host session
  API with caller-owned memory, scratch workspace, and GTest/GMock coverage.
* Stage 3: migrate Rust host CLI/TUI to call the core host session through a
  static C library / FFI boundary while preserving current user-visible
  behavior.
* Stage 4: add host compatibility behavior using core-owned version policy, with
  old-device, newer-device unsupported-version, and feature-degraded tests.
* Stage 5: remove or quarantine duplicated Rust protocol logic once parity tests
  prove behavior is unchanged.

### Chosen SDK Surface

Use the high-level host session SDK direction.

Core responsibilities:

* Maintain a host session object.
* Accept transport byte chunks.
* Scan mixed terminal/protocol streams.
* Decode envelopes, manifests, batches, and compressed batch records.
* Build manifest request/input frames for host writes.
* Compute protocol API compatibility from manifest protocol version and feature
  flags.
* Emit typed events to the host layer.
* Use caller-provided scratch memory for temporary decode/expand work.
* Never return core-owned heap event objects to host code.

Rust host responsibilities:

* Open/reconnect serial ports.
* Feed bytes into the core session.
* Render terminal bytes, records, manifests, and diagnostics in CLI/TUI.
* Own CLI arguments, UI state, and file diagnostics.
* Copy any manifest, channel, payload, or diagnostic data it needs to keep after
  a callback returns.

### Memory Ownership Model

Use pure caller-owned / callback-scope views.

Rules:

* `wiremux_host_session_t` may store only its own persistent parser/session
  state and references to caller-provided callbacks/configuration.
* `wiremux_host_session_feed()` accepts byte chunks owned by the caller.
* Event payloads, decoded strings, records, manifests, terminal bytes, and error
  details exposed to callbacks are valid only during that callback.
* Core must not return event objects that require `wiremux_*_free()` or a
  matching release call.
* If core needs temporary mutable memory for decompression or batch expansion,
  it must use caller-provided scratch workspace with explicit capacity.
* If Rust host needs data after a callback returns, Rust copies it into
  Rust-owned structs.
* Size failures from scratch exhaustion must be deterministic and test-covered,
  not undefined behavior or partial callback emission.

### Testing Requirements

* Continue using GoogleTest for core tests and add GoogleMock where callback or
  event ordering assertions are clearer than manual vectors.
* Add tests for every session event type and every documented error branch.
* Add tests for caller-owned buffer lifetimes: events that point into temporary
  input must not escape unless explicitly copied, and any copied payload must
  have a documented release path.
* Add scratch-exhaustion tests for compressed batches and large manifests.
* Add parity/regression tests using existing host fixture bytes where possible
  before deleting duplicated Rust logic.
* Add API snapshot tests/checks that fail if `current` changes without updating
  frozen API/version metadata.

## MVP Scope Decision

Include the largest option in the MVP:

* High-level core host session SDK.
* Rust host migration to that core SDK.
* Old-device compatibility behavior based on core-owned API version policy.
* Current/frozen proto API tracking, with documented bump rules and checks/tests
  inspired by Stable AIDL.
* Newer-device rejection behavior when `device_api_version >
  host_current_api_version`, including an actionable "upgrade host SDK/tool"
  diagnostic.

## Expansion Sweep

### Future evolution

* Future proto major versions may need side-by-side `wiremux.v1`, `wiremux.v2`,
  etc. support in host while device SDK defaults to `current`.
* Future host integrations should be able to use the C host session SDK without
  depending on the Rust CLI.

### Related scenarios

* Manifest request/response, regular records, compressed batches, and TUI/CLI
  diagnostics all need consistent compatibility behavior.
* ESP device SDK should continue using the latest current API by default.

### Failure and edge cases

* Unsupported frame version remains a frame-level parse issue.
* Unsupported protocol API version should be a compatibility decision, not a
  generic decode crash.
* A device reporting a newer API than the host compiled current is unsupported
  and should produce a deterministic upgrade-host diagnostic.
* Scratch exhaustion, truncated input, CRC mismatch, invalid UTF-8, unknown
  fields, unsupported compression, and malformed manifests must be deterministic
  and test-covered.
