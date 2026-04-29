# brainstorm: proto stable api cleanup

## Goal

Determine whether the legacy-looking top-level `sources/api/proto/wiremux.proto`
is still needed now that stable API versions exist under `sources/api/proto/api/`,
and decide whether the internal `api/` directory should be renamed to a clearer
versioned namespace.

## What I already know

* `sources/api/proto/wiremux.proto` has the same md5 as
  `sources/api/proto/api/current/wiremux.proto`.
* `sources/api/proto/api/2/wiremux.proto` has the same md5 as `api/current`.
* `sources/api/proto/api/1/wiremux.proto` differs.
* The repository is migrating toward a product layout with shared API
  definitions under `sources/api`.
* Git history shows `sources/core/proto/wiremux.proto` was created before stable
  API snapshots, then `sources/core/proto/api/current/` and `api/1/` were added
  later, then the whole tree moved to `sources/api/proto`.
* The archived proto API versioning PRD explicitly called the top-level proto
  the canonical proto schema.
* The PR2 migration PRD explicitly required both `sources/api/proto/wiremux.proto`
  and `sources/api/proto/api/current/wiremux.proto` plus numbered snapshots to
  exist after migration.
* Current core tests intentionally assert:
  top-level canonical == `api/current` == `api/2`, and top-level canonical !=
  `api/1`.

## Assumptions (temporary)

* The top-level `wiremux.proto` is a compatibility/canonical path retained from
  before stable API version directories were introduced.
* Any deletion or rename must account for build scripts, generated sources, docs,
  and downstream consumers.

## Open Questions

* Confirm final implementation scope before editing source paths.

## Requirements (evolving)

* Inspect repo references and history for `sources/api/proto/wiremux.proto`.
* Determine whether `sources/api/proto/api/` is the intended stable API root or
  an awkward nested name that should be renamed.
* Propose concrete cleanup options with trade-offs.
* If implementation proceeds, update docs/specs/tests together so there is a
  single clearly documented source of truth.
* Delete the top-level `sources/api/proto/wiremux.proto` so developers cannot
  edit a duplicate latest schema by accident.
* Rename the internal snapshot directory from `sources/api/proto/api/` to
  `sources/api/proto/versions/` so the stable API path reads clearly after the
  product-layout migration.
* Treat `sources/api/proto/versions/current/wiremux.proto` as the only editable
  latest schema.
* Preserve numbered frozen snapshots under `sources/api/proto/versions/<n>/`.
* Do not change protobuf package names, fields, tags, enum values, or protocol
  API version constants.

## Acceptance Criteria (evolving)

* [x] Existing references to the top-level proto path are identified.
* [x] Git history or repo docs explain when/why the duplicate appeared, if
  recoverable.
* [x] A recommendation is recorded with risks and migration steps.
* [x] `sources/api/proto/wiremux.proto` no longer exists.
* [x] `sources/api/proto/versions/current/wiremux.proto` exists and is byte-equal
  to the former current schema.
* [x] `sources/api/proto/versions/1/wiremux.proto` and
  `sources/api/proto/versions/2/wiremux.proto` are preserved.
* [x] Active docs/specs/tests no longer refer to `sources/api/proto/wiremux.proto`
  as the protocol schema path.
* [x] Active docs/specs/tests no longer refer to `sources/api/proto/api/`.
* [x] Snapshot tests assert `versions/current == versions/2` and
  `versions/current != versions/1`.

## Definition of Done (team quality bar)

* Tests added/updated if implementation changes behavior.
* Lint / typecheck / CI green if implementation proceeds.
* Docs/notes updated if path semantics change.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* Changing protobuf message schemas.
* Introducing a new API version.

## Technical Notes

* Brainstorm started from user observation that top-level, current, and v2 proto
  files are byte-identical while v1 differs.
* Direct references found:
  * `docs/zh/esp-idf-console-integration.md`
  * `sources/core/README.md`
  * `sources/core/c/tests/wiremux_core_test.cpp`
  * `.trellis/spec/backend/database-guidelines.md`
  * `.trellis/spec/frontend/type-safety.md`
  * `.trellis/spec/backend/quality-guidelines.md`
* Relevant history:
  * `9c54ea1 refactor: migrate project to wiremux core architecture` added
    `sources/core/proto/wiremux.proto`.
  * `6a8a876 feat!: consolidate host protocol handling in core C` added
    `sources/core/proto/api/1/`, `api/current/`, and the snapshot README.
  * `9be7731 feat: add manifest-driven console passthrough` added API v2 and
    changed the snapshot test to expect v1 differs while v2 matches current.
  * `505ea91 refactor: productize source layout and build orchestration` moved
    `sources/core/proto` to `sources/api/proto`.

## Research Notes

### What similar versioned API layouts do

