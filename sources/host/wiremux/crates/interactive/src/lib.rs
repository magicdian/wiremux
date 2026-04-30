use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use generic_enhanced::{BuiltInProviderError, GenericEnhancedRegistry};
use host_session::{
    ChannelDescriptor, DeviceManifest, MuxEnvelope, PassthroughPolicy,
    CHANNEL_INTERACTION_PASSTHROUGH, DIRECTION_INPUT, NEWLINE_POLICY_CR, NEWLINE_POLICY_CRLF,
    NEWLINE_POLICY_LF, PAYLOAD_KIND_TEXT,
};
use serde::{Deserialize, Serialize};
use serialport::{DataBits, FlowControl, Parity, SerialPortBuilder, StopBits};

pub const PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS: u64 = 750;
pub const INTERACTIVE_SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(5);
pub const DEFAULT_BAUD: u32 = 115_200;
pub const DEFAULT_DATA_BITS: u8 = 8;
pub const DEFAULT_STOP_BITS: u8 = 1;
const VIRTUAL_SERIAL_OUTPUT_QUEUE_LIMIT: usize = 1024 * 1024;
const ESP32_BOOTLOADER_RESET_HOLD: Duration = Duration::from_millis(100);
const ESP32_BOOTLOADER_RELEASE_WAIT: Duration = Duration::from_millis(50);
const ESP32_USB_JTAG_RESET_STEP: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HostConfig {
    #[serde(default)]
    pub serial: SerialConfig,
    #[serde(default)]
    pub virtual_serial: VirtualSerialConfig,
    #[serde(skip)]
    pub virtual_serial_configured: bool,
}

impl HostConfig {
    pub fn load_default() -> io::Result<Self> {
        Self::load(default_config_path())
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err),
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        fs::write(path, text)
    }

    pub fn from_serial_profile(profile: &SerialProfile) -> Self {
        Self {
            serial: SerialConfig::from_profile(profile),
            virtual_serial: VirtualSerialConfig::default(),
            virtual_serial_configured: false,
        }
    }

    pub fn from_serial_profile_and_virtual(
        profile: &SerialProfile,
        virtual_serial: VirtualSerialConfig,
    ) -> Self {
        Self {
            serial: SerialConfig::from_profile(profile),
            virtual_serial,
            virtual_serial_configured: true,
        }
    }
}

impl<'de> Deserialize<'de> for HostConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct HostConfigWire {
            #[serde(default)]
            serial: SerialConfig,
            virtual_serial: Option<VirtualSerialConfig>,
        }

        let wire = HostConfigWire::deserialize(deserializer)?;
        Ok(Self {
            serial: wire.serial,
            virtual_serial_configured: wire.virtual_serial.is_some(),
            virtual_serial: wire.virtual_serial.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualSerialConfig {
    #[serde(default = "default_virtual_serial_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub export: VirtualSerialExport,
    #[serde(default = "default_virtual_serial_name_template")]
    pub name_template: String,
}

impl Default for VirtualSerialConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export: VirtualSerialExport::AllManifestChannels,
            name_template: default_virtual_serial_name_template(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VirtualSerialExport {
    #[default]
    AllManifestChannels,
}

fn default_virtual_serial_enabled() -> bool {
    true
}

fn default_virtual_serial_name_template() -> String {
    "wiremux-{device}-{channel}".to_string()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialConfig {
    #[serde(default)]
    pub port: Option<PathBuf>,
    #[serde(default = "default_baud")]
    pub baud: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default)]
    pub parity: SerialParity,
    #[serde(default)]
    pub flow_control: SerialFlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: None,
            baud: DEFAULT_BAUD,
            data_bits: DEFAULT_DATA_BITS,
            stop_bits: DEFAULT_STOP_BITS,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
        }
    }
}

impl SerialConfig {
    pub fn from_profile(profile: &SerialProfile) -> Self {
        Self {
            port: Some(profile.port.clone()),
            baud: profile.baud,
            data_bits: profile.data_bits,
            stop_bits: profile.stop_bits,
            parity: profile.parity,
            flow_control: profile.flow_control,
        }
    }

