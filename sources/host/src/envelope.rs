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

pub const DIRECTION_INPUT: u32 = 1;
pub const DIRECTION_OUTPUT: u32 = 2;
pub const PAYLOAD_KIND_TEXT: u32 = 1;
pub const PAYLOAD_KIND_BINARY: u32 = 2;
pub const PAYLOAD_KIND_PROTOBUF: u32 = 3;
pub const PAYLOAD_KIND_CONTROL: u32 = 4;
pub const PAYLOAD_KIND_EVENT: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnsupportedWireType(u64),
    InvalidUtf8,
}

pub fn encode_envelope(envelope: &MuxEnvelope) -> Vec<u8> {
    let mut out = Vec::with_capacity(encoded_len(envelope));
    write_varint_field(&mut out, 1, u64::from(envelope.channel_id));
    write_varint_field(&mut out, 2, u64::from(envelope.direction));
    write_varint_field(&mut out, 3, u64::from(envelope.sequence));
    write_varint_field(&mut out, 4, envelope.timestamp_us);
    write_varint_field(&mut out, 5, u64::from(envelope.kind));
    if !envelope.payload_type.is_empty() {
        write_bytes_field(&mut out, 6, envelope.payload_type.as_bytes());
    }
    write_bytes_field(&mut out, 7, &envelope.payload);
    write_varint_field(&mut out, 8, u64::from(envelope.flags));
    out
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

fn encoded_len(envelope: &MuxEnvelope) -> usize {
    varint_field_len(1, u64::from(envelope.channel_id))
        + varint_field_len(2, u64::from(envelope.direction))
        + varint_field_len(3, u64::from(envelope.sequence))
        + varint_field_len(4, envelope.timestamp_us)
        + varint_field_len(5, u64::from(envelope.kind))
        + if envelope.payload_type.is_empty() {
            0
        } else {
            bytes_field_len(6, envelope.payload_type.len())
        }
        + bytes_field_len(7, envelope.payload.len())
        + varint_field_len(8, u64::from(envelope.flags))
}

fn varint_len(mut value: u64) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn varint_field_len(field_number: u32, value: u64) -> usize {
    varint_len((u64::from(field_number) << 3) | 0) + varint_len(value)
}

fn bytes_field_len(field_number: u32, len: usize) -> usize {
    varint_len((u64::from(field_number) << 3) | 2) + varint_len(len as u64) + len
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn write_varint_field(out: &mut Vec<u8>, field_number: u32, value: u64) {
    write_varint(out, (u64::from(field_number) << 3) | 0);
    write_varint(out, value);
}

fn write_bytes_field(out: &mut Vec<u8>, field_number: u32, value: &[u8]) {
    write_varint(out, (u64::from(field_number) << 3) | 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value);
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
    use super::{
        decode_envelope, encode_envelope, DecodeError, MuxEnvelope, DIRECTION_INPUT,
        PAYLOAD_KIND_TEXT,
    };

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

    #[test]
    fn encodes_input_mux_envelope() {
        let envelope = MuxEnvelope {
            channel_id: 1,
            direction: DIRECTION_INPUT,
            sequence: 7,
            timestamp_us: 0,
            kind: PAYLOAD_KIND_TEXT,
            payload_type: String::new(),
            payload: b"help".to_vec(),
            flags: 0,
        };

        assert_eq!(decode_envelope(&encode_envelope(&envelope)), Ok(envelope));
    }
}
