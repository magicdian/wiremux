use crate::batch::{COMPRESSION_HEATSHRINK, COMPRESSION_LZ4, COMPRESSION_NONE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    OutputTooSmall,
    InvalidInput,
    UnsupportedAlgorithm(u32),
}

pub fn compress(algorithm: u32, input: &[u8]) -> Result<Vec<u8>, CodecError> {
    match algorithm {
        COMPRESSION_NONE => Ok(input.to_vec()),
        COMPRESSION_HEATSHRINK => heatshrink_profile_compress(input),
        COMPRESSION_LZ4 => lz4_block_compress(input),
        other => Err(CodecError::UnsupportedAlgorithm(other)),
    }
}

pub fn decompress(
    algorithm: u32,
    input: &[u8],
    uncompressed_len: usize,
) -> Result<Vec<u8>, CodecError> {
    match algorithm {
        COMPRESSION_NONE => {
            if input.len() == uncompressed_len {
                Ok(input.to_vec())
            } else {
                Err(CodecError::InvalidInput)
            }
        }
        COMPRESSION_HEATSHRINK => heatshrink_profile_decompress(input, uncompressed_len),
        COMPRESSION_LZ4 => lz4_block_decompress(input, uncompressed_len),
        other => Err(CodecError::UnsupportedAlgorithm(other)),
    }
}

fn heatshrink_profile_compress(input: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let flags_pos = out.len();
        out.push(0);
        let mut flags = 0u8;
        for bit in 0..8 {
            if pos >= input.len() {
                break;
            }
            let (offset, len) = find_match(input, pos, 255, 18);
            if len >= 3 {
                flags |= 1 << bit;
                out.push(offset as u8);
                out.push(len as u8);
                pos += len;
            } else {
                out.push(input[pos]);
                pos += 1;
            }
        }
        out[flags_pos] = flags;
    }
    Ok(out)
}

fn heatshrink_profile_decompress(
    input: &[u8],
    uncompressed_len: usize,
) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(uncompressed_len);
    let mut pos = 0;
    while pos < input.len() {
        let flags = *input.get(pos).ok_or(CodecError::InvalidInput)?;
        pos += 1;
        for bit in 0..8 {
            if pos >= input.len() {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(input[pos]);
                pos += 1;
                continue;
            }
            let offset = *input.get(pos).ok_or(CodecError::InvalidInput)? as usize;
            let len = *input.get(pos + 1).ok_or(CodecError::InvalidInput)? as usize;
            pos += 2;
            if offset == 0 || offset > out.len() || len < 3 {
                return Err(CodecError::InvalidInput);
            }
            for _ in 0..len {
                let byte = out[out.len() - offset];
                out.push(byte);
            }
        }
    }
    if out.len() == uncompressed_len {
        Ok(out)
    } else {
        Err(CodecError::InvalidInput)
    }
}

fn lz4_block_compress(input: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(input.len());
    let mut anchor = 0;
    let mut pos = 0;
    while pos + 4 <= input.len() {
        let (offset, len) = find_match(input, pos, 65535, 130);
        if len < 4 {
            pos += 1;
            continue;
        }
        emit_lz4_sequence(&mut out, &input[anchor..pos], offset, len)?;
        pos += len;
        anchor = pos;
    }
    emit_lz4_last_literals(&mut out, &input[anchor..])?;
    Ok(out)
}

