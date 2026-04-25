#include "esp_serial_mux_console.h"

#include <stdbool.h>
#include <string.h>

#include "esp_console.h"
#include "esp_serial_mux.h"

static esp_serial_mux_console_config_t s_console_config;
static bool s_console_bound;

void esp_serial_mux_console_config_init(esp_serial_mux_console_config_t *config)
{
    if (config == NULL) {
        return;
    }

    memset(config, 0, sizeof(*config));
    config->channel_id = 1;
    config->mode = ESP_SERIAL_MUX_CONSOLE_MODE_LINE;
    config->name = "console";
    config->prompt = "esp> ";
    config->input_queue_size = 4;
    config->output_queue_size = 8;
    config->write_timeout_ms = 20;
}

esp_err_t esp_serial_mux_bind_console(const esp_serial_mux_console_config_t *config)
{
    if (config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (config->mode == ESP_SERIAL_MUX_CONSOLE_MODE_PASSTHROUGH) {
        return ESP_ERR_NOT_SUPPORTED;
    }
    if (config->mode == ESP_SERIAL_MUX_CONSOLE_MODE_DISABLED) {
        s_console_bound = false;
        return ESP_OK;
    }

    const esp_serial_mux_channel_config_t channel = {
        .channel_id = config->channel_id,
        .name = config->name != NULL ? config->name : "console",
        .description = "ESP-IDF console line-mode adapter",
        .directions = ESP_SERIAL_MUX_DIRECTION_INPUT | ESP_SERIAL_MUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_SERIAL_MUX_PAYLOAD_KIND_TEXT,
        .flush_policy = ESP_SERIAL_MUX_FLUSH_IMMEDIATE,
        .backpressure_policy = ESP_SERIAL_MUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT,
    };

    esp_err_t err = esp_serial_mux_register_channel(&channel);
    if (err != ESP_OK) {
        return err;
    }

    s_console_config = *config;
    s_console_bound = true;
    return ESP_OK;
}

esp_err_t esp_serial_mux_console_run_line(const char *line, int *command_ret)
{
    if (!s_console_bound || line == NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    if (s_console_config.mode != ESP_SERIAL_MUX_CONSOLE_MODE_LINE) {
        return ESP_ERR_NOT_SUPPORTED;
    }

    int ret = 0;
    esp_err_t err = esp_console_run(line, &ret);
    if (command_ret != NULL) {
        *command_ret = ret;
    }

    if (err == ESP_ERR_NOT_FOUND) {
        (void)esp_serial_mux_write_text(s_console_config.channel_id,
                                        ESP_SERIAL_MUX_DIRECTION_OUTPUT,
                                        "command not found\n",
                                        s_console_config.write_timeout_ms);
    }

    return err;
}
