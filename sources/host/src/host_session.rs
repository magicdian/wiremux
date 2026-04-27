use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::os::raw::{c_char, c_uint};
use std::ptr;

const WIREMUX_STATUS_OK: u32 = 0;
const WIREMUX_STATUS_INVALID_SIZE: u32 = 2;

pub const DEFAULT_MAX_PAYLOAD_LEN: usize = 1024 * 1024;
pub const DIRECTION_INPUT: u32 = 1;
pub const DIRECTION_OUTPUT: u32 = 2;
pub const PAYLOAD_KIND_TEXT: u32 = 1;
pub const PAYLOAD_KIND_CONTROL: u32 = 4;
pub const MANIFEST_REQUEST_PAYLOAD_TYPE: &str = "wiremux.v1.DeviceManifestRequest";
pub const CHANNEL_NAME_MAX_BYTES: usize = 15;

const EVENT_TERMINAL: u32 = 1;
const EVENT_RECORD: u32 = 2;
const EVENT_CRC_ERROR: u32 = 3;
const EVENT_DECODE_ERROR: u32 = 4;
const EVENT_MANIFEST_BEGIN: u32 = 5;
const EVENT_MANIFEST_CHANNEL_BEGIN: u32 = 6;
const EVENT_MANIFEST_CHANNEL_DIRECTION: u32 = 7;
const EVENT_MANIFEST_CHANNEL_PAYLOAD_KIND: u32 = 8;
const EVENT_MANIFEST_CHANNEL_PAYLOAD_TYPE: u32 = 9;
const EVENT_MANIFEST_CHANNEL_INTERACTION_MODE: u32 = 10;
const EVENT_MANIFEST_CHANNEL_END: u32 = 11;
const EVENT_MANIFEST_END: u32 = 12;
const EVENT_PROTOCOL_COMPATIBILITY: u32 = 13;
const EVENT_BATCH_SUMMARY: u32 = 14;

