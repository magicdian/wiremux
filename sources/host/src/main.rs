use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wiremux::envelope::{
    decode_envelope, encode_envelope, MuxEnvelope, DIRECTION_INPUT, PAYLOAD_KIND_TEXT,
};
use wiremux::frame::{
    build_frame_payload_with_max, BuildFrameError, FrameError, FrameScanner, StreamEvent,
    DEFAULT_MAX_PAYLOAD_LEN,
};

#[derive(Debug)]
enum CliCommand {
    Listen(ListenArgs),
    Send(SendArgs),
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
    }
}

fn listen(args: ListenArgs) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let reconnect_delay = Duration::from_millis(args.reconnect_delay_ms);

    writeln!(
        stdout,
        "[wiremux] listening on {} at {} baud; reconnect_delay={}ms",
        args.port.display(),
        args.baud,
        args.reconnect_delay_ms
    )?;

    loop {
        let (connected_port, mut input) = match open_available_port(&args.port, args.baud) {
            Ok((path, file)) => {
                writeln!(stdout, "[wiremux] connected: {}", path.display())?;
                stdout.flush()?;
                (path, file)
            }
            Err(err) => {
                writeln!(
                    stdout,
                    "[wiremux] waiting for {}: {}",
                    args.port.display(),
                    err
                )?;
                stdout.flush()?;
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
                stdout,
                "[wiremux] sent {} bytes to channel {}",
                line.len(),
                channel
            )?;
            stdout.flush()?;
        }

        let mut scanner = FrameScanner::new(args.max_payload_len);
        let mut buf = [0u8; 4096];

        loop {
            match input.read(&mut buf) {
                Ok(0) => {
                    writeln!(stdout, "\n[wiremux] disconnected: EOF")?;
                    break;
                }
                Ok(read_len) => {
                    for event in scanner.push(&buf[..read_len]) {
                        write_event(&mut stdout, event, args.channel)?;
                    }
                    stdout.flush()?;
                }
                Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                Err(err) => {
                    writeln!(
                        stdout,
                        "\n[wiremux] disconnected {}: {err}",
                        connected_port.display()
                    )?;
                    break;
                }
            }
        }

        for event in scanner.finish() {
            write_event(&mut stdout, event, args.channel)?;
        }
        stdout.flush()?;

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

fn open_available_port(
    requested: &Path,
    baud: u32,
) -> io::Result<(PathBuf, Box<dyn serialport::SerialPort>)> {
    let mut last_err = None;

    for candidate in port_candidates(requested) {
        match open_serial_port(&candidate, baud) {
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

fn open_serial_port(path: &Path, baud: u32) -> io::Result<Box<dyn serialport::SerialPort>> {
    let path = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "serial path is not UTF-8"))?;
    serialport::new(path, baud)
        .timeout(Duration::from_millis(100))
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

fn write_event<W: Write>(
    out: &mut W,
    event: StreamEvent,
    channel_filter: Option<u32>,
) -> io::Result<()> {
    match event {
        StreamEvent::Terminal(bytes) => {
            if channel_filter.is_none() {
                out.write_all(&bytes)?;
            }
            Ok(())
        }
        StreamEvent::Frame(frame) => match decode_envelope(&frame.payload) {
            Ok(envelope) => {
                if channel_filter.is_some_and(|channel| channel != envelope.channel_id) {
                    return Ok(());
                }
                writeln!(
                    out,
                    "\n[wiremux] ch={} dir={} seq={} ts={} kind={} type={} flags={} payload={}",
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
            Err(err) => writeln!(
                out,
                "\n[wiremux] frame version={} flags={} payload_len={} envelope_decode_error={:?} payload={}",
                frame.version,
                frame.flags,
                frame.payload.len(),
                err,
                printable_payload(&frame.payload)
            ),
        },
        StreamEvent::FrameError(FrameError::CrcMismatch {
            version,
            flags,
            payload_len,
            expected_crc,
            actual_crc,
        }) => {
            if channel_filter.is_none() {
                writeln!(
                    out,
                    "\n[wiremux] crc_error version={version} flags={flags} payload_len={payload_len} expected=0x{expected_crc:08x} actual=0x{actual_crc:08x}"
                )?;
            }
            Ok(())
        }
    }
}

fn build_input_frame(
    channel: u8,
    payload: &[u8],
    max_payload_len: usize,
) -> Result<Vec<u8>, BuildFrameError> {
    let envelope = MuxEnvelope {
        channel_id: u32::from(channel),
        direction: DIRECTION_INPUT,
        sequence: 1,
        timestamp_us: now_micros(),
        kind: PAYLOAD_KIND_TEXT,
        payload_type: String::new(),
        payload: payload.to_vec(),
        flags: 0,
    };
    let payload = encode_envelope(&envelope);
    build_frame_payload_with_max(0, &payload, max_payload_len)
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
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
    "usage:\n  wiremux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id] [--line text] [--send-channel id]\n  wiremux send --port <path> --channel <id> --line <text> [--baud 115200] [--max-payload bytes]".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        build_input_frame, paired_tty_cu_path, parse_args, printable_payload, usbmodem_fragment,
        CliCommand,
    };
    use super::{port_candidates, requested_file_name_starts_with};
    use std::path::PathBuf;
    use wiremux::envelope::{decode_envelope, DIRECTION_INPUT, PAYLOAD_KIND_TEXT};
    use wiremux::frame::{FrameScanner, StreamEvent};

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
        assert_eq!(
            args.max_payload_len,
            wiremux::frame::DEFAULT_MAX_PAYLOAD_LEN
        );
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
        let frame = build_input_frame(1, b"help", wiremux::frame::DEFAULT_MAX_PAYLOAD_LEN)
            .expect("valid input frame");
        let mut scanner = FrameScanner::default();
        let events = scanner.push(&frame);
        assert_eq!(events.len(), 1);

        let StreamEvent::Frame(frame) = &events[0] else {
            panic!("expected frame event");
        };
        let envelope = decode_envelope(&frame.payload).expect("valid envelope");
        assert_eq!(envelope.channel_id, 1);
        assert_eq!(envelope.direction, DIRECTION_INPUT);
        assert_eq!(envelope.kind, PAYLOAD_KIND_TEXT);
        assert_eq!(envelope.payload, b"help");
    }
}
