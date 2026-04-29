# Brainstorm: Source Layout and Build Productization

## Goal

Explore and define whether Wiremux should perform an early productization
refactor of its source layout and build system so future core, host, profile,
and vendor implementations can evolve with clearer boundaries and faster
adapter development.

## What I Already Know

- The product architecture now separates `wiremux-core`, host enhanced tooling,
  profile contracts, transports, and device SDK adapters.
- The user wants future host tools under `host/wiremux/` because host-side may
  contain additional tools beyond the current Wiremux CLI/TUI binary.
- The user is willing to perform a large source-layout and build-system refactor
  now, even though the immediate payoff may be limited, because the expected
  future adapter/productization payoff is high.
- The user wants to learn from AOSP/Treble layering and Yocto/BitBake-style
  customization.
- The current repo has `sources/core`, `sources/host`, and `sources/esp32`.
- Current host Rust code is a single Cargo package under `sources/host`, with a
  `wiremux` binary and library. It already mixes CLI, passthrough, TUI, host
  session FFI, and tests.
- Current ESP-IDF adapter directly references portable core C sources using
  relative paths from `sources/esp32/components/esp-wiremux/CMakeLists.txt`.
- Current release packaging script hard-codes `sources/core/c`,
  `sources/esp32/components/esp-wiremux`, and the ESP32 example path.
- README, Chinese docs, Trellis specs, ESP registry docs, and example docs all
  contain current `sources/*` paths.

## Assumptions (Temporary)

- The public command should likely remain `wiremux`, with CLI/TUI modes as
  subcommands or modes rather than separate user-facing products.
- Source layout can change before the project has many external downstreams,
  but migration cost across Cargo, CMake, ESP-IDF, docs, release packaging, and
  tests must be explicitly handled.
- A lightweight build-profile system may be preferable to adopting BitBake
  directly.

## Open Questions

- Which initial refactor scope should be the MVP: layout-only, layout plus build
  profiles, or full modular build packaging?
- If choosing full architecture refactor, how much host crate splitting and
  build orchestration must be included in the first implementation PR?
- Final implementation will be split into small PRs/subtasks with explicit
  goals and acceptance criteria.

## Requirements (Evolving)

- Preserve a product architecture where core is vendor-neutral.
- Move toward a layout that can support multiple host tools and multiple vendor
  SDK adapters.
- Preserve or improve buildability of core, host, and current ESP32 component.
- Record design decisions in docs before implementation.
- Put the current Wiremux host tool under a future-proof host namespace, likely
  `sources/host/wiremux/`, so future host-side tools can live beside it.
- Avoid embedding vendor-specific behavior into `wiremux-core` during layout
  refactoring.
- The user is leaning toward the full architecture refactor option for MVP.
- Build-system implementation can stay minimal in MVP, but it must be designed
  for long-term use as Wiremux's own product orchestration layer.
- The build orchestration layer should include a reproducibility contract from
  the start, even if it does not implement a full hermetic build system.
- Host should move to a Cargo workspace skeleton during the architecture
  refactor. The first implementation only needs to compile and preserve current
  behavior; deeper host code refactoring can happen in later tasks.
- CI reproducibility should be strict, but local user builds should be tolerant
  of compatible patch/minor tool versions. Local builds may warn or mark
  artifacts as dirty/deviated when tool versions differ from the pinned CI
  contract.
- Build configuration should support a lunch-like selection flow. Initial axes:
  device side choices such as `espressif-esp32-s3`, `espressif-esp32-p4`, and
  `core-only`; host side choices such as `all-feature` and `device-only`.
- Lunch/product definitions must be data-driven and customizable, not hard-coded
  only in Python or Rust code.
- Build tooling implementation may use a hybrid bootstrap model where a very
  small Python or shell-compatible launcher finds or builds a Rust helper, then
  delegates real logic to that helper.
- Target directory layout should migrate ESP32 into
  `sources/vendor/espressif/`, with `generic`, `s3`, and `p4` directories.
  Current implementation can live in `generic`; model-specific implementation
  can move into `s3` or `p4` later.
- Empty placeholder directories introduced by the architecture should include a
  short `README.md` so Git tracks them and future contributors understand their
  purpose.
