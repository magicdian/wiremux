# brainstorm: core gtest gmock unit tests

## Goal

Introduce GoogleTest and GoogleMock for the portable Wiremux core so core C
behavior can be validated on the host without flashing ESP hardware. This
should make future protocol work more maintainable and give the project a
standard unit-test foundation.

## What I already know

* The user wants the core portion to use gtest/gmock for unit tests.
* The motivation is host-side validation of basic core behavior and a more
  regular testing foundation for future development.
* The current portable core lives under `sources/core/c`.
* Existing core validation is a single hand-written C smoke test at
  `sources/core/c/tests/wiremux_core_smoke_test.c`.
* The smoke test covers CRC32, envelope round trip, manifest encoding, frame
  decoding, and CRC mismatch behavior through `assert()`.
* There is no current `sources/core/c/CMakeLists.txt` or dedicated host-side
  CMake test project.
* The ESP-IDF component includes core C sources directly from
  `sources/core/c/src`.
* Rust host tests already exist under `sources/host/src`, but they do not test
  the portable C core.

## Assumptions (temporary)

* The MVP should target host-side unit tests for `sources/core/c`, not ESP-IDF
  device-side Unity tests.
* The first step should migrate or mirror the existing smoke coverage into
  GoogleTest assertions.
* GoogleMock should be introduced in the test dependency and link surface even
  if the first tests do not need mocks yet.
* The production core C API should remain C-compatible; tests may be C++.

## Open Questions

* None.

## Requirements

* Add a host-runnable unit-test entry point for portable core C code.
* Use GoogleTest and GoogleMock for new core tests.
* Fetch GoogleTest/GoogleMock through CMake `FetchContent` using a pinned
  revision.
* Keep CMake build outputs and fetched dependency trees out of git.
* Migrate the current smoke-test coverage into GoogleTest tests.
* Add first-pass boundary/error tests for frame, envelope, and manifest core
  APIs.
* Link GoogleMock so future collaboration-style tests can use it; add gmock
  tests only if the current core exposes a real mockable dependency and avoid
  artificial production abstractions solely for mocks.
* Keep ESP-IDF adapter behavior and build integration unchanged unless needed
  for the test build.
* Preserve C ABI and C source layout for portable core implementation.

## Acceptance Criteria

* [x] A developer can configure and run core C unit tests on the host.
* [x] Existing smoke-test scenarios are covered with GoogleTest assertions.
* [x] GoogleMock is available to future tests.
* [x] New tests cover representative invalid argument, short buffer, bad magic,
  bad version, payload length, CRC mismatch, unsupported wire type, truncated
  varint/field, and invalid manifest descriptor cases.
* [x] The core test command is documented.
* [x] Existing Rust host tests and ESP-IDF build assumptions are not broken.

## Definition of Done (team quality bar)

* Tests added/updated.
* Lint / typecheck / build checks run where appropriate.
* Docs/notes updated for new test command.
* Rollout/rollback considered if dependency fetching or vendoring is risky.

## Out of Scope (explicit)

* Device-side ESP-IDF Unity test app.
* Rewriting ESP adapter tests.
* Large protocol refactors unrelated to introducing the test framework.
* Replacing Rust host tests.
* Introducing artificial interfaces or callbacks only to demonstrate GoogleMock.

## Implementation Summary

* Added `sources/core/c/CMakeLists.txt` with a `wiremux_core_c` static library,
  pinned GoogleTest/GoogleMock `FetchContent`, and CTest discovery.
* Replaced the assert-based C smoke test with
  `sources/core/c/tests/wiremux_core_test.cpp`.
* Added 16 host-side tests covering CRC, frame encode/decode, envelope
  encode/decode, manifest encoding, and representative error/status branches.
* Added `/sources/core/c/build/` to `.gitignore`.
* Updated `sources/core/README.md` and backend specs with the new CMake/CTest
  validation command.

## Verification

