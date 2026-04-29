use std::path::PathBuf;

use interactive::InteractiveBackendMode;

#[derive(Debug, Clone)]
pub struct TuiArgs {
    pub port: PathBuf,
    pub baud: u32,
    pub max_payload_len: usize,
    pub reconnect_delay_ms: u64,
    pub interactive_backend: InteractiveBackendMode,
    pub tui_fps: Option<u16>,
}
