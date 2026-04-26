use crate::envelope::{
    bytes_field_len, read_len_delimited, read_varint, varint_field_len, write_bytes_field,
    write_varint, write_varint_field, DecodeError, MuxEnvelope,
};

pub const BATCH_PAYLOAD_TYPE: &str = "wiremux.v1.MuxBatch";
pub const COMPRESSION_NONE: u32 = 0;
pub const COMPRESSION_HEATSHRINK: u32 = 1;
pub const COMPRESSION_LZ4: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxBatch {
    pub compression: u32,
    pub records: Vec<u8>,
    pub uncompressed_len: u32,
}

pub fn encode_batch_records(records: &[MuxEnvelope]) -> Vec<u8> {
    let mut out = Vec::with_capacity(records_encoded_len(records));
    for record in records {
        let record_bytes = encode_record(record);
        write_varint(&mut out, (1u64 << 3) | 2);
        write_varint(&mut out, record_bytes.len() as u64);
        out.extend_from_slice(&record_bytes);
    }
    out
}

pub fn decode_batch_records(bytes: &[u8]) -> Result<Vec<MuxEnvelope>, DecodeError> {
    let mut cursor = 0;
    let mut records = Vec::new();
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        match (field_number, wire_type) {
            (1, 2) => records.push(decode_record(&read_len_delimited(bytes, &mut cursor)?)?),
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }
    Ok(records)
}

pub fn encode_batch(batch: &MuxBatch) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        varint_field_len(1, u64::from(batch.compression))
            + bytes_field_len(2, batch.records.len())
            + varint_field_len(3, u64::from(batch.uncompressed_len)),
    );
    write_varint_field(&mut out, 1, u64::from(batch.compression));
    write_bytes_field(&mut out, 2, &batch.records);
    write_varint_field(&mut out, 3, u64::from(batch.uncompressed_len));
    out
}

pub fn decode_batch(bytes: &[u8]) -> Result<MuxBatch, DecodeError> {
    let mut cursor = 0;
    let mut batch = MuxBatch {
        compression: COMPRESSION_NONE,
        records: Vec::new(),
        uncompressed_len: 0,
    };
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        match (field_number, wire_type) {
            (1, 0) => batch.compression = read_varint(bytes, &mut cursor)? as u32,
            (2, 2) => batch.records = read_len_delimited(bytes, &mut cursor)?,
            (3, 0) => batch.uncompressed_len = read_varint(bytes, &mut cursor)? as u32,
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }
    Ok(batch)
}

fn records_encoded_len(records: &[MuxEnvelope]) -> usize {
    records
        .iter()
        .map(|record| bytes_field_len(1, record_encoded_len(record)))
        .sum()
}

fn record_encoded_len(record: &MuxEnvelope) -> usize {
    varint_field_len(1, u64::from(record.channel_id))
        + varint_field_len(2, u64::from(record.direction))
        + varint_field_len(3, u64::from(record.sequence))
        + varint_field_len(4, record.timestamp_us)
        + varint_field_len(5, u64::from(record.kind))
        + if record.payload_type.is_empty() {
            0
        } else {
            bytes_field_len(6, record.payload_type.len())
        }
        + bytes_field_len(7, record.payload.len())
        + varint_field_len(8, u64::from(record.flags))
}

fn encode_record(record: &MuxEnvelope) -> Vec<u8> {
    let mut out = Vec::with_capacity(record_encoded_len(record));
    write_varint_field(&mut out, 1, u64::from(record.channel_id));
    write_varint_field(&mut out, 2, u64::from(record.direction));
    write_varint_field(&mut out, 3, u64::from(record.sequence));
    write_varint_field(&mut out, 4, record.timestamp_us);
    write_varint_field(&mut out, 5, u64::from(record.kind));
    if !record.payload_type.is_empty() {
        write_bytes_field(&mut out, 6, record.payload_type.as_bytes());
    }
    write_bytes_field(&mut out, 7, &record.payload);
    write_varint_field(&mut out, 8, u64::from(record.flags));
    out
}

fn decode_record(bytes: &[u8]) -> Result<MuxEnvelope, DecodeError> {
    let mut cursor = 0;
    let mut record = MuxEnvelope {
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
            (1, 0) => record.channel_id = read_varint(bytes, &mut cursor)? as u32,
            (2, 0) => record.direction = read_varint(bytes, &mut cursor)? as u32,
            (3, 0) => record.sequence = read_varint(bytes, &mut cursor)? as u32,
            (4, 0) => record.timestamp_us = read_varint(bytes, &mut cursor)?,
            (5, 0) => record.kind = read_varint(bytes, &mut cursor)? as u32,
            (6, 2) => {
                record.payload_type = String::from_utf8(read_len_delimited(bytes, &mut cursor)?)
                    .map_err(|_| DecodeError::InvalidUtf8)?;
            }
            (7, 2) => record.payload = read_len_delimited(bytes, &mut cursor)?,
            (8, 0) => record.flags = read_varint(bytes, &mut cursor)? as u32,
            (_, 0) => {
                let _ = read_varint(bytes, &mut cursor)?;
            }
            (_, 2) => {
                let _ = read_len_delimited(bytes, &mut cursor)?;
            }
            (_, unsupported) => return Err(DecodeError::UnsupportedWireType(unsupported)),
        }
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_batch, decode_batch_records, encode_batch, encode_batch_records, MuxBatch,
        BATCH_PAYLOAD_TYPE, COMPRESSION_HEATSHRINK,
    };
    use crate::envelope::{MuxEnvelope, DIRECTION_OUTPUT, PAYLOAD_KIND_TEXT};

    #[test]
    fn encodes_and_decodes_batch_records() {
        let records = vec![
            MuxEnvelope {
                channel_id: 2,
                direction: DIRECTION_OUTPUT,
                sequence: 1,
                timestamp_us: 10,
                kind: PAYLOAD_KIND_TEXT,
                payload_type: String::new(),
                payload: b"hello".to_vec(),
                flags: 0,
            },
            MuxEnvelope {
                channel_id: 3,
                direction: DIRECTION_OUTPUT,
                sequence: 2,
                timestamp_us: 20,
                kind: PAYLOAD_KIND_TEXT,
                payload_type: BATCH_PAYLOAD_TYPE.to_string(),
                payload: b"world".to_vec(),
                flags: 1,
            },
        ];

        assert_eq!(
            decode_batch_records(&encode_batch_records(&records)),
            Ok(records)
        );
    }

    #[test]
    fn encodes_and_decodes_batch_metadata() {
        let batch = MuxBatch {
            compression: COMPRESSION_HEATSHRINK,
            records: vec![1, 2, 3],
            uncompressed_len: 9,
        };

        assert_eq!(decode_batch(&encode_batch(&batch)), Ok(batch));
    }
}
