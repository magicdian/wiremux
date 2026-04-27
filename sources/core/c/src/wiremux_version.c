#include "wiremux_version.h"

wiremux_protocol_compatibility_t wiremux_protocol_api_compatibility(uint32_t device_api_version)
{
    if (device_api_version < WIREMUX_PROTOCOL_API_VERSION_MIN_SUPPORTED) {
        return WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_OLD;
    }
    if (device_api_version > WIREMUX_PROTOCOL_API_VERSION_CURRENT) {
        return WIREMUX_PROTOCOL_COMPAT_UNSUPPORTED_NEW;
    }
    return WIREMUX_PROTOCOL_COMPAT_SUPPORTED;
}
