# Wiremux Host API Contracts

This tree contains host-side API contracts that are not part of the portable
device/host core protocol in `sources/api/proto`.

Host APIs may describe services, extension points, and tooling contracts used by
Wiremux host applications or overlay providers. They must not be required by a
minimal device SDK unless a specific host API document says otherwise.

Current host API families:

- `generic_enhanced`: vendor-neutral enhanced host services that vendor
  overlays can depend on.
- `vendor_enhanced/espressif`: Wiremux-maintained Espressif vendor enhanced
  services, including the ESP-IDF/esptool bridge. Vendor enhanced APIs may
  declare required generic enhanced capabilities by stable API name and frozen
  version, but they do not import generic enhanced proto files.
