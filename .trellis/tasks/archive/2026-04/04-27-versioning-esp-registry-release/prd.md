# brainstorm: versioning and ESP Registry release

## Goal

Define a date-based release version policy for both SDK and host artifacts, then determine and implement the changes needed to publish the ESP32 component to the Espressif Component Registry.

## What I already know

* Desired version format is `YYMM.DD.BuildNumber`.
* The first target version should be `2604.27.1`.
* SDK and host-side artifacts should follow the same versioning rule.
* For another release on the same date, increment `BuildNumber`.
* For a release on a different date, update the date portion and reset `BuildNumber` to `1`.
* The ESP component currently references shared `core` code through a parent-relative path, and that may be incompatible with registry packaging.
* Host publishable version is currently `0.1.0` in `sources/host/Cargo.toml`.
* ESP component version is currently `0.1.0` in `sources/esp32/components/esp-wiremux/idf_component.yml`.
* The ESP component `CMakeLists.txt` currently compiles `../../../core/c/src/*.c` and includes `../../../core/c/include`.
* The shell used for this session does not currently have `compote` or `idf.py` on PATH, so registry package validation and ESP-IDF build validation require an ESP-IDF environment.
* The local ESP-IDF environment is available at `/Users/magicdian/esp/v5.4.2/esp-idf`.
* After sourcing `/Users/magicdian/esp/v5.4.2/esp-idf/export.sh`, `idf.py` reports `ESP-IDF v5.4.2-dirty` and `compote version` reports `2.2.2`.

## Assumptions (temporary)

* "SDK" refers to the ESP-IDF component/library side of this repository.
* "Host" refers to the Rust host CLI/library side of this repository.
* The desired output includes repo changes, not only documentation of the release process.

## Open Questions

* None currently.

## Requirements (evolving)

* Establish the project version as `2604.27.1` wherever the SDK and host publishable artifacts declare their version.
* Document the version bump rule so future releases can apply it consistently.
* Research whether the current ESP component layout can be published to `components.espressif.com`.
* Define the ESP Registry publish flow and any structure changes needed before publishing.
* Publishability should be solved with a separate `wiremux-core` ESP-IDF component and an `esp-wiremux` registry dependency, not by vendoring core files into the adapter component.
* Add a complete release flow for ESP publishing.
* CI publishing should be triggered by GitHub Releases on the `main` branch.
* ESP publishing must not affect the core code layout or standalone core compile configuration, to avoid implying that `sources/core` exists only for ESP32.
* ESP registry package directories should be generated at release time, not committed as canonical source trees.
* The generated `wiremux-core` package should be self-contained and built from the platform-neutral `sources/core/c` source files.
* The generated `esp-wiremux` package should be self-contained and depend on the registry-published `wiremux-core` package.

## Acceptance Criteria (evolving)

* [x] The repo has one clear policy for `YYMM.DD.BuildNumber` versioning.
* [x] SDK and host publishable version declarations are updated to `2604.27.1`.
* [x] The portable C core is packageable as an ESP-IDF component.
* [x] The `esp-wiremux` component can be packaged without parent-relative source references that break registry publication.
* [x] The `esp-wiremux` component declares a registry-compatible dependency on the core component.
* [x] The release documentation includes the publish steps for Espressif Component Registry.
* [x] CI release automation can publish the ESP components from a GitHub Release on `main`.
* [x] The standalone core CMake build remains unchanged in purpose and continues to read as platform-neutral.

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* Lint / typecheck / relevant build checks pass.
* Docs/notes updated for behavior or release-process changes.
* Release rollback or retry concerns are considered for registry publication.

## Out of Scope (explicit)

* Actually publishing a release to the public registry during brainstorming.
* Changing protocol behavior unrelated to packaging/versioning.

## Technical Notes

