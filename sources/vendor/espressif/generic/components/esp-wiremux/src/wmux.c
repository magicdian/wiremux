#include "wmux.h"

#include <limits.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#include "esp_err.h"
#include "esp_wiremux.h"

struct wmux_channel {
    uint8_t channel_id;
    uint32_t default_timeout_ms;
};

typedef struct {
    wmux_receive_callback_t callback;
    void *user_ctx;
} wmux_callback_ctx_t;

static bool s_wmux_initialized;
static bool s_wmux_started;
static wmux_config_t s_wmux_config;
static wmux_callback_ctx_t s_wmux_callbacks[ESP_WIREMUX_MAX_CHANNELS];

static int wmux_from_esp(esp_err_t err);
static esp_err_t wmux_callback_bridge(uint8_t channel_id,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      void *user_ctx);
static int ensure_system_channel(void);
static int ensure_default_channel(void);
static int ensure_simple_channel(const wmux_channel_config_t *config, bool register_queue);
static esp_wiremux_channel_interaction_mode_t map_channel_mode(wmux_channel_mode_t mode);

void wmux_config_init(wmux_config_t *config)
{
    if (config == NULL) {
        return;
    }
    memset(config, 0, sizeof(*config));
    config->queue_depth = 8;
    config->max_payload_len = 512;
    config->default_timeout_ms = 20;
    config->auto_manifest = 0;
}

int wmux_init(const wmux_config_t *config)
{
    if (config == NULL || config->queue_depth == 0 || config->max_payload_len == 0) {
        return WMUX_ERR_INVALID_ARG;
    }
    if (s_wmux_initialized) {
        return WMUX_ERR_INVALID_STATE;
    }

    esp_wiremux_config_t esp_config;
    esp_wiremux_config_init(&esp_config);
    esp_config.queue_depth = config->queue_depth;
    esp_config.max_payload_len = config->max_payload_len;
    esp_config.default_write_timeout_ms = config->default_timeout_ms;

    int err = wmux_from_esp(esp_wiremux_init(&esp_config));
    if (err < 0) {
        return err;
    }
    s_wmux_config = *config;
    s_wmux_initialized = true;
    return WMUX_OK;
}

int wmux_start(void)
{
    if (!s_wmux_initialized) {
        return WMUX_ERR_INVALID_STATE;
    }
    if (s_wmux_started) {
        return WMUX_ERR_INVALID_STATE;
    }

    int err = ensure_system_channel();
    if (err < 0) {
        return err;
    }

    err = wmux_from_esp(esp_wiremux_start());
    if (err < 0) {
        return err;
    }
    s_wmux_started = true;

    if (s_wmux_config.auto_manifest) {
        err = wmux_emit_manifest();
        if (err < 0) {
            return err;
        }
    }
    return WMUX_OK;
}

int wmux_begin(void)
{
    wmux_config_t config;
    wmux_config_init(&config);
    config.auto_manifest = 1;

    int err = wmux_init(&config);
    if (err < 0) {
        return err;
    }
    err = ensure_default_channel();
    if (err < 0) {
        return err;
    }
    err = wmux_start();
    if (err < 0) {
        return err;
    }
    return WMUX_OK;
}

int wmux_emit_manifest(void)
{
    return wmux_from_esp(esp_wiremux_emit_manifest(s_wmux_config.default_timeout_ms));
}

void wmux_channel_config_init(wmux_channel_config_t *config)
{
    if (config == NULL) {
        return;
    }
    memset(config, 0, sizeof(*config));
    config->channel_id = WMUX_DEFAULT_CHANNEL;
    config->name = "wmux";
    config->description = "Wiremux simple channel";
    config->queue_depth = 8;
    config->default_timeout_ms = 20;
    config->mode = WMUX_CHANNEL_MODE_STREAM;
}

int wmux_channel_open(uint8_t channel_id, wmux_channel_handle_t *out)
{
    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = channel_id;
    return wmux_channel_open_with_config(&config, out);
}