fn lz4_block_decompress(input: &[u8], uncompressed_len: usize) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(uncompressed_len);
    let mut pos = 0;
    while pos < input.len() {
        let token = input[pos];
        pos += 1;
        let mut literal_len = (token >> 4) as usize;
        if literal_len == 15 {
            loop {
                let byte = *input.get(pos).ok_or(CodecError::InvalidInput)? as usize;
                pos += 1;
                literal_len += byte;
                if byte != 255 {
                    break;
                }
            }
        }
        if pos + literal_len > input.len() {
            return Err(CodecError::InvalidInput);
        }
        out.extend_from_slice(&input[pos..pos + literal_len]);
        pos += literal_len;
        if pos == input.len() {
            break;
        }
        if pos + 2 > input.len() {
            return Err(CodecError::InvalidInput);
        }
        let offset = usize::from(input[pos]) | (usize::from(input[pos + 1]) << 8);
        pos += 2;
        if offset == 0 || offset > out.len() {
            return Err(CodecError::InvalidInput);
        }
        let mut match_len = usize::from(token & 0x0f) + 4;
        if token & 0x0f == 15 {
            loop {
                let byte = *input.get(pos).ok_or(CodecError::InvalidInput)? as usize;
                pos += 1;
                match_len += byte;
                if byte != 255 {
                    break;
                }
            }
        }
        for _ in 0..match_len {
            let byte = out[out.len() - offset];
            out.push(byte);
        }
    }
    if out.len() == uncompressed_len {
        Ok(out)
    } else {
        Err(CodecError::InvalidInput)
    }
}

fn emit_lz4_sequence(
    out: &mut Vec<u8>,
    literals: &[u8],
    offset: usize,
    match_len: usize,
) -> Result<(), CodecError> {
    if offset == 0 || offset > u16::MAX as usize || match_len < 4 {
        return Err(CodecError::InvalidInput);
    }
    let token_pos = out.len();
    let literal_token = literals.len().min(15) as u8;
    let match_token = (match_len - 4).min(15) as u8;
    out.push((literal_token << 4) | match_token);
    emit_len(out, literals.len(), 15);
    out.extend_from_slice(literals);
    out.push((offset & 0xff) as u8);
    out.push((offset >> 8) as u8);
    emit_len(out, match_len - 4, 15);
    if literals.len() < 15 && match_len - 4 < 15 {
        out[token_pos] = ((literals.len() as u8) << 4) | ((match_len - 4) as u8);
    }
    Ok(())
}

fn emit_lz4_last_literals(out: &mut Vec<u8>, literals: &[u8]) -> Result<(), CodecError> {
    out.push((literals.len().min(15) as u8) << 4);
    emit_len(out, literals.len(), 15);
    out.extend_from_slice(literals);
    Ok(())
}

fn emit_len(out: &mut Vec<u8>, len: usize, base: usize) {
    if len < base {
        return;
    }
    let mut remaining = len - base;
    while remaining >= 255 {
        out.push(255);
        remaining -= 255;
    }
    out.push(remaining as u8);
}

fn find_match(input: &[u8], pos: usize, max_distance: usize, max_len: usize) -> (usize, usize) {
    let start = pos.saturating_sub(max_distance);
    let mut best_offset = 0;
    let mut best_len = 0;
    for candidate in start..pos {
        let mut len = 0;
        while len < max_len && pos + len < input.len() && input[candidate + len] == input[pos + len]
        {
            len += 1;
        }
        if len > best_len {
            best_offset = pos - candidate;
            best_len = len;
        }
    }
    (best_offset, best_len)
}

#[cfg(test)]
mod tests {
    use super::{compress, decompress, CodecError};
    use crate::batch::{COMPRESSION_HEATSHRINK, COMPRESSION_LZ4};

    #[test]
    fn heatshrink_round_trips_repeated_payload() {
        let input = b"ESP_LOGI demo demo demo demo demo telemetry telemetry telemetry";
        let compressed = compress(COMPRESSION_HEATSHRINK, input).expect("compress");
        assert_eq!(
            decompress(COMPRESSION_HEATSHRINK, &compressed, input.len()),
            Ok(input.to_vec())
        );
    }

    #[test]
    fn lz4_round_trips_repeated_payload() {
        let input = b"channel=2 level=info value=42 channel=2 level=info value=43";
        let compressed = compress(COMPRESSION_LZ4, input).expect("compress");
        assert_eq!(
            decompress(COMPRESSION_LZ4, &compressed, input.len()),
            Ok(input.to_vec())
        );
    }

    #[test]
    fn rejects_unknown_algorithm() {
        assert_eq!(
            compress(99, b"abc"),
            Err(CodecError::UnsupportedAlgorithm(99))
        );
    }
}
