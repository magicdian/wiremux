#pragma once

#include <stddef.h>
#include <stdint.h>

#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    ESP_WIREMUX_CONSOLE_MODE_DISABLED = 0,
    ESP_WIREMUX_CONSOLE_MODE_LINE = 1,
    ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH = 2,
} esp_wiremux_console_mode_t;

typedef struct {
    uint8_t channel_id;
    esp_wiremux_console_mode_t mode;
    const char *name;
    const char *prompt;
    size_t input_queue_size;
    size_t output_queue_size;
    uint32_t write_timeout_ms;
} esp_wiremux_console_config_t;

void esp_wiremux_console_config_init(esp_wiremux_console_config_t *config);

esp_err_t esp_wiremux_bind_console(const esp_wiremux_console_config_t *config);

esp_err_t esp_wiremux_console_run_line(const char *line, int *command_ret);

#ifdef __cplusplus
}
#endif
