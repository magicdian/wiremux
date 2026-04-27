# PR2 ESP manifest request response

## Goal

Let ESP respond to host manifest requests and align ESP console mode constants
with core-defined interaction modes.

## Requirements

* Use core-defined interaction mode values in ESP console config.
* Include channel interaction mode in emitted manifest descriptors.
* Decode `DeviceManifestRequest` on system channel 0.
* Emit `DeviceManifest` in response.
* Preserve current startup/demo manifest behavior and line-mode console.

## Acceptance Criteria

* [x] System-channel manifest request triggers a manifest response.
* [x] Line-mode console behavior is unchanged.
* [x] Passthrough can remain unsupported but is represented by core mode values.

## Technical Notes

Parent task: `.trellis/tasks/04-27-host-ratatui-tui`.
