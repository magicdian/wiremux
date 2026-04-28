use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremux::host_session::{
    self, display_channel_name, BuildFrameError, DeviceManifest, HostDecodeStage, HostEvent,
    HostSession, MuxEnvelope, PassthroughPolicy, ProtocolCompatibilityKind,
    CHANNEL_INTERACTION_PASSTHROUGH, DEFAULT_MAX_PAYLOAD_LEN, NEWLINE_POLICY_CR,
    NEWLINE_POLICY_CRLF, NEWLINE_POLICY_LF,
};

mod tui;

const PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS: u64 = 750;
const DEFAULT_SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(100);
const INTERACTIVE_SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(5);

#[derive(Debug)]
enum CliCommand {
    Listen(ListenArgs),
    Send(SendArgs),
    Passthrough(PassthroughArgs),
    Tui(TuiArgs),
}

#[derive(Debug)]
struct ListenArgs {
    port: PathBuf,
    baud: u32,
    max_payload_len: usize,
    reconnect_delay_ms: u64,
    channel: Option<u32>,
    send_channel: Option<u8>,
    line: Option<String>,
}

#[derive(Debug)]
struct SendArgs {
    port: PathBuf,
    baud: u32,
    max_payload_len: usize,
    channel: u8,
    line: String,
}

#[derive(Debug)]
struct PassthroughArgs {
    port: PathBuf,
    baud: u32,
    max_payload_len: usize,
    channel: u8,
}

#[derive(Debug, Clone)]
struct TuiArgs {
    port: PathBuf,
    baud: u32,
    max_payload_len: usize,
    reconnect_delay_ms: u64,
}

struct DisplayOutput<W: Write> {
    out: W,
    channel_filter: Option<u32>,
    line_open: bool,
    line_channel: Option<u32>,
    channel_names: HashMap<u32, String>,
}

impl<W: Write> DisplayOutput<W> {
    fn new(out: W, channel_filter: Option<u32>) -> Self {
        Self {
            out,
            channel_filter,
            line_open: false,
            line_channel: None,
            channel_names: HashMap::new(),
        }
    }

    fn write_terminal(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.channel_filter.is_some() {
            return Ok(());
        }

        self.out.write_all(bytes)?;
        self.update_line_state(bytes, None);
        Ok(())
    }

    fn write_record(&mut self, envelope: &MuxEnvelope) -> io::Result<()> {
        if self
            .channel_filter
            .is_some_and(|channel| channel != envelope.channel_id)
        {
            return Ok(());
        }

        if self.channel_filter.is_some() {
            self.out.write_all(&envelope.payload)?;
            return Ok(());
        }

        self.prepare_unfiltered_record(envelope.channel_id)?;
        write!(self.out, "{}> ", self.channel_prefix(envelope.channel_id))?;
        self.line_open = true;
        self.line_channel = Some(envelope.channel_id);
        self.out.write_all(&envelope.payload)?;
        self.update_line_state(&envelope.payload, Some(envelope.channel_id));
        Ok(())
    }

    fn update_manifest(&mut self, manifest: &DeviceManifest) {
        self.channel_names.clear();
        for channel in &manifest.channels {
            if let Some(name) = display_channel_name(&channel.name) {
                self.channel_names.insert(channel.channel_id, name);
            }
        }
    }

    fn channel_prefix(&self, channel_id: u32) -> String {
        match self.channel_names.get(&channel_id) {
            Some(name) => format!("ch{channel_id}({name})"),
            None => format!("ch{channel_id}"),
        }
    }

