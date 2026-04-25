use crate::crc32::crc32;

pub const MAGIC: [u8; 4] = *b"ESMX";
pub const SUPPORTED_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 14;
pub const DEFAULT_MAX_PAYLOAD_LEN: usize = 1024 * 1024;

const VERSION_OFFSET: usize = 4;
const FLAGS_OFFSET: usize = 5;
const LENGTH_OFFSET: usize = 6;
const CRC_OFFSET: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxFrame {
    pub version: u8,
    pub flags: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Terminal(Vec<u8>),
    Frame(MuxFrame),
    FrameError(FrameError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    CrcMismatch {
        version: u8,
        flags: u8,
        payload_len: usize,
        expected_crc: u32,
        actual_crc: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildFrameError {
    PayloadTooLarge { len: usize, max: usize },
}

#[derive(Debug)]
pub struct FrameScanner {
    buffer: Vec<u8>,
    max_payload_len: usize,
}

impl Default for FrameScanner {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_PAYLOAD_LEN)
    }
}

impl FrameScanner {
    pub fn new(max_payload_len: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_payload_len,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<StreamEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();

        loop {
            let Some(magic_pos) = find_magic(&self.buffer) else {
                let keep_len = magic_prefix_suffix_len(&self.buffer);
                let emit_len = self.buffer.len().saturating_sub(keep_len);
                if emit_len > 0 {
                    events.push(StreamEvent::Terminal(
                        self.buffer.drain(..emit_len).collect(),
                    ));
                }
                break;
            };

            if magic_pos > 0 {
                events.push(StreamEvent::Terminal(
                    self.buffer.drain(..magic_pos).collect(),
                ));
                continue;
            }

            if self.buffer.len() < HEADER_LEN {
                break;
            }

            let version = self.buffer[VERSION_OFFSET];
            if version != SUPPORTED_VERSION {
                events.push(StreamEvent::Terminal(self.buffer.drain(..1).collect()));
                continue;
            }

            let flags = self.buffer[FLAGS_OFFSET];
            let payload_len = u32::from_le_bytes(
                self.buffer[LENGTH_OFFSET..LENGTH_OFFSET + 4]
                    .try_into()
                    .expect("length slice is exactly four bytes"),
            ) as usize;

            if payload_len > self.max_payload_len {
                events.push(StreamEvent::Terminal(self.buffer.drain(..1).collect()));
                continue;
            }

            let total_len = HEADER_LEN + payload_len;
            if self.buffer.len() < total_len {
                break;
            }

            let expected_crc = u32::from_le_bytes(
                self.buffer[CRC_OFFSET..CRC_OFFSET + 4]
                    .try_into()
                    .expect("crc slice is exactly four bytes"),
            );
            let payload = self.buffer[HEADER_LEN..total_len].to_vec();

            let actual_crc = crc32(&payload);
            if actual_crc != expected_crc {
                self.buffer.drain(..total_len);
                events.push(StreamEvent::FrameError(FrameError::CrcMismatch {
                    version,
                    flags,
                    payload_len,
                    expected_crc,
                    actual_crc,
                }));
                continue;
            }

            self.buffer.drain(..total_len);
            events.push(StreamEvent::Frame(MuxFrame {
                version,
                flags,
                payload,
            }));
        }

        merge_terminal_events(events)
    }

    pub fn finish(&mut self) -> Vec<StreamEvent> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        vec![StreamEvent::Terminal(self.buffer.drain(..).collect())]
    }
}

pub fn build_frame_payload(flags: u8, payload: &[u8]) -> Result<Vec<u8>, BuildFrameError> {
    build_frame_payload_with_max(flags, payload, DEFAULT_MAX_PAYLOAD_LEN)
}

pub fn build_frame_payload_with_max(
    flags: u8,
    payload: &[u8],
    max_payload_len: usize,
) -> Result<Vec<u8>, BuildFrameError> {
    if payload.len() > max_payload_len {
        return Err(BuildFrameError::PayloadTooLarge {
            len: payload.len(),
            max: max_payload_len,
        });
    }

    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&MAGIC);
    frame.push(SUPPORTED_VERSION);
    frame.push(flags);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&crc32(payload).to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

fn find_magic(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(MAGIC.len())
        .position(|candidate| candidate == MAGIC)
}

fn magic_prefix_suffix_len(buffer: &[u8]) -> usize {
    let max_len = buffer.len().min(MAGIC.len() - 1);

    for len in (1..=max_len).rev() {
        if buffer[buffer.len() - len..] == MAGIC[..len] {
            return len;
        }
    }

    0
}