const COMPAT_SUPPORTED: u32 = 0;
const COMPAT_UNSUPPORTED_OLD: u32 = 1;
const COMPAT_UNSUPPORTED_NEW: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    Terminal(Vec<u8>),
    Record(MuxEnvelope),
    CrcError(HostCrcError),
    DecodeError(HostDecodeError),
    Manifest(DeviceManifest),
    ProtocolCompatibility(ProtocolCompatibility),
    BatchSummary(BatchSummary),
}

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
pub struct ChannelDescriptor {
    pub channel_id: u32,
    pub name: String,
    pub description: String,
    pub directions: Vec<u32>,
    pub payload_kinds: Vec<u32>,
    pub payload_types: Vec<String>,
    pub flags: u32,
    pub default_payload_kind: u32,
    pub interaction_modes: Vec<u32>,
    pub default_interaction_mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceManifest {
    pub device_name: String,
    pub firmware_version: String,
    pub protocol_version: u32,
    pub max_channels: u32,
    pub channels: Vec<ChannelDescriptor>,
    pub native_endianness: u32,
    pub max_payload_len: u32,
    pub transport: String,
    pub feature_flags: u32,
    pub sdk_name: String,
    pub sdk_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildFrameError {
    PayloadTooLarge { len: usize, max: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCrcError {
    pub version: u8,
    pub flags: u8,
    pub payload_len: usize,
    pub expected_crc: u32,
    pub actual_crc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDecodeError {
    pub stage: HostDecodeStage,
    pub status: u32,
    pub detail: u32,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostDecodeStage {
    Envelope,
    Manifest,
    Batch,
    BatchRecords,
    Compression,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolCompatibilityKind {
    Supported,
    UnsupportedOld,
    UnsupportedNew,
    Unknown(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolCompatibility {
    pub device_api_version: u32,
    pub host_min_api_version: u32,
    pub host_current_api_version: u32,
    pub compatibility: ProtocolCompatibilityKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSummary {
    pub compression: u32,
    pub encoded_bytes: usize,
    pub raw_bytes: usize,
    pub record_count: usize,
}

pub fn display_channel_name(name: &str) -> Option<String> {
    let len = utf8_prefix_len(name, CHANNEL_NAME_MAX_BYTES);
    let label = name[..len]
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>();
    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}

fn utf8_prefix_len(value: &str, max_bytes: usize) -> usize {
    if value.len() <= max_bytes {
        return value.len();
    }
    let mut len = 0;
    for (index, ch) in value.char_indices() {
        let next = index + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        len = next;
    }
    len
}

pub struct HostSession {
    raw: CWiremuxHostSession,
    buffer: Vec<u8>,
    scratch: Vec<u8>,
}

impl HostSession {
    pub fn new(max_payload_len: usize) -> Result<Self, u32> {
        let buffer_len = max_payload_len
            .saturating_add(C_WIREMUX_FRAME_HEADER_LEN)
            .max(64);
        let scratch_len = max_payload_len.max(DEFAULT_MAX_PAYLOAD_LEN.min(1024 * 1024));
        let mut session = Self {
            raw: CWiremuxHostSession::default(),
            buffer: vec![0; buffer_len],
            scratch: vec![0; scratch_len],
        };
        let config = CWiremuxHostSessionConfig {
            max_payload_len,
            buffer: session.buffer.as_mut_ptr(),
            buffer_capacity: session.buffer.len(),
            scratch: session.scratch.as_mut_ptr(),
            scratch_capacity: session.scratch.len(),
            on_event: Some(capture_event),
            user_ctx: ptr::null_mut(),
        };
        let status = unsafe { wiremux_host_session_init(&mut session.raw, &config) };
        if status == WIREMUX_STATUS_OK {
            Ok(session)
        } else {
            Err(status)
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<HostEvent>, u32> {
        let mut state = CallbackState::default();
        self.raw.config.user_ctx = (&mut state as *mut CallbackState).cast();
        let status =
            unsafe { wiremux_host_session_feed(&mut self.raw, bytes.as_ptr(), bytes.len()) };
        self.raw.config.user_ctx = ptr::null_mut();
        if status == WIREMUX_STATUS_OK {
            Ok(state.events)
        } else {
            Err(status)
        }
    }

    pub fn finish(&mut self) -> Result<Vec<HostEvent>, u32> {
        let mut state = CallbackState::default();
        self.raw.config.user_ctx = (&mut state as *mut CallbackState).cast();
        let status = unsafe { wiremux_host_session_finish(&mut self.raw) };
        self.raw.config.user_ctx = ptr::null_mut();
        if status == WIREMUX_STATUS_OK {
            Ok(state.events)
        } else {
            Err(status)
        }
    }
}

pub fn build_input_frame(
    channel_id: u8,
    payload: &[u8],
    max_payload_len: usize,
) -> Result<Vec<u8>, BuildFrameError> {
    if payload.len() > max_payload_len {
        return Err(BuildFrameError::PayloadTooLarge {
            len: payload.len(),
            max: max_payload_len,
        });
    }
    let mut capacity = payload.len().saturating_add(128).max(128);
    loop {
        let mut out = vec![0; capacity];
        let mut written = 0usize;
        let status = unsafe {
            wiremux_host_build_input_frame(
                u32::from(channel_id),
                payload.as_ptr(),
                payload.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written,
            )
        };
        if status == WIREMUX_STATUS_OK {
            out.truncate(written);
            return Ok(out);
        }
        if status != WIREMUX_STATUS_INVALID_SIZE || capacity > max_payload_len.saturating_add(1024)
        {
            return Err(BuildFrameError::PayloadTooLarge {
                len: payload.len(),
                max: max_payload_len,
            });
        }
        capacity = capacity.saturating_mul(2);
    }
}

pub fn build_manifest_request_frame(max_payload_len: usize) -> Result<Vec<u8>, BuildFrameError> {
    let mut capacity = 128usize;
    loop {
        let mut out = vec![0; capacity];
        let mut written = 0usize;
        let status = unsafe {
            wiremux_host_build_manifest_request_frame(out.as_mut_ptr(), out.len(), &mut written)
        };
        if status == WIREMUX_STATUS_OK {
            out.truncate(written);
            return Ok(out);
        }
        if status != WIREMUX_STATUS_INVALID_SIZE || capacity > max_payload_len.saturating_add(1024)
        {
            return Err(BuildFrameError::PayloadTooLarge {
                len: 0,
                max: max_payload_len,
            });
        }
        capacity = capacity.saturating_mul(2);
    }
}

#[derive(Default)]
struct CallbackState {
    events: Vec<HostEvent>,
    pending_manifest: Option<DeviceManifest>,
    pending_channel: Option<ChannelDescriptor>,
}

unsafe extern "C" fn capture_event(event: *const CWiremuxHostEvent, user_ctx: *mut c_void) {
    if event.is_null() || user_ctx.is_null() {
        return;
    }
    let state = &mut *(user_ctx as *mut CallbackState);
    let event = &*event;
    match event.event_type {
        EVENT_TERMINAL => push_terminal(state, event.data.terminal),
        EVENT_RECORD => state
            .events
            .push(HostEvent::Record(copy_envelope(event.data.record))),
        EVENT_CRC_ERROR => {
            let crc = event.data.crc_error;
            state.events.push(HostEvent::CrcError(HostCrcError {
                version: crc.version,
                flags: crc.flags,
                payload_len: crc.payload_len,
                expected_crc: crc.expected_crc,
                actual_crc: crc.actual_crc,
            }));
        }
        EVENT_DECODE_ERROR => {
            let err = event.data.decode_error;
            state.events.push(HostEvent::DecodeError(HostDecodeError {
                stage: decode_stage(err.stage),
                status: err.status,
                detail: err.detail,
                payload: copy_bytes(err.payload),
            }));
        }
        EVENT_MANIFEST_BEGIN => {
            let begin = event.data.manifest_begin;
            state.pending_manifest = Some(DeviceManifest {
                device_name: copy_string(begin.device_name),
                firmware_version: copy_string(begin.firmware_version),
                protocol_version: begin.protocol_version,
                max_channels: begin.max_channels,
                channels: Vec::new(),
                native_endianness: begin.native_endianness,
                max_payload_len: begin.max_payload_len,
                transport: copy_string(begin.transport),
                feature_flags: begin.feature_flags,
                sdk_name: copy_string(begin.sdk_name),
                sdk_version: copy_string(begin.sdk_version),
            });
        }
        EVENT_MANIFEST_CHANNEL_BEGIN => {
            let channel = event.data.manifest_channel;
            state.pending_channel = Some(ChannelDescriptor {
                channel_id: channel.channel_id,
                name: copy_string(channel.name),
                description: copy_string(channel.description),
                directions: Vec::new(),
                payload_kinds: Vec::new(),
                payload_types: Vec::new(),
                flags: channel.flags,
                default_payload_kind: channel.default_payload_kind,
                interaction_modes: Vec::new(),
                default_interaction_mode: channel.default_interaction_mode,
            });
        }
        EVENT_MANIFEST_CHANNEL_DIRECTION => {
            let value = event.data.manifest_channel_value;
            if let Some(channel) = state.pending_channel.as_mut() {
                channel.directions.push(value);
            }
        }
        EVENT_MANIFEST_CHANNEL_PAYLOAD_KIND => {
            let value = event.data.manifest_channel_value;
            if let Some(channel) = state.pending_channel.as_mut() {
                channel.payload_kinds.push(value);
            }
        }
        EVENT_MANIFEST_CHANNEL_PAYLOAD_TYPE => {
            let value = event.data.manifest_channel_payload_type;
            if let Some(channel) = state.pending_channel.as_mut() {
                channel.payload_types.push(copy_string(value));
            }
        }
        EVENT_MANIFEST_CHANNEL_INTERACTION_MODE => {
            let value = event.data.manifest_channel_value;
            if let Some(channel) = state.pending_channel.as_mut() {
                channel.interaction_modes.push(value);
            }
        }
        EVENT_MANIFEST_CHANNEL_END => {
            if let (Some(manifest), Some(channel)) = (
                state.pending_manifest.as_mut(),
                state.pending_channel.take(),
            ) {
                manifest.channels.push(channel);
            }
        }
        EVENT_MANIFEST_END => {
            if let Some(manifest) = state.pending_manifest.take() {
                state.events.push(HostEvent::Manifest(manifest));
            }
        }
        EVENT_PROTOCOL_COMPATIBILITY => {
            let compatibility = event.data.protocol_compatibility;
            state
                .events
                .push(HostEvent::ProtocolCompatibility(ProtocolCompatibility {
                    device_api_version: compatibility.device_api_version,
                    host_min_api_version: compatibility.host_min_api_version,
                    host_current_api_version: compatibility.host_current_api_version,
                    compatibility: compatibility_kind(compatibility.compatibility),
                }));
        }
        EVENT_BATCH_SUMMARY => {
            let summary = event.data.batch_summary;
            state.events.push(HostEvent::BatchSummary(BatchSummary {
                compression: summary.compression,
                encoded_bytes: summary.encoded_bytes,
                raw_bytes: summary.raw_bytes,
                record_count: summary.record_count,
            }));
        }
        _ => {}
    }
}

fn push_terminal(state: &mut CallbackState, view: CBytesView) {
    let bytes = copy_bytes(view);
    if bytes.is_empty() {
        return;
    }
    match state.events.last_mut() {
        Some(HostEvent::Terminal(existing)) => existing.extend_from_slice(&bytes),
        _ => state.events.push(HostEvent::Terminal(bytes)),
    }
}

fn copy_envelope(envelope: CWiremuxEnvelope) -> MuxEnvelope {
    MuxEnvelope {
        channel_id: envelope.channel_id,
        direction: envelope.direction,
        sequence: envelope.sequence,
        timestamp_us: envelope.timestamp_us,
        kind: envelope.kind,
        payload_type: copy_c_string(envelope.payload_type, envelope.payload_type_len),
        payload: copy_ptr_bytes(envelope.payload, envelope.payload_len),
        flags: envelope.flags,
    }
}

fn copy_bytes(view: CBytesView) -> Vec<u8> {
    copy_ptr_bytes(view.data, view.len)
}

fn copy_string(view: CStringView) -> String {
    copy_c_string(view.data, view.len)
}

fn copy_ptr_bytes(ptr: *const u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(ptr, len).to_vec() }
}

fn copy_c_string(ptr: *const c_char, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_stage(value: u32) -> HostDecodeStage {
    match value {
        1 => HostDecodeStage::Envelope,
        2 => HostDecodeStage::Manifest,
        3 => HostDecodeStage::Batch,
        4 => HostDecodeStage::BatchRecords,
        5 => HostDecodeStage::Compression,
        other => HostDecodeStage::Unknown(other),
    }
}

fn compatibility_kind(value: u32) -> ProtocolCompatibilityKind {
    match value {
        COMPAT_SUPPORTED => ProtocolCompatibilityKind::Supported,
        COMPAT_UNSUPPORTED_OLD => ProtocolCompatibilityKind::UnsupportedOld,
        COMPAT_UNSUPPORTED_NEW => ProtocolCompatibilityKind::UnsupportedNew,
        other => ProtocolCompatibilityKind::Unknown(other),
    }
}

const C_WIREMUX_FRAME_HEADER_LEN: usize = 14;

#[repr(C)]
#[derive(Clone, Copy)]
struct CBytesView {
    data: *const u8,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CStringView {
    data: *const c_char,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxEnvelope {
    channel_id: u32,
    direction: u32,
    sequence: u32,
    timestamp_us: u64,
    kind: u32,
    payload_type: *const c_char,
    payload_type_len: usize,
    payload: *const u8,
    payload_len: usize,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostDecodeError {
    stage: u32,
    status: u32,
    detail: u32,
    payload: CBytesView,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostCrcError {
    version: u8,
    flags: u8,
    payload_len: usize,
    expected_crc: u32,
    actual_crc: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostManifestBegin {
    device_name: CStringView,
    firmware_version: CStringView,
    protocol_version: u32,
    max_channels: u32,
    native_endianness: u32,
    max_payload_len: u32,
    transport: CStringView,
    feature_flags: u32,
    sdk_name: CStringView,
    sdk_version: CStringView,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostManifestChannel {
    channel_id: u32,
    name: CStringView,
    description: CStringView,
    flags: u32,
    default_payload_kind: u32,
    default_interaction_mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostProtocolCompatibility {
    device_api_version: u32,
    host_min_api_version: u32,
    host_current_api_version: u32,
    compatibility: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostBatchSummary {
    compression: u32,
    encoded_bytes: usize,
    raw_bytes: usize,
    record_count: usize,
}

#[repr(C)]
union CWiremuxHostEventData {
    terminal: CBytesView,
    record: CWiremuxEnvelope,
    crc_error: CWiremuxHostCrcError,
    decode_error: CWiremuxHostDecodeError,
    manifest_begin: CWiremuxHostManifestBegin,
    manifest_channel: CWiremuxHostManifestChannel,
    manifest_channel_value: u32,
    manifest_channel_payload_type: CStringView,
    protocol_compatibility: CWiremuxHostProtocolCompatibility,
    batch_summary: CWiremuxHostBatchSummary,
}

#[repr(C)]
struct CWiremuxHostEvent {
    event_type: u32,
    data: CWiremuxHostEventData,
}

type CEventCallback = unsafe extern "C" fn(*const CWiremuxHostEvent, *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostSessionConfig {
    max_payload_len: usize,
    buffer: *mut u8,
    buffer_capacity: usize,
    scratch: *mut u8,
    scratch_capacity: usize,
    on_event: Option<CEventCallback>,
    user_ctx: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CWiremuxHostSession {
    config: CWiremuxHostSessionConfig,
    buffer_len: usize,
    last_device_api_version: u32,
    last_compatibility: u32,
    manifest_seen: u8,
}

impl Default for CWiremuxHostSession {
    fn default() -> Self {
        unsafe { MaybeUninit::<Self>::zeroed().assume_init() }
    }
}

extern "C" {
    fn wiremux_host_session_init(
        session: *mut CWiremuxHostSession,
        config: *const CWiremuxHostSessionConfig,
    ) -> c_uint;
    fn wiremux_host_session_feed(
        session: *mut CWiremuxHostSession,
        data: *const u8,
        len: usize,
    ) -> c_uint;
    fn wiremux_host_session_finish(session: *mut CWiremuxHostSession) -> c_uint;
    fn wiremux_host_build_input_frame(
        channel_id: u32,
        payload: *const u8,
        payload_len: usize,
        out: *mut u8,
        out_capacity: usize,
        written: *mut usize,
    ) -> c_uint;
    fn wiremux_host_build_manifest_request_frame(
        out: *mut u8,
        out_capacity: usize,
        written: *mut usize,
    ) -> c_uint;
}