    pub fn resolve_profile(
        &self,
        overrides: SerialProfileOverrides,
    ) -> Result<SerialProfile, String> {
        let port = overrides
            .port
            .or_else(|| self.port.clone())
            .ok_or_else(|| {
                "serial port is required; pass --port <path> or configure [serial].port".to_string()
            })?;
        let profile = SerialProfile {
            port,
            baud: overrides.baud.unwrap_or(self.baud),
            data_bits: overrides.data_bits.unwrap_or(self.data_bits),
            stop_bits: overrides.stop_bits.unwrap_or(self.stop_bits),
            parity: overrides.parity.unwrap_or(self.parity),
            flow_control: overrides.flow_control.unwrap_or(self.flow_control),
        };
        profile.validate()?;
        Ok(profile)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SerialProfileOverrides {
    pub port: Option<PathBuf>,
    pub baud: Option<u32>,
    pub data_bits: Option<u8>,
    pub stop_bits: Option<u8>,
    pub parity: Option<SerialParity>,
    pub flow_control: Option<SerialFlowControl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialProfile {
    pub port: PathBuf,
    pub baud: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: SerialParity,
    pub flow_control: SerialFlowControl,
}

impl SerialProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.baud == 0 {
            return Err("baud must be greater than 0".to_string());
        }
        data_bits_to_serialport(self.data_bits)?;
        stop_bits_to_serialport(self.stop_bits)?;
        Ok(())
    }

    pub fn summary(&self) -> String {
        format!(
            "{} {} {}{}{} flow={}",
            self.port.display(),
            self.baud,
            self.data_bits,
            self.parity.short_label(),
            self.stop_bits,
            self.flow_control
        )
    }

    pub fn apply_to_builder(&self, builder: SerialPortBuilder) -> io::Result<SerialPortBuilder> {
        let data_bits = data_bits_to_serialport(self.data_bits)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let stop_bits = stop_bits_to_serialport(self.stop_bits)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        Ok(builder
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .parity(self.parity.into())
            .flow_control(self.flow_control.into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

impl Default for SerialParity {
    fn default() -> Self {
        Self::None
    }
}

impl SerialParity {
    pub const VALUES: [Self; 3] = [Self::None, Self::Odd, Self::Even];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "odd" => Some(Self::Odd),
            "even" => Some(Self::Even),
            _ => None,
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            Self::None => "N",
            Self::Odd => "O",
            Self::Even => "E",
        }
    }
}

impl fmt::Display for SerialParity {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        frame.write_str(match self {
            Self::None => "none",
            Self::Odd => "odd",
            Self::Even => "even",
        })
    }
}

impl From<SerialParity> for Parity {
    fn from(value: SerialParity) -> Self {
        match value {
            SerialParity::None => Self::None,
            SerialParity::Odd => Self::Odd,
            SerialParity::Even => Self::Even,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

impl Default for SerialFlowControl {
    fn default() -> Self {
        Self::None
    }
}

impl SerialFlowControl {
    pub const VALUES: [Self; 3] = [Self::None, Self::Software, Self::Hardware];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "software" => Some(Self::Software),
            "hardware" => Some(Self::Hardware),
            _ => None,
        }
    }
}

impl fmt::Display for SerialFlowControl {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        frame.write_str(match self {
            Self::None => "none",
            Self::Software => "software",
            Self::Hardware => "hardware",
        })
    }
}

impl From<SerialFlowControl> for FlowControl {
    fn from(value: SerialFlowControl) -> Self {
        match value {
            SerialFlowControl::None => Self::None,
            SerialFlowControl::Software => Self::Software,
            SerialFlowControl::Hardware => Self::Hardware,
        }
    }
}

pub fn default_config_path() -> PathBuf {
    if let Some(path) = env::var_os("WIREMUX_CONFIG") {
        return PathBuf::from(path);
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("wiremux/config.toml");
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            return home.join("Library/Application Support/wiremux/config.toml");
        }
        #[cfg(not(target_os = "macos"))]
        {
            return home.join(".config/wiremux/config.toml");
        }
    }
    PathBuf::from("wiremux-config.toml")
}

fn default_baud() -> u32 {
    DEFAULT_BAUD
}

fn default_data_bits() -> u8 {
    DEFAULT_DATA_BITS
}

fn default_stop_bits() -> u8 {
    DEFAULT_STOP_BITS
}

fn data_bits_to_serialport(value: u8) -> Result<DataBits, String> {
    match value {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err(format!(
            "invalid data bits: {value}; expected 5, 6, 7, or 8"
        )),
    }
}

fn stop_bits_to_serialport(value: u8) -> Result<StopBits, String> {
    match value {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err(format!("invalid stop bits: {value}; expected 1 or 2")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractiveBackendMode {
    Auto,
    Compat,
    Mio,
}

impl InteractiveBackendMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "compat" => Some(Self::Compat),
            "mio" => Some(Self::Mio),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum InteractiveEvent {
    SerialBytes(Vec<u8>),
    SerialEof,
    SerialError(io::Error),
    Terminal(Event),
    Timeout,
}

enum SerialReadEvent {
    Bytes(Vec<u8>),
    Eof,
    Error(io::Error),
}

pub struct ConnectedInteractiveBackend {
    label: String,
    inner: ConnectedInteractiveBackendInner,
}

enum ConnectedInteractiveBackendInner {
    Compat(CompatBackend),
    #[cfg(unix)]
    Mio(UnixMioBackend),
}

impl ConnectedInteractiveBackend {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn next_event(&mut self, deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.next_event(deadline),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.next_event(deadline),
        }
    }

    pub fn write_data_terminal_ready(&mut self, level: bool) -> io::Result<()> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => {
                backend.write_data_terminal_ready(level)
            }
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => {
                backend.write_data_terminal_ready(level)
            }
        }
    }

    pub fn write_request_to_send(&mut self, level: bool) -> io::Result<()> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => {
                backend.write_request_to_send(level)
            }
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.write_request_to_send(level),
        }
    }

    pub fn set_baud_rate(&mut self, baud: u32) -> io::Result<()> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.set_baud_rate(baud),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.set_baud_rate(baud),
        }
    }

    pub fn enter_esp32_bootloader_with_dtr_rts(&mut self) -> io::Result<()> {
        self.enter_esp32_bootloader_with_reset_mode(Esp32BootloaderResetMode::UsbToSerialBridge)
    }

    pub fn enter_esp32_bootloader_with_reset_mode(
        &mut self,
        mode: Esp32BootloaderResetMode,
    ) -> io::Result<()> {
        match mode {
            Esp32BootloaderResetMode::UsbToSerialBridge => {
                self.enter_esp32_bootloader_with_classic_reset()
            }
            Esp32BootloaderResetMode::UsbJtagSerial => {
                self.enter_esp32_bootloader_with_usb_jtag_reset()
            }
        }
    }

    pub fn hard_reset_esp32_with_reset_mode(
        &mut self,
        mode: Esp32BootloaderResetMode,
    ) -> io::Result<()> {
        self.write_request_to_send(true)?;
        match mode {
            Esp32BootloaderResetMode::UsbJtagSerial => {
                thread::sleep(Duration::from_millis(200));
                self.write_request_to_send(false)?;
                thread::sleep(Duration::from_millis(200));
            }
            Esp32BootloaderResetMode::UsbToSerialBridge => {
                thread::sleep(ESP32_BOOTLOADER_RESET_HOLD);
                self.write_request_to_send(false)?;
            }
        }
        Ok(())
    }

    fn enter_esp32_bootloader_with_classic_reset(&mut self) -> io::Result<()> {
        self.write_data_terminal_ready(false)?; // IO0 high
        self.write_request_to_send(true)?; // EN low, chip in reset
        thread::sleep(ESP32_BOOTLOADER_RESET_HOLD);
        self.write_data_terminal_ready(true)?; // IO0 low
        self.write_request_to_send(false)?; // EN high, chip out of reset
        thread::sleep(ESP32_BOOTLOADER_RELEASE_WAIT);
        self.write_data_terminal_ready(false)?; // IO0 high
        Ok(())
    }

    fn enter_esp32_bootloader_with_usb_jtag_reset(&mut self) -> io::Result<()> {
        self.write_request_to_send(false)?;
        self.write_data_terminal_ready(false)?;
        thread::sleep(ESP32_USB_JTAG_RESET_STEP);
        self.write_data_terminal_ready(true)?;
        self.write_request_to_send(false)?;
        thread::sleep(ESP32_USB_JTAG_RESET_STEP);
        self.write_request_to_send(true)?;
        self.write_data_terminal_ready(false)?;
        self.write_request_to_send(true)?;
        thread::sleep(ESP32_USB_JTAG_RESET_STEP);
        self.write_data_terminal_ready(false)?;
        self.write_request_to_send(false)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Esp32BootloaderResetMode {
    UsbToSerialBridge,
    UsbJtagSerial,
}

impl Write for ConnectedInteractiveBackend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.write(buf),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            ConnectedInteractiveBackendInner::Compat(backend) => backend.flush(),
            #[cfg(unix)]
            ConnectedInteractiveBackendInner::Mio(backend) => backend.flush(),
        }
    }
}

pub fn open_interactive_backend(
    profile: &SerialProfile,
    mode: InteractiveBackendMode,
    read_timeout: Duration,
) -> io::Result<(PathBuf, ConnectedInteractiveBackend)> {
    let mut last_err = None;

    for candidate in port_candidates(&profile.port) {
        match open_candidate(&candidate, profile, mode, read_timeout) {
            Ok(backend) => return Ok((candidate, backend)),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no candidate ports found for {}", profile.port.display()),
        )
    }))
}

pub fn port_candidates(requested: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if requested_file_name_starts_with(requested, "tty.") {
        if let Some(pair) = paired_tty_cu_path(requested) {
            push_unique(&mut candidates, pair);
        }
        push_unique(&mut candidates, requested.to_path_buf());
    } else {
        push_unique(&mut candidates, requested.to_path_buf());
        if let Some(pair) = paired_tty_cu_path(requested) {
            push_unique(&mut candidates, pair);
        }
    }

    if let Some(parent) = requested.parent() {
        if let Some(fragment) = usbmodem_fragment(requested) {
            for prefer_cu in [true, false] {
                if let Ok(entries) = fs::read_dir(parent) {
                    let mut matches = entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.file_name().is_some_and(|name| {
                                let name = name.to_string_lossy();
                                name.contains(fragment)
                                    && if prefer_cu {
                                        name.starts_with("cu.")
                                    } else {
                                        name.starts_with("tty.")
                                    }
                            })
                        })
                        .collect::<Vec<_>>();
                    matches.sort();
                    for path in matches {
                        push_unique(&mut candidates, path);
                    }
                }
            }
        }
    }

    candidates
}

pub fn paired_tty_cu_path(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_string_lossy();
    let paired_name = if let Some(rest) = file_name.strip_prefix("tty.") {
        format!("cu.{rest}")
    } else if let Some(rest) = file_name.strip_prefix("cu.") {
        format!("tty.{rest}")
    } else {
        return None;
    };

    Some(path.with_file_name(paired_name))
}

pub fn usbmodem_fragment(path: &Path) -> Option<&'static str> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name.contains("usbmodem") {
        Some("usbmodem")
    } else if file_name.contains("usbserial") {
        Some("usbserial")
    } else {
        None
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

pub fn requested_file_name_starts_with(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
}

pub fn passthrough_key_payload(key: KeyEvent, policy: PassthroughPolicy) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ascii_control_byte(ch).map(|byte| vec![byte])
        }
        KeyCode::Char(ch) => {
            let mut out = [0; 4];
            Some(ch.encode_utf8(&mut out).as_bytes().to_vec())
        }
        KeyCode::Enter => Some(newline_bytes(policy.input_newline_policy).to_vec()),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        _ => None,
    }
}

pub fn is_passthrough_exit_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('\u{1d}'))
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char(']') | KeyCode::Char('}')))
}

pub fn is_passthrough_meta_exit_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && is_passthrough_escape_exit_suffix(key)
}

pub fn is_passthrough_escape_exit_suffix(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
}

pub fn generic_enhanced_registry() -> Result<GenericEnhancedRegistry, BuiltInProviderError> {
    #[cfg(feature = "generic-enhanced")]
    {
        let mut registry = GenericEnhancedRegistry::new();
        generic_enhanced::register_virtual_serial_provider(&mut registry)?;
        Ok(registry)
    }

    #[cfg(not(feature = "generic-enhanced"))]
    {
        Ok(GenericEnhancedRegistry::new())
    }
}

