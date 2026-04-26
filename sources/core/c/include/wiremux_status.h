#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    WIREMUX_STATUS_OK = 0,
    WIREMUX_STATUS_INVALID_ARG = 1,
    WIREMUX_STATUS_INVALID_SIZE = 2,
    WIREMUX_STATUS_NOT_SUPPORTED = 3,
    WIREMUX_STATUS_INCOMPLETE = 4,
    WIREMUX_STATUS_BAD_MAGIC = 5,
    WIREMUX_STATUS_BAD_VERSION = 6,
    WIREMUX_STATUS_CRC_MISMATCH = 7,
} wiremux_status_t;

#ifdef __cplusplus
}
#endif
