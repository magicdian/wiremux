# Registry Example Packaging and 2604.27.2 Patch

## Goal

Publish a patch release that fixes release documentation after the first ESP
Registry upload and packages the ESP-IDF console demo as an example for
`magicdian/esp-wiremux`.

## Requirements

- Bump release declarations from `2604.27.1` to `2604.27.2`.
- Update docs/specs to record that release-triggered GitHub OIDC uses tag refs,
  so Trusted Uploaders for this workflow should leave Branch empty and rely on
  the workflow's explicit main-ancestry check.
- Include the ESP-IDF console demo under the generated `esp-wiremux` registry
  package `examples/` directory so the Registry examples tab is populated.
- Keep `sources/core/c` platform-neutral and avoid changing its local CMake
  project into an ESP-IDF component.
- Keep source-tree local ESP example development working.

## Validation

- Generate registry packages.
- Pack `wiremux-core` and `esp-wiremux` with `compote component pack`.
- Verify `esp-wiremux` archive includes the example project.
- Build the source-tree ESP-IDF demo with ESP-IDF v5.4.2.
- Run host and portable core checks as needed for the version bump.
