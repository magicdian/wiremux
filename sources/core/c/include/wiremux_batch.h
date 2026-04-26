#pragma once

#include <stddef.h>
#include <stdint.h>

#include "wiremux_envelope.h"
#include "wiremux_status.h"

#ifdef __cplusplus
extern "C" {
#endif

#define WIREMUX_BATCH_PAYLOAD_TYPE "wiremux.v1.MuxBatch"

typedef enum {
    WIREMUX_COMPRESSION_NONE = 0,
    WIREMUX_COMPRESSION_HEATSHRINK = 1,
    WIREMUX_COMPRESSION_LZ4 = 2,
} wiremux_compression_algorithm_t;

typedef wiremux_envelope_t wiremux_record_t;

typedef struct {
    uint32_t compression;
    const uint8_t *records;
    size_t records_len;
    uint32_t uncompressed_len;
} wiremux_batch_t;

size_t wiremux_record_encoded_len(const wiremux_record_t *record);

size_t wiremux_batch_records_encoded_len(const wiremux_record_t *records, size_t record_count);

wiremux_status_t wiremux_batch_records_encode(const wiremux_record_t *records,
                                              size_t record_count,
                                              uint8_t *out,
                                              size_t out_capacity,
                                              size_t *written);

wiremux_status_t wiremux_batch_records_decode(const uint8_t *data,
                                              size_t len,
                                              wiremux_record_t *records,
                                              size_t record_capacity,
                                              size_t *record_count);

size_t wiremux_batch_encoded_len(const wiremux_batch_t *batch);

wiremux_status_t wiremux_batch_encode(const wiremux_batch_t *batch,
                                      uint8_t *out,
                                      size_t out_capacity,
                                      size_t *written);

wiremux_status_t wiremux_batch_decode(const uint8_t *data,
                                      size_t len,
                                      wiremux_batch_t *batch);

#ifdef __cplusplus
}
#endif
