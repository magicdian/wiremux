#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_batch.h"
#include "wiremux_status.h"

#ifdef __cplusplus
extern "C" {
#endif

wiremux_status_t wiremux_compress(uint32_t algorithm,
                                  const uint8_t *input,
                                  size_t input_len,
                                  uint8_t *out,
                                  size_t out_capacity,
                                  size_t *written);

wiremux_status_t wiremux_decompress(uint32_t algorithm,
                                    const uint8_t *input,
                                    size_t input_len,
                                    uint8_t *out,
                                    size_t out_capacity,
                                    size_t *written);

#ifdef __cplusplus
}
#endif