    fn write_marker_line(&mut self, message: &str) -> io::Result<()> {
        if self.line_open {
            self.out.write_all(b"\n")?;
        }
        writeln!(self.out, "wiremux> {message}")?;
        self.line_open = false;
        self.line_channel = None;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn prepare_unfiltered_record(&mut self, channel_id: u32) -> io::Result<()> {
        if !self.line_open || self.line_channel == Some(channel_id) {
            return Ok(());
        }

        match self.line_channel {
            Some(previous_channel) => {
                self.out.write_all(b"\n")?;
                writeln!(
                    self.out,
                    "wiremux> continued after partial ch{} line",
                    previous_channel
                )?;
            }
            None => {
                self.out.write_all(b"\n")?;
            }
        }
        self.line_open = false;
        self.line_channel = None;
        Ok(())
    }

    fn update_line_state(&mut self, bytes: &[u8], channel: Option<u32>) {
        if bytes.is_empty() {
            return;
        }

        if bytes
            .last()
            .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
        {
            self.line_open = false;
            self.line_channel = None;
        } else {
            self.line_open = true;
            self.line_channel = channel;
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let Some(command) = parse_args(env::args().skip(1))? else {
        println!("{}", usage());
        return Ok(());
    };
    match command {
        CliCommand::Listen(args) => listen(args).map_err(|err| err.to_string()),
        CliCommand::Send(args) => send(args).map_err(|err| err.to_string()),
        CliCommand::Passthrough(args) => passthrough(args).map_err(|err| err.to_string()),
        CliCommand::Tui(args) => tui::run(args).map_err(|err| err.to_string()),
    }
}

fn listen(args: ListenArgs) -> io::Result<()> {
    let (diagnostics_path, mut diagnostics) = create_diagnostics_file(&args.port)?;
    let mut display = DisplayOutput::new(io::stdout().lock(), args.channel);
    let reconnect_delay = Duration::from_millis(args.reconnect_delay_ms);

    writeln!(
        diagnostics,
        "[wiremux] listening on {} at {} baud; reconnect_delay={}ms",
        args.port.display(),
        args.baud,
        args.reconnect_delay_ms
    )?;
    display.write_marker_line(&format!("diagnostics: {}", diagnostics_path.display()))?;
    display.flush()?;

    loop {
        let (connected_port, mut input) = match open_available_port(&args.port, args.baud) {
            Ok((path, file)) => {
                writeln!(diagnostics, "[wiremux] connected: {}", path.display())?;
                diagnostics.flush()?;
                (path, file)
            }
            Err(err) => {
                writeln!(
                    diagnostics,
                    "[wiremux] waiting for {}: {}",
                    args.port.display(),
                    err
                )?;
                diagnostics.flush()?;
                thread::sleep(reconnect_delay);
                continue;
            }
        };

        if let (Some(channel), Some(line)) = (args.send_channel, args.line.as_deref()) {
            let frame = build_input_frame(channel, line.as_bytes(), args.max_payload_len)
                .map_err(build_frame_error_to_io)?;
            input.write_all(&frame)?;
            input.flush()?;
            writeln!(
                diagnostics,
                "[wiremux] sent {} bytes to channel {}",
                line.len(),
                channel
            )?;
            diagnostics.flush()?;
        }

        let mut session = HostSession::new(args.max_payload_len).map_err(|status| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("host session init failed: {status}"),
            )
        })?;
        let mut buf = [0u8; 4096];

        loop {
            match input.read(&mut buf) {
                Ok(0) => {
                    writeln!(diagnostics, "[wiremux] disconnected: EOF")?;
                    break;
                }
                Ok(read_len) => {
                    for event in session.feed(&buf[..read_len]).map_err(|status| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("host session feed failed: {status}"),
                        )
                    })? {
                        write_event(&mut display, &mut diagnostics, event)?;
                    }
                    display.flush()?;
                    diagnostics.flush()?;
                }
                Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                Err(err) => {
                    writeln!(
                        diagnostics,
                        "[wiremux] disconnected {}: {err}",
                        connected_port.display()
                    )?;
                    break;
                }
            }
        }

        for event in session.finish().map_err(|status| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("host session finish failed: {status}"),
            )
        })? {
            write_event(&mut display, &mut diagnostics, event)?;
        }
        display.flush()?;
        diagnostics.flush()?;

        thread::sleep(reconnect_delay);
    }
}

fn send(args: SendArgs) -> io::Result<()> {
    let (connected_port, mut output) = open_available_port(&args.port, args.baud)?;
    let frame = build_input_frame(args.channel, args.line.as_bytes(), args.max_payload_len)
        .map_err(build_frame_error_to_io)?;
    output.write_all(&frame)?;
    output.flush()?;
    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "[wiremux] sent {} bytes to channel {} on {}",
        args.line.len(),
        args.channel,
        connected_port.display()
    )?;
    Ok(())
}

fn passthrough(args: PassthroughArgs) -> io::Result<()> {
    let (diagnostics_path, mut diagnostics) = create_diagnostics_file(&args.port)?;
    let (connected_port, mut port) =
        open_available_port_with_timeout(&args.port, args.baud, INTERACTIVE_SERIAL_READ_TIMEOUT)?;
    writeln!(
        diagnostics,
        "[wiremux] passthrough connected: {} channel={}",
        connected_port.display(),
        args.channel
    )?;

    let request =
        build_manifest_request_frame(args.max_payload_len).map_err(build_frame_error_to_io)?;
    port.write_all(&request)?;
    port.flush()?;

    {
        let mut stdout = io::stdout().lock();
        writeln!(
            stdout,
            "wiremux> diagnostics: {}; passthrough ch{}; Ctrl-] or Esc x quits",
            diagnostics_path.display(),
            args.channel
        )?;
        stdout.flush()?;
    }

    enable_raw_mode()?;
    let result = passthrough_loop(args, &mut port, &mut diagnostics);
    disable_raw_mode()?;
    result
}

fn passthrough_loop(
    args: PassthroughArgs,
    port: &mut Box<dyn serialport::SerialPort>,
    diagnostics: &mut File,
) -> io::Result<()> {
    let mut session = HostSession::new(args.max_payload_len).map_err(|status| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("host session init failed: {status}"),
        )
    })?;
    let mut manifest = None;
    let mut stdout = io::stdout().lock();
    let mut buf = [0u8; 4096];
    let mut exit_escape_started_at = None;

    loop {
        match port.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(read_len) => {
                for event in session.feed(&buf[..read_len]).map_err(|status| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("host session feed failed: {status}"),
                    )
                })? {
                    handle_passthrough_event(
                        &mut stdout,
                        diagnostics,
                        &mut manifest,
                        u32::from(args.channel),
                        event,
                    )?;
                }
                stdout.flush()?;
                diagnostics.flush()?;
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
            Err(err) => return Err(err),
        }

        if exit_escape_started_at.is_some_and(|started_at: Instant| {
            started_at.elapsed() >= Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
        }) {
            send_passthrough_key(
                args.channel,
                KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
                manifest.as_ref(),
                port,
                args.max_payload_len,
            )?;
            exit_escape_started_at = None;
        }

        while event::poll(Duration::from_millis(1))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if is_passthrough_exit_key(key) || is_passthrough_meta_exit_key(key) {
                return Ok(());
            }

            if exit_escape_started_at.take().is_some() {
                if is_passthrough_escape_exit_suffix(key) {
                    return Ok(());
                }
                send_passthrough_key(
                    args.channel,
                    KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
                    manifest.as_ref(),
                    port,
                    args.max_payload_len,
                )?;
            }

            if key.code == KeyCode::Esc {
                exit_escape_started_at = Some(Instant::now());
                continue;
            }

            send_passthrough_key(
                args.channel,
                key,
                manifest.as_ref(),
                port,
                args.max_payload_len,
            )?;
        }
    }
}

