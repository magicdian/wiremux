# bugfix: fix ESP registry vendor example path

## Goal

Fix the ESP Registry release validation failure observed for version `2605.3.2`
and record the root cause so future releases do not regress to a stale example
path.

## Problem

The ESP release workflow failed during:

```bash
tools/wiremux-build doctor
tools/wiremux-build check vendor
tools/wiremux-build package esp-registry
```

The visible error was:

```text
+ idf.py set-target esp32s3
error: spawn idf.py: No such file or directory (os error 2)
```

`doctor` had already found `idf.py`, so the message was misleading. The direct
cause was that `build/wiremux-vendors.toml` pointed ESP32-S3 dispatch at:

```text
sources/vendor/espressif/generic/examples/esp_wiremux_console_demo
```

That directory is absent in the `2605.3.2` tag because earlier work split the
single console-oriented example into separate beginner, advanced, and
professional examples, but the vendor build selector config was not updated in
the same change. The available source-tree examples are:

```text
esp_wiremux_beginner_demo
esp_wiremux_advanced_demo
esp_wiremux_professional_demo
```

Rust reported the missing `current_dir` as a spawn failure for `idf.py`, which
obscured the stale config path.

## Requirements

* Update vendor build dispatch to use an existing ESP-IDF example path.
* Keep the build selector config aligned with the documented release example.
* Preserve ESP Registry packaging of all shipped examples.
* Add a clear preflight error when a vendor `example_path` does not exist, before
  invoking `idf.py`.
* Add focused regression coverage for the missing example path error.
* Update backend quality guidance so future sessions use the current example
  path.

## Packaging Note

`tools/esp-registry/generate-packages.sh` packages three ESP examples:

```text
esp_wiremux_beginner_demo
esp_wiremux_advanced_demo
esp_wiremux_professional_demo
```

This bugfix changes vendor build validation to compile the professional demo; it
does not reduce the generated registry examples to one.

## Acceptance Criteria

* [x] `build/wiremux-vendors.toml` uses
      `sources/vendor/espressif/generic/examples/esp_wiremux_professional_demo`
      for ESP32-S3 vendor dispatch.
* [x] `tools/wiremux-build-helper` reports a missing `example_path` clearly
      before spawning `idf.py`.
* [x] Unit coverage verifies missing example paths are reported clearly.
* [x] Backend quality guidelines reference the professional demo for ESP-IDF
      build validation.
* [x] `tools/wiremux-build package esp-registry` generates packages containing
      beginner, advanced, and professional examples.

## Validation

Completed locally:

```bash
cargo fmt --check --manifest-path tools/wiremux-build-helper/Cargo.toml
cargo test --manifest-path tools/wiremux-build-helper/Cargo.toml
tools/wiremux-build package esp-registry
tools/wiremux-build check vendor
```

Local `check vendor` skipped the ESP-IDF build because this shell does not have
`idf.py`. CI release validation has ESP-IDF installed and should exercise the
real `idf.py set-target esp32s3` and `idf.py build` path.

## Rollout

Release `2605.3.2` points at the old tag content. Re-running that exact release
workflow without moving the tag will still use the stale path. The practical
rollout is to merge this fix and publish a patch version, or intentionally
retag/rebuild `2605.3.2`.