pub fn host_supports_virtual_serial_provider() -> bool {
    #[cfg(feature = "generic-enhanced")]
    {
        let registry =
            generic_enhanced_registry().expect("built-in generic enhanced registry is valid");
        let capability_id = generic_enhanced::latest_virtual_serial_capability_id()
            .expect("built-in virtual serial capability is declared");
        registry.supports(&capability_id)
    }

    #[cfg(not(feature = "generic-enhanced"))]
    {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualSerialInputOwner {
    Host,
    VirtualSerial,
}

impl fmt::Display for VirtualSerialInputOwner {
    fn fmt(&self, frame: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host => frame.write_str("host"),
            Self::VirtualSerial => frame.write_str("virtual-serial"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualSerialEndpointInfo {
    pub channel_id: u32,
    pub name: String,
    pub path: Option<PathBuf>,
    pub input_capable: bool,
    pub input_owner: VirtualSerialInputOwner,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualSerialInputEvent {
    Forward {
        channel_id: u8,
        payload: Vec<u8>,
    },
    Discarded {
        channel_id: u32,
        reason: String,
        bytes: usize,
    },
}

pub struct VirtualSerialBroker {
    config: VirtualSerialConfig,
    enabled: bool,
    source_port: Option<PathBuf>,
    endpoints: Vec<VirtualSerialEndpoint>,
}

struct VirtualSerialEndpoint {
    channel_id: u32,
    name: String,
    input_capable: bool,
    input_owner: VirtualSerialInputOwner,
    output_record_delimited: bool,
    output_previous_was_cr: bool,
    output_pending: Vec<u8>,
    output_dropped_bytes: usize,
    backend: VirtualSerialEndpointBackend,
}

enum VirtualSerialEndpointBackend {
    Active(VirtualSerialEndpointHandle),
    Unsupported(String),
    Failed(String),
}

trait VirtualSerialEndpointIo {
    fn path(&self) -> &Path;
    fn real_path(&self) -> &Path {
        self.path()
    }
    fn write_output(&mut self, bytes: &[u8]) -> io::Result<usize>;
    fn read_input(&mut self, buf: &mut [u8]) -> io::Result<VirtualSerialRead>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualSerialRead {
    Bytes(usize),
    NoData,
    ClientGone,
}

pub struct VirtualSerialEndpointHandle {
    inner: Box<dyn VirtualSerialEndpointIo>,
}

impl VirtualSerialEndpointHandle {
    pub fn open(name: &str) -> io::Result<Self> {
        open_virtual_serial_endpoint(name).map(|inner| Self { inner })
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    pub fn real_path(&self) -> &Path {
        self.inner.real_path()
    }

    pub fn write_output(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write_output(bytes)
    }

    pub fn read_input(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        match self.read_input_event(buf)? {
            VirtualSerialRead::Bytes(read_len) => Ok(Some(read_len)),
            VirtualSerialRead::NoData | VirtualSerialRead::ClientGone => Ok(None),
        }
    }

    pub fn read_input_event(&mut self, buf: &mut [u8]) -> io::Result<VirtualSerialRead> {
        self.inner.read_input(buf)
    }
}

impl VirtualSerialBroker {
    pub fn new(config: VirtualSerialConfig) -> Self {
        let enabled = config.enabled;
        Self {
            config,
            enabled,
            source_port: None,
            endpoints: Vec::new(),
        }
    }

    pub fn with_source_port(config: VirtualSerialConfig, source_port: PathBuf) -> Self {
        let mut broker = Self::new(config);
        broker.source_port = Some(source_port);
        broker
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn config(&self) -> &VirtualSerialConfig {
        &self.config
    }

    pub fn set_source_port(&mut self, source_port: PathBuf) {
        if self.source_port.as_ref() != Some(&source_port) {
            self.source_port = Some(source_port);
            self.endpoints.clear();
        }
    }

    pub fn clear_endpoints(&mut self) {
        self.endpoints.clear();
    }

    pub fn set_enabled(&mut self, enabled: bool, manifest: Option<&DeviceManifest>) {
        self.enabled = enabled;
        if enabled {
            if let Some(manifest) = manifest {
                self.sync_manifest(manifest);
            }
        } else {
            self.endpoints.clear();
        }
    }

    pub fn sync_manifest(&mut self, manifest: &DeviceManifest) {
        if !self.enabled {
            self.endpoints.clear();
            return;
        }

        let mut old_endpoints = std::mem::take(&mut self.endpoints);
        let mut endpoints = Vec::with_capacity(manifest.channels.len());

        for channel in &manifest.channels {
            let spec = self.endpoint_spec(manifest, channel);
            if let Some(index) = old_endpoints
                .iter()
                .position(|endpoint| endpoint.matches_spec(&spec))
            {
                endpoints.push(old_endpoints.swap_remove(index));
            } else {
                endpoints.push(self.create_endpoint(spec));
            }
        }

        self.endpoints = endpoints;
    }

    pub fn endpoint_infos(&self) -> Vec<VirtualSerialEndpointInfo> {
        self.endpoints
            .iter()
            .map(VirtualSerialEndpoint::info)
            .collect()
    }

    pub fn summary(&self) -> String {
        if !self.enabled {
            return "virtual serial disabled".to_string();
        }
        if self.endpoints.is_empty() {
            return "virtual serial waiting for manifest".to_string();
        }
        let active = self
            .endpoints
            .iter()
            .filter(|endpoint| matches!(endpoint.backend, VirtualSerialEndpointBackend::Active(_)))
            .count();
        format!("virtual serial {active}/{} endpoints", self.endpoints.len())
    }

    pub fn endpoint_path_for_channel(&self, channel_id: u32) -> Option<PathBuf> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.channel_id == channel_id)
            .and_then(|endpoint| match &endpoint.backend {
                VirtualSerialEndpointBackend::Active(backend) => Some(backend.path().to_path_buf()),
                _ => None,
            })
    }

    pub fn input_owner(&self, channel_id: u32) -> Option<VirtualSerialInputOwner> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.channel_id == channel_id)
            .map(|endpoint| endpoint.input_owner)
    }

    pub fn toggle_input_owner(&mut self, channel_id: u32) -> Option<VirtualSerialInputOwner> {
        let endpoint = self
            .endpoints
            .iter_mut()
            .find(|endpoint| endpoint.channel_id == channel_id && endpoint.input_capable)?;
        endpoint.input_owner = match endpoint.input_owner {
            VirtualSerialInputOwner::Host => VirtualSerialInputOwner::VirtualSerial,
            VirtualSerialInputOwner::VirtualSerial => VirtualSerialInputOwner::Host,
        };
        Some(endpoint.input_owner)
    }

    pub fn write_output(&mut self, envelope: &MuxEnvelope) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| endpoint.channel_id == envelope.channel_id)
        {
            if matches!(endpoint.backend, VirtualSerialEndpointBackend::Active(_)) {
                if envelope.kind == PAYLOAD_KIND_TEXT {
                    let output = terminal_text_output_bytes(
                        &envelope.payload,
                        &mut endpoint.output_previous_was_cr,
                        endpoint.output_record_delimited,
                    );
                    endpoint.enqueue_output(&output);
                } else {
                    endpoint.output_previous_was_cr = false;
                    endpoint.enqueue_output(&envelope.payload);
                }
                endpoint.flush_output()?;
            }
        }
        Ok(())
    }

    pub fn poll_input(&mut self) -> io::Result<Vec<VirtualSerialInputEvent>> {
        if !self.enabled {
            return Ok(Vec::new());
        }

        let mut events = Vec::new();
        let mut buf = [0u8; 4096];
        for endpoint in &mut self.endpoints {
            endpoint.flush_output()?;
            let VirtualSerialEndpointBackend::Active(backend) = &mut endpoint.backend else {
                continue;
            };
            loop {
                let read_len = match backend.read_input_event(&mut buf)? {
                    VirtualSerialRead::Bytes(read_len) => read_len,
                    VirtualSerialRead::NoData | VirtualSerialRead::ClientGone => break,
                };
                if read_len == 0 {
                    break;
                }
                if !endpoint.input_capable {
                    events.push(VirtualSerialInputEvent::Discarded {
                        channel_id: endpoint.channel_id,
                        reason: "channel is output-only".to_string(),
                        bytes: read_len,
                    });
                    continue;
                }
                if endpoint.input_owner != VirtualSerialInputOwner::VirtualSerial {
                    events.push(VirtualSerialInputEvent::Discarded {
                        channel_id: endpoint.channel_id,
                        reason: "host owns input".to_string(),
                        bytes: read_len,
                    });
                    continue;
                }
                let channel_id = match u8::try_from(endpoint.channel_id) {
                    Ok(channel_id) => channel_id,
                    Err(_) => {
                        events.push(VirtualSerialInputEvent::Discarded {
                            channel_id: endpoint.channel_id,
                            reason: "channel id exceeds host input frame range".to_string(),
                            bytes: read_len,
                        });
                        continue;
                    }
                };
                events.push(VirtualSerialInputEvent::Forward {
                    channel_id,
                    payload: buf[..read_len].to_vec(),
                });
            }
        }
        Ok(events)
    }

    fn endpoint_spec(
        &self,
        manifest: &DeviceManifest,
        channel: &ChannelDescriptor,
    ) -> VirtualSerialEndpointSpec {
        let name = virtual_serial_name(
            &self.config.name_template,
            manifest,
            channel,
            self.source_port.as_deref(),
        );
        VirtualSerialEndpointSpec {
            channel_id: channel.channel_id,
            name,
            input_capable: channel.directions.contains(&DIRECTION_INPUT),
            output_record_delimited: virtual_serial_record_delimited_output(channel),
        }
    }

    fn create_endpoint(&self, spec: VirtualSerialEndpointSpec) -> VirtualSerialEndpoint {
        let backend = match VirtualSerialEndpointHandle::open(&spec.name) {
            Ok(backend) => VirtualSerialEndpointBackend::Active(backend),
            Err(err) if err.kind() == io::ErrorKind::Unsupported => {
                VirtualSerialEndpointBackend::Unsupported(err.to_string())
            }
            Err(err) => VirtualSerialEndpointBackend::Failed(err.to_string()),
        };
        VirtualSerialEndpoint {
            channel_id: spec.channel_id,
            name: spec.name,
            input_capable: spec.input_capable,
            input_owner: VirtualSerialInputOwner::Host,
            output_record_delimited: spec.output_record_delimited,
            output_previous_was_cr: false,
            output_pending: Vec::new(),
            output_dropped_bytes: 0,
            backend,
        }
    }
}

struct VirtualSerialEndpointSpec {
    channel_id: u32,
    name: String,
    input_capable: bool,
    output_record_delimited: bool,
}

pub fn terminal_text_output_bytes(
    payload: &[u8],
    previous_was_cr: &mut bool,
    record_delimited: bool,
) -> Vec<u8> {
    let needs_record_break =
        record_delimited && !payload.is_empty() && !matches!(payload.last(), Some(b'\r' | b'\n'));
    let mut output = Vec::with_capacity(payload.len() + usize::from(needs_record_break) * 2);
    for &byte in payload {
        if byte == b'\n' && !*previous_was_cr {
            output.push(b'\r');
        }
        output.push(byte);
        *previous_was_cr = byte == b'\r';
    }
    if needs_record_break {
        output.extend_from_slice(b"\r\n");
        *previous_was_cr = false;
    }
    output
}

fn virtual_serial_record_delimited_output(channel: &ChannelDescriptor) -> bool {
    channel.default_interaction_mode != CHANNEL_INTERACTION_PASSTHROUGH
        && !channel
            .interaction_modes
            .contains(&CHANNEL_INTERACTION_PASSTHROUGH)
}

impl VirtualSerialEndpoint {
    fn matches_spec(&self, spec: &VirtualSerialEndpointSpec) -> bool {
        self.channel_id == spec.channel_id
            && self.name == spec.name
            && self.input_capable == spec.input_capable
            && self.output_record_delimited == spec.output_record_delimited
    }

    fn enqueue_output(&mut self, bytes: &[u8]) {
        let available = VIRTUAL_SERIAL_OUTPUT_QUEUE_LIMIT.saturating_sub(self.output_pending.len());
        let queued_len = bytes.len().min(available);
        self.output_pending.extend_from_slice(&bytes[..queued_len]);
        self.output_dropped_bytes += bytes.len() - queued_len;
    }

    fn flush_output(&mut self) -> io::Result<()> {
        let VirtualSerialEndpointBackend::Active(backend) = &mut self.backend else {
            return Ok(());
        };
        while !self.output_pending.is_empty() {
            let written = backend.write_output(&self.output_pending)?;
            if written == 0 {
                break;
            }
            self.output_pending.drain(..written);
        }
        Ok(())
    }

    fn info(&self) -> VirtualSerialEndpointInfo {
        let (path, status) = match &self.backend {
            VirtualSerialEndpointBackend::Active(backend) => (
                Some(backend.path().to_path_buf()),
                format!("active real={}", backend.real_path().display()),
            ),
            VirtualSerialEndpointBackend::Unsupported(reason) => {
                (None, format!("unsupported: {reason}"))
            }
            VirtualSerialEndpointBackend::Failed(reason) => (None, format!("failed: {reason}")),
        };
        VirtualSerialEndpointInfo {
            channel_id: self.channel_id,
            name: self.name.clone(),
            path,
            input_capable: self.input_capable,
            input_owner: self.input_owner,
            status,
        }
    }
}

fn virtual_serial_name(
    template: &str,
    manifest: &DeviceManifest,
    channel: &ChannelDescriptor,
    source_port: Option<&Path>,
) -> String {
    let device = source_port
        .map(source_port_virtual_serial_name_part)
        .unwrap_or_else(|| sanitize_virtual_serial_name_part(&manifest.device_name));
    let channel_name = if channel.name.is_empty() {
        String::new()
    } else {
        sanitize_virtual_serial_name_part(&channel.name)
    };
    let channel_name = if channel_name.is_empty() {
        format!("ch{}", channel.channel_id)
    } else {
        channel_name
    };
    template
        .replace(
            "{device}",
            if device.is_empty() { "device" } else { &device },
        )
        .replace("{channel}", &channel_name)
        .replace("{channel_id}", &channel.channel_id.to_string())
}

fn sanitize_virtual_serial_name_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn source_port_virtual_serial_name_part(path: &Path) -> String {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let file_name = file_name.as_ref();
    let value = file_name
        .strip_prefix("tty.")
        .or_else(|| file_name.strip_prefix("cu."))
        .unwrap_or(file_name);
    truncate_chars(&sanitize_virtual_serial_name_part(value), 15)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(unix)]
fn open_virtual_serial_endpoint(_name: &str) -> io::Result<Box<dyn VirtualSerialEndpointIo>> {
    unix_virtual_serial::UnixVirtualSerialEndpoint::open(_name)
        .map(|endpoint| Box::new(endpoint) as Box<dyn VirtualSerialEndpointIo>)
}

#[cfg(not(unix))]
fn open_virtual_serial_endpoint(_name: &str) -> io::Result<Box<dyn VirtualSerialEndpointIo>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "virtual serial backend is not implemented on this platform",
    ))
}