fn send_passthrough_key(
    channel: u8,
    key: KeyEvent,
    manifest: Option<&DeviceManifest>,
    port: &mut Box<dyn serialport::SerialPort>,
    max_payload_len: usize,
) -> io::Result<()> {
    let policy = manifest
        .and_then(|manifest| passthrough_policy_for_channel(manifest, u32::from(channel)))
        .unwrap_or_default();
    if let Some(payload) = passthrough_key_payload(key, policy) {
        let frame = build_input_frame(channel, &payload, max_payload_len)
            .map_err(build_frame_error_to_io)?;
        port.write_all(&frame)?;
        port.flush()?;
    }
    Ok(())
}

fn handle_passthrough_event<W: Write, D: Write>(
    stdout: &mut W,
    diagnostics: &mut D,
    manifest_state: &mut Option<DeviceManifest>,
    channel_id: u32,
    event: HostEvent,
) -> io::Result<()> {
    match event {
        HostEvent::Terminal(bytes) => {
            writeln!(diagnostics, "[wiremux] terminal {}", printable_payload(&bytes))
        }
        HostEvent::Record(envelope) => {
            write_envelope_diagnostics(diagnostics, &envelope)?;
            if envelope.channel_id == channel_id {
                stdout.write_all(&envelope.payload)?;
            }
            Ok(())
        }
        HostEvent::Manifest(manifest) => {
            writeln!(
                diagnostics,
                "[wiremux] manifest received: {} channels",
                manifest.channels.len()
            )?;
            if !channel_supports_passthrough(&manifest, channel_id) {
                writeln!(
                    diagnostics,
                    "[wiremux] channel {} does not advertise passthrough; continuing by explicit command",
                    channel_id
                )?;
            }
            *manifest_state = Some(manifest);
            Ok(())
        }
        HostEvent::ProtocolCompatibility(compatibility) => {
            match compatibility.compatibility {
                ProtocolCompatibilityKind::Supported => writeln!(
                    diagnostics,
                    "[wiremux] protocol_api supported device={} host_min={} host_current={}",
                    compatibility.device_api_version,
                    compatibility.host_min_api_version,
                    compatibility.host_current_api_version
                ),
                ProtocolCompatibilityKind::UnsupportedNew => writeln!(
                    diagnostics,
                    "[wiremux] protocol_api unsupported_new device={} host_current={} action=upgrade_host_sdk",
                    compatibility.device_api_version,
                    compatibility.host_current_api_version
                ),
                ProtocolCompatibilityKind::UnsupportedOld => writeln!(
                    diagnostics,
                    "[wiremux] protocol_api unsupported_old device={} host_min={}",
                    compatibility.device_api_version,
                    compatibility.host_min_api_version
                ),
                ProtocolCompatibilityKind::Unknown(value) => writeln!(
                    diagnostics,
                    "[wiremux] protocol_api unknown compatibility={value}"
                ),
            }
        }
        HostEvent::BatchSummary(summary) => writeln!(
            diagnostics,
            "[wiremux] batch records={} compression={} encoded_bytes={} raw_bytes={}",
            summary.record_count, summary.compression, summary.encoded_bytes, summary.raw_bytes
        ),
        HostEvent::DecodeError(err) => writeln!(
            diagnostics,
            "[wiremux] decode_error stage={:?} status={} detail={} payload={}",
            err.stage,
            err.status,
            err.detail,
            printable_payload(&err.payload)
        ),
        HostEvent::CrcError(wiremux::host_session::HostCrcError {
            version,
            flags,
            payload_len,
            expected_crc,
            actual_crc,
        }) => writeln!(
            diagnostics,
            "[wiremux] crc_error version={version} flags={flags} payload_len={payload_len} expected=0x{expected_crc:08x} actual=0x{actual_crc:08x}"
        ),
    }
}

fn open_available_port(
    requested: &Path,
    baud: u32,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    open_available_port_with_timeout(requested, baud, DEFAULT_SERIAL_READ_TIMEOUT)
}

fn open_available_port_with_timeout(
    requested: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    let mut last_err = None;

    for candidate in port_candidates(requested) {
        match open_serial_port(&candidate, baud, read_timeout) {
            Ok(port) => return Ok((candidate, port)),
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no candidate ports found for {}", requested.display()),
        )
    }))
}

fn open_serial_port(
    path: &Path,
    baud: u32,
    read_timeout: Duration,
) -> io::Result<Box<dyn serialport::SerialPort>> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    serialport::new(path, baud)
        .timeout(read_timeout)
        .open()
        .map_err(|err| io::Error::other(err.to_string()))
}

