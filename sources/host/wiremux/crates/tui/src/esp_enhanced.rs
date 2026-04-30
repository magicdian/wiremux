use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use host_session::{DeviceManifest, MuxEnvelope, PAYLOAD_KIND_TEXT};
use interactive::{
    terminal_text_output_bytes, ConnectedInteractiveBackend, Esp32BootloaderResetMode,
    VirtualSerialEndpointHandle, VirtualSerialRead,
};

const ESP_ENHANCED_ENDPOINT_NAME: &str = "wiremux-esp-enhanced";
const PENDING_INPUT_LIMIT: usize = 64 * 1024;
const PENDING_INPUT_TIMEOUT: Duration = Duration::from_secs(1);
const SLIP_END: u8 = 0xc0;
const SLIP_ESC: u8 = 0xdb;
const SLIP_ESC_END: u8 = 0xdc;
const SLIP_ESC_ESC: u8 = 0xdd;
const ESPTOOL_REQUEST: u8 = 0x00;
const ESPTOOL_SYNC: u8 = 0x08;
const ESPTOOL_FLASH_BEGIN: u8 = 0x02;
const ESPTOOL_FLASH_DATA: u8 = 0x03;
const ESPTOOL_FLASH_END: u8 = 0x04;
const ESPTOOL_CHANGE_BAUDRATE: u8 = 0x0f;
const ESPTOOL_SYNC_PREFIX: &[u8] = &[0x07, 0x07, 0x12, 0x20];