* Task created from `$brainstorm` on 2026-04-27.
* `sources/core/README.md` defines `sources/core` as the shared portable protocol boundary and `sources/esp32/components/esp-wiremux` as the ESP-IDF adapter.
* `sources/esp32/components/esp-wiremux/idf_component.yml` already exists but lacks richer registry metadata such as `license`, `repository`, `repository_info.path`, `documentation`, `tags`, and examples.
* `sources/core/c` already has `include/`, `src/`, and standalone CMake tests; it is a natural candidate for the `wiremux-core` ESP-IDF component root if its `CMakeLists.txt` is made dual-use for standalone CMake and ESP-IDF.
* Official ESP Component Manager docs state that a registry component needs a component directory with packaging metadata including manifest, README, and license.
* Official manifest docs state that `idf_component.yml` must be in the component root, `version` is required when uploading, and files uploaded are controlled from the component directory archive.
* Official manifest docs warn that local directory and Git dependencies are not supported when uploading components to the ESP Component Registry.
* Official manifest docs support `override_path` for local development/testing of registry dependencies; this can keep examples using in-repo components before production publication.
* Official versioning docs define the managed component version as `major.minor.patch~revision-prerelease+build`; `2604.27.1` maps cleanly to `major=2604`, `minor=27`, `patch=1`.
* Official CLI docs provide `compote component pack`, `compote component upload`, `--dry-run`, `--profile`, `--namespace`, `--repository-path`, and `--version` options.
* Official GitHub Actions guidance recommends OIDC for registry uploads, with `id-token: write` and `espressif/upload-components-ci-action@v2`.
* `upload-components-ci-action@v2` accepts multiple `components` entries in `component_name:relative/path` format and can run `dry_run`.
* GitHub release workflows can use `on: release: types: [published]`; GitHub sets `GITHUB_REF` to the release tag ref and `GITHUB_SHA` to the last commit in the tagged release.
* There is no existing `.github/` workflow directory in this repository.
* Remote repository is `git@github.com:magicdian/wiremux.git`, so the OIDC trusted uploader repository should likely be `magicdian/wiremux`.

## Research Notes

### What similar/official flows do

* Espressif's recommended ongoing publish path is GitHub Actions; CLI upload is also supported.
* The tutorial recommends validating on the staging registry before publishing to production.
* Upload is run from the component directory where `idf_component.yml` lives: `compote component upload --name <component>`.
* Staging upload uses `compote component upload --profile "staging" --name <component>`.
* The production registry can use the default namespace or an explicit `--namespace`.
* The `files` manifest block filters files included in the component archive; the docs describe packaging the component directory, so files outside the component root should not be relied on for registry consumers.

### Constraints from this repo/project

* ESP registry consumers will receive only the component package, not this repository's sibling `sources/core` directory.
* Keeping `sources/core` canonical is valuable because the host and future SDKs share protocol semantics.
* Duplicating core files manually would create drift risk unless automated or documented.
* Both ESP Registry and Cargo can accept `2604.27.1` as a three-segment version, but the semantic meaning becomes calendar-major/calendar-minor/build-patch rather than API-major/API-minor/API-patch.
* Because upload manifests cannot use local or Git dependencies, the core dependency in `esp-wiremux` should be a registry dependency for publishability. Local development can use `override_path` from consuming examples/projects.
* A release workflow triggered by GitHub Release should additionally verify the tagged commit is contained in `origin/main`, because the release event itself runs on a tag ref.

### Packaging approaches that avoid making `sources/core` ESP-specific

**Approach 2A: Generate registry package directories during release** (Recommended)

* How it works: keep `sources/core/c` unchanged; add release templates/scripts under an ESP/release tooling path; CI/local release scripts assemble temporary package directories for `wiremux-core` and `esp-wiremux`, copying the necessary core source into the `wiremux-core` package and using registry manifests there.
* Pros: does not change core build config or make core look ESP-specific; registry packages are self-contained; CI can upload generated package directories.
* Cons: release packaging depends on an assembly script; generated package directories need validation to catch drift.

**Approach 2B: Add a committed ESP registry wrapper component outside core**

* How it works: add a `wiremux-core` ESP component under an ESP-specific path with its own manifest/CMake and either copied source or local development references.
* Pros: easy to inspect as an ESP component in the repo; simpler CI component path.
* Cons: either duplicates source in the repo or still needs special packaging; developers may still see another "core" and wonder which is canonical.

**Approach 2C: Make `sources/core/c` dual-use as standalone CMake and ESP-IDF**

* How it works: add ESP-IDF manifest and conditional CMake behavior directly in `sources/core/c`.
* Pros: no generated package layer and no duplicate source.
* Cons: conflicts with the preference that core remains platform-neutral and not ESP-looking.

### Feasible approaches here

**Approach A: Vendor core into the ESP component with a sync script** (Recommended)