fn port_candidates(requested: &Path) -> Vec<PathBuf> {
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

fn paired_tty_cu_path(path: &Path) -> Option<PathBuf> {
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

fn usbmodem_fragment(path: &Path) -> Option<&'static str> {
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

fn requested_file_name_starts_with(path: &Path, prefix: &str) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
}

fn create_diagnostics_file(requested_port: &Path) -> io::Result<(PathBuf, File)> {
    let dir = env::temp_dir().join("wiremux");
    fs::create_dir_all(&dir)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let filename = format!(
        "wiremux-{}-{:06}-{}.log",
        now.as_secs(),
        now.subsec_micros(),
        sanitize_port_for_filename(requested_port)
    );
    let path = dir.join(filename);
    let file = File::create(&path)?;
    Ok((path, file))
}

fn sanitize_port_for_filename(port: &Path) -> String {
    let value = port.to_string_lossy();
    let mut sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    while sanitized.starts_with('_') {
        sanitized.remove(0);
    }
    while sanitized.ends_with('_') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        "port".to_string()
    } else {
        sanitized
    }
}

fn write_event<W: Write, D: Write>(
    display: &mut DisplayOutput<W>,
    diagnostics: &mut D,
    event: HostEvent,
) -> io::Result<()> {
    match event {
        HostEvent::Terminal(bytes) => display.write_terminal(&bytes),
        HostEvent::Record(envelope) => {
            write_envelope_diagnostics(diagnostics, &envelope)?;
            display.write_record(&envelope)
        }
        HostEvent::Manifest(manifest) => {
            writeln!(
                diagnostics,
                "[wiremux] manifest received: {} channels",
                manifest.channels.len()
            )?;
            display.update_manifest(&manifest);
            Ok(())
        }
        HostEvent::ProtocolCompatibility(compatibility) => {
            write_protocol_compatibility(display, diagnostics, compatibility)
        }
        HostEvent::BatchSummary(summary) => {
            writeln!(
                diagnostics,
                "[wiremux] batch records={} compression={} encoded_bytes={} raw_bytes={}",
                summary.record_count, summary.compression, summary.encoded_bytes, summary.raw_bytes
            )?;
            Ok(())
        }
        HostEvent::DecodeError(err) => {
            writeln!(
                diagnostics,
                "[wiremux] decode_error stage={:?} status={} detail={} payload={}",
                err.stage,
                err.status,
                err.detail,
                printable_payload(&err.payload)
            )?;
            if display.channel_filter.is_none() {
                display.write_marker_line(decode_error_marker(err.stage))?;
            }
            Ok(())
        }
        HostEvent::CrcError(wiremux::host_session::HostCrcError {
            version,
            flags,
            payload_len,
            expected_crc,
            actual_crc,
        }) => {
            writeln!(
                diagnostics,
                "[wiremux] crc_error version={version} flags={flags} payload_len={payload_len} expected=0x{expected_crc:08x} actual=0x{actual_crc:08x}"
            )?;
            if display.channel_filter.is_none() {
                display.write_marker_line("crc error; details in diagnostics")?;
            }
            Ok(())
        }
    }
}

fn write_protocol_compatibility<W: Write, D: Write>(
    display: &mut DisplayOutput<W>,
    diagnostics: &mut D,
    compatibility: wiremux::host_session::ProtocolCompatibility,
) -> io::Result<()> {
    match compatibility.compatibility {
        ProtocolCompatibilityKind::Supported => {
            writeln!(
                diagnostics,
                "[wiremux] protocol_api supported device={} host_min={} host_current={}",
                compatibility.device_api_version,
                compatibility.host_min_api_version,
                compatibility.host_current_api_version
            )?;
        }
        ProtocolCompatibilityKind::UnsupportedNew => {
            writeln!(
                diagnostics,
                "[wiremux] protocol_api unsupported_new device={} host_current={} action=upgrade_host_sdk",
                compatibility.device_api_version, compatibility.host_current_api_version
            )?;
            if display.channel_filter.is_none() {
                display.write_marker_line("device protocol is newer; upgrade host SDK/tool")?;
            }
        }
        ProtocolCompatibilityKind::UnsupportedOld => {
            writeln!(
                diagnostics,
                "[wiremux] protocol_api unsupported_old device={} host_min={}",
                compatibility.device_api_version, compatibility.host_min_api_version
            )?;
            if display.channel_filter.is_none() {
                display.write_marker_line("device protocol is too old for this host")?;
            }
        }
        ProtocolCompatibilityKind::Unknown(value) => {
            writeln!(
                diagnostics,
                "[wiremux] protocol_api unknown compatibility={value}"
            )?;
        }
    }
    Ok(())
}

fn decode_error_marker(stage: HostDecodeStage) -> &'static str {
    match stage {
        HostDecodeStage::Envelope => "envelope decode error; details in diagnostics",
        HostDecodeStage::Manifest => "manifest decode error; details in diagnostics",
        HostDecodeStage::Batch => "batch decode error; details in diagnostics",
        HostDecodeStage::BatchRecords => "batch records decode error; details in diagnostics",
        HostDecodeStage::Compression => "batch payload decode error; details in diagnostics",
        HostDecodeStage::Unknown(_) => "protocol decode error; details in diagnostics",
    }
}

fn write_envelope_diagnostics<W: Write>(out: &mut W, envelope: &MuxEnvelope) -> io::Result<()> {
    writeln!(
        out,
        "[wiremux] ch={} dir={} seq={} ts={} kind={} type={} flags={} payload={}",
        envelope.channel_id,
        envelope.direction,
        envelope.sequence,
        envelope.timestamp_us,
        envelope.kind,
        printable_payload_type(&envelope.payload_type),
        envelope.flags,
        printable_payload(&envelope.payload)
    )
}

