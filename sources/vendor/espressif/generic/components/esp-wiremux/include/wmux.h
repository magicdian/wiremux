#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define WMUX_OK 0
#define WMUX_ERR_INVALID_ARG (-1)
#define WMUX_ERR_INVALID_STATE (-2)
#define WMUX_ERR_NO_MEM (-3)
#define WMUX_ERR_TIMEOUT (-4)
#define WMUX_ERR_NOT_FOUND (-5)
#define WMUX_ERR_NOT_SUPPORTED (-6)
#define WMUX_ERR_INVALID_SIZE (-7)
#define WMUX_ERR_PLATFORM (-100)

#define WMUX_DEFAULT_CHANNEL 1

typedef enum {
    WMUX_CHANNEL_MODE_STREAM = 0,
    WMUX_CHANNEL_MODE_LINE = 1,
    WMUX_CHANNEL_MODE_PASSTHROUGH = 2,
} wmux_channel_mode_t;

typedef struct {
    size_t queue_depth;
    size_t max_payload_len;
    uint32_t default_timeout_ms;
    int auto_manifest;
} wmux_config_t;

typedef struct {
    uint8_t channel_id;
    const char *name;
    const char *description;
    size_t queue_depth;
    uint32_t default_timeout_ms;
    wmux_channel_mode_t mode;
} wmux_channel_config_t;

typedef struct wmux_channel *wmux_channel_handle_t;

typedef void (*wmux_receive_callback_t)(uint8_t channel_id,
                                        const uint8_t *data,
                                        size_t len,
                                        void *user_ctx);

void wmux_config_init(wmux_config_t *config);
int wmux_init(const wmux_config_t *config);
int wmux_start(void);
int wmux_begin(void);
int wmux_emit_manifest(void);

void wmux_channel_config_init(wmux_channel_config_t *config);
int wmux_channel_open(uint8_t channel_id, wmux_channel_handle_t *out);
int wmux_channel_open_with_config(const wmux_channel_config_t *config,
                                  wmux_channel_handle_t *out);
int wmux_channel_close(wmux_channel_handle_t channel);
int wmux_channel_write(wmux_channel_handle_t channel, const void *data, size_t len);
int wmux_channel_write_text(wmux_channel_handle_t channel, const char *text);
int wmux_channel_read(wmux_channel_handle_t channel,
                      void *buffer,
                      size_t len,
                      uint32_t timeout_ms);

int wmux_write(const void *data, size_t len);
int wmux_write_text(const char *text);
int wmux_read(void *buffer, size_t len, uint32_t timeout_ms);
int wmux_write_ch(uint8_t channel_id, const void *data, size_t len);
int wmux_write_text_ch(uint8_t channel_id, const char *text);
int wmux_read_ch(uint8_t channel_id, void *buffer, size_t len, uint32_t timeout_ms);
int wmux_on_receive(uint8_t channel_id,
                    wmux_receive_callback_t callback,
                    void *user_ctx);

const char *wmux_strerror(int err);

#ifdef __cplusplus
}
#endif