fn newline_bytes(policy: u32) -> &'static [u8] {
    match policy {
        NEWLINE_POLICY_LF => b"\n",
        NEWLINE_POLICY_CR => b"\r",
        NEWLINE_POLICY_CRLF => b"\r\n",
        _ => b"\r",
    }
}

fn ascii_control_byte(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) & 0x1f)
    } else if matches!(ch, '[' | '\\' | ']' | '^' | '_') {
        Some((ch as u8) & 0x1f)
    } else {
        None
    }
}

fn open_candidate(
    path: &Path,
    profile: &SerialProfile,
    mode: InteractiveBackendMode,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    match mode {
        InteractiveBackendMode::Auto => open_auto_backend(path, profile, read_timeout),
        InteractiveBackendMode::Compat => open_compat_backend(path, profile, read_timeout),
        InteractiveBackendMode::Mio => open_mio_backend(path, profile, read_timeout),
    }
}

fn open_auto_backend(
    path: &Path,
    profile: &SerialProfile,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    #[cfg(unix)]
    {
        match open_mio_backend(path, profile, read_timeout) {
            Ok(backend) => return Ok(backend),
            Err(mio_err) => match open_compat_backend(path, profile, read_timeout) {
                Ok(mut backend) => {
                    backend.label = format!("compat (mio fallback: {mio_err})");
                    return Ok(backend);
                }
                Err(compat_err) => {
                    return Err(io::Error::new(
                        compat_err.kind(),
                        format!("mio failed: {mio_err}; compat failed: {compat_err}"),
                    ));
                }
            },
        }
    }

    #[cfg(not(unix))]
    {
        open_compat_backend(path, baud, read_timeout)
    }
}