fn merge_terminal_events(events: Vec<StreamEvent>) -> Vec<StreamEvent> {
    let mut merged = Vec::new();

    for event in events {
        match (merged.last_mut(), event) {
            (Some(StreamEvent::Terminal(existing)), StreamEvent::Terminal(mut next)) => {
                existing.append(&mut next);
            }
            (_, next) => merged.push(next),
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::{
        build_frame_payload, build_frame_payload_with_max, FrameError, FrameScanner, StreamEvent,
        HEADER_LEN, MAGIC,
    };

    fn collect_events_in_chunks(input: &[u8], chunk_size: usize) -> Vec<StreamEvent> {
        let mut scanner = FrameScanner::default();
        let mut events = Vec::new();

        for chunk in input.chunks(chunk_size) {
            events.extend(scanner.push(chunk));
        }

        events.extend(scanner.finish());
        normalize_events(events)
    }

    fn normalize_events(events: Vec<StreamEvent>) -> Vec<StreamEvent> {
        super::merge_terminal_events(events)
    }

    #[test]
    fn parses_valid_frame() {
        let frame = build_frame_payload(7, b"hello").expect("valid frame");
        let mut scanner = FrameScanner::default();

        assert_eq!(
            scanner.push(&frame),
            vec![StreamEvent::Frame(super::MuxFrame {
                version: 1,
                flags: 7,
                payload: b"hello".to_vec()
            })]
        );
        assert!(scanner.finish().is_empty());
    }

    #[test]
    fn waits_for_partial_frame() {
        let frame = build_frame_payload(0, b"payload").expect("valid frame");
        let mut scanner = FrameScanner::default();

        assert!(scanner.push(&frame[..HEADER_LEN - 1]).is_empty());
        assert_eq!(
            scanner.push(&frame[HEADER_LEN - 1..]),
            vec![StreamEvent::Frame(super::MuxFrame {
                version: 1,
                flags: 0,
                payload: b"payload".to_vec()
            })]
        );
    }

    #[test]
    fn extracts_frame_from_mixed_terminal_stream() {
        let mut stream = b"boot log\n".to_vec();
        stream.extend(build_frame_payload(1, b"mux").expect("valid frame"));
        stream.extend_from_slice(b"\nplain log");

        assert_eq!(
            collect_events_in_chunks(&stream, stream.len()),
            vec![
                StreamEvent::Terminal(b"boot log\n".to_vec()),
                StreamEvent::Frame(super::MuxFrame {
                    version: 1,
                    flags: 1,
                    payload: b"mux".to_vec()
                }),
                StreamEvent::Terminal(b"\nplain log".to_vec())
            ]
        );
    }

    #[test]
    fn preserves_terminal_suffix_that_may_be_magic_prefix() {
        let frame = build_frame_payload(0, b"ok").expect("valid frame");
        let mut stream = b"hello E".to_vec();
        stream.extend_from_slice(&frame[1..]);

        assert_eq!(
            collect_events_in_chunks(&stream, 2),
            vec![
                StreamEvent::Terminal(b"hello ".to_vec()),
                StreamEvent::Frame(super::MuxFrame {
                    version: 1,
                    flags: 0,
                    payload: b"ok".to_vec()
                })
            ]
        );
    }

    #[test]
    fn false_magic_with_bad_crc_reports_error() {
        let mut frame = build_frame_payload(0, b"bad").expect("valid frame");
        frame[HEADER_LEN] ^= 0xff;
        let mut stream = b"prefix ".to_vec();
        stream.extend_from_slice(&frame);
        stream.extend_from_slice(b" suffix");

        let actual_crc = crate::crc32::crc32(&frame[HEADER_LEN..]);
        let expected_crc = u32::from_le_bytes(frame[10..14].try_into().expect("crc bytes"));

        assert_eq!(
            collect_events_in_chunks(&stream, 3),
            vec![
                StreamEvent::Terminal(b"prefix ".to_vec()),
                StreamEvent::FrameError(FrameError::CrcMismatch {
                    version: 1,
                    flags: 0,
                    payload_len: 3,
                    expected_crc,
                    actual_crc
                }),
                StreamEvent::Terminal(b" suffix".to_vec())
            ]
        );
    }

    #[test]
    fn unsupported_version_resynchronizes_to_next_frame() {
        let mut bad = build_frame_payload(0, b"bad").expect("valid frame");
        bad[4] = 99;
        let good = build_frame_payload(2, b"good").expect("valid frame");
        let mut stream = bad.clone();
        stream.extend_from_slice(&good);

        assert_eq!(
            collect_events_in_chunks(&stream, 4),
            vec![
                StreamEvent::Terminal(bad),
                StreamEvent::Frame(super::MuxFrame {
                    version: 1,
                    flags: 2,
                    payload: b"good".to_vec()
                })
            ]
        );
    }

    #[test]
    fn oversized_payload_header_resynchronizes() {
        let mut bad = Vec::new();
        bad.extend_from_slice(&MAGIC);
        bad.push(1);
        bad.push(0);
        bad.extend_from_slice(&999u32.to_le_bytes());
        bad.extend_from_slice(&0u32.to_le_bytes());

        let good = build_frame_payload_with_max(0, b"ok", 8).expect("valid frame");
        let mut stream = bad.clone();
        stream.extend_from_slice(&good);

        let mut scanner = FrameScanner::new(8);
        let mut events = Vec::new();
        events.extend(scanner.push(&stream));
        events.extend(scanner.finish());

        assert_eq!(
            events,
            vec![
                StreamEvent::Terminal(bad),
                StreamEvent::Frame(super::MuxFrame {
                    version: 1,
                    flags: 0,
                    payload: b"ok".to_vec()
                })
            ]
        );
    }

    #[test]
    fn one_byte_replay_matches_whole_stream() {
        let mut stream = b"one\n".to_vec();
        stream.extend(build_frame_payload(3, b"first").expect("valid frame"));
        stream.extend_from_slice(b"two");
        stream.extend(build_frame_payload(4, b"second").expect("valid frame"));

        assert_eq!(
            collect_events_in_chunks(&stream, 1),
            collect_events_in_chunks(&stream, stream.len())
        );
    }

    #[test]
    fn rejects_payload_larger_than_limit() {
        assert_eq!(
            build_frame_payload_with_max(0, b"toolong", 3),
            Err(super::BuildFrameError::PayloadTooLarge { len: 7, max: 3 })
        );
    }
}
