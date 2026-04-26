# 通道绑定

ESP32 侧采用“架构动态、实现静态”的方式：

- 固件编译期固定最大通道数量，默认最多 8 个。
- 每个通道通过数字 `channel_id` 路由。
- 通道描述信息由 channel 0 的 `wiremux.v1.DeviceManifest` protobuf manifest 输出，不放在每一帧数据中。
- 用户显式把 console、log、telemetry 等功能绑定到通道。

示例：

```c
const esp_wiremux_channel_config_t telemetry_channel = {
    .channel_id = 3,
    .name = "telemetry",
    .description = "Demo application text output",
    .directions = ESP_WIREMUX_DIRECTION_OUTPUT,
    .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
    .flush_policy = ESP_WIREMUX_FLUSH_PERIODIC,
    .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_DROP_OLDEST,
};
ESP_ERROR_CHECK(esp_wiremux_register_channel(&telemetry_channel));
```

推荐约定：

- `0`: system/control/manifest
- `1`: console
- `2`: log
- `3+`: 应用 telemetry 或自定义业务数据

## 输入处理

允许 host 输入的 channel 需要同时满足：

- channel config 的 `directions` 包含 `ESP_WIREMUX_DIRECTION_INPUT`。
- 调用 `esp_wiremux_register_input_handler()` 注册 handler。
- host 发送的是 `direction = input` 的 `MuxEnvelope`。

console line-mode adapter 会自动为 channel 1 注册 input handler：

```c
esp_wiremux_console_config_t console_config;
esp_wiremux_console_config_init(&console_config);
console_config.channel_id = 1;
console_config.mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
ESP_ERROR_CHECK(esp_wiremux_bind_console(&console_config));
```

非法 direction、未注册 channel、output-only channel、CRC 错误和超长 payload 都不会调用 input handler。

## Backpressure

首期建议：

- console/control 使用短超时或 immediate flush。
- log 使用 drop oldest，并通过后续统计字段暴露 dropped counter。
- telemetry 根据业务语义选择 drop newest 或 drop oldest。
- ISR 场景首期不承诺完整支持。

## Manifest

`esp_wiremux_emit_manifest()` 会在 system channel 0 输出 `MuxEnvelope`：

- `kind = control`
- `payload_type = "wiremux.v1.DeviceManifest"`
- `payload = DeviceManifest` protobuf bytes

`DeviceManifest` 包含 protocol version、最大 channel 数、最大 payload 长度、
native endianness、transport 名称、SDK 名称/版本、feature flags，以及每个已注册
channel 的名称、描述、方向、可选 payload kind/type 列表和默认 payload kind。
大小端信息用于诊断和业务二进制 payload 解释，不影响 `WMUX` frame 或 protobuf
envelope 的 wire encoding。
