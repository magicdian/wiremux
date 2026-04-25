# Logging Guidelines

> How logging is done in this project.

---

## Overview

ESP log integration is provided through `esp_log_set_vprintf()`. The adapter forwards formatted log text into a mux channel while optionally teeing to the previous ESP log backend.

## ESP Log Adapter Contract

Public API:

```c
void esp_serial_mux_log_config_init(esp_serial_mux_log_config_t *config);
esp_err_t esp_serial_mux_bind_esp_log(const esp_serial_mux_log_config_t *config);
esp_err_t esp_serial_mux_unbind_esp_log(void);
```

Default config:

- `channel_id = 2`
- `max_line_len = 256`
- `write_timeout_ms = 0`
- `tee_to_previous = true`

## Required Behavior

- The vprintf callback must be re-entrant enough for ESP-IDF logging contexts.
- Use `va_copy()` before formatting or forwarding `va_list`.
- Do not call `ESP_LOGx` inside mux logging code.
- If line formatting exceeds `max_line_len`, truncate and send the bounded line.
- Queue failures must not crash the logging caller.

## Wrong vs Correct

Wrong:

```c
static int mux_log_vprintf(const char *fmt, va_list args)
{
    ESP_LOGI("mux", "forwarding log"); // recursive
    return vprintf(fmt, args);
}
```

Correct:

```c
va_list copy;
va_copy(copy, args);
int len = vsnprintf(buffer, sizeof(buffer), fmt, copy);
va_end(copy);
(void)esp_serial_mux_write(...);
```