#[derive(Default)]
pub struct EspressifEnhancedHost {
    endpoint: Option<VirtualSerialEndpointHandle>,
    source_port: Option<PathBuf>,
    monitor_baud: Option<u32>,
    state: BridgeState,
    pending_input: Vec<u8>,
    pending_started_at: Option<Instant>,
    classifier: EsptoolSlipClassifier,
    serial_frame_reader: SlipFrameReader,
    pending_physical_baud: Option<u32>,
    active_physical_baud: Option<u32>,
    flash_intent_seen: bool,
    output_previous_was_cr: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BridgeState {
    #[default]
    WaitingForEspressif,
    AggregateMonitor,
    RawBridge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EspressifEnhancedPoll {
    pub dirty: bool,
    pub reset_host_session: bool,
}

impl EspressifEnhancedHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_source_port(&mut self, source_port: PathBuf) {
        if self.source_port.as_ref() != Some(&source_port) {
            self.source_port = Some(source_port);
            self.clear();
        }
    }

    pub fn set_monitor_baud(&mut self, baud: u32) {
        self.monitor_baud = Some(baud);
    }

    pub fn clear(&mut self) {
        self.endpoint = None;
        self.state = BridgeState::WaitingForEspressif;
        self.pending_input.clear();
        self.pending_started_at = None;
        self.classifier.reset();
        self.serial_frame_reader.reset();
        self.pending_physical_baud = None;
        self.active_physical_baud = None;
        self.flash_intent_seen = false;
        self.output_previous_was_cr = false;
    }

    pub fn sync_manifest(&mut self, manifest: &DeviceManifest) -> Vec<String> {
        if !is_espressif_manifest(manifest) {
            self.clear();
            return Vec::new();
        }

        if self.endpoint.is_none() {
            match VirtualSerialEndpointHandle::open(ESP_ENHANCED_ENDPOINT_NAME) {
                Ok(endpoint) => {
                    let path = endpoint.path().display().to_string();
                    self.endpoint = Some(endpoint);
                    self.state = BridgeState::AggregateMonitor;
                    return vec![format!("esp enhanced tty {path}")];
                }
                Err(err) => {
                    self.state = BridgeState::WaitingForEspressif;
                    return vec![format!("esp enhanced tty unavailable: {err}")];
                }
            }
        }

        self.state = match self.state {
            BridgeState::WaitingForEspressif => BridgeState::AggregateMonitor,
            state => state,
        };
        Vec::new()
    }

    pub fn summary(&self) -> String {
        match (&self.endpoint, self.state) {
            (Some(endpoint), BridgeState::AggregateMonitor) => {
                format!("esp-enhanced monitor {}", endpoint.path().display())
            }
            (Some(endpoint), BridgeState::RawBridge) => {
                if self.flash_intent_seen {
                    format!("esp-enhanced flashing {}", endpoint.path().display())
                } else {
                    format!("esp-enhanced raw bridge {}", endpoint.path().display())
                }
            }
            (Some(endpoint), BridgeState::WaitingForEspressif) => {
                format!("esp-enhanced waiting {}", endpoint.path().display())
            }
            (None, _) => "esp-enhanced unavailable".to_string(),
        }
    }

    pub fn is_raw_bridge_active(&self) -> bool {
        self.state == BridgeState::RawBridge
    }

    pub fn should_poll(&self) -> bool {
        self.endpoint.is_some()
    }

    pub fn mirror_mux_output(&mut self, envelope: &MuxEnvelope) -> io::Result<()> {
        if self.state != BridgeState::AggregateMonitor {
            return Ok(());
        }
        let Some(endpoint) = self.endpoint.as_mut() else {
            return Ok(());
        };

        let mut output = Vec::new();
        output.extend_from_slice(format!("ch{}> ", envelope.channel_id).as_bytes());
        if envelope.kind == PAYLOAD_KIND_TEXT {
            output.extend_from_slice(&terminal_text_output_bytes(
                &envelope.payload,
                &mut self.output_previous_was_cr,
                true,
            ));
        } else {
            self.output_previous_was_cr = false;
            output.extend_from_slice(&envelope.payload);
            if !ends_with_newline(&output) {
                output.extend_from_slice(b"\r\n");
            }
        }
        write_all_best_effort(endpoint, &output)
    }

    pub fn write_raw_serial_output(
        &mut self,
        bytes: &[u8],
        serial: &mut ConnectedInteractiveBackend,
        diagnostics: &mut dyn Write,
    ) -> io::Result<bool> {
        if self.state != BridgeState::RawBridge {
            return Ok(false);
        }
        let Some(endpoint) = self.endpoint.as_mut() else {
            return Ok(false);
        };
        write_all_best_effort(endpoint, bytes)?;
        if self.pending_physical_baud.is_some() && !self.serial_frame_reader.feed(bytes).is_empty()
        {
            let baud = self
                .pending_physical_baud
                .take()
                .expect("baud was checked before applying");
            writeln!(
                diagnostics,
                "[wiremux] esp_enhanced applying physical baud after response baud={baud}"
            )?;
            serial.set_baud_rate(baud)?;
            self.active_physical_baud = Some(baud);
        }
        Ok(true)
    }

    pub fn poll_input(
        &mut self,
        serial: &mut ConnectedInteractiveBackend,
        diagnostics: &mut dyn Write,
    ) -> io::Result<EspressifEnhancedPoll> {
        let mut result = EspressifEnhancedPoll::default();
        if self.endpoint.is_none() {
            return Ok(result);
        }

        let mut buf = [0u8; 4096];
        loop {
            let read_event = {
                let endpoint = self
                    .endpoint
                    .as_mut()
                    .expect("endpoint was checked before polling");
                endpoint.read_input_event(&mut buf)?
            };
            match read_event {
                VirtualSerialRead::Bytes(0) if self.state == BridgeState::RawBridge => {
                    writeln!(
                        diagnostics,
                        "[wiremux] esp_enhanced raw bridge client closed"
                    )?;
                    self.finish_raw_bridge(serial, diagnostics)?;
                    self.reset_to_monitor();
                    result.dirty = true;
                    result.reset_host_session = true;
                    break;
                }
                VirtualSerialRead::Bytes(0) | VirtualSerialRead::NoData => break,
                VirtualSerialRead::ClientGone => {
                    if self.state == BridgeState::RawBridge {
                        writeln!(diagnostics, "[wiremux] esp_enhanced raw bridge client gone")?;
                        self.finish_raw_bridge(serial, diagnostics)?;
                        self.reset_to_monitor();
                        result.dirty = true;
                        result.reset_host_session = true;
                    }
                    break;
                }
                VirtualSerialRead::Bytes(read_len) => {
                    let bytes = &buf[..read_len];
                    match self.state {
                        BridgeState::AggregateMonitor => {
                            self.handle_monitor_input(bytes, serial, diagnostics, &mut result)?;
                        }
                        BridgeState::RawBridge => {
                            serial.write_all(bytes)?;
                            serial.flush()?;
                            for command in self.classifier.feed(bytes) {
                                if is_flash_command(command.opcode) {
                                    self.flash_intent_seen = true;
                                }
                                if let Some(baud) = change_baud_rate(&command) {
                                    self.pending_physical_baud = Some(baud);
                                    writeln!(
                                        diagnostics,
                                        "[wiremux] esp_enhanced change-baud command detected baud={baud}"
                                    )?;
                                }
                            }
                            result.dirty = true;
                        }
                        BridgeState::WaitingForEspressif => {}
                    }
                }
            }
        }

        self.expire_pending_if_needed(diagnostics, &mut result)?;
        Ok(result)
    }

    fn handle_monitor_input(
        &mut self,
        bytes: &[u8],
        serial: &mut ConnectedInteractiveBackend,
        diagnostics: &mut dyn Write,
        result: &mut EspressifEnhancedPoll,
    ) -> io::Result<()> {
        if self.pending_started_at.is_none() {
            self.pending_started_at = Some(Instant::now());
        }
        if self.pending_input.len().saturating_add(bytes.len()) > PENDING_INPUT_LIMIT {
            writeln!(
                diagnostics,
                "[wiremux] esp_enhanced pending input overflow bytes={}",
                self.pending_input.len().saturating_add(bytes.len())
            )?;
            self.drop_pending();
            result.dirty = true;
            return Ok(());
        }

        self.pending_input.extend_from_slice(bytes);
        let commands = self.classifier.feed(bytes);
        if commands
            .iter()
            .any(|command| command.opcode == ESPTOOL_SYNC)
        {
            let reset_mode = esp32_bootloader_reset_mode(self.source_port.as_ref());
            writeln!(
                diagnostics,
                "[wiremux] esp_enhanced esptool sync detected; entering raw bridge reset_mode={reset_mode:?} replay_bytes={}",
                self.pending_input.len()
            )?;
            serial.enter_esp32_bootloader_with_reset_mode(reset_mode)?;
            serial.write_all(&self.pending_input)?;
            serial.flush()?;
            self.pending_input.clear();
            self.pending_started_at = None;
            self.state = BridgeState::RawBridge;
            result.dirty = true;
            result.reset_host_session = true;
        }
        Ok(())
    }

    fn finish_raw_bridge(
        &mut self,
        serial: &mut ConnectedInteractiveBackend,
        diagnostics: &mut dyn Write,
    ) -> io::Result<()> {
        let reset_mode = esp32_bootloader_reset_mode(self.source_port.as_ref());
        if self.flash_intent_seen {
            writeln!(
                diagnostics,
                "[wiremux] esp_enhanced raw bridge finished flash; hard resetting reset_mode={reset_mode:?}"
            )?;
            serial.hard_reset_esp32_with_reset_mode(reset_mode)?;
        }
        self.restore_monitor_baud(serial, diagnostics)
    }

    fn restore_monitor_baud(
        &mut self,
        serial: &mut ConnectedInteractiveBackend,
        diagnostics: &mut dyn Write,
    ) -> io::Result<()> {
        let Some(monitor_baud) = self.monitor_baud else {
            return Ok(());
        };
        if self.active_physical_baud == Some(monitor_baud) {
            return Ok(());
        }
        writeln!(
            diagnostics,
            "[wiremux] esp_enhanced restoring monitor baud={monitor_baud}"
        )?;
        serial.set_baud_rate(monitor_baud)?;
        self.active_physical_baud = Some(monitor_baud);
        Ok(())
    }

    fn expire_pending_if_needed(
        &mut self,
        diagnostics: &mut dyn Write,
        result: &mut EspressifEnhancedPoll,
    ) -> io::Result<()> {
        let Some(started_at) = self.pending_started_at else {
            return Ok(());
        };
        if started_at.elapsed() < PENDING_INPUT_TIMEOUT {
            return Ok(());
        }
        writeln!(
            diagnostics,
            "[wiremux] esp_enhanced pending input timed out bytes={}",
            self.pending_input.len()
        )?;
        self.drop_pending();
        result.dirty = true;
        Ok(())
    }

    fn reset_to_monitor(&mut self) {
        self.state = if self.endpoint.is_some() {
            BridgeState::AggregateMonitor
        } else {
            BridgeState::WaitingForEspressif
        };
        self.drop_pending();
        self.flash_intent_seen = false;
    }

    fn drop_pending(&mut self) {
        self.pending_input.clear();
        self.pending_started_at = None;
        self.classifier.reset();
        self.serial_frame_reader.reset();
        self.pending_physical_baud = None;
        self.active_physical_baud = None;
    }
}

fn is_espressif_manifest(manifest: &DeviceManifest) -> bool {
    let sdk = manifest.sdk_name.to_ascii_lowercase();
    let device = manifest.device_name.to_ascii_lowercase();
    sdk.contains("esp-wiremux")
        || sdk.contains("espressif")
        || device.contains("esp-wiremux")
        || device.contains("esp32")
        || device.contains("espressif")
}

fn write_all_best_effort(
    endpoint: &mut VirtualSerialEndpointHandle,
    mut bytes: &[u8],
) -> io::Result<()> {
    while !bytes.is_empty() {
        let written = endpoint.write_output(bytes)?;
        if written == 0 {
            break;
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

fn ends_with_newline(bytes: &[u8]) -> bool {
    matches!(bytes.last(), Some(b'\r' | b'\n'))
}

fn is_flash_command(command: u8) -> bool {
    matches!(
        command,
        ESPTOOL_FLASH_BEGIN | ESPTOOL_FLASH_DATA | ESPTOOL_FLASH_END
    )
}

fn change_baud_rate(command: &EsptoolCommand) -> Option<u32> {
    if command.opcode != ESPTOOL_CHANGE_BAUDRATE || command.payload.len() < 4 {
        return None;
    }
    let baud = u32::from_le_bytes([
        command.payload[0],
        command.payload[1],
        command.payload[2],
        command.payload[3],
    ]);
    (baud > 0).then_some(baud)
}

fn esp32_bootloader_reset_mode(source_port: Option<&PathBuf>) -> Esp32BootloaderResetMode {
    if source_port.is_some_and(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().contains("usbmodem"))
    }) {
        Esp32BootloaderResetMode::UsbJtagSerial
    } else {
        Esp32BootloaderResetMode::UsbToSerialBridge
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EsptoolCommand {
    opcode: u8,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
struct SlipFrameReader {
    in_frame: bool,
    escaped: bool,
    frame: Vec<u8>,
}

impl SlipFrameReader {
    fn reset(&mut self) {
        self.in_frame = false;
        self.escaped = false;
        self.frame.clear();
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for byte in bytes {
            match *byte {
                SLIP_END => {
                    if self.in_frame && !self.frame.is_empty() {
                        frames.push(std::mem::take(&mut self.frame));
                    }
                    self.in_frame = true;
                    self.escaped = false;
                    self.frame.clear();
                }
                SLIP_ESC if self.in_frame => {
                    self.escaped = true;
                }
                value if self.escaped => {
                    self.escaped = false;
                    match value {
                        SLIP_ESC_END => self.frame.push(SLIP_END),
                        SLIP_ESC_ESC => self.frame.push(SLIP_ESC),
                        _ => {
                            self.in_frame = false;
                            self.frame.clear();
                        }
                    }
                }
                value if self.in_frame => {
                    self.frame.push(value);
                }
                _ => {}
            }
        }
        frames
    }
}

#[derive(Debug, Default)]
struct EsptoolSlipClassifier {
    reader: SlipFrameReader,
}

impl EsptoolSlipClassifier {
    fn reset(&mut self) {
        self.reader.reset();
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<EsptoolCommand> {
        self.reader
            .feed(bytes)
            .into_iter()
            .filter_map(|frame| esptool_command(&frame))
            .collect()
    }
}

fn esptool_command(frame: &[u8]) -> Option<EsptoolCommand> {
    if frame.len() < 8 || frame[0] != ESPTOOL_REQUEST {
        return None;
    }
    let command = frame[1];
    let size = u16::from_le_bytes([frame[2], frame[3]]) as usize;
    if frame.len() < 8 + size {
        return None;
    }
    if command == ESPTOOL_SYNC && !is_sync_frame(frame) {
        return None;
    }
    Some(EsptoolCommand {
        opcode: command,
        payload: frame[8..8 + size].to_vec(),
    })
}

fn is_sync_frame(frame: &[u8]) -> bool {
    if frame.len() < 8 + ESPTOOL_SYNC_PREFIX.len() {
        return false;
    }
    let size = u16::from_le_bytes([frame[2], frame[3]]) as usize;
    if size < ESPTOOL_SYNC_PREFIX.len() || frame.len() < 8 + size {
        return false;
    }
    &frame[8..8 + ESPTOOL_SYNC_PREFIX.len()] == ESPTOOL_SYNC_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slip_frame(payload: &[u8]) -> Vec<u8> {
        let mut out = vec![SLIP_END];
        for byte in payload {
            match *byte {
                SLIP_END => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_END]),
                SLIP_ESC => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_ESC]),
                value => out.push(value),
            }
        }
        out.push(SLIP_END);
        out
    }

    fn sync_payload() -> Vec<u8> {
        let mut payload = vec![0x00, ESPTOOL_SYNC, 36, 0, 0, 0, 0, 0];
        payload.extend_from_slice(ESPTOOL_SYNC_PREFIX);
        payload.extend_from_slice(&[0x55; 32]);
        payload
    }

    #[test]
    fn classifier_accepts_complete_esptool_sync_frame() {
        let mut classifier = EsptoolSlipClassifier::default();
        let commands = classifier.feed(&slip_frame(&sync_payload()));
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].opcode, ESPTOOL_SYNC);
        assert_eq!(
            &commands[0].payload[..ESPTOOL_SYNC_PREFIX.len()],
            ESPTOOL_SYNC_PREFIX
        );
    }