fn build_input_frame(
    channel: u8,
    payload: &[u8],
    max_payload_len: usize,
) -> Result<Vec<u8>, BuildFrameError> {
    host_session::build_input_frame(channel, payload, max_payload_len)
}

fn build_manifest_request_frame(max_payload_len: usize) -> Result<Vec<u8>, BuildFrameError> {
    host_session::build_manifest_request_frame(max_payload_len)
}

fn build_frame_error_to_io(err: BuildFrameError) -> io::Error {
    match err {
        BuildFrameError::PayloadTooLarge { len, max } => io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("input frame payload is too large: {len} bytes > {max} bytes"),
        ),
    }
}

fn printable_payload(payload: &[u8]) -> String {
    match std::str::from_utf8(payload) {
        Ok(text) => text.escape_default().to_string(),
        Err(_) => payload
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn printable_payload_type(payload_type: &str) -> &str {
    if payload_type.is_empty() {
        "-"
    } else {
        payload_type
    }
}

fn channel_supports_passthrough(manifest: &DeviceManifest, channel_id: u32) -> bool {
    manifest
        .channels
        .iter()
        .find(|channel| channel.channel_id == channel_id)
        .is_some_and(|channel| {
            channel.default_interaction_mode == CHANNEL_INTERACTION_PASSTHROUGH
                || channel
                    .interaction_modes
                    .contains(&CHANNEL_INTERACTION_PASSTHROUGH)
        })
}

fn passthrough_policy_for_channel(
    manifest: &DeviceManifest,
    channel_id: u32,
) -> Option<PassthroughPolicy> {
    manifest
        .channels
        .iter()
        .find(|channel| channel.channel_id == channel_id)
        .map(|channel| channel.passthrough_policy)
}

fn passthrough_key_payload(key: KeyEvent, policy: PassthroughPolicy) -> Option<Vec<u8>> {
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

fn is_passthrough_exit_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('\u{1d}'))
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char(']') | KeyCode::Char('}')))
}

fn is_passthrough_meta_exit_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && is_passthrough_escape_exit_suffix(key)
}

fn is_passthrough_escape_exit_suffix(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
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

fn parse_args<I>(args: I) -> Result<Option<CliCommand>, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().peekable();
    let command = match args.peek().map(String::as_str) {
        Some("listen") => {
            args.next();
            "listen"
        }
        Some("send") => {
            args.next();
            "send"
        }
        Some("passthrough" | "attach") => {
            args.next();
            "passthrough"
        }
        Some("tui") => {
            args.next();
            "tui"
        }
        Some("-h" | "--help") => return Ok(None),
        _ => "listen",
    };

    let mut port = None;
    let mut baud = 115_200;
    let mut max_payload_len = DEFAULT_MAX_PAYLOAD_LEN;
    let mut reconnect_delay_ms = 500;
    let mut channel = None;
    let mut send_channel = None;
    let mut line = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--port requires a value".to_string())?;
                port = Some(PathBuf::from(value));
            }
            "--baud" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--baud requires a value".to_string())?;
                baud = value
                    .parse()
                    .map_err(|_| format!("invalid --baud value: {value}"))?;
            }
            "--max-payload" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--max-payload requires a value".to_string())?;
                max_payload_len = value
                    .parse()
                    .map_err(|_| format!("invalid --max-payload value: {value}"))?;
            }
            "--reconnect-delay-ms" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--reconnect-delay-ms requires a value".to_string())?;
                reconnect_delay_ms = value
                    .parse()
                    .map_err(|_| format!("invalid --reconnect-delay-ms value: {value}"))?;
            }
            "--channel" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--channel requires a value".to_string())?;
                channel = Some(parse_channel(&value)?);
            }
            "--send-channel" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--send-channel requires a value".to_string())?;
                send_channel = Some(parse_channel(&value)?);
            }
            "--line" => {
                line = Some(
                    args.next()
                        .ok_or_else(|| "--line requires a value".to_string())?,
                );
            }
            "-h" | "--help" => return Ok(None),
            unknown => return Err(format!("unknown argument: {unknown}\n{}", usage())),
        }
    }

    let port = port.ok_or_else(usage)?;
    match command {
        "listen" => Ok(Some(CliCommand::Listen(ListenArgs {
            port,
            baud,
            max_payload_len,
            reconnect_delay_ms,
            channel: channel.map(u32::from),
            send_channel: line.as_ref().map(|_| send_channel.or(channel).unwrap_or(1)),
            line,
        }))),
        "send" => Ok(Some(CliCommand::Send(SendArgs {
            port,
            baud,
            max_payload_len,
            channel: channel.ok_or_else(|| "send requires --channel <id>".to_string())?,
            line: line.ok_or_else(|| "send requires --line <text>".to_string())?,
        }))),
        "passthrough" => {
            if send_channel.is_some() || line.is_some() {
                return Err(format!(
                    "passthrough does not accept --send-channel or --line\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Passthrough(PassthroughArgs {
                port,
                baud,
                max_payload_len,
                channel: channel
                    .ok_or_else(|| "passthrough requires --channel <id>".to_string())?,
            })))
        }
        "tui" => {
            if channel.is_some() || send_channel.is_some() || line.is_some() {
                return Err(format!(
                    "tui does not accept --channel, --send-channel, or --line\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Tui(TuiArgs {
                port,
                baud,
                max_payload_len,
                reconnect_delay_ms,
            })))
        }
        _ => unreachable!("command is normalized before parsing"),
    }
}

