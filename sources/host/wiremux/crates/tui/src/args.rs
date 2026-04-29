use std::path::PathBuf;

use interactive::{InteractiveBackendMode, SerialProfile, VirtualSerialConfig};

#[derive(Debug, Clone)]
pub struct TuiArgs {
    pub serial: SerialProfile,
    pub config_path: PathBuf,
    pub max_payload_len: usize,
    pub reconnect_delay_ms: u64,
    pub interactive_backend: InteractiveBackendMode,
    pub tui_fps: Option<u16>,
    pub virtual_serial: VirtualSerialConfig,
    pub virtual_serial_supported: bool,
}
