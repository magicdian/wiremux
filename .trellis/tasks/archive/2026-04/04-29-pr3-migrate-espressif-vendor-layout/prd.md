# PR3: Migrate Espressif Vendor Layout

## Goal

Move ESP32-specific implementation into the vendor tree while preserving current
ESP-IDF component and example behavior.

## Requirements

- Move current ESP32 implementation from `sources/esp32` to
  `sources/vendor/espressif/generic`.
- Target layout:
  - `sources/vendor/espressif/README.md`
  - `sources/vendor/espressif/generic/components/esp-wiremux/`
  - `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo/`
  - `sources/vendor/espressif/s3/README.md`
  - `sources/vendor/espressif/p4/README.md`
- Keep model-specific directories as README-only placeholders for now.
- Update CMake relative paths, ESP registry packaging, docs, specs, and README
  references.
- Do not preserve old `sources/esp32` symlinks or compatibility aliases.

## Acceptance Criteria

- [ ] ESP-IDF component and example live under `sources/vendor/espressif/generic`.
- [ ] Placeholder `README.md` files track `s3` and `p4` directories.
- [ ] `tools/esp-registry/generate-packages.sh` packages from the new paths.
- [ ] No active docs/specs refer to `sources/esp32` except migration notes.

## Validation

- `rg "sources/esp32|sources/vendor/espressif"`
- `tools/esp-registry/generate-packages.sh`
- ESP-IDF build from `sources/vendor/espressif/generic/examples/esp_wiremux_console_demo` when ESP-IDF is available.
