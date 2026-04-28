# ESP Registry Release

This project uses one release version for host and SDK artifacts.

## Version Rule

Version format: `YYMM.DD.BuildNumber`.

Current release: `2604.28.1`.

When publishing another release on the same date, increment `BuildNumber`. When
publishing on a different date, update `YYMM.DD` and reset `BuildNumber` to `1`.

Examples:

- `2604.27.1`: first release on 2026-04-27.
- `2604.27.3`: passthrough console release on 2026-04-27.
- `2604.27.2`: second release on 2026-04-27.
- `2604.28.1`: first release on 2026-04-28.

Before a release, update:

- `VERSION`
- `sources/host/wiremux/crates/wiremux-cli/Cargo.toml`
- `sources/host/wiremux/Cargo.lock`
- `sources/vendor/espressif/generic/components/esp-wiremux/idf_component.yml`
- `sources/vendor/espressif/generic/components/esp-wiremux/include/esp_wiremux.h`

## ESP Registry Package Shape

The portable core remains platform-neutral under `sources/core/c`. Do not add
ESP-IDF component metadata or ESP-specific CMake behavior there.

ESP Registry packages are generated under `dist/esp-registry/`:

- `wiremux-core`: generated from `sources/core/c`.
- `esp-wiremux`: generated from `sources/vendor/espressif/generic/components/esp-wiremux`.

The generated `esp-wiremux` package depends on the generated registry package
`<namespace>/wiremux-core` at the same version.

The generated `esp-wiremux` package also includes the console demo under
`examples/esp_wiremux_console_demo`. The source-tree demo uses
`EXTRA_COMPONENT_DIRS` for local development, but the generated registry example
has a registry-friendly project `CMakeLists.txt` and a `main/idf_component.yml`
dependency on `<namespace>/esp-wiremux` with `override_path: "../../../"`.
The registry strips `override_path` from downloaded examples.

## Generate Packages Locally

Activate ESP-IDF when you want to validate with `compote` or `idf.py`:

```bash
. /Users/magicdian/esp/v5.4.2/esp-idf/export.sh
```

Generate production packages:

```bash
tools/wiremux-build package esp-registry
```

Use a custom namespace if the registry namespace differs from the default:

```bash
WIREMUX_ESP_REGISTRY_NAMESPACE=<namespace> tools/wiremux-build package esp-registry
```

Generate staging packages by adding a staging registry URL to the dependency
manifest:

```bash
WIREMUX_ESP_REGISTRY_NAMESPACE=<namespace> \
WIREMUX_ESP_REGISTRY_URL=https://components-staging.espressif.com \
tools/wiremux-build package esp-registry
```

Direct script invocation remains available for low-level troubleshooting:

```bash
tools/esp-registry/generate-packages.sh
```

## Local Validation

Build and test the platform-neutral C core:

```bash
cmake -S sources/core/c -B sources/core/c/build
cmake --build sources/core/c/build
ctest --test-dir sources/core/c/build --output-on-failure
```

Build and test the host crate:

```bash
cd sources/host/wiremux
cargo fmt --check
cargo check
cargo test
```

Build the ESP-IDF example from the source tree:

```bash
cd sources/vendor/espressif/generic/examples/esp_wiremux_console_demo
idf.py set-target esp32s3
idf.py build
```

Pack generated registry components:

```bash
cd dist/esp-registry/wiremux-core
compote component pack --name wiremux-core

cd ../esp-wiremux
compote component pack --name esp-wiremux
```

## Manual Upload

Publish `wiremux-core` before `esp-wiremux`.

Production login:

```bash
compote registry login --profile "default" \
  --registry-url "https://components.espressif.com" \
  --default-namespace <namespace>
```

Production upload:

```bash
cd dist/esp-registry/wiremux-core
compote component upload --name wiremux-core --namespace <namespace>

cd ../esp-wiremux
compote component upload --name esp-wiremux --namespace <namespace>
```

Staging login:

```bash
compote registry login --profile "staging" \
  --registry-url "https://components-staging.espressif.com" \
  --default-namespace <namespace>
```

Staging upload:

```bash
cd dist/esp-registry/wiremux-core
compote component upload --profile "staging" --name wiremux-core --namespace <namespace>

cd ../esp-wiremux
compote component upload --profile "staging" --name esp-wiremux --namespace <namespace>
```

If `esp-wiremux` upload fails because `wiremux-core` has not propagated through
the registry yet, retry the `esp-wiremux` upload after the core component version
is visible.

## GitHub Release CI

The CI workflow publishes when a GitHub Release is published. The workflow checks
that the release tag version matches `VERSION` and that the tagged commit is
contained in `origin/main` before uploading. It runs:

- `tools/wiremux-build doctor`
- `tools/wiremux-build check all`
- `tools/wiremux-build package esp-registry`

The workflow installs ESP-IDF `v5.4.1` before checks. In CI, vendor-espressif
validation is strict: missing or mismatched `idf.py` fails validation.

Registry setup required before the first CI upload:

1. Sign in to the ESP Component Registry.
2. Create or select the target namespace.
3. Create empty components for `wiremux-core` and `esp-wiremux` if they do not
   exist yet.
4. For each component, add a Trusted Uploader:
   - Repository: `magicdian/wiremux`
   - Workflow: `esp-registry-release.yml`
   - Branch: leave empty
   - Environment: leave empty
5. Ensure the workflow namespace matches the registry namespace.

GitHub Release events run from tag refs, for example
`refs/tags/v2604.28.1`. Do not set Trusted Uploader Branch to `main` for this
workflow, or the registry OIDC authorization will not match. The workflow itself
still fetches `origin/main` and fails before upload if the tagged release commit
is not contained in `main`.

The workflow uses OIDC and does not require a long-lived
`IDF_COMPONENT_API_TOKEN` secret.