int wmux_channel_open_with_config(const wmux_channel_config_t *config,
                                  wmux_channel_handle_t *out)
{
    if (config == NULL || out == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    int err = ensure_simple_channel(config, true);
    if (err < 0) {
        return err;
    }

    struct wmux_channel *channel = calloc(1, sizeof(*channel));
    if (channel == NULL) {
        return WMUX_ERR_NO_MEM;
    }
    channel->channel_id = config->channel_id;
    channel->default_timeout_ms = config->default_timeout_ms;
    *out = channel;
    return WMUX_OK;
}

int wmux_channel_close(wmux_channel_handle_t channel)
{
    if (channel == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    free(channel);
    return WMUX_OK;
}

int wmux_channel_write(wmux_channel_handle_t channel, const void *data, size_t len)
{
    if (channel == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    return wmux_write_ch(channel->channel_id, data, len);
}

int wmux_channel_write_text(wmux_channel_handle_t channel, const char *text)
{
    if (channel == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    return wmux_write_text_ch(channel->channel_id, text);
}

int wmux_channel_read(wmux_channel_handle_t channel,
                      void *buffer,
                      size_t len,
                      uint32_t timeout_ms)
{
    if (channel == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    return wmux_read_ch(channel->channel_id, buffer, len, timeout_ms);
}

int wmux_write(const void *data, size_t len)
{
    return wmux_write_ch(WMUX_DEFAULT_CHANNEL, data, len);
}

int wmux_write_text(const char *text)
{
    return wmux_write_text_ch(WMUX_DEFAULT_CHANNEL, text);
}

int wmux_read(void *buffer, size_t len, uint32_t timeout_ms)
{
    return wmux_read_ch(WMUX_DEFAULT_CHANNEL, buffer, len, timeout_ms);
}

int wmux_write_ch(uint8_t channel_id, const void *data, size_t len)
{
    if (len > 0 && data == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    if (len > INT_MAX) {
        return WMUX_ERR_INVALID_SIZE;
    }
    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = channel_id;
    int err = ensure_simple_channel(&config, false);
    if (err < 0) {
        return err;
    }

    err = wmux_from_esp(esp_wiremux_write(channel_id,
                                          ESP_WIREMUX_DIRECTION_OUTPUT,
                                          ESP_WIREMUX_PAYLOAD_KIND_BINARY,
                                          0,
                                          (const uint8_t *)data,
                                          len,
                                          s_wmux_config.default_timeout_ms));
    return err < 0 ? err : (int)len;
}

int wmux_write_text_ch(uint8_t channel_id, const char *text)
{
    if (text == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = channel_id;
    int err = ensure_simple_channel(&config, false);
    if (err < 0) {
        return err;
    }

    const size_t len = strlen(text);
    if (len > INT_MAX) {
        return WMUX_ERR_INVALID_SIZE;
    }
    err = wmux_from_esp(esp_wiremux_write_text(channel_id,
                                               ESP_WIREMUX_DIRECTION_OUTPUT,
                                               text,
                                               s_wmux_config.default_timeout_ms));
    return err < 0 ? err : (int)len;
}

int wmux_read_ch(uint8_t channel_id, void *buffer, size_t len, uint32_t timeout_ms)
{
    if (buffer == NULL) {
        return WMUX_ERR_INVALID_ARG;
    }
    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = channel_id;
    int err = ensure_simple_channel(&config, true);
    if (err < 0) {
        return err;
    }

    size_t read_len = 0;
    err = wmux_from_esp(esp_wiremux_channel_read(channel_id,
                                                 (uint8_t *)buffer,
                                                 len,
                                                 &read_len,
                                                 timeout_ms));
    if (err < 0) {
        return err;
    }
    return read_len > INT_MAX ? WMUX_ERR_INVALID_SIZE : (int)read_len;
}

int wmux_on_receive(uint8_t channel_id,
                    wmux_receive_callback_t callback,
                    void *user_ctx)
{
    if (callback == NULL || channel_id >= ESP_WIREMUX_MAX_CHANNELS) {
        return WMUX_ERR_INVALID_ARG;
    }

    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = channel_id;
    int err = ensure_simple_channel(&config, false);
    if (err < 0) {
        return err;
    }

    s_wmux_callbacks[channel_id].callback = callback;
    s_wmux_callbacks[channel_id].user_ctx = user_ctx;
    return wmux_from_esp(esp_wiremux_register_input_handler(channel_id,
                                                            wmux_callback_bridge,
                                                            &s_wmux_callbacks[channel_id]));
}

const char *wmux_strerror(int err)
{
    switch (err) {
    case WMUX_OK:
        return "ok";
    case WMUX_ERR_INVALID_ARG:
        return "invalid argument";
    case WMUX_ERR_INVALID_STATE:
        return "invalid state";
    case WMUX_ERR_NO_MEM:
        return "out of memory";
    case WMUX_ERR_TIMEOUT:
        return "timeout";
    case WMUX_ERR_NOT_FOUND:
        return "not found";
    case WMUX_ERR_NOT_SUPPORTED:
        return "not supported";
    case WMUX_ERR_INVALID_SIZE:
        return "invalid size";
    case WMUX_ERR_PLATFORM:
        return "platform error";
    default:
        return "unknown error";
    }
}

static int ensure_system_channel(void)
{
    bool registered = false;
    esp_err_t err = esp_wiremux_is_channel_registered(ESP_WIREMUX_CHANNEL_SYSTEM, &registered);
    if (err != ESP_OK) {
        return wmux_from_esp(err);
    }
    if (registered) {
        return WMUX_OK;
    }

    const esp_wiremux_channel_config_t system_channel = {
        .channel_id = ESP_WIREMUX_CHANNEL_SYSTEM,
        .name = "system",
        .description = "System manifest and control messages",
        .directions = ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT,
        .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_CONTROL,
        .flush_policy = ESP_WIREMUX_FLUSH_IMMEDIATE,
        .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT,
    };
    return wmux_from_esp(esp_wiremux_register_channel(&system_channel));
}

static int ensure_default_channel(void)
{
    wmux_channel_config_t config;
    wmux_channel_config_init(&config);
    config.channel_id = WMUX_DEFAULT_CHANNEL;
    config.name = "default";
    config.description = "Wiremux default simple channel";
    return ensure_simple_channel(&config, true);
}

static int ensure_simple_channel(const wmux_channel_config_t *config, bool register_queue)
{
    if (!s_wmux_initialized || config == NULL ||
        config->channel_id >= ESP_WIREMUX_MAX_CHANNELS ||
        config->channel_id == ESP_WIREMUX_CHANNEL_SYSTEM) {
        return WMUX_ERR_INVALID_ARG;
    }

    bool registered = false;
    int err = wmux_from_esp(esp_wiremux_is_channel_registered(config->channel_id, &registered));
    if (err < 0) {
        return err;
    }

    if (!registered) {
        const esp_wiremux_channel_config_t channel = {
            .channel_id = config->channel_id,
            .name = config->name != NULL ? config->name : "wmux",
            .description = config->description != NULL ? config->description : "Wiremux simple channel",
            .directions = ESP_WIREMUX_DIRECTION_INPUT | ESP_WIREMUX_DIRECTION_OUTPUT,
            .default_payload_kind = ESP_WIREMUX_PAYLOAD_KIND_TEXT,
            .flush_policy = ESP_WIREMUX_FLUSH_IMMEDIATE,
            .backpressure_policy = ESP_WIREMUX_BACKPRESSURE_BLOCK_WITH_TIMEOUT,
            .interaction_mode = map_channel_mode(config->mode),
        };
        err = wmux_from_esp(esp_wiremux_register_channel(&channel));
        if (err < 0) {
            return err;
        }
    }

    if (register_queue) {
        esp_wiremux_input_consumer_t consumer = ESP_WIREMUX_INPUT_CONSUMER_NONE;
        esp_err_t esp_err = esp_wiremux_get_input_consumer(config->channel_id, &consumer);
        if (esp_err != ESP_OK) {
            return wmux_from_esp(esp_err);
        }
        if (consumer == ESP_WIREMUX_INPUT_CONSUMER_CALLBACK) {
            return WMUX_ERR_INVALID_STATE;
        }
        if (consumer != ESP_WIREMUX_INPUT_CONSUMER_QUEUE) {
            const size_t queue_depth = config->queue_depth > 0 ? config->queue_depth : 8;
            err = wmux_from_esp(esp_wiremux_register_rx_queue(config->channel_id, queue_depth));
            if (err < 0) {
                return err;
            }
        }
    }

    return WMUX_OK;
}

static esp_wiremux_channel_interaction_mode_t map_channel_mode(wmux_channel_mode_t mode)
{
    switch (mode) {
    case WMUX_CHANNEL_MODE_LINE:
        return ESP_WIREMUX_CHANNEL_INTERACTION_LINE;
    case WMUX_CHANNEL_MODE_PASSTHROUGH:
        return ESP_WIREMUX_CHANNEL_INTERACTION_PASSTHROUGH;
    case WMUX_CHANNEL_MODE_STREAM:
    default:
        return ESP_WIREMUX_CHANNEL_INTERACTION_UNSPECIFIED;
    }
}

static esp_err_t wmux_callback_bridge(uint8_t channel_id,
                                      const uint8_t *payload,
                                      size_t payload_len,
                                      void *user_ctx)
{
    wmux_callback_ctx_t *ctx = (wmux_callback_ctx_t *)user_ctx;
    if (ctx == NULL || ctx->callback == NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    ctx->callback(channel_id, payload, payload_len, ctx->user_ctx);
    return ESP_OK;
}

static int wmux_from_esp(esp_err_t err)
{
    switch (err) {
    case ESP_OK:
        return WMUX_OK;
    case ESP_ERR_INVALID_ARG:
        return WMUX_ERR_INVALID_ARG;
    case ESP_ERR_INVALID_STATE:
        return WMUX_ERR_INVALID_STATE;
    case ESP_ERR_NO_MEM:
        return WMUX_ERR_NO_MEM;
    case ESP_ERR_TIMEOUT:
        return WMUX_ERR_TIMEOUT;
    case ESP_ERR_NOT_FOUND:
        return WMUX_ERR_NOT_FOUND;
    case ESP_ERR_NOT_SUPPORTED:
        return WMUX_ERR_NOT_SUPPORTED;
    case ESP_ERR_INVALID_SIZE:
        return WMUX_ERR_INVALID_SIZE;
    default:
        return WMUX_ERR_PLATFORM;
    }
}