fn open_compat_backend(
    path: &Path,
    profile: &SerialProfile,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    let path_text = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    let write_port = profile
        .apply_to_builder(serialport::new(path_text, profile.baud).timeout(read_timeout))?
        .open()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let read_port = write_port
        .try_clone()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || read_serial_thread(read_port, tx));

    Ok(ConnectedInteractiveBackend {
        label: "compat".to_string(),
        inner: ConnectedInteractiveBackendInner::Compat(CompatBackend { write_port, rx }),
    })
}

fn read_serial_thread(
    mut port: Box<dyn serialport::SerialPort>,
    tx: mpsc::Sender<SerialReadEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match port.read(&mut buf) {
            Ok(0) => {
                let _ = tx.send(SerialReadEvent::Eof);
                break;
            }
            Ok(read_len) => {
                if tx
                    .send(SerialReadEvent::Bytes(buf[..read_len].to_vec()))
                    .is_err()
                {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => {
                let _ = tx.send(SerialReadEvent::Error(err));
                break;
            }
        }
    }
}

struct CompatBackend {
    write_port: Box<dyn serialport::SerialPort>,
    rx: mpsc::Receiver<SerialReadEvent>,
}

impl CompatBackend {
    fn next_event(&mut self, deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
        loop {
            if let Some(event) = drain_terminal_event()? {
                return Ok(InteractiveEvent::Terminal(event));
            }
            match self.rx.try_recv() {
                Ok(event) => return Ok(map_serial_event(event)),
                Err(mpsc::TryRecvError::Disconnected) => return Ok(InteractiveEvent::SerialEof),
                Err(mpsc::TryRecvError::Empty) => {}
            }

            let Some(wait_for) = compat_wait_duration(deadline) else {
                return Ok(InteractiveEvent::Timeout);
            };
            match self.rx.recv_timeout(wait_for) {
                Ok(event) => return Ok(map_serial_event(event)),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(InteractiveEvent::SerialEof);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        return Ok(InteractiveEvent::Timeout);
                    }
                }
            }
        }
    }

    fn write_data_terminal_ready(&mut self, level: bool) -> io::Result<()> {
        self.write_port
            .write_data_terminal_ready(level)
            .map_err(|err| io::Error::other(err.to_string()))
    }

    fn write_request_to_send(&mut self, level: bool) -> io::Result<()> {
        self.write_port
            .write_request_to_send(level)
            .map_err(|err| io::Error::other(err.to_string()))
    }

    fn set_baud_rate(&mut self, baud: u32) -> io::Result<()> {
        self.write_port
            .set_baud_rate(baud)
            .map_err(|err| io::Error::other(err.to_string()))
    }
}

impl Write for CompatBackend {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_port.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_port.flush()
    }
}

fn compat_wait_duration(deadline: Option<Instant>) -> Option<Duration> {
    const COMPAT_POLL_INTERVAL: Duration = Duration::from_millis(10);
    match deadline {
        Some(deadline) => {
            let now = Instant::now();
            if now >= deadline {
                None
            } else {
                Some(
                    deadline
                        .saturating_duration_since(now)
                        .min(COMPAT_POLL_INTERVAL),
                )
            }
        }
        None => Some(COMPAT_POLL_INTERVAL),
    }
}

fn map_serial_event(event: SerialReadEvent) -> InteractiveEvent {
    match event {
        SerialReadEvent::Bytes(bytes) => InteractiveEvent::SerialBytes(bytes),
        SerialReadEvent::Eof => InteractiveEvent::SerialEof,
        SerialReadEvent::Error(err) => InteractiveEvent::SerialError(err),
    }
}

pub fn wait_terminal_event(deadline: Option<Instant>) -> io::Result<InteractiveEvent> {
    if let Some(event) = drain_terminal_event()? {
        return Ok(InteractiveEvent::Terminal(event));
    }

    let Some(timeout) = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
    else {
        return Ok(InteractiveEvent::Timeout);
    };
    if timeout.is_zero() {
        return Ok(InteractiveEvent::Timeout);
    }
    if retry_interrupted(|| event::poll(timeout))? {
        Ok(InteractiveEvent::Terminal(retry_interrupted(event::read)?))
    } else {
        Ok(InteractiveEvent::Timeout)
    }
}

pub fn drain_terminal_event() -> io::Result<Option<Event>> {
    if retry_interrupted(|| event::poll(Duration::ZERO))? {
        Ok(Some(retry_interrupted(event::read)?))
    } else {
        Ok(None)
    }
}

pub fn retry_interrupted<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match op() {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

#[cfg(unix)]
fn open_mio_backend(
    path: &Path,
    profile: &SerialProfile,
    read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    Ok(ConnectedInteractiveBackend {
        label: "mio".to_string(),
        inner: ConnectedInteractiveBackendInner::Mio(UnixMioBackend::open(
            path,
            profile,
            read_timeout,
        )?),
    })
}

#[cfg(not(unix))]
fn open_mio_backend(
    _path: &Path,
    _profile: &SerialProfile,
    _read_timeout: Duration,
) -> io::Result<ConnectedInteractiveBackend> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mio backend is only available on Unix",
    ))
}