fn parse_channel(value: &str) -> Result<u8, String> {
    let channel: u16 = value
        .parse()
        .map_err(|_| format!("invalid --channel value: {value}"))?;
    u8::try_from(channel).map_err(|_| format!("invalid --channel value: {value}; must be 0..255"))
}

fn usage() -> String {
    "usage:\n  wiremux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id] [--line text] [--send-channel id]\n  wiremux send --port <path> --channel <id> --line <text> [--baud 115200] [--max-payload bytes]\n  wiremux passthrough --port <path> --channel <id> [--baud 115200] [--max-payload bytes]\n  wiremux tui --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500]".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        build_input_frame, build_manifest_request_frame, is_passthrough_exit_key,
        is_passthrough_meta_exit_key, paired_tty_cu_path, parse_args, passthrough_key_payload,
        printable_payload, sanitize_port_for_filename, usbmodem_fragment, write_event, CliCommand,
        DisplayOutput,
    };
    use super::{port_candidates, requested_file_name_starts_with};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::path::PathBuf;
    use wiremux::host_session::{
        BatchSummary, ChannelDescriptor, DeviceManifest, HostEvent, HostSession, MuxEnvelope,
        DEFAULT_MAX_PAYLOAD_LEN, DIRECTION_INPUT, DIRECTION_OUTPUT, MANIFEST_REQUEST_PAYLOAD_TYPE,
        PAYLOAD_KIND_CONTROL, PAYLOAD_KIND_TEXT,
    };

    fn output_envelope(channel_id: u32, payload: &[u8]) -> MuxEnvelope {
        MuxEnvelope {
            channel_id,
            direction: DIRECTION_OUTPUT,
            sequence: 1,
            timestamp_us: 10,
            kind: PAYLOAD_KIND_TEXT,
            payload_type: String::new(),
            payload: payload.to_vec(),
            flags: 0,
        }
    }

    fn manifest_with_channel_name(channel_id: u32, name: &str) -> DeviceManifest {
        DeviceManifest {
            device_name: String::new(),
            firmware_version: String::new(),
            protocol_version: 1,
            max_channels: 8,
            channels: vec![ChannelDescriptor {
                channel_id,
                name: name.to_string(),
                description: String::new(),
                directions: Vec::new(),
                payload_kinds: Vec::new(),
                payload_types: Vec::new(),
                flags: 0,
                default_payload_kind: 0,
                interaction_modes: Vec::new(),
                default_interaction_mode: 0,
                passthrough_policy: Default::default(),
            }],
            native_endianness: 0,
            max_payload_len: 512,
            transport: String::new(),
            feature_flags: 0,
            sdk_name: String::new(),
            sdk_version: String::new(),
        }
    }

    #[test]
    fn parses_required_port_with_defaults() {
        let command = parse_args(["--port", "/dev/tty.usbmodem2101"].map(String::from))
            .expect("args parse")
            .expect("valid args");
        let CliCommand::Listen(args) = command else {
            panic!("expected listen command");
        };

        assert_eq!(args.port, PathBuf::from("/dev/tty.usbmodem2101"));
        assert_eq!(args.baud, 115_200);
        assert_eq!(args.max_payload_len, DEFAULT_MAX_PAYLOAD_LEN);
        assert_eq!(args.reconnect_delay_ms, 500);
        assert_eq!(args.channel, None);
        assert_eq!(args.send_channel, None);
        assert_eq!(args.line, None);
    }

    #[test]
    fn parses_listen_subcommand_and_overrides() {
        let command = parse_args(
            [
                "listen",
                "--port",
                "/tmp/capture.bin",
                "--baud",
                "921600",
                "--max-payload",
                "4096",
                "--reconnect-delay-ms",
                "100",
                "--channel",
                "3",
                "--line",
                "help",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Listen(args) = command else {
            panic!("expected listen command");
        };

        assert_eq!(args.port, PathBuf::from("/tmp/capture.bin"));
        assert_eq!(args.baud, 921_600);
        assert_eq!(args.max_payload_len, 4096);
        assert_eq!(args.reconnect_delay_ms, 100);
        assert_eq!(args.channel, Some(3));
        assert_eq!(args.send_channel, Some(3));
        assert_eq!(args.line, Some("help".to_string()));
    }

    #[test]
    fn parses_listen_line_without_filter_as_console_input() {
        let command = parse_args(
            [
                "listen",
                "--port",
                "/dev/cu.usbmodem2101",
                "--line",
                "mux_log",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Listen(args) = command else {
            panic!("expected listen command");
        };

        assert_eq!(args.channel, None);
        assert_eq!(args.send_channel, Some(1));
        assert_eq!(args.line, Some("mux_log".to_string()));
    }

    #[test]
    fn parses_listen_line_with_explicit_send_channel() {
        let command = parse_args(
            [
                "listen",
                "--port",
                "/dev/cu.usbmodem2101",
                "--channel",
                "2",
                "--send-channel",
                "1",
                "--line",
                "mux_log",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Listen(args) = command else {
            panic!("expected listen command");
        };

        assert_eq!(args.channel, Some(2));
        assert_eq!(args.send_channel, Some(1));
        assert_eq!(args.line, Some("mux_log".to_string()));
    }

    #[test]
    fn parses_send_subcommand() {
        let command = parse_args(
            [
                "send",
                "--port",
                "/dev/cu.usbmodem2101",
                "--channel",
                "1",
                "--line",
                "help",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Send(args) = command else {
            panic!("expected send command");
        };

        assert_eq!(args.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.channel, 1);
        assert_eq!(args.line, "help");
    }

    #[test]
    fn parses_tui_subcommand() {
        let command = parse_args(
            ["tui", "--port", "/dev/cu.usbmodem2101", "--baud", "921600"].map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Tui(args) = command else {
            panic!("expected tui command");
        };

        assert_eq!(args.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.baud, 921_600);
        assert_eq!(args.max_payload_len, DEFAULT_MAX_PAYLOAD_LEN);
    }

    #[test]
    fn parses_passthrough_subcommand() {
        let command = parse_args(
            [
                "passthrough",
                "--port",
                "/dev/cu.usbmodem2101",
                "--channel",
                "1",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Passthrough(args) = command else {
            panic!("expected passthrough command");
        };

        assert_eq!(args.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.channel, 1);
    }

    #[test]
    fn rejects_tui_channel_filter_args() {
        let err = parse_args(
            ["tui", "--port", "/dev/cu.usbmodem2101", "--channel", "1"].map(String::from),
        )
        .expect_err("invalid tui args");

        assert!(err.contains("tui does not accept"));
    }

    #[test]
    fn requires_port() {
        let err = parse_args(["--baud", "115200"].map(String::from)).expect_err("missing port");

        assert!(err.contains("usage:"));
    }

    #[test]
    fn rejects_invalid_baud() {
        let err = parse_args(["--port", "/tmp/fake", "--baud", "fast"].map(String::from))
            .expect_err("invalid baud");

        assert!(err.contains("invalid --baud value"));
    }

    #[test]
    fn rejects_invalid_channel() {
        let err = parse_args(
            [
                "send",
                "--port",
                "/tmp/fake",
                "--channel",
                "300",
                "--line",
                "help",
            ]
            .map(String::from),
        )
        .expect_err("invalid channel");

        assert!(err.contains("invalid --channel value"));
    }

    #[test]
    fn rejects_missing_send_line() {
        let err = parse_args(["send", "--port", "/tmp/fake", "--channel", "1"].map(String::from))
            .expect_err("missing line");

        assert!(err.contains("send requires --line"));
    }

    #[test]
    fn help_is_not_an_error() {
        assert!(parse_args(["--help"].map(String::from))
            .expect("help parses")
            .is_none());
    }

    #[test]
    fn prints_utf8_payload_with_escape_sequences() {
        assert_eq!(printable_payload(b"hello\n"), "hello\\n");
    }

    #[test]
    fn prints_binary_payload_as_hex() {
        assert_eq!(printable_payload(&[0xff, 0x00, 0x10]), "ff 00 10");
    }

    #[test]
    fn filtered_display_writes_raw_payload_and_preserves_newlines() {
        let mut display = DisplayOutput::new(Vec::new(), Some(1));

        display
            .write_record(&output_envelope(1, b"line1\r\nline2\rline3\n"))
            .expect("write record");
        display
            .write_record(&output_envelope(2, b"hidden\n"))
            .expect("filtered record");

        assert_eq!(display.out, b"line1\r\nline2\rline3\n");
    }

    #[test]
    fn unfiltered_display_prefixes_record_once_and_preserves_payload_newlines() {
        let mut display = DisplayOutput::new(Vec::new(), None);

        display
            .write_record(&output_envelope(3, b"line1\r\nline2\n"))
            .expect("write record");

        assert_eq!(display.out, b"ch3> line1\r\nline2\n");
    }

    #[test]
    fn unfiltered_display_uses_manifest_channel_names() {
        let mut display = DisplayOutput::new(Vec::new(), None);
        let manifest = manifest_with_channel_name(4, "🚗🎒😄🔥");
        display.update_manifest(&manifest);

        display
            .write_record(&output_envelope(4, "你好 UTF-8 😄\n".as_bytes()))
            .expect("write record");

        assert_eq!(
            String::from_utf8(display.out).expect("utf8 output"),
            "ch4(🚗🎒😄)> 你好 UTF-8 😄\n"
        );
    }

    #[test]
    fn unfiltered_display_ignores_empty_manifest_channel_names() {
        let mut display = DisplayOutput::new(Vec::new(), None);
        let manifest = manifest_with_channel_name(4, "\n\r");
        display.update_manifest(&manifest);

        display
            .write_record(&output_envelope(4, b"plain\n"))
            .expect("write record");

        assert_eq!(display.out, b"ch4> plain\n");
    }

    #[test]
    fn unfiltered_display_marks_channel_switch_after_partial_line() {
        let mut display = DisplayOutput::new(Vec::new(), None);

        display
            .write_record(&output_envelope(1, b"booting"))
            .expect("write first record");
        display
            .write_record(&output_envelope(2, b"sensor ready\n"))
            .expect("write second record");

        assert_eq!(
            display.out,
            b"ch1> booting\nwiremux> continued after partial ch1 line\nch2> sensor ready\n"
        );
    }

    #[test]
    fn unfiltered_display_switches_channels_without_marker_after_newline() {
        let mut display = DisplayOutput::new(Vec::new(), None);

        display
            .write_record(&output_envelope(1, b"ready\n"))
            .expect("write first record");
        display
            .write_record(&output_envelope(2, b"sensor ready\n"))
            .expect("write second record");

        assert_eq!(display.out, b"ch1> ready\nch2> sensor ready\n");
    }

    #[test]
    fn sanitizes_port_for_diagnostics_filename() {
        assert_eq!(
            sanitize_port_for_filename(&PathBuf::from("/dev/cu.usbmodem2101")),
            "dev_cu.usbmodem2101"
        );
        assert_eq!(sanitize_port_for_filename(&PathBuf::from("/")), "port");
    }

    #[test]
    fn batch_event_writes_payloads_to_display_and_summary_to_diagnostics() {
        let records = vec![
            output_envelope(3, b"alpha\n"),
            output_envelope(2, b"beta\n"),
        ];
        let mut display = DisplayOutput::new(Vec::new(), None);
        let mut diagnostics = Vec::new();

        for event in [
            HostEvent::Record(records[0].clone()),
            HostEvent::Record(records[1].clone()),
            HostEvent::BatchSummary(BatchSummary {
                compression: 0,
                encoded_bytes: 0,
                raw_bytes: 0,
                record_count: records.len(),
            }),
        ] {
            write_event(&mut display, &mut diagnostics, event).expect("write batch event");
        }

        assert_eq!(display.out, b"ch3> alpha\nch2> beta\n");
        let diagnostics = String::from_utf8(diagnostics).expect("utf8 diagnostics");
        assert!(diagnostics.contains("[wiremux] batch records=2 compression=0"));
        assert!(diagnostics.contains("[wiremux] ch=3"));
        assert!(diagnostics.contains("[wiremux] ch=2"));
    }

    #[test]
    fn listen_manifest_event_updates_labels_without_displaying_payload() {
        let record = output_envelope(4, "demo 😄\n".as_bytes());
        let mut display = DisplayOutput::new(Vec::new(), None);
        let mut diagnostics = Vec::new();

        write_event(
            &mut display,
            &mut diagnostics,
            HostEvent::Manifest(manifest_with_channel_name(4, "🚗🎒😄")),
        )
        .expect("manifest event");
        assert!(display.out.is_empty());

        write_event(&mut display, &mut diagnostics, HostEvent::Record(record))
            .expect("record event");

        assert_eq!(
            String::from_utf8(display.out).expect("utf8 output"),
            "ch4(🚗🎒😄)> demo 😄\n"
        );
        assert!(String::from_utf8(diagnostics)
            .expect("utf8 diagnostics")
            .contains("manifest received: 1 channels"));
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
    fn keeps_requested_port_first_when_it_is_cu() {
        let candidates = port_candidates(&PathBuf::from("/dev/cu.usbmodem2101"));

        assert_eq!(candidates[0], PathBuf::from("/dev/cu.usbmodem2101"));
    }

    #[test]
    fn detects_tty_prefix() {
        assert!(requested_file_name_starts_with(
            &PathBuf::from("/dev/tty.usbmodem2101"),
            "tty."
        ));
        assert!(!requested_file_name_starts_with(
            &PathBuf::from("/dev/cu.usbmodem2101"),
            "tty."
        ));
    }

    #[test]
    fn detects_supported_usb_serial_fragments() {
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
    fn builds_input_frame_that_round_trips_through_scanner() {
        let frame =
            build_input_frame(1, b"help", DEFAULT_MAX_PAYLOAD_LEN).expect("valid input frame");
        let mut session = HostSession::new(DEFAULT_MAX_PAYLOAD_LEN).expect("session");
        let events = session.feed(&frame).expect("feed");
        assert_eq!(events.len(), 1);

        let HostEvent::Record(envelope) = &events[0] else {
            panic!("expected record event");
        };
        assert_eq!(envelope.channel_id, 1);
        assert_eq!(envelope.direction, DIRECTION_INPUT);
        assert_eq!(envelope.kind, PAYLOAD_KIND_TEXT);
        assert_eq!(envelope.payload, b"help");
    }

    #[test]
    fn builds_manifest_request_frame() {
        let frame =
            build_manifest_request_frame(DEFAULT_MAX_PAYLOAD_LEN).expect("valid request frame");
        let mut session = HostSession::new(DEFAULT_MAX_PAYLOAD_LEN).expect("session");
        let events = session.feed(&frame).expect("feed");
        assert_eq!(events.len(), 1);

        let HostEvent::Record(envelope) = &events[0] else {
            panic!("expected record event");
        };
        assert_eq!(envelope.channel_id, 0);
        assert_eq!(envelope.direction, DIRECTION_INPUT);
        assert_eq!(envelope.kind, PAYLOAD_KIND_CONTROL);
        assert_eq!(envelope.payload_type, MANIFEST_REQUEST_PAYLOAD_TYPE);
        assert!(envelope.payload.is_empty());
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
    fn passthrough_exit_key_accepts_crossterm_control_variants() {
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
    }

    #[test]
    fn passthrough_meta_exit_key_accepts_alt_x_variant() {
        assert!(is_passthrough_meta_exit_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::ALT
        )));
        assert!(is_passthrough_meta_exit_key(KeyEvent::new(
            KeyCode::Char('X'),
            KeyModifiers::ALT | KeyModifiers::SHIFT
        )));
        assert!(!is_passthrough_meta_exit_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::empty()
        )));
    }
}