- Host should use a workspace skeleton, but the first pass should minimize code
  movement inside the host implementation beyond what is required to compile.
- `wiremux-build` should use a Python bootstrap plus a Rust helper.
- Build/lunch configuration should use TOML.
- Dirty/deviated local builds should generate a build metadata file.
- CI version strictness should be configurable per tool.
- Project specs must be updated as part of the refactor.
- Old paths should not be preserved through symlinks or compatibility aliases.
- Build output unification can remain incremental, but any output path that is
  easy to migrate during this refactor should be migrated.
- Lunch should use a hybrid state model:
  - `.wiremux/build/selected.toml` is the persistent source of truth.
  - `wiremux-build env --shell bash|zsh` can emit optional environment exports
    derived from selected config.
  - Environment variables are not the ordinary source of truth.
  - Resolution priority is `CLI args > selected.toml > product defaults`.
- `.gitignore` must be updated for new generated directories and local state,
  including build outputs, selected local lunch state, helper build artifacts,
  and generated metadata that should not be committed.
- Root-level local/build layout:
  - `.wiremux/` for local generated state, gitignored
  - `build/` for committed product/profile/toolchain configs
  - `build/out/` for generated output, gitignored
  - `tools/wiremux-build` as bootstrap launcher
  - `tools/wiremux-build-helper/` as Rust helper crate
- First `wiremux-build` command set:
  - `lunch <device> <host-preset>`
  - `env --shell bash|zsh`
  - `doctor`
  - `check core`
  - `check host`
  - `check vendor-espressif`
  - `check all`
  - `build core`
  - `build host`
  - `build vendor-espressif` may initially be equivalent to the check/build
    path until richer artifacts exist
  - `package esp-registry`
- Host preset names should use normalized forms:
  - `all-features`
  - `device-only`
  - `generic-only`
- Device target names should use readable hyphenated forms:
  - `core-only`
  - `espressif-esp32-s3`
  - `espressif-esp32-p4`
- Lunch compatibility rules should hide or reject invalid combinations. In
  particular, `core-only + device-only` is invalid because no device vendor is
  selected; meaningful host presets for `core-only` are `all-features` and
  `generic-only`.
- ESP32 implementation should migrate to:
  - `sources/vendor/espressif/generic/components/esp-wiremux/`
  - `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo/`
  - `sources/vendor/espressif/s3/README.md`
  - `sources/vendor/espressif/p4/README.md`
- Protocol schema should migrate from `sources/core/proto` to
  `sources/api/proto`.
- `sources/profiles/` should be created as a README-only skeleton for now,
  including generic profile placeholders such as transfer, console, and pty.
- Host Cargo features can be introduced as a non-restrictive skeleton, with
  real behavior gating deferred until the workspace and code boundaries are
  clearer.

## Acceptance Criteria (Evolving)

- [ ] Final requirements describe target source layout.
- [ ] Final requirements describe build-profile behavior.
- [ ] Final requirements identify migration risks and validation commands.
- [ ] Final requirements define what is explicitly out of scope.

## Definition of Done (Team Quality Bar)

- Tests added/updated where behavior or build paths change.
- Lint, typecheck, and relevant build checks pass.
- Docs/notes updated if behavior or paths change.
- Rollout and rollback considered for risky path moves.

## Out of Scope (Explicit)

- Deciding this brainstorm alone does not move runtime files.
- Adding actual ESP32 OTA, Raspberry Pi, or firmware update protocol behavior is
  out of scope for the source-layout refactor.

## Technical Notes

- Current key files inspected:
  - `sources/host/Cargo.toml`
  - `sources/host/build.rs`
  - `sources/core/c/CMakeLists.txt`
  - `sources/esp32/components/esp-wiremux/CMakeLists.txt`
  - `.github/workflows/esp-registry-release.yml`
  - `tools/esp-registry/generate-packages.sh`
  - `.gitignore`
- Current generated build outputs are ignored at:
  - `/sources/host/target/`
  - `/sources/core/c/build/`
  - `/sources/esp32/examples/*/build/`
  - `/dist/`