* `cmake -S sources/core/c -B sources/core/c/build`
* `cmake --build sources/core/c/build`
* `ctest --test-dir sources/core/c/build --output-on-failure`
* `git diff --check`

## Research Notes

### What official docs suggest

* GoogleTest's CMake quickstart uses CMake `FetchContent` to declare and make
  GoogleTest available, then links test binaries against GoogleTest targets.
* CMake's `GoogleTest` module supports test registration through
  `gtest_discover_tests()` and `gtest_add_tests()`. The docs describe
  `gtest_discover_tests()` as more robust for parameterized tests, while
  `gtest_add_tests()` can be more convenient for cross-compiling environments.
* CMake's `FindGTest` module exposes imported targets including `GTest::gtest`,
  `GTest::gtest_main`, `GTest::gmock`, and `GTest::gmock_main` in modern CMake.

### Constraints from this repo/project

* The portable core is plain C and should stay usable from ESP-IDF.
* Tests can be C++ because GoogleTest/GoogleMock are C++ frameworks.
* The repository currently has no top-level CMake project for `sources/core/c`.
* A dependency-download strategy affects offline builds and CI reproducibility.

### Feasible approaches here

**Approach A: Core-local CMake + FetchContent** (Recommended)

* How it works: add `sources/core/c/CMakeLists.txt`, build the C core as a test
  target, use `FetchContent` to fetch a pinned GoogleTest revision, and register
  tests with CTest.
* Pros: easy for new contributors, self-contained, follows GoogleTest
  quickstart, no vendored dependency churn.
* Cons: first configure needs network access unless dependency cache is already
  populated.

**Approach B: Core-local CMake + find_package(GTest)**

* How it works: add the same local test project, but require GoogleTest to be
  installed on the host and found through CMake.
* Pros: no network fetch during configure; friendly to distro/package-manager
  dependency policy.
* Cons: more setup friction; CI and developer machines need preinstalled
  compatible packages.

**Approach C: Vendor GoogleTest as a submodule or third-party source**

* How it works: add GoogleTest to the repository or as a git submodule and build
  it from that pinned source.
* Pros: reproducible and offline after checkout/submodule init.
* Cons: larger repo footprint and dependency maintenance overhead.

## Technical Notes

* Relevant files inspected:
  * `sources/core/README.md`
  * `sources/core/c/tests/wiremux_core_smoke_test.c`
  * `sources/esp32/components/esp-wiremux/CMakeLists.txt`
  * `sources/host/Cargo.toml`
  * `.gitignore`
* Decision: use Approach A, core-local CMake with `FetchContent`.
* `.gitignore` currently ignores Rust target output and ESP-IDF example build
  output, but does not yet ignore a core CMake build directory. Add an ignore
  entry such as `/sources/core/c/build/`; CMake FetchContent downloads normally
  live under the build tree, commonly `build/_deps/`, so ignoring the build
  directory prevents fetched GoogleTest sources from being accidentally tracked.
* Candidate first-pass test areas from source inspection:
  * `wiremux_frame_encode()` and `wiremux_frame_decode()` status branches:
    invalid args, undersized output, short input, bad magic, bad version,
    max payload rejection, incomplete full frame, CRC mismatch.
  * `wiremux_envelope_encode()` and `wiremux_envelope_decode()` status
    branches: invalid args, insufficient output, round trip, unknown varint
    fields ignored, unsupported wire type, truncated varint, truncated
    length-delimited field.
  * `wiremux_device_manifest_encode()` status branches: invalid args, invalid
    channel descriptor pointer/count combinations, insufficient output, optional
    empty strings omitted.
* Official references:
  * GoogleTest CMake quickstart:
    https://google.github.io/googletest/quickstart-cmake.html
  * CMake GoogleTest module:
    https://cmake.org/cmake/help/latest/module/GoogleTest.html
  * CMake FindGTest module:
    https://cmake.org/cmake/help/latest/module/FindGTest.html
