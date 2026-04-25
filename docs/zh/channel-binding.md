# 通道绑定

ESP32 侧采用“架构动态、实现静态”的方式：

- 固件编译期固定最大通道数量，默认最多 8 个。
- 每个通道通过数字 `channel_id` 路由。
- 通道描述信息由 manifest 输出，不放在每一帧数据中。
- 用户显式把 console、log、telemetry 等功能绑定到通道。

示例：

```c
const esp_serial_mux_channel_config_t telemetry_channel = {
    .channel_id = 3,
    .name = "telemetry",
    .description = "Demo application text output",
    .directions = ESP_SERIAL_MUX_DIRECTION_OUTPUT,
    .default_payload_kind = ESP_SERIAL_MUX_PAYLOAD_KIND_TEXT,
    .flush_policy = ESP_SERIAL_MUX_FLUSH_PERIODIC,
    .backpressure_policy = ESP_SERIAL_MUX_BACKPRESSURE_DROP_OLDEST,
};
ESP_ERROR_CHECK(esp_serial_mux_register_channel(&telemetry_channel));
```

推荐约定：

- `0`: system/control/manifest
- `1`: console
- `2`: log
- `3+`: 应用 telemetry 或自定义业务数据

## Backpressure

首期建议：

- console/control 使用短超时或 immediate flush。
- log 使用 drop oldest，并通过后续统计字段暴露 dropped counter。
- telemetry 根据业务语义选择 drop newest 或 drop oldest。
- ISR 场景首期不承诺完整支持。