* Stable API systems commonly keep a "current" editable contract and frozen
  numbered snapshots. The existing Wiremux PRD explicitly copied that mental
  model from Stable AIDL.
* Protobuf compatibility rules support additive evolution, but the project still
  needs explicit snapshot discipline because host/device versions may differ.

### Constraints from this repo

* The path `sources/api/proto/api/` now reads awkwardly because `proto` moved
  under `sources/api`; before the move, `sources/core/proto/api/` meant "API
  snapshots under the proto schema root."
* The top-level file is currently the "canonical latest schema" anchor used by
  docs and tests. Deleting it is possible, but it would convert `api/current/`
  into the canonical latest schema and requires doc/spec/test updates.
* Renaming `api/` to `versions/` would improve local path semantics but would
  churn every stable API reference and make the current README wording obsolete.

### Feasible approaches here

**Approach A: Keep current layout, improve docs/tests** (lowest risk)

* How it works: keep `sources/api/proto/wiremux.proto` as canonical latest and
  keep snapshots under `sources/api/proto/api/`.
* Pros: matches existing PRDs/tests; almost no churn; preserves compatibility
  for any external path users.
* Cons: `api/proto/api` remains semantically noisy.

**Approach B: Delete top-level canonical file; make `api/current` canonical**

* How it works: remove `sources/api/proto/wiremux.proto`; update docs/specs/tests
  to refer to `sources/api/proto/api/current/wiremux.proto` as latest.
* Pros: removes real duplication; stable API tree becomes the only schema
  surface.
* Cons: breaks old canonical path; longer path for the common latest schema; may
  make docs less friendly.

**Approach C: Rename snapshots directory from `api/` to `versions/`**

* How it works: move `sources/api/proto/api/{current,1,2}` to
  `sources/api/proto/versions/{current,1,2}` and update docs/specs/tests.
* Pros: best path readability after the source-layout move; avoids `api/proto/api`.
* Cons: pure path churn; breaks existing snapshot references; still leaves the
  question of whether top-level canonical should remain.

**Approach D: Combine B + C** (cleanest semantics, highest churn)

* How it works: remove top-level canonical and rename snapshots to
  `sources/api/proto/versions/current`.
* Pros: one source of truth and clearer version namespace.
* Cons: largest migration; all consumers must use the longer versioned path.

## Decision (ADR-lite)

**Context**: The top-level proto file started as the canonical schema before
stable API snapshots existed. After the source-layout migration, keeping both
`sources/api/proto/wiremux.proto` and `sources/api/proto/api/current/wiremux.proto`
creates two apparent latest-schema edit points, and the nested `api/proto/api`
path obscures the versioning model.

**Decision**: Use Approach D. Delete the top-level canonical proto file and make
`sources/api/proto/versions/current/wiremux.proto` the only latest schema. Move
numbered snapshots from `sources/api/proto/api/<n>/` to
`sources/api/proto/versions/<n>/`.

**Consequences**: The repo gets one clear source of truth and clearer path
semantics. This intentionally breaks the old friendly canonical path, so all
docs/specs/tests and any generation/build references must be updated in the same
change. The proto wire schema itself must remain unchanged.

## Expansion Sweep

### Future evolution

* Future major protobuf packages such as `wiremux.v2` may need side-by-side roots
  separate from minor/additive API snapshots.
* Release tooling may later need a snapshot drift check that fails when current
  changes without freezing a numbered snapshot.

### Related scenarios

* Docs, specs, core tests, host SDK generation, and ESP release packaging should
  agree on one "latest schema" path.
* Any path rename should avoid changing protobuf package names or wire fields.

### Failure and edge cases

* Removing the top-level file may break users or scripts that read the friendly
  canonical path.
* Keeping two byte-identical files without a documented relationship invites
  future drift, so either documentation or an automated check must stay.

## Implementation Notes

* Moved `sources/api/proto/api/` to `sources/api/proto/versions/`.
* Deleted `sources/api/proto/wiremux.proto`.
* Updated active docs/specs and `sources/core/README.md` to use
  `sources/api/proto/versions/current/wiremux.proto`.
* Updated `sources/core/c/tests/wiremux_core_test.cpp` so
  `versions/current` is the test baseline.

## Validation

* `rg -n "sources/api/proto/wiremux\\.proto|sources/api/proto/api|proto/api|api/current|api/[0-9]+/wiremux\\.proto|api/<version>|api/<n>|/api/current|/api/1|/api/2" sources docs .trellis/spec README.md README_CN.md build tools .github`
  returned no matches.
* `cmake -S sources/core/c -B sources/core/c/build`
* `cmake --build sources/core/c/build`
* `ctest --test-dir sources/core/c/build --output-on-failure` passed 35/35 tests.
