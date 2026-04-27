# wiremux-core

Portable Wiremux protocol primitives packaged for ESP-IDF consumers.

`wiremux-core` contains the platform-neutral C implementation for:

- `WMUX` frame encoding and decoding with CRC32 validation.
- Protobuf-compatible mux envelopes.
- Device manifest encoding.
- Batch record encoding and decoding.
- Heatshrink-style and LZ4-compatible payload compression helpers.

This package is generated from `sources/core/c` in the Wiremux repository. The source tree remains platform-neutral; the ESP Component Registry package is assembled at release time.

## Add to a Project

Most applications should depend on `esp-wiremux` instead of using `wiremux-core` directly:

```yaml
dependencies:
  {{namespace}}/esp-wiremux: "{{version}}"
```

Use `wiremux-core` directly only when you are writing another ESP-IDF adapter component:

```yaml
dependencies:
  {{namespace}}/wiremux-core: "{{version}}"
```

## API Entry Points

```c
#include "wiremux_frame.h"
#include "wiremux_envelope.h"
#include "wiremux_manifest.h"
#include "wiremux_batch.h"
#include "wiremux_compression.h"
```

The component requires ESP-IDF v5.4 or newer.

## Source

Canonical source: {{repository_url}}/tree/main/sources/core/c

Release packaging: {{repository_url}}/blob/main/tools/esp-registry/generate-packages.sh
