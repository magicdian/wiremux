#include "wiremux_compression.h"

#include <stdbool.h>
#include <string.h>

static wiremux_status_t copy_codec(const uint8_t *input,
                                   size_t input_len,
                                   uint8_t *out,
                                   size_t out_capacity,
                                   size_t *written);
static wiremux_status_t heatshrink_profile_compress(const uint8_t *input,
                                                    size_t input_len,
                                                    uint8_t *out,
                                                    size_t out_capacity,
                                                    size_t *written);
static wiremux_status_t heatshrink_profile_decompress(const uint8_t *input,
                                                      size_t input_len,
                                                      uint8_t *out,
                                                      size_t out_capacity,
                                                      size_t *written);
static wiremux_status_t lz4_block_compress(const uint8_t *input,
                                           size_t input_len,
                                           uint8_t *out,
                                           size_t out_capacity,
                                           size_t *written);
static wiremux_status_t lz4_block_decompress(const uint8_t *input,
                                             size_t input_len,
                                             uint8_t *out,
                                             size_t out_capacity,
                                             size_t *written);
static size_t find_match(const uint8_t *input,
                         size_t input_len,
                         size_t pos,
                         size_t max_distance,
                         size_t max_len,
                         size_t *offset);

wiremux_status_t wiremux_compress(uint32_t algorithm,
                                  const uint8_t *input,
                                  size_t input_len,
                                  uint8_t *out,
                                  size_t out_capacity,
                                  size_t *written)
{
    if ((input_len > 0 && input == NULL) || out == NULL || written == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    switch (algorithm) {
    case WIREMUX_COMPRESSION_NONE:
        return copy_codec(input, input_len, out, out_capacity, written);
    case WIREMUX_COMPRESSION_HEATSHRINK:
        return heatshrink_profile_compress(input, input_len, out, out_capacity, written);
    case WIREMUX_COMPRESSION_LZ4:
        return lz4_block_compress(input, input_len, out, out_capacity, written);
    default:
        return WIREMUX_STATUS_NOT_SUPPORTED;
    }
}

wiremux_status_t wiremux_decompress(uint32_t algorithm,
                                    const uint8_t *input,
                                    size_t input_len,
                                    uint8_t *out,
                                    size_t out_capacity,
                                    size_t *written)
{
    if ((input_len > 0 && input == NULL) || out == NULL || written == NULL) {
        return WIREMUX_STATUS_INVALID_ARG;
    }

    switch (algorithm) {
    case WIREMUX_COMPRESSION_NONE:
        return copy_codec(input, input_len, out, out_capacity, written);
    case WIREMUX_COMPRESSION_HEATSHRINK:
        return heatshrink_profile_decompress(input, input_len, out, out_capacity, written);
    case WIREMUX_COMPRESSION_LZ4:
        return lz4_block_decompress(input, input_len, out, out_capacity, written);
    default:
        return WIREMUX_STATUS_NOT_SUPPORTED;
    }
}

static wiremux_status_t copy_codec(const uint8_t *input,
                                   size_t input_len,
                                   uint8_t *out,
                                   size_t out_capacity,
                                   size_t *written)
{
    if (out_capacity < input_len) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }
    if (input_len > 0) {
        memcpy(out, input, input_len);
    }
    *written = input_len;
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t heatshrink_profile_compress(const uint8_t *input,
                                                    size_t input_len,
                                                    uint8_t *out,
                                                    size_t out_capacity,
                                                    size_t *written)
{
    size_t in_pos = 0;
    size_t out_pos = 0;

    while (in_pos < input_len) {
        const size_t flags_pos = out_pos++;
        if (flags_pos >= out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }

        uint8_t flags = 0;
        for (uint8_t bit = 0; bit < 8 && in_pos < input_len; ++bit) {
            size_t offset = 0;
            const size_t match_len = find_match(input, input_len, in_pos, 255, 18, &offset);
            if (match_len >= 3) {
                flags |= (uint8_t)(1u << bit);
                if (out_pos + 2 > out_capacity) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                out[out_pos++] = (uint8_t)offset;
                out[out_pos++] = (uint8_t)match_len;
                in_pos += match_len;
            } else {
                if (out_pos >= out_capacity) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                out[out_pos++] = input[in_pos++];
            }
        }
        out[flags_pos] = flags;
    }

    *written = out_pos;
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t heatshrink_profile_decompress(const uint8_t *input,
                                                      size_t input_len,
                                                      uint8_t *out,
                                                      size_t out_capacity,
                                                      size_t *written)
{
    size_t in_pos = 0;
    size_t out_pos = 0;

    while (in_pos < input_len) {
        const uint8_t flags = input[in_pos++];
        for (uint8_t bit = 0; bit < 8 && in_pos < input_len; ++bit) {
            if ((flags & (uint8_t)(1u << bit)) == 0) {
                if (out_pos >= out_capacity) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                out[out_pos++] = input[in_pos++];
                continue;
            }

            if (in_pos + 2 > input_len) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            const size_t offset = input[in_pos++];
            const size_t match_len = input[in_pos++];
            if (offset == 0 || offset > out_pos || match_len < 3) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            if (out_pos + match_len > out_capacity) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            for (size_t i = 0; i < match_len; ++i) {
                out[out_pos] = out[out_pos - offset];
                out_pos++;
            }
        }
    }

    *written = out_pos;
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t lz4_block_compress(const uint8_t *input,
                                           size_t input_len,
                                           uint8_t *out,
                                           size_t out_capacity,
                                           size_t *written)
{
    size_t anchor = 0;
    size_t pos = 0;
    size_t out_pos = 0;

    while (pos + 4 <= input_len) {
        size_t offset = 0;
        size_t match_len = find_match(input, input_len, pos, 65535, 130, &offset);
        if (match_len < 4) {
            pos++;
            continue;
        }

        const size_t literal_len = pos - anchor;
        const size_t token_pos = out_pos++;
        if (token_pos >= out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }

        uint8_t token = (uint8_t)((literal_len < 15 ? literal_len : 15) << 4);
        if (literal_len >= 15) {
            size_t remaining = literal_len - 15;
            while (remaining >= 255) {
                if (out_pos >= out_capacity) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                out[out_pos++] = 255;
                remaining -= 255;
            }
            if (out_pos >= out_capacity) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            out[out_pos++] = (uint8_t)remaining;
        }
        if (out_pos + literal_len + 2 > out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        memcpy(&out[out_pos], &input[anchor], literal_len);
        out_pos += literal_len;
        out[out_pos++] = (uint8_t)(offset & 0xffu);
        out[out_pos++] = (uint8_t)((offset >> 8) & 0xffu);

        const size_t encoded_match_len = match_len - 4;
        token |= (uint8_t)(encoded_match_len < 15 ? encoded_match_len : 15);
        out[token_pos] = token;
        if (encoded_match_len >= 15) {
            size_t remaining = encoded_match_len - 15;
            while (remaining >= 255) {
                if (out_pos >= out_capacity) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                out[out_pos++] = 255;
                remaining -= 255;
            }
            if (out_pos >= out_capacity) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            out[out_pos++] = (uint8_t)remaining;
        }

        pos += match_len;
        anchor = pos;
    }

    const size_t literal_len = input_len - anchor;
    const size_t token_pos = out_pos++;
    if (token_pos >= out_capacity) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }
    out[token_pos] = (uint8_t)((literal_len < 15 ? literal_len : 15) << 4);
    if (literal_len >= 15) {
        size_t remaining = literal_len - 15;
        while (remaining >= 255) {
            if (out_pos >= out_capacity) {
                return WIREMUX_STATUS_INVALID_SIZE;
            }
            out[out_pos++] = 255;
            remaining -= 255;
        }
        if (out_pos >= out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        out[out_pos++] = (uint8_t)remaining;
    }
    if (out_pos + literal_len > out_capacity) {
        return WIREMUX_STATUS_INVALID_SIZE;
    }
    if (literal_len > 0) {
        memcpy(&out[out_pos], &input[anchor], literal_len);
        out_pos += literal_len;
    }

    *written = out_pos;
    return WIREMUX_STATUS_OK;
}

static wiremux_status_t lz4_block_decompress(const uint8_t *input,
                                             size_t input_len,
                                             uint8_t *out,
                                             size_t out_capacity,
                                             size_t *written)
{
    size_t in_pos = 0;
    size_t out_pos = 0;

    while (in_pos < input_len) {
        const uint8_t token = input[in_pos++];
        size_t literal_len = (size_t)(token >> 4);
        if (literal_len == 15) {
            uint8_t byte = 0;
            do {
                if (in_pos >= input_len) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                byte = input[in_pos++];
                literal_len += byte;
            } while (byte == 255);
        }
        if (in_pos + literal_len > input_len || out_pos + literal_len > out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        if (literal_len > 0) {
            memcpy(&out[out_pos], &input[in_pos], literal_len);
            in_pos += literal_len;
            out_pos += literal_len;
        }
        if (in_pos == input_len) {
            break;
        }
        if (in_pos + 2 > input_len) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        const size_t offset = (size_t)input[in_pos] | ((size_t)input[in_pos + 1] << 8);
        in_pos += 2;
        if (offset == 0 || offset > out_pos) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }

        size_t match_len = (size_t)(token & 0x0fu) + 4;
        if ((token & 0x0fu) == 15) {
            uint8_t byte = 0;
            do {
                if (in_pos >= input_len) {
                    return WIREMUX_STATUS_INVALID_SIZE;
                }
                byte = input[in_pos++];
                match_len += byte;
            } while (byte == 255);
        }
        if (out_pos + match_len > out_capacity) {
            return WIREMUX_STATUS_INVALID_SIZE;
        }
        for (size_t i = 0; i < match_len; ++i) {
            out[out_pos] = out[out_pos - offset];
            out_pos++;
        }
    }

    *written = out_pos;
    return WIREMUX_STATUS_OK;
}

static size_t find_match(const uint8_t *input,
                         size_t input_len,
                         size_t pos,
                         size_t max_distance,
                         size_t max_len,
                         size_t *offset)
{
    const size_t start = pos > max_distance ? pos - max_distance : 0;
    size_t best_len = 0;
    size_t best_offset = 0;

    for (size_t candidate = start; candidate < pos; ++candidate) {
        size_t len = 0;
        while (len < max_len && pos + len < input_len && input[candidate + len] == input[pos + len]) {
            len++;
        }
        if (len > best_len) {
            best_len = len;
            best_offset = pos - candidate;
        }
    }

    *offset = best_offset;
    return best_len;
}