#[cfg(unix)]
mod unix_virtual_serial {
    use std::env;
    use std::fs;
    use std::io::{self, Read, Write};
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};

    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};

    use super::VirtualSerialEndpointIo;

    pub(super) struct UnixVirtualSerialEndpoint {
        master: PtyMaster,
        path: PathBuf,
        real_path: PathBuf,
        alias_path: Option<PathBuf>,
    }

    impl UnixVirtualSerialEndpoint {
        pub(super) fn open(name: &str) -> io::Result<Self> {
            let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).map_err(nix_error_to_io)?;
            grantpt(&master).map_err(nix_error_to_io)?;
            unlockpt(&master).map_err(nix_error_to_io)?;
            let real_path = unsafe { ptsname(&master) }
                .map(PathBuf::from)
                .map_err(nix_error_to_io)?;
            set_nonblocking(&master)?;
            let alias_path = create_stable_alias(name, &real_path)?;
            let path = alias_path.clone().unwrap_or_else(|| real_path.clone());
            Ok(Self {
                master,
                path,
                real_path,
                alias_path,
            })
        }
    }

    impl Drop for UnixVirtualSerialEndpoint {
        fn drop(&mut self) {
            revoke_real_path_if_supported(&self.real_path);
            if let Some(alias_path) = &self.alias_path {
                remove_alias_if_ours(alias_path, &self.real_path);
            }
        }
    }

    impl VirtualSerialEndpointIo for UnixVirtualSerialEndpoint {
        fn path(&self) -> &Path {
            &self.path
        }

        fn real_path(&self) -> &Path {
            &self.real_path
        }

        fn write_output(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match self.master.write(bytes) {
                Ok(written) => Ok(written),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => Ok(0),
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    Ok(0)
                }
                Err(err) if is_pty_client_gone_error(&err) => Ok(0),
                Err(err) => Err(err),
            }
        }

        fn read_input(&mut self, buf: &mut [u8]) -> io::Result<super::VirtualSerialRead> {
            match self.master.read(buf) {
                Ok(read_len) => Ok(super::VirtualSerialRead::Bytes(read_len)),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    Ok(super::VirtualSerialRead::NoData)
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    Ok(super::VirtualSerialRead::NoData)
                }
                Err(err) if is_pty_client_gone_error(&err) => {
                    Ok(super::VirtualSerialRead::ClientGone)
                }
                Err(err) => Err(err),
            }
        }
    }

    pub(super) fn is_pty_client_gone_error(err: &io::Error) -> bool {
        err.raw_os_error() == Some(nix::libc::EIO)
    }

    fn set_nonblocking(master: &PtyMaster) -> io::Result<()> {
        let flags = fcntl(master.as_raw_fd(), FcntlArg::F_GETFL).map_err(nix_error_to_io)?;
        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(master.as_raw_fd(), FcntlArg::F_SETFL(flags)).map_err(nix_error_to_io)?;
        Ok(())
    }

    fn create_stable_alias(name: &str, real_path: &Path) -> io::Result<Option<PathBuf>> {
        let alias_name = format!("tty.{name}");
        let dev_alias = Path::new("/dev").join(&alias_name);
        match replace_stale_symlink(&dev_alias, real_path) {
            Ok(()) => return Ok(Some(dev_alias)),
            Err(err) if should_fallback_alias_dir(&err) => {}
            Err(err) => return Err(err),
        }

        let fallback_dir = default_alias_dir();
        fs::create_dir_all(&fallback_dir)?;
        let fallback_alias = fallback_dir.join(alias_name);
        replace_stale_symlink(&fallback_alias, real_path)?;
        Ok(Some(fallback_alias))
    }

    fn replace_stale_symlink(alias_path: &Path, real_path: &Path) -> io::Result<()> {
        match fs::symlink_metadata(alias_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(alias_path)?;
                if target.exists() && target != real_path {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!(
                            "{} already points to active {}",
                            alias_path.display(),
                            target.display()
                        ),
                    ));
                }
                fs::remove_file(alias_path)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "{} already exists and is not a symlink",
                        alias_path.display()
                    ),
                ));
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }

        std::os::unix::fs::symlink(real_path, alias_path)
    }

    fn remove_alias_if_ours(alias_path: &Path, real_path: &Path) {
        if fs::read_link(alias_path).is_ok_and(|target| target == real_path) {
            let _ = fs::remove_file(alias_path);
        }
    }

    fn should_fallback_alias_dir(err: &io::Error) -> bool {
        !matches!(err.kind(), io::ErrorKind::AlreadyExists)
    }

    fn default_alias_dir() -> PathBuf {
        if let Some(path) = env::var_os("WIREMUX_VIRTUAL_SERIAL_DIR") {
            return PathBuf::from(path);
        }
        PathBuf::from("/tmp/wiremux/tty")
    }

    #[cfg(target_os = "macos")]
    fn revoke_real_path_if_supported(path: &Path) {
        use std::ffi::CString;
        use std::os::raw::c_char;
        use std::os::unix::ffi::OsStrExt;

        extern "C" {
            fn revoke(path: *const c_char) -> i32;
        }

        let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
            return;
        };
        unsafe {
            revoke(path.as_ptr());
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn revoke_real_path_if_supported(_path: &Path) {
        // Linux has no portable revoke(2) equivalent for PTY slaves.
    }

    fn nix_error_to_io(err: nix::Error) -> io::Error {
        io::Error::new(io::ErrorKind::Other, err.to_string())
    }
}

#[cfg(unix)]
mod unix_mio {
    use std::fs::File;
    use std::io::{self, IsTerminal, Read, Write};
    use std::os::fd::AsRawFd;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crossterm::event::Event;
    use mio::unix::SourceFd;
    use mio::{Events, Interest, Poll, Token};
    use serialport::SerialPort;
    use signal_hook::consts::SIGWINCH;
    use signal_hook_mio::v1_0::Signals;

    use super::{drain_terminal_event, InteractiveEvent};

    const SERIAL_TOKEN: Token = Token(0);
    const TERMINAL_TOKEN: Token = Token(1);
    const SIGNAL_TOKEN: Token = Token(2);

    pub(super) struct UnixMioBackend {
        port: serialport::TTYPort,
        poll: Poll,
        events: Events,
        _terminal_file: Option<File>,
        signals: Signals,
    }

    impl UnixMioBackend {
        pub(super) fn open(
            path: &Path,
            profile: &super::SerialProfile,
            read_timeout: Duration,
        ) -> io::Result<Self> {
            let path_text = path.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8")
            })?;
            let mut port = profile
                .apply_to_builder(serialport::new(path_text, profile.baud).timeout(read_timeout))?
                .open_native()
                .map_err(|err| io::Error::other(err.to_string()))?;
            port.set_timeout(Duration::from_millis(1))
                .map_err(|err| io::Error::other(err.to_string()))?;

            let poll = Poll::new()?;
            let registry = poll.registry();

            let serial_fd = port.as_raw_fd();
            let mut serial_source = SourceFd(&serial_fd);
            registry.register(&mut serial_source, SERIAL_TOKEN, Interest::READABLE)?;

            let (terminal_fd, terminal_file) = terminal_fd()?;
            let mut terminal_source = SourceFd(&terminal_fd);
            registry.register(&mut terminal_source, TERMINAL_TOKEN, Interest::READABLE)?;

            let mut signals = Signals::new([SIGWINCH])?;
            registry.register(&mut signals, SIGNAL_TOKEN, Interest::READABLE)?;

            Ok(Self {
                port,
                poll,
                events: Events::with_capacity(8),
                _terminal_file: terminal_file,
                signals,
            })
        }

        pub(super) fn next_event(
            &mut self,
            deadline: Option<Instant>,
        ) -> io::Result<InteractiveEvent> {
            if let Some(event) = drain_terminal_event()? {
                return Ok(InteractiveEvent::Terminal(event));
            }

            let timeout =
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            super::retry_interrupted(|| self.poll.poll(&mut self.events, timeout))?;
            if self.events.is_empty() {
                return Ok(InteractiveEvent::Timeout);
            }

            for token in self.events.iter().map(|event| event.token()) {
                match token {
                    SERIAL_TOKEN => {
                        let mut buf = [0u8; 4096];
                        match self.port.read(&mut buf) {
                            Ok(0) => return Ok(InteractiveEvent::SerialEof),
                            Ok(read_len) => {
                                return Ok(InteractiveEvent::SerialBytes(buf[..read_len].to_vec()));
                            }
                            Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                            Err(err) => return Ok(InteractiveEvent::SerialError(err)),
                        }
                    }
                    TERMINAL_TOKEN => {
                        if let Some(event) = drain_terminal_event()? {
                            return Ok(InteractiveEvent::Terminal(event));
                        }
                    }
                    SIGNAL_TOKEN => {
                        if self.signals.pending().any(|signal| signal == SIGWINCH) {
                            let (cols, rows) = super::retry_interrupted(crossterm::terminal::size)?;
                            return Ok(InteractiveEvent::Terminal(Event::Resize(cols, rows)));
                        }
                    }
                    _ => {}
                }
            }

            Ok(InteractiveEvent::Timeout)
        }
    }

    impl Write for UnixMioBackend {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.port.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.port.flush()
        }
    }

    impl UnixMioBackend {
        pub(super) fn write_data_terminal_ready(&mut self, level: bool) -> io::Result<()> {
            self.port
                .write_data_terminal_ready(level)
                .map_err(|err| io::Error::other(err.to_string()))
        }

        pub(super) fn write_request_to_send(&mut self, level: bool) -> io::Result<()> {
            self.port
                .write_request_to_send(level)
                .map_err(|err| io::Error::other(err.to_string()))
        }

        pub(super) fn set_baud_rate(&mut self, baud: u32) -> io::Result<()> {
            self.port
                .set_baud_rate(baud)
                .map_err(|err| io::Error::other(err.to_string()))
        }
    }

    fn terminal_fd() -> io::Result<(i32, Option<File>)> {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            return Ok((stdin.as_raw_fd(), None));
        }

        let file = File::options().read(true).write(true).open("/dev/tty")?;
        let fd = file.as_raw_fd();
        Ok((fd, Some(file)))
    }
}

#[cfg(unix)]
use unix_mio::UnixMioBackend;