- Path migration impacts docs and specs broadly. `rg` found hard-coded
  `sources/core`, `sources/host`, and `sources/esp32` references across README,
  docs, `.trellis/spec`, release docs, and ESP example docs.

## Research Notes

### What Similar Tools Do

- AOSP uses Repo manifest files to aggregate many Git projects into one source
  tree and associates each project with a specific directory. Reference:
  <https://source.android.com/docs/setup/start> and
  <https://source.android.com/docs/setup/reference/repo>.
- AOSP uses Soong with `Android.bp` module files and build flags. The useful
  lesson is explicit module boundaries and build-time selection, not copying
  Soong itself. Reference: <https://source.android.com/docs/setup/build> and
  <https://source.android.com/docs/setup/reference/androidbp>.
- Android VINTF separates what a device provides from what the framework
  requires using manifests and compatibility matrices. The Wiremux analog is
  `DeviceManifest` plus profile contracts and conformance tests. Reference:
  <https://source.android.com/docs/core/architecture/vintf>.
- Yocto/BitBake uses recipes for package build instructions and layers to group
  related metadata. The useful lesson is layer composition and overrideable
  metadata. Reference: <https://www.yoctoproject.org/development/technical-overview/>
  and <https://docs.yoctoproject.org/dev/singleindex.html>.
- Cargo features are the standard Rust mechanism for optional dependencies and
  conditional compilation. Features should be additive; mutually exclusive
  features are discouraged. Reference:
  <https://doc.rust-lang.org/cargo/reference/features.html>.
- CMake Presets provide shareable configure/build/test/package/workflow presets,
  with project-wide `CMakePresets.json` and local `CMakeUserPresets.json`.
  Reference: <https://cmake.org/cmake/help/latest/manual/cmake-presets.7.html>.

### Constraints From This Repo

- Portable C core must stay platform-neutral.
- Host currently compiles portable C core through `sources/host/build.rs` using
  a relative `../core/c` path.
- ESP-IDF component currently compiles core C sources directly through relative
  `../../../core/c` paths.
- ESP registry release packaging currently generates separate `wiremux-core`
  and `esp-wiremux` registry packages from source paths.
- Large directory moves must update Trellis specs or future AI sessions will
  keep using stale source paths.

### Feasible Approaches Here

**Approach A: Product Layout First, Lightweight Build Profiles** (Recommended)

- How it works:
  - Move physical layout toward `sources/api`, `sources/core`,
    `sources/host/wiremux`, `sources/vendor/espressif`, and future
    `sources/profiles`.
  - Keep one Rust host Cargo package initially, relocated under
    `sources/host/wiremux`.
  - Add a small repo-level build orchestration script/config later, backed by
    Cargo features and CMake/ESP-IDF commands.
- Pros:
  - Aligns source tree with product architecture now.
  - Keeps the first implementation tractable.
  - Enables future host tools under `sources/host/<tool>`.
  - Avoids adopting a heavyweight build system before multiple vendors exist.
- Cons:
  - Still requires many path updates and validation.
  - Build customization starts as conventions and scripts, not a full recipe
    engine.

**Approach B: Layout Plus Repo Build Layer Immediately**

- How it works:
  - Perform the same source layout move.
  - Add a first-class `build/` or `buildsys/` metadata layer with build profiles
    such as `core-only`, `host-generic`, `host-esp32`, and `vendor-esp32`.
  - Add one wrapper command/script that executes Cargo, CMake, and ESP-IDF
    checks according to the selected profile.
- Pros:
  - Makes productization intent concrete immediately.
  - Creates one place for future Yocto-like customization.
  - Reduces future ambiguity about supported build combinations.
- Cons:
  - Higher first PR risk.
  - Wrapper can become a second build system if not kept thin.
  - Requires careful test coverage for profile selection.

**Approach C: Full Modular Workspace and Recipe System**

- How it works:
  - Split host into multiple crates immediately.
  - Add recipe/layer metadata inspired by Yocto.
  - Model core, profiles, vendors, host tools, and release packages as build
    units.
- Pros:
  - Strongest long-term modularity.
  - Closest to a formal product platform.
- Cons:
  - Too much design surface before the second vendor exists.
  - High migration risk across Cargo, CMake, ESP-IDF, docs, and releases.
  - Easy to overbuild and slow down current feature work.