* How it works: keep `sources/core/c` as canonical; add a generated/vendor copy under `sources/esp32/components/esp-wiremux` and update the component CMake to compile local files only. Add a repo script/check to refresh or verify the copy before packaging.
* Pros: single registry component, easy for users to consume, compatible with registry packaging, preserves a canonical source with automation.
* Cons: adds copied files to the repo and requires a drift-prevention check.

**Approach B: Publish a separate `wiremux-core` ESP component**

* How it works: make the portable C core its own ESP-IDF component with an `idf_component.yml`; make `esp-wiremux` depend on it through the registry.
* Pros: no vendored duplication; clean dependency boundary; future platform components can reuse the same registry artifact.
* Cons: two components to publish/version/test; dependency resolution and namespace setup must work before `esp-wiremux` is usable.

**Approach C: Move the canonical C core inside the ESP component tree**

* How it works: physically relocate `sources/core/c` under `sources/esp32/components/esp-wiremux` and update host/core build paths accordingly.
* Pros: one component package without generated copies.
* Cons: weakens the current shared-core boundary and makes non-ESP platform ownership less clear.

## Decision (ADR-lite, evolving)

**Context**: The ESP Component Registry packages component directories, while the current ESP adapter references shared core files through parent-relative paths.

**Decision**: Use Approach B. Create or expose the portable C core as a separate ESP-IDF component, then make `esp-wiremux` depend on that core component for registry consumption.

**Consequences**: This preserves the shared-core boundary and avoids vendored source duplication, but release flow must publish and validate two ESP components in dependency order.

## Scope Decision

The MVP should cover the complete ESP release flow, including local package validation, release documentation, and CI automation triggered by GitHub Releases from the `main` branch.

## Packaging Decision (ADR-lite)

**Context**: `sources/core/c` must remain platform-neutral and should not gain ESP-specific manifests or build semantics, while ESP Registry packages must be self-contained component directories.

**Decision**: Use Approach 2A. Add release tooling that generates temporary ESP Registry package directories for `wiremux-core` and `esp-wiremux` from canonical repo sources.

**Consequences**: Core source and standalone CMake remain clean and platform-neutral. Release automation must validate generated package contents and keep generated manifests/version values in sync with the repo release version.

## Technical Approach

* Add a canonical release version source for this repo and set it to `2604.27.1`.
* Update host and SDK-facing version declarations to `2604.27.1`.
* Add release documentation for `YYMM.DD.BuildNumber`:
  * Same date: increment `BuildNumber`.
  * Different date: update `YYMM.DD` and reset `BuildNumber` to `1`.
* Add an ESP Registry packaging script that generates self-contained component directories under an ignored/dist path.
* Generate `wiremux-core` from `sources/core/c` without changing core's existing CMake configuration.
* Generate `esp-wiremux` from `sources/esp32/components/esp-wiremux` with local core references replaced by registry dependency usage.
* Add a GitHub Actions workflow triggered by `release.published`; it checks the release tag commit is contained in `origin/main`, generates packages, validates/builds, then uploads both components using OIDC and `espressif/upload-components-ci-action@v2`.
* Document required ESP Registry Trusted Uploader setup for `magicdian/wiremux`, workflow file, and `main` branch.

## Implementation Notes

* Added `VERSION` as the canonical release version file.
* Added `tools/esp-registry/generate-packages.sh` to generate self-contained `wiremux-core` and `esp-wiremux` registry package directories under `dist/esp-registry/`.
* Added `.github/workflows/esp-registry-release.yml` for GitHub Release publication with an explicit `origin/main` ancestry guard.
* Added `docs/esp-registry-release.md` with local validation, manual staging/production upload, and CI Trusted Uploader setup.
* Kept `sources/core/c/CMakeLists.txt` unchanged.
* Validation run:
  * `tools/esp-registry/generate-packages.sh`
  * `compote component pack --name wiremux-core`
  * `compote component pack --name esp-wiremux`
  * `cargo fmt --check`
  * `cargo check`
  * `cargo test`
  * `cmake -S sources/core/c -B sources/core/c/build`
  * `cmake --build sources/core/c/build`
  * `ctest --test-dir sources/core/c/build --output-on-failure`
  * `. /Users/magicdian/esp/v5.4.2/esp-idf/export.sh && idf.py set-target esp32s3 && idf.py build`
  * `git diff --check`
