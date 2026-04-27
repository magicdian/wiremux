# wiremux-core

面向 ESP-IDF 用户发布的平台无关 Wiremux 协议核心。

`wiremux-core` 包含以下 C 实现：

- 带 CRC32 校验的 `WMUX` frame 编码和解码。
- protobuf-compatible mux envelope。
- device manifest 编码。
- batch record 编码和解码。
- heatshrink-style 和 LZ4-compatible payload compression helper。

该包由 Wiremux 仓库中的 `sources/core/c` 生成。源码目录仍保持平台无关；ESP Component Registry 发布包只在 release 时生成。

## 添加依赖

大多数应用应该直接依赖 `esp-wiremux`，而不是直接使用 `wiremux-core`：

```yaml
dependencies:
  {{namespace}}/esp-wiremux: "{{version}}"
```

只有在编写另一个 ESP-IDF adapter component 时，才建议直接依赖 `wiremux-core`：

```yaml
dependencies:
  {{namespace}}/wiremux-core: "{{version}}"
```

## API 入口

```c
#include "wiremux_frame.h"
#include "wiremux_envelope.h"
#include "wiremux_manifest.h"
#include "wiremux_batch.h"
#include "wiremux_compression.h"
```

该 component 需要 ESP-IDF v5.4 或更新版本。

## 源码

Canonical source: {{repository_url}}/tree/main/sources/core/c

Release packaging: {{repository_url}}/blob/main/tools/esp-registry/generate-packages.sh
