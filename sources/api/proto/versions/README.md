# Wiremux Protocol API Snapshots

`current/` is the single latest protocol schema used by new device SDK builds.
Numbered directories are frozen API snapshots that host SDK builds must keep
supporting at compile time.

Version rules:

* Additive protobuf-compatible changes may update `current/`.
* Any release that ships changed `current/` must freeze a numbered snapshot.
* Do not reuse protobuf tag numbers or enum numeric values.
* Deleted protobuf fields or enum values must be reserved in future snapshots.
* `WIREMUX_PROTOCOL_API_VERSION_CURRENT` must match the newest frozen API
  version compiled into the host SDK.
* Host SDKs support their compiled `current` version and all older frozen
  versions included in the source tree.
* A device reporting a newer API than the host compiled current is unsupported;
  host tools should tell the user to upgrade the host SDK/tool.
