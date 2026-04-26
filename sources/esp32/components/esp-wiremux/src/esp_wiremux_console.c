#include "esp_wiremux_console.h"

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "esp_console.h"
#include "esp_wiremux.h"

static esp_wiremux_console_config_t s_console_config;
static bool s_console_bound;

static esp_err_t esp_wiremux_console_input_handler(uint8_t channel_id,
                                                      const uint8_t *payload,
                                                      size_t payload_len,
                                                      void *user_ctx);

void esp_wiremux_console_config_init(esp_wiremux_console_config_t *config)
{
    if (config == NULL) {
        return;
    }

    memset(config, 0, sizeof(*config));
    config->channel_id = 1;
    config->mode = ESP_WIREMUX_CONSOLE_MODE_LINE;
    config->name = "console";
    config->prompt = "esp> ";
    config->input_queue_size = 4;
    config->output_queue_size = 8;
    config->write_timeout_ms = 20;
}

esp_err_t esp_wiremux_bind_console(const esp_wiremux_console_config_t *config)
{
    if (config == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (config->mode == ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH) {
        return ESP_ERR_NOT_SUPPORTED;
    }
    if (config->mode == ESP_WIREMUX_CONSOLE_MODE_DISABLED) {
        s_console_bound = false;
        return ESP_OK;
    }

    const esp_wiremux_channel_config_t channel = {
        .channel_id = config->channel_id,
        .name = config->name != NULL ? config->name : "console",
        .description = "ESP-IDF console line-mode adapter",
        .directions = ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
        .flush_policy = ESP_WIREMUX_FLUSH_IMMEDIATE,
        .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT,
    };

    esp_err_t err = esp_wiremux_register_channel(&channel);
    if (err != ESP_OK) {
        return err;
    }

    s_console_config = *config;
    err = esp_wiremux_register_input_handler(config->channel_id,
                                                esp_wiremux_console_input_handler,
                                                NULL);
    if (err != ESP_OK) {
        s_console_bound = false;
        return err;
    }

    s_console_bound = true;
    return ESP_OK;
}

esp_err_t esp_wiremux_console_run_line(const char *line, int *command_ret)
{
    if (!s_console_bound || line == NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    if (s_console_config.mode != ESP_WIREMUX_CONSOLE_MODE_LINE) {
        return ESP_ERR_NOT_SUPPORTED;
    }

    int ret = 0;
    esp_err_t err = esp_console_run(line, &ret);
    if (command_ret != NULL) {
        *command_ret = ret;
    }

    if (err == ESP_ERR_NOT_FOUND) {
        (void)esp_wiremux_write_text(s_console_config.channel_id,
                                        ESP_WIREMUX_DIRECTION_OUTPUT,
                                        "command not found\n",
                                        s_console_config.write_timeout_ms);
    }

    return err;
}

static esp_err_t esp_wiremux_console_input_handler(uint8_t channel_id,
                                                      const uint8_t *payload,
                                                      size_t payload_len,
                                                      void *user_ctx)
{
    (void)channel_id;
    (void)user_ctx;

    if (payload_len == 0 || payload == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    if (s_console_config.mode != ESP_WIREMUX_CONSOLE_MODE_LINE) {
        return ESP_ERR_NOT_SUPPORTED;
    }

    char line[256];
    if (payload_len >= sizeof(line)) {
        return ESP_ERR_INVALID_SIZE;
    }
    size_t copy_len = payload_len < sizeof(line) - 1 ? payload_len : sizeof(line) - 1;
    memcpy(line, payload, copy_len);
    line[copy_len] = '\0';

    while (copy_len > 0 && (line[copy_len - 1] == '\n' || line[copy_len - 1] == '\r')) {
        line[copy_len - 1] = '\0';
        copy_len--;
    }

    int command_ret = 0;
    esp_err_t err = esp_wiremux_console_run_line(line, &command_ret);
    if (err == ESP_OK && command_ret != 0) {
        char status[48];
        snprintf(status, sizeof(status), "command returned %d\n", command_ret);
        (void)esp_wiremux_write_text(s_console_config.channel_id,
                                        ESP_WIREMUX_DIRECTION_OUTPUT,
                                        status,
                                        s_console_config.write_timeout_ms);
    }
    return err;
}
