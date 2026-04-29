#include "esp_wiremux_console.h"

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "esp_console.h"
#include "esp_wiremux.h"

static esp_wiremux_console_config_t s_console_config;
static bool s_console_bound;
static char s_passthrough_line[256];
static size_t s_passthrough_line_len;
static bool s_passthrough_last_was_cr;

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
    config->passthrough_backend = ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE;
    config->passthrough_policy.input_newline_policy = ESP_WIREMUX_NEWLINE_POLICY_CR;
    config->passthrough_policy.output_newline_policy = ESP_WIREMUX_NEWLINE_POLICY_PRESERVE;
    config->passthrough_policy.echo_policy = ESP_WIREMUX_ECHO_POLICY_REMOTE;
    config->passthrough_policy.control_key_policy = ESP_WIREMUX_CONTROL_KEY_POLICY_FORWARDED;
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
    if (config->mode == ESP_WIREMUX_CONSOLE_MODE_DISABLED) {
        s_console_bound = false;
        s_passthrough_line_len = 0;
        s_passthrough_last_was_cr = false;
        return ESP_OK;
    }
    if (config->mode != ESP_WIREMUX_CONSOLE_MODE_LINE &&
        config->mode != ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH) {
        return ESP_ERR_INVALID_ARG;
    }
    if (config->mode == ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH &&
        config->passthrough_backend == ESP_WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK &&
        config->passthrough_raw_handler == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    const esp_wiremux_channel_config_t channel = {
        .channel_id = config->channel_id,
        .name = config->name != NULL ? config->name : "console",
        .description = config->mode == ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH ?
            "ESP-IDF console passthrough adapter" :
            "ESP-IDF console line-mode adapter",
        .directions = ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
        .flush_policy = ESP_WIREMUX_FLUSH_IMMEDIATE,
        .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT,
        .interaction_mode = (esp_wiremux_channel_interaction_mode_t)config->mode,
        .passthrough_policy = config->passthrough_policy,
    };

    esp_err_t err = esp_wiremux_register_channel(&channel);
    if (err != ESP_OK) {
        return err;
    }

    s_console_config = *config;
    s_passthrough_line_len = 0;
    s_passthrough_last_was_cr = false;
    if (config->mode == ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH &&
        config->passthrough_backend == ESP_WIREMUX_PASSTHROUGH_BACKEND_RAW_CALLBACK) {
        err = esp_wiremux_register_input_handler(config->channel_id,
                                                 config->passthrough_raw_handler,
                                                 config->passthrough_raw_user_ctx);
    } else {
        err = esp_wiremux_register_input_handler(config->channel_id,
                                                 esp_wiremux_console_input_handler,
                                                 NULL);
    }
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
    if (s_console_config.mode != ESP_WIREMUX_CONSOLE_MODE_LINE &&
        s_console_config.mode != ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH) {
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

    if (s_console_config.mode == ESP_WIREMUX_CONSOLE_MODE_PASSTHROUGH) {
        if (s_console_config.passthrough_backend != ESP_WIREMUX_PASSTHROUGH_BACKEND_CONSOLE_LINE_DISCIPLINE &&
            s_console_config.passthrough_backend != ESP_WIREMUX_PASSTHROUGH_BACKEND_ESP_REPL) {
            return ESP_ERR_NOT_SUPPORTED;
        }

        for (size_t i = 0; i < payload_len; ++i) {
            const uint8_t byte = payload[i];
            if (byte == '\r' || byte == '\n') {
                if (byte == '\n' && s_passthrough_last_was_cr) {
                    s_passthrough_last_was_cr = false;
                    continue;
                }
                s_passthrough_last_was_cr = byte == '\r';
                if (s_console_config.passthrough_policy.echo_policy == ESP_WIREMUX_ECHO_POLICY_REMOTE) {
                    (void)esp_wiremux_write_text(s_console_config.channel_id,
                                                 ESP_WIREMUX_DIRECTION_OUTPUT,
                                                 "\r\n",
                                                 s_console_config.write_timeout_ms);
                }
                s_passthrough_line[s_passthrough_line_len] = '\0';
                int command_ret = 0;
                esp_err_t err = esp_wiremux_console_run_line(s_passthrough_line, &command_ret);
                s_passthrough_line_len = 0;
                if (err != ESP_OK) {
                    return err;
                }
                if (command_ret != 0) {
                    char status[48];
                    snprintf(status, sizeof(status), "command returned %d\n", command_ret);
                    (void)esp_wiremux_write_text(s_console_config.channel_id,
                                                 ESP_WIREMUX_DIRECTION_OUTPUT,
                                                 status,
                                                 s_console_config.write_timeout_ms);
                }
            } else if (byte == 0x08 || byte == 0x7f) {
                s_passthrough_last_was_cr = false;
                if (s_passthrough_line_len > 0) {
                    s_passthrough_line_len--;
                    if (s_console_config.passthrough_policy.echo_policy == ESP_WIREMUX_ECHO_POLICY_REMOTE) {
                        (void)esp_wiremux_write_text(s_console_config.channel_id,
                                                     ESP_WIREMUX_DIRECTION_OUTPUT,
                                                     "\b \b",
                                                     s_console_config.write_timeout_ms);
                    }
                }
            } else if (byte >= 0x20 && byte != 0x7f) {
                s_passthrough_last_was_cr = false;
                if (s_passthrough_line_len >= sizeof(s_passthrough_line) - 1) {
                    s_passthrough_line_len = 0;
                    return ESP_ERR_INVALID_SIZE;
                }
                s_passthrough_line[s_passthrough_line_len++] = (char)byte;
                if (s_console_config.passthrough_policy.echo_policy == ESP_WIREMUX_ECHO_POLICY_REMOTE) {
                    (void)esp_wiremux_write(s_console_config.channel_id,
                                            ESP_WIREMUX_DIRECTION_OUTPUT,
                                            ESP_WIREMUX_PAYLOAD_KIND_TEXT,
                                            0,
                                            &byte,
                                            1,
                                            s_console_config.write_timeout_ms);
                }
            }
        }
        return ESP_OK;
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
