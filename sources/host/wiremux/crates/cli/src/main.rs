use cli::args::{parse_args, usage, CliCommand, ListenArgs, PassthroughArgs, SendArgs};
use cli::diagnostics::create_diagnostics_file;
use cli::display::DisplayOutput;
use cli::serial::open_available_port;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use host_session::{
    build_frame_error_to_io, build_input_frame, build_manifest_request_frame,
    channel_supports_passthrough, decode_error_marker, passthrough_policy_for_channel,
    printable_payload, write_envelope_diagnostics, DeviceManifest, HostCrcError, HostEvent,
    HostSession, ProtocolCompatibility, ProtocolCompatibilityKind,
};
use interactive::{
    is_passthrough_escape_exit_suffix, is_passthrough_exit_key, is_passthrough_meta_exit_key,
    passthrough_key_payload, INTERACTIVE_SERIAL_READ_TIMEOUT, PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS,
};
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

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
        CliCommand::Tui(args) => run_tui(args).map_err(|err| err.to_string()),
    }
}

fn run_tui(args: tui::TuiArgs) -> io::Result<()> {
    let (diagnostics_path, diagnostics) = create_diagnostics_file(&args.serial.port)?;
    tui::run(args, diagnostics_path.display().to_string(), diagnostics)
}

fn listen(args: ListenArgs) -> io::Result<()> {
    let (diagnostics_path, mut diagnostics) = create_diagnostics_file(&args.serial.port)?;
    let mut display = DisplayOutput::new(io::stdout().lock(), args.channel);
    let reconnect_delay = Duration::from_millis(args.reconnect_delay_ms);

    writeln!(
        diagnostics,
        "[wiremux] listening on {}; reconnect_delay={}ms",
        args.serial.summary(),
        args.reconnect_delay_ms
    )?;
    display.write_marker_line(&format!("diagnostics: {}", diagnostics_path.display()))?;
    display.flush()?;

    loop {
        let (connected_port, mut input) = match open_available_port(&args.serial) {
            Ok((path, file)) => {
                writeln!(diagnostics, "[wiremux] connected: {}", path.display())?;
                diagnostics.flush()?;
                (path, file)
            }
            Err(err) => {
                writeln!(
                    diagnostics,
                    "[wiremux] waiting for {}: {}",
                    args.serial.port.display(),
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
    let (connected_port, mut output) = open_available_port(&args.serial)?;
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
    let (diagnostics_path, mut diagnostics) = create_diagnostics_file(&args.serial.port)?;
    let (connected_port, mut backend) = interactive::open_interactive_backend(
        &args.serial,
        args.interactive_backend,
        INTERACTIVE_SERIAL_READ_TIMEOUT,
    )?;
    writeln!(
        diagnostics,
        "[wiremux] passthrough connected: {} channel={} backend={}",
        connected_port.display(),
        args.channel,
        backend.label()
    )?;

    let request =
        build_manifest_request_frame(args.max_payload_len).map_err(build_frame_error_to_io)?;
    backend.write_all(&request)?;
    backend.flush()?;

    {
        let mut stdout = io::stdout().lock();
        writeln!(
            stdout,
            "wiremux> diagnostics: {}; passthrough ch{}; backend {}; Ctrl-] or Esc x quits",
            diagnostics_path.display(),
            args.channel,
            backend.label()
        )?;
        stdout.flush()?;
    }

    enable_raw_mode()?;
    let result = passthrough_loop(args, &mut backend, &mut diagnostics);
    disable_raw_mode()?;
    result
}

fn passthrough_loop(
    args: PassthroughArgs,
    backend: &mut interactive::ConnectedInteractiveBackend,
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
    let mut exit_escape_started_at = None;

    loop {
        let deadline = exit_escape_started_at.map(|started_at: Instant| {
            started_at + Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
        });
        match backend.next_event(deadline)? {
            interactive::InteractiveEvent::SerialBytes(bytes) => {
                for event in session.feed(&bytes).map_err(|status| {
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
            interactive::InteractiveEvent::SerialEof => return Ok(()),
            interactive::InteractiveEvent::SerialError(err) => return Err(err),
            interactive::InteractiveEvent::Terminal(Event::Key(key)) => {
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
                        backend,
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
                    backend,
                    args.max_payload_len,
                )?;
            }
            interactive::InteractiveEvent::Terminal(_) => {}
            interactive::InteractiveEvent::Timeout => {
                if exit_escape_started_at.is_some() {
                    send_passthrough_key(
                        args.channel,
                        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
                        manifest.as_ref(),
                        backend,
                        args.max_payload_len,
                    )?;
                    exit_escape_started_at = None;
                }
            }
        }
    }
}

fn send_passthrough_key(
    channel: u8,
    key: KeyEvent,
    manifest: Option<&DeviceManifest>,
    port: &mut dyn Write,
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
        HostEvent::CrcError(HostCrcError {
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
        HostEvent::CrcError(HostCrcError {
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
    compatibility: ProtocolCompatibility,
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

#[cfg(test)]
mod tests {
    use super::{
        build_input_frame, build_manifest_request_frame, is_passthrough_exit_key,
        is_passthrough_meta_exit_key, passthrough_key_payload, printable_payload, write_event,
        CliCommand, DisplayOutput,
    };
    use cli::args::parse_args_with_config;
    use cli::diagnostics::sanitize_port_for_filename;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use host_session::{
        BatchSummary, ChannelDescriptor, DeviceManifest, HostEvent, HostSession, MuxEnvelope,
        DEFAULT_MAX_PAYLOAD_LEN, DIRECTION_INPUT, DIRECTION_OUTPUT, MANIFEST_REQUEST_PAYLOAD_TYPE,
        PAYLOAD_KIND_CONTROL, PAYLOAD_KIND_TEXT,
    };
    use interactive::{
        paired_tty_cu_path, port_candidates, requested_file_name_starts_with, usbmodem_fragment,
        HostConfig, InteractiveBackendMode,
    };
    use std::path::PathBuf;

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

    fn parse_test_args<I>(args: I) -> Result<Option<CliCommand>, String>
    where
        I: IntoIterator<Item = String>,
    {
        parse_args_with_config(args, HostConfig::default())
    }

    #[test]
    fn parses_required_port_with_defaults() {
        let command = parse_test_args(["--port", "/dev/tty.usbmodem2101"].map(String::from))
            .expect("args parse")
            .expect("valid args");
        let CliCommand::Listen(args) = command else {
            panic!("expected listen command");
        };

        assert_eq!(args.serial.port, PathBuf::from("/dev/tty.usbmodem2101"));
        assert_eq!(args.serial.baud, 115_200);
        assert_eq!(args.serial.data_bits, 8);
        assert_eq!(args.serial.stop_bits, 1);
        assert_eq!(args.max_payload_len, DEFAULT_MAX_PAYLOAD_LEN);
        assert_eq!(args.reconnect_delay_ms, 500);
        assert_eq!(args.channel, None);
        assert_eq!(args.send_channel, None);
        assert_eq!(args.line, None);
    }

    #[test]
    fn resolves_serial_profile_from_config_when_port_is_omitted() {
        let mut config = HostConfig::default();
        config.serial.port = Some(PathBuf::from("/dev/cu.configured"));
        config.serial.baud = 460_800;

        let command = parse_args_with_config(["tui"].map(String::from), config)
            .expect("args parse")
            .expect("valid args");
        let CliCommand::Tui(args) = command else {
            panic!("expected tui command");
        };

        assert_eq!(args.serial.port, PathBuf::from("/dev/cu.configured"));
        assert_eq!(args.serial.baud, 460_800);
    }

    #[test]
    fn parses_listen_subcommand_and_overrides() {
        let command = parse_test_args(
            [
                "listen",
                "--port",
                "/tmp/capture.bin",
                "--baud",
                "921600",
                "--data-bits",
                "7",
                "--stop-bits",
                "2",
                "--parity",
                "even",
                "--flow-control",
                "hardware",
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

        assert_eq!(args.serial.port, PathBuf::from("/tmp/capture.bin"));
        assert_eq!(args.serial.baud, 921_600);
        assert_eq!(args.serial.data_bits, 7);
        assert_eq!(args.serial.stop_bits, 2);
        assert_eq!(args.serial.parity.to_string(), "even");
        assert_eq!(args.serial.flow_control.to_string(), "hardware");
        assert_eq!(args.max_payload_len, 4096);
        assert_eq!(args.reconnect_delay_ms, 100);
        assert_eq!(args.channel, Some(3));
        assert_eq!(args.send_channel, Some(3));
        assert_eq!(args.line, Some("help".to_string()));
    }

    #[test]
    fn parses_listen_line_without_filter_as_console_input() {
        let command = parse_test_args(
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
        let command = parse_test_args(
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
        let command = parse_test_args(
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

        assert_eq!(args.serial.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.channel, 1);
        assert_eq!(args.line, "help");
    }

    #[test]
    fn parses_tui_subcommand() {
        let command = parse_test_args(
            ["tui", "--port", "/dev/cu.usbmodem2101", "--baud", "921600"].map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Tui(args) = command else {
            panic!("expected tui command");
        };

        assert_eq!(args.serial.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.serial.baud, 921_600);
        assert_eq!(args.max_payload_len, DEFAULT_MAX_PAYLOAD_LEN);
        assert_eq!(args.interactive_backend, InteractiveBackendMode::Auto);
        assert_eq!(args.tui_fps, None);
    }

    #[test]
    fn parses_tui_interactive_backend_and_fps() {
        let command = parse_test_args(
            [
                "tui",
                "--port",
                "/dev/cu.usbmodem2101",
                "--interactive-backend",
                "compat",
                "--tui-fps",
                "120",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Tui(args) = command else {
            panic!("expected tui command");
        };

        assert_eq!(args.interactive_backend, InteractiveBackendMode::Compat);
        assert_eq!(args.tui_fps, Some(120));
    }

    #[test]
    fn parses_passthrough_subcommand() {
        let command = parse_test_args(
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

        assert_eq!(args.serial.port, PathBuf::from("/dev/cu.usbmodem2101"));
        assert_eq!(args.channel, 1);
        assert_eq!(args.interactive_backend, InteractiveBackendMode::Auto);
    }

    #[test]
    fn parses_passthrough_interactive_backend() {
        let command = parse_test_args(
            [
                "passthrough",
                "--port",
                "/dev/cu.usbmodem2101",
                "--channel",
                "1",
                "--interactive-backend",
                "mio",
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");
        let CliCommand::Passthrough(args) = command else {
            panic!("expected passthrough command");
        };

        assert_eq!(args.interactive_backend, InteractiveBackendMode::Mio);
    }

    #[test]
    fn rejects_invalid_tui_fps() {
        let err = parse_test_args(
            ["tui", "--port", "/dev/cu.usbmodem2101", "--tui-fps", "144"].map(String::from),
        )
        .expect_err("invalid fps");

        assert!(err.contains("invalid --tui-fps value"));
    }

    #[test]
    fn rejects_invalid_interactive_backend() {
        let err = parse_test_args(
            [
                "tui",
                "--port",
                "/dev/cu.usbmodem2101",
                "--interactive-backend",
                "fast",
            ]
            .map(String::from),
        )
        .expect_err("invalid backend");

        assert!(err.contains("invalid --interactive-backend value"));
    }

    #[test]
    fn rejects_tui_channel_filter_args() {
        let err = parse_test_args(
            ["tui", "--port", "/dev/cu.usbmodem2101", "--channel", "1"].map(String::from),
        )
        .expect_err("invalid tui args");

        assert!(err.contains("tui does not accept"));
    }

    #[test]
    fn requires_port() {
        let err =
            parse_test_args(["--baud", "115200"].map(String::from)).expect_err("missing port");

        assert!(err.contains("serial port is required"));
    }

    #[test]
    fn rejects_invalid_baud() {
        let err = parse_test_args(["--port", "/tmp/fake", "--baud", "fast"].map(String::from))
            .expect_err("invalid baud");

        assert!(err.contains("invalid --baud value"));
    }

    #[test]
    fn rejects_invalid_channel() {
        let err = parse_test_args(
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
        let err =
            parse_test_args(["send", "--port", "/tmp/fake", "--channel", "1"].map(String::from))
                .expect_err("missing line");

        assert!(err.contains("send requires --line"));
    }

    #[test]
    fn help_is_not_an_error() {
        assert!(parse_test_args(["--help"].map(String::from))
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