#[cfg(test)]
mod tests {
    use super::{
        host_supports_virtual_serial_provider, is_passthrough_exit_key,
        is_passthrough_meta_exit_key, paired_tty_cu_path, passthrough_key_payload, port_candidates,
        requested_file_name_starts_with, retry_interrupted, source_port_virtual_serial_name_part,
        terminal_text_output_bytes, usbmodem_fragment, virtual_serial_name,
        virtual_serial_record_delimited_output, HostConfig, SerialConfig, SerialFlowControl,
        SerialParity, SerialProfileOverrides, VirtualSerialBroker, VirtualSerialConfig,
        VirtualSerialEndpoint, VirtualSerialEndpointBackend, VirtualSerialEndpointHandle,
        VirtualSerialEndpointIo, VirtualSerialExport, VirtualSerialInputOwner, VirtualSerialRead,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use host_session::{
        ChannelDescriptor, DeviceManifest, CHANNEL_INTERACTION_PASSTHROUGH, DIRECTION_INPUT,
        DIRECTION_OUTPUT,
    };
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    #[test]
    fn serial_config_resolves_profile_with_defaults_and_overrides() {
        let config = HostConfig {
            serial: SerialConfig {
                port: Some(PathBuf::from("/dev/tty.usbserial-a")),
                baud: 115_200,
                data_bits: 8,
                stop_bits: 1,
                parity: SerialParity::None,
                flow_control: SerialFlowControl::None,
            },
            ..Default::default()
        };

        let profile = config
            .serial
            .resolve_profile(SerialProfileOverrides {
                baud: Some(921_600),
                data_bits: Some(7),
                parity: Some(SerialParity::Even),
                ..Default::default()
            })
            .expect("profile resolves");

        assert_eq!(profile.port, PathBuf::from("/dev/tty.usbserial-a"));
        assert_eq!(profile.baud, 921_600);
        assert_eq!(profile.data_bits, 7);
        assert_eq!(profile.stop_bits, 1);
        assert_eq!(profile.parity, SerialParity::Even);
        assert_eq!(profile.flow_control, SerialFlowControl::None);
    }

    #[test]
    fn serial_config_rejects_missing_port_and_invalid_options() {
        let missing = SerialConfig::default()
            .resolve_profile(SerialProfileOverrides::default())
            .expect_err("missing port should fail");
        assert!(missing.contains("serial port is required"));

        let invalid = SerialConfig {
            port: Some(PathBuf::from("/dev/tty.usbserial-a")),
            data_bits: 9,
            ..Default::default()
        }
        .resolve_profile(SerialProfileOverrides::default())
        .expect_err("invalid data bits should fail");
        assert!(invalid.contains("invalid data bits"));
    }

    #[test]
    fn host_config_round_trips_toml() {
        let input = r#"
[serial]
port = "/dev/tty.usbserial-a"
baud = 921600
data_bits = 7
stop_bits = 2
parity = "even"
flow_control = "hardware"
"#;
        let config: HostConfig = toml::from_str(input).expect("toml parses");
        assert!(!config.virtual_serial_configured);
        assert!(config.virtual_serial.enabled);
        assert_eq!(
            config.virtual_serial.export,
            VirtualSerialExport::AllManifestChannels
        );
        let profile = config
            .serial
            .resolve_profile(SerialProfileOverrides::default())
            .expect("profile resolves");

        assert_eq!(profile.port, PathBuf::from("/dev/tty.usbserial-a"));
        assert_eq!(profile.baud, 921_600);
        assert_eq!(profile.data_bits, 7);
        assert_eq!(profile.stop_bits, 2);
        assert_eq!(profile.parity, SerialParity::Even);
        assert_eq!(profile.flow_control, SerialFlowControl::Hardware);

        let serialized =
            toml::to_string(&HostConfig::from_serial_profile(&profile)).expect("toml serializes");
        assert!(serialized.contains("baud = 921600"));
        assert!(serialized.contains("data_bits = 7"));
        assert!(serialized.contains("parity = \"even\""));
    }

    #[test]
    fn host_config_parses_virtual_serial_section() {
        let input = r#"
[serial]
port = "/dev/tty.usbserial-a"

[virtual_serial]
enabled = false
export = "all-manifest-channels"
name_template = "wiremux-{device}-{channel_id}"
"#;
        let config: HostConfig = toml::from_str(input).expect("toml parses");
        assert!(config.virtual_serial_configured);
        assert_eq!(
            config.virtual_serial,
            VirtualSerialConfig {
                enabled: false,
                export: VirtualSerialExport::AllManifestChannels,
                name_template: "wiremux-{device}-{channel_id}".to_string(),
            }
        );
    }

    #[test]
    fn virtual_serial_text_output_maps_lf_to_crlf() {
        let mut previous_was_cr = false;

        assert_eq!(
            terminal_text_output_bytes(b"one\ntwo\r\nthree\r", &mut previous_was_cr, false),
            b"one\r\ntwo\r\nthree\r"
        );
        assert!(previous_was_cr);

        assert_eq!(
            terminal_text_output_bytes(b"\nfour\n", &mut previous_was_cr, false),
            b"\nfour\r\n"
        );
        assert!(!previous_was_cr);
    }

    #[test]
    fn generic_enhanced_registry_matches_virtual_serial_feature_support() {
        assert_eq!(
            host_supports_virtual_serial_provider(),
            cfg!(feature = "generic-enhanced")
        );
    }

    #[test]
    fn virtual_serial_text_output_appends_record_break_for_line_channels() {
        let mut previous_was_cr = false;

        assert_eq!(
            terminal_text_output_bytes(b"stress record", &mut previous_was_cr, true),
            b"stress record\r\n"
        );
        assert!(!previous_was_cr);

        assert_eq!(
            terminal_text_output_bytes(b"already ended\n", &mut previous_was_cr, true),
            b"already ended\r\n"
        );
        assert!(!previous_was_cr);

        assert_eq!(
            terminal_text_output_bytes(b"carriage\r", &mut previous_was_cr, true),
            b"carriage\r"
        );
        assert!(previous_was_cr);
    }

    #[test]
    fn virtual_serial_text_output_preserves_passthrough_stream_boundaries() {
        let mut previous_was_cr = false;

        assert_eq!(
            terminal_text_output_bytes(b"partial", &mut previous_was_cr, false),
            b"partial"
        );
        assert!(!previous_was_cr);
    }

    fn test_channel_descriptor(
        channel_id: u32,
        interaction_modes: Vec<u32>,
        default_interaction_mode: u32,
    ) -> ChannelDescriptor {
        ChannelDescriptor {
            channel_id,
            name: "test".to_string(),
            description: String::new(),
            directions: vec![DIRECTION_OUTPUT],
            payload_kinds: Vec::new(),
            payload_types: Vec::new(),
            flags: 0,
            default_payload_kind: 0,
            interaction_modes,
            default_interaction_mode,
            passthrough_policy: Default::default(),
        }
    }

    #[test]
    fn virtual_serial_uses_record_delimiters_only_for_non_passthrough_channels() {
        let line_channel = test_channel_descriptor(1, Vec::new(), 0);
        assert!(virtual_serial_record_delimited_output(&line_channel));

        let passthrough_channel =
            test_channel_descriptor(1, vec![CHANNEL_INTERACTION_PASSTHROUGH], 0);
        assert!(!virtual_serial_record_delimited_output(
            &passthrough_channel
        ));

        let default_passthrough_channel =
            test_channel_descriptor(1, Vec::new(), CHANNEL_INTERACTION_PASSTHROUGH);
        assert!(!virtual_serial_record_delimited_output(
            &default_passthrough_channel
        ));
    }

    #[test]
    fn virtual_serial_name_uses_source_port_and_channel_fallbacks() {
        let manifest = DeviceManifest {
            device_name: "esp-wiremux".to_string(),
            firmware_version: String::new(),
            protocol_version: 0,
            max_channels: 0,
            channels: Vec::new(),
            native_endianness: 0,
            max_payload_len: 0,
            transport: String::new(),
            feature_flags: 0,
            sdk_name: String::new(),
            sdk_version: String::new(),
        };
        let named = ChannelDescriptor {
            name: "telemetry".to_string(),
            ..test_channel_descriptor(3, Vec::new(), 0)
        };
        let emoji = ChannelDescriptor {
            channel_id: 4,
            name: "🚗🎒😄".to_string(),
            ..test_channel_descriptor(4, Vec::new(), 0)
        };

        assert_eq!(
            source_port_virtual_serial_name_part(&PathBuf::from("/dev/tty.HUAWEIFreeClip2345")),
            "HUAWEIFreeClip2"
        );
        assert_eq!(
            virtual_serial_name(
                "wiremux-{device}-{channel}",
                &manifest,
                &named,
                Some(Path::new("/dev/tty.usbmodem41301")),
            ),
            "wiremux-usbmodem41301-telemetry"
        );
        assert_eq!(
            virtual_serial_name(
                "wiremux-{device}-{channel}",
                &manifest,
                &emoji,
                Some(Path::new("/dev/tty.usbmodem41301")),
            ),
            "wiremux-usbmodem41301-ch4"
        );
    }

    #[test]
    fn virtual_serial_sync_reuses_matching_endpoint() {
        struct NullEndpoint;

        impl VirtualSerialEndpointIo for NullEndpoint {
            fn path(&self) -> &Path {
                Path::new("/dev/tty.wiremux-test")
            }

            fn real_path(&self) -> &Path {
                Path::new("/dev/ttys999")
            }

            fn write_output(&mut self, buf: &[u8]) -> io::Result<usize> {
                Ok(buf.len())
            }

            fn read_input(&mut self, _buf: &mut [u8]) -> io::Result<VirtualSerialRead> {
                Ok(VirtualSerialRead::NoData)
            }
        }

        let mut broker = VirtualSerialBroker::with_source_port(
            VirtualSerialConfig::default(),
            PathBuf::from("/dev/tty.usbmodem41301"),
        );
        broker.endpoints.push(VirtualSerialEndpoint {
            channel_id: 3,
            name: "wiremux-usbmodem41301-telemetry".to_string(),
            input_capable: true,
            input_owner: VirtualSerialInputOwner::VirtualSerial,
            output_record_delimited: true,
            output_previous_was_cr: false,
            output_pending: b"pending".to_vec(),
            output_dropped_bytes: 0,
            backend: VirtualSerialEndpointBackend::Active(VirtualSerialEndpointHandle {
                inner: Box::new(NullEndpoint),
            }),
        });
        let manifest = DeviceManifest {
            device_name: "esp-wiremux".to_string(),
            firmware_version: String::new(),
            protocol_version: 0,
            max_channels: 0,
            channels: vec![ChannelDescriptor {
                channel_id: 3,
                name: "telemetry".to_string(),
                description: String::new(),
                directions: vec![DIRECTION_INPUT, DIRECTION_OUTPUT],
                payload_kinds: Vec::new(),
                payload_types: Vec::new(),
                flags: 0,
                default_payload_kind: 0,
                interaction_modes: Vec::new(),
                default_interaction_mode: 0,
                passthrough_policy: Default::default(),
            }],
            native_endianness: 0,
            max_payload_len: 0,
            transport: String::new(),
            feature_flags: 0,
            sdk_name: String::new(),
            sdk_version: String::new(),
        };

        broker.sync_manifest(&manifest);

        assert_eq!(broker.endpoints.len(), 1);
        assert_eq!(
            broker.input_owner(3),
            Some(VirtualSerialInputOwner::VirtualSerial)
        );
        assert_eq!(broker.endpoints[0].output_pending, b"pending");
        assert_eq!(
            broker.endpoint_path_for_channel(3),
            Some(PathBuf::from("/dev/tty.wiremux-test"))
        );
    }

    #[test]
    fn virtual_serial_output_backpressure_is_queued() {
        struct ScriptedEndpoint {
            writes: Rc<std::cell::RefCell<VecDeque<usize>>>,
            output: Rc<std::cell::RefCell<Vec<u8>>>,
        }

        impl VirtualSerialEndpointIo for ScriptedEndpoint {
            fn path(&self) -> &Path {
                Path::new("/dev/null")
            }

            fn write_output(&mut self, buf: &[u8]) -> io::Result<usize> {
                let written = self.writes.borrow_mut().pop_front().unwrap_or(0);
                self.output.borrow_mut().extend_from_slice(&buf[..written]);
                Ok(written)
            }

            fn read_input(&mut self, _buf: &mut [u8]) -> io::Result<VirtualSerialRead> {
                Ok(VirtualSerialRead::NoData)
            }
        }

        let writes = Rc::new(std::cell::RefCell::new(VecDeque::from([3, 0, 3])));
        let output = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut endpoint = VirtualSerialEndpoint {
            channel_id: 1,
            name: "console".to_string(),
            input_capable: false,
            input_owner: VirtualSerialInputOwner::Host,
            output_record_delimited: true,
            output_previous_was_cr: false,
            output_pending: Vec::new(),
            output_dropped_bytes: 0,
            backend: VirtualSerialEndpointBackend::Active(VirtualSerialEndpointHandle {
                inner: Box::new(ScriptedEndpoint {
                    writes,
                    output: output.clone(),
                }),
            }),
        };

        endpoint.enqueue_output(b"abcdef");
        endpoint
            .flush_output()
            .expect("backpressure should keep pending output queued");
        assert_eq!(&*output.borrow(), b"abc");
        assert_eq!(endpoint.output_pending, b"def");

        endpoint
            .flush_output()
            .expect("later writable poll should drain pending output");
        assert_eq!(&*output.borrow(), b"abcdef");
        assert!(endpoint.output_pending.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unix_pty_client_gone_error_is_nonfatal() {
        let err = io::Error::from_raw_os_error(nix::libc::EIO);
        assert!(super::unix_virtual_serial::is_pty_client_gone_error(&err));
    }

    #[test]
    fn retry_interrupted_retries_until_success() {
        let attempts = Cell::new(0);

        let value = retry_interrupted(|| {
            let next = attempts.get() + 1;
            attempts.set(next);
            if next < 3 {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                Ok("ready")
            }
        })
        .expect("interrupted operations should be retried");

        assert_eq!(value, "ready");
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn retry_interrupted_preserves_non_interrupted_error() {
        let err = retry_interrupted(|| -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        })
        .expect_err("non-interrupted errors should still propagate");

        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn maps_tty_and_cu_port_pairs() {
        assert_eq!(
            paired_tty_cu_path(&PathBuf::from("/dev/tty.usbmodem2101")),
            Some(PathBuf::from("/dev/cu.usbmodem2101"))
        );
        assert_eq!(
            paired_tty_cu_path(&PathBuf::from("/dev/cu.usbmodem2101")),
            Some(PathBuf::from("/dev/tty.usbmodem2101"))
        );
    }

    #[test]
    fn prefers_cu_pair_when_requested_port_is_tty() {
        let candidates = port_candidates(&PathBuf::from("/dev/tty.usbmodem2101"));

        assert_eq!(candidates[0], PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(candidates[1], PathBuf::from("/dev/tty.usbmodem2101"));
    }

    #[test]
    fn detects_tty_prefix_and_usb_serial_fragments() {
        assert!(requested_file_name_starts_with(
            &PathBuf::from("/dev/tty.usbmodem2101"),
            "tty."
        ));
        assert!(!requested_file_name_starts_with(
            &PathBuf::from("/dev/cu.usbmodem2101"),
            "tty."
        ));
        assert_eq!(
            usbmodem_fragment(&PathBuf::from("/dev/tty.usbmodem2101")),
            Some("usbmodem")
        );
        assert_eq!(
            usbmodem_fragment(&PathBuf::from("/dev/cu.usbserial-0001")),
            Some("usbserial")
        );
        assert_eq!(usbmodem_fragment(&PathBuf::from("/tmp/file")), None);
    }

    #[test]
    fn passthrough_key_payload_maps_terminal_keys() {
        assert_eq!(
            passthrough_key_payload(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
                Default::default()
            ),
            Some(vec![b'a'])
        );
        assert_eq!(
            passthrough_key_payload(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Default::default()
            ),
            Some(vec![0x03])
        );
        assert_eq!(
            passthrough_key_payload(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
                Default::default()
            ),
            Some(vec![b'\r'])
        );
    }

    #[test]
    fn passthrough_exit_keys_accept_control_and_alt_variants() {
        assert!(is_passthrough_exit_key(KeyEvent::new(
            KeyCode::Char(']'),
            KeyModifiers::CONTROL
        )));
        assert!(is_passthrough_exit_key(KeyEvent::new(
            KeyCode::Char('\u{1d}'),
            KeyModifiers::empty()
        )));
        assert!(!is_passthrough_exit_key(KeyEvent::new(
            KeyCode::Char(']'),
            KeyModifiers::empty()
        )));
        assert!(is_passthrough_meta_exit_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::ALT
        )));
        assert!(is_passthrough_meta_exit_key(KeyEvent::new(
            KeyCode::Char('X'),
            KeyModifiers::ALT | KeyModifiers::SHIFT
        )));
    }
}