    #[test]
    fn classifier_rejects_plain_terminal_text() {
        let mut classifier = EsptoolSlipClassifier::default();
        let commands = classifier.feed(b"help\r\n");
        assert!(commands.is_empty());
    }

    #[test]
    fn classifier_rejects_sync_command_without_magic_payload() {
        let mut payload = vec![0x00, ESPTOOL_SYNC, 4, 0, 0, 0, 0, 0];
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let mut classifier = EsptoolSlipClassifier::default();
        let commands = classifier.feed(&slip_frame(&payload));
        assert!(commands.is_empty());
    }

    #[test]
    fn classifier_extracts_change_baud_rate() {
        let mut payload = vec![0x00, ESPTOOL_CHANGE_BAUDRATE, 8, 0, 0, 0, 0, 0];
        payload.extend_from_slice(&460_800u32.to_le_bytes());
        payload.extend_from_slice(&115_200u32.to_le_bytes());
        let mut classifier = EsptoolSlipClassifier::default();
        let commands = classifier.feed(&slip_frame(&payload));

        assert_eq!(commands.len(), 1);
        assert_eq!(change_baud_rate(&commands[0]), Some(460_800));
    }

    #[test]
    fn slip_frame_reader_waits_for_complete_frame() {
        let frame = slip_frame(&[0x01, 0x02, SLIP_END, SLIP_ESC]);
        let split = frame.len() - 1;
        let mut reader = SlipFrameReader::default();

        assert!(reader.feed(&frame[..split]).is_empty());
        assert_eq!(
            reader.feed(&frame[split..]),
            vec![vec![0x01, 0x02, SLIP_END, SLIP_ESC]]
        );
    }

    #[test]
    fn usbmodem_source_port_uses_usb_jtag_reset() {
        let path = PathBuf::from("/dev/cu.usbmodem41301");
        assert_eq!(
            esp32_bootloader_reset_mode(Some(&path)),
            Esp32BootloaderResetMode::UsbJtagSerial
        );
    }

    #[test]
    fn espressif_manifest_matches_current_sdk_name() {
        let manifest = DeviceManifest {
            device_name: "esp-wiremux".to_string(),
            sdk_name: "esp-wiremux".to_string(),
            firmware_version: String::new(),
            protocol_version: 0,
            max_channels: 0,
            channels: Vec::new(),
            native_endianness: 0,
            max_payload_len: 0,
            transport: String::new(),
            feature_flags: 0,
            sdk_version: String::new(),
        };
        assert!(is_espressif_manifest(&manifest));
    }
}
