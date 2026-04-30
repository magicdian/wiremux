# Espressif Vendor Enhanced Host API Snapshots

`current/` is the latest Espressif vendor enhanced host API schema used by new
host tools and overlay providers. Numbered directories are frozen API snapshots
that released providers may target.

This API is host-side. It is not the core Wiremux device/host protocol under
`sources/api/proto/versions`, and it must not make vendor enhanced services
mandatory for core-only or generic-enhanced-only Wiremux integrations.

The schema defines the host's Espressif vendor enhanced capability catalog.
Runtime code still owns each implementation. A resolver maps a declared
`api_name` and `frozen_version` to a built-in or installed provider such as the
ESP-IDF/esptool bridge.

Version rules:

- Additive protobuf-compatible changes may update `current/`.
- Any release that ships changed `current/` must freeze a numbered snapshot.
- Do not reuse protobuf tag numbers or enum numeric values.
- Deleted protobuf fields or enum values must be reserved in future snapshots.
- `current_version` describes the newest frozen Espressif vendor enhanced API
  version exposed by the host API catalog.
- Hosts may support their compiled `current` schema and any older frozen
  snapshots included in the source tree.
- API consumers use one `frozen_version` value for each declared vendor enhanced
  API. Compatibility ranges are a host or overlay resolver concern.

Espressif vendor enhanced API names use the
`wiremux.vendor.enhanced.espressif.*` namespace. The first frozen API is
`wiremux.vendor.enhanced.espressif.esptool_bridge` at `frozen_version = 1`.
It declares a requirement on the generic enhanced virtual serial capability by
stable API name and frozen version, without importing the generic enhanced proto.

Future private or closed-source overlay plugins should use the same model:
declare required generic enhanced capabilities by API name and frozen version,
then let the host registry validate compatibility and resolve matching built-in
or installed implementations.
