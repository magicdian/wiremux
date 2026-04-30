# Generic Enhanced Host API Snapshots

`current/` is the latest generic enhanced host API schema used by new host
tools and overlay providers. Numbered directories are frozen API snapshots that
released overlays may target.

This API is host-side. It is not the core Wiremux device/host protocol under
`sources/api/proto/versions`, and it must not make generic enhanced services
mandatory for core-only Wiremux integrations.

The schema defines the host's generic enhanced capability catalog. Host runtime
code still owns each implementation. A resolver maps a declared `api_name` and
`frozen_version` to a built-in or installed provider such as the virtual serial
broker.

Version rules:

- Additive protobuf-compatible changes may update `current/`.
- Any release that ships changed `current/` must freeze a numbered snapshot.
- Do not reuse protobuf tag numbers or enum numeric values.
- Deleted protobuf fields or enum values must be reserved in future snapshots.
- `current_version` describes the newest frozen generic enhanced API version
  exposed by the host API catalog.
- Hosts may support their compiled `current` schema and any older frozen
  snapshots included in the source tree.
- API consumers use one `frozen_version` value for each declared generic
  enhanced API. Compatibility ranges are a host or overlay resolver concern,
  not a device-declared range.

Generic enhanced API names use the `wiremux.generic.enhanced.*` namespace. The
first frozen API is `wiremux.generic.enhanced.virtual_serial` at
`frozen_version = 1`; it derives endpoint behavior from core manifest channel
descriptors and has no dedicated typed config message in version 1.

Future overlay package identity, package trust metadata, and TUI contribution
contracts should be added through new fields or API families after the
overlay-runtime package model is designed. The generic enhanced v1 schema leaves
field-number space for those additive declarations but does not define them yet.
