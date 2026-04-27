#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define WIREMUX_PROTOCOL_API_VERSION_CURRENT 2u
#define WIREMUX_PROTOCOL_API_VERSION_MIN_SUPPORTED 1u

typedef enum {
    WIREMUX_PROTOCOL_COMPAT_SUPPORTED = 0,
    WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_OLD = 1,
    WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_NEW = 2,
} wiremux_protocol_compatibility_t;

wiremux_protocol_compatibility_t wiremux_protocol_api_compatibility(uint32_t device_api_version);

#ifdef __cplusplus
}
#endif