Current user preference: choose the full architecture direction, but implement
the build system as a minimal long-term orchestration layer rather than a full
recipe engine.

### Build-System Direction Notes

- Directly adopting BitBake as the primary build system is not a good fit for
  the current repo. BitBake/Yocto is strongest when building complete embedded
  Linux distributions, SDKs, root filesystems, and image artifacts. Wiremux must
  support MCUs, ESP-IDF components, Rust host tools, portable C libraries, and
  vendor adapters without forcing all users into a Linux distribution build
  workflow.
- A future Yocto layer is still valuable as an integration artifact for Linux
  gateways or products that already use Yocto:
  `integrations/yocto/meta-wiremux`.
- The better long-term model is likely a Wiremux product/build orchestrator that
  preserves native build systems underneath:
  - Cargo for Rust host tools.
  - CMake for portable C core.
  - ESP-IDF `idf.py` for ESP targets.
  - Future Zephyr `west` for Zephyr-based targets.
  - Future PlatformIO integration for users who already use PlatformIO.
- The orchestrator should describe products, devices, vendors, features, and
  artifacts, but it should not replace vendor-native builds.
- A good target shape is closer to "repo/west-like workspace + thin product
  build profiles" than to a full BitBake clone.
- If the MVP chooses the full architecture refactor, the build system should
  still stay scoped to required orchestration:
  - declare product profiles and backends
  - invoke native tools
  - print exact native commands
  - pin or validate tool versions where feasible
  - avoid custom dependency solving and custom caching in v1
- Reproducibility should be handled through explicit toolchain contracts first:
  - `rust-toolchain.toml`
  - CMake minimum version and presets
  - ESP-IDF supported version declaration
  - Python version declaration for tools
  - lockfiles where native ecosystems already provide them
  - optional future Nix dev shell or container image
- Local reproducibility policy should distinguish CI and developer machines:
  - CI uses pinned versions and should fail on mismatch.
  - Local builds accept compatible versions by default and print warnings.
  - Build metadata should record the actual tool versions used.
  - Release artifacts can include a dirty/deviated marker when built outside the
    pinned contract.
- A lunch-like flow should be represented as product metadata:
  - products combine a device-side target, host-side feature preset, build
    backends, checks, and artifact rules
  - vendors can contribute their own product definitions in their source
    directories or integration directories
  - local overrides can add products without editing the main tool code
- Lunch state decision:
  - `.wiremux/build/selected.toml` is the source of truth.
  - optional shell env output can be generated from selected state.
  - ordinary command resolution does not let environment variables override
    selected config.
  - command-line arguments always win.
- A Rust implementation is attractive for the main helper because it gives a
  typed CLI, native binary distribution, strong TOML parsing, and easier
  cross-platform process handling than shell-heavy scripts.
- Rust helper risks:
  - bootstrap needs Rust installed before the helper can build
  - first run is slower because the helper may need compilation
  - cross-compiling the helper itself is extra release work
  - care is needed to avoid making the bootstrap depend on the same complex
    environment it is trying to validate
- A thin bootstrap launcher can remain intentionally boring:
  - locate repo root
  - check whether the helper binary exists and is fresh enough
  - run `cargo run` or `cargo build` for the helper if needed
  - exec the helper with all original arguments
  - avoid Python package dependencies and support broad Python 3 versions

## Expansion Sweep

### Future Evolution

- Multiple host tools may exist under `sources/host/`, with `wiremux` as only
  one product binary.
- Multiple vendor SDKs may exist under `sources/vendor/`, with adapters selected
  by manifest-declared profiles and host build features.

### Related Scenarios

- ESP Component Registry packaging must continue generating `wiremux-core` and
  `esp-wiremux` packages.
- Developer commands should remain straightforward: core tests, host tests, ESP
  example build, and registry package generation.
- `core-only` device selection should not present or accept host presets that
  only make sense when a concrete device/vendor target exists.

### Failure and Edge Cases

- Stale path references in docs/specs can make future AI sessions and release
  steps wrong even if code compiles.
- Relative paths in Cargo build scripts and ESP-IDF CMake are fragile during
  moves and need explicit validation.
