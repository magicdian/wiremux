#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxEnvelope {
    pub channel_id: u32,
    pub direction: u32,
    pub sequence: u32,
    pub timestamp_us: u64,
    pub kind: u32,
    pub payload_type: String,
    pub payload: Vec<u8>,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnsupportedWireType(u64),
    InvalidUtf8,
}

pub fn decode_envelope(bytes: &[u8]) -> Result<MuxEnvelope, DecodeError> {
    let mut cursor = 0;
    let mut envelope = MuxEnvelope {
        channel_id: 0,
        direction: 0,
        sequence: 0,
        timestamp_us: 0,
        kind: 0,
        payload_type: String::new(),
        payload: Vec::new(),
        flags: 0,
    };

    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;

        match (field_number, wire_type) {
            (1, 0) => envelope.channel_id = read_varint(bytes, &mut cursor)? as u32,
            (2, 0) => envelope.direction = read_varint(bytes, &mut cursor)? as u32,
            (3, 0) => envelope.sequence = read_varint(bytes, &mut cursor)? as u32,
            (4, 0) => envelope.timestamp_us = read_varint(bytes, &mut cursor)?,
            (5, 0) => envelope.kind = read_varint(bytes, &mut cursor)? as u32,
            (6, 2) => {
                envelope.payload_type = String::from_utf8(read_len_delimited(bytes, &mut cursor)?)
                    .map_err(|_| DecodeError::InvalidUtf8)?;
            }
            (7, 2) => envelope.payload = read_len_delimited(bytes, &mut cursor)?,
            (8, 0) => envelope.flags = read_varint(bytes, &mut cursor)? as u32,
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }

    Ok(envelope)
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Result<u64, DecodeError> {
    let mut result = 0u64;

    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*cursor).ok_or(DecodeError::Truncated)?;
        *cursor += 1;
        result |= u64::from(byte & 0x7f) << shift;

        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }

    Err(DecodeError::Truncated)
}

fn read_len_delimited(bytes: &[u8], cursor: &mut usize) -> Result<Vec<u8>, DecodeError> {
    let len = read_varint(bytes, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or(DecodeError::Truncated)?;
    if end > bytes.len() {
        return Err(DecodeError::Truncated);
    }

    let value = bytes[*cursor..end].to_vec();
    *cursor = end;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{decode_envelope, DecodeError, MuxEnvelope};

    #[test]
    fn decodes_mux_envelope() {
        let bytes = [
            0x08, 0x03, 0x10, 0x02, 0x18, 0x04, 0x20, 0x96, 0x01, 0x28, 0x01, 0x3a, 0x05, b'h',
            b'e', b'l', b'l', b'o', 0x40, 0x00,
        ];

        assert_eq!(
            decode_envelope(&bytes),
            Ok(MuxEnvelope {
                channel_id: 3,
                direction: 2,
                sequence: 4,
                timestamp_us: 150,
                kind: 1,
                payload_type: String::new(),
                payload: b"hello".to_vec(),
                flags: 0
            })
        );
    }

    #[test]
    fn rejects_truncated_payload() {
        assert_eq!(
            decode_envelope(&[0x3a, 0x05, b'h']),
            Err(DecodeError::Truncated)
        );
    }
}
