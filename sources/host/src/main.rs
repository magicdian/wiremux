use esp_serial_mux::envelope::decode_envelope;
use esp_serial_mux::frame::{FrameError, FrameScanner, StreamEvent, DEFAULT_MAX_PAYLOAD_LEN};
use std::env;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct Args {
    port: PathBuf,
    baud: u32,
    max_payload_len: usize,
    reconnect_delay_ms: u64,
    channel: Option<u32>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let Some(args) = parse_args(env::args().skip(1))? else {
        println!("{}", usage());
        return Ok(());
    };
    listen(args).map_err(|err| err.to_string())
}

fn listen(args: Args) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let reconnect_delay = Duration::from_millis(args.reconnect_delay_ms);

    writeln!(
        stdout,
        "[esp-serial-mux] listening on {} at {} baud; reconnect_delay={}ms",
        args.port.display(),
        args.baud,
        args.reconnect_delay_ms
    )?;

    loop {
        let (connected_port, mut input) = match open_available_port(&args.port, args.baud) {
            Ok((path, file)) => {
                writeln!(stdout, "[esp-serial-mux] connected: {}", path.display())?;
                stdout.flush()?;
                (path, file)
            }
            Err(err) => {
                writeln!(
                    stdout,
                    "[esp-serial-mux] waiting for {}: {}",
                    args.port.display(),
                    err
                )?;
                stdout.flush()?;
                thread::sleep(reconnect_delay);
                continue;
            }
        };

        let mut scanner = FrameScanner::new(args.max_payload_len);
        let mut buf = [0u8; 4096];

        loop {
            match input.read(&mut buf) {
                Ok(0) => {
                    writeln!(stdout, "\n[esp-serial-mux] disconnected: EOF")?;
                    break;
                }
                Ok(read_len) => {
                    for event in scanner.push(&buf[..read_len]) {
                        write_event(&mut stdout, event, args.channel)?;
                    }
                    stdout.flush()?;
                }
                Err(err) => {
                    writeln!(
                        stdout,
                        "\n[esp-serial-mux] disconnected {}: {err}",
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

fn open_available_port(requested: &Path, baud: u32) -> io::Result<(PathBuf, File)> {
    let mut last_err = None;

    for candidate in port_candidates(requested) {
        let _ = configure_tty_raw(&candidate, baud);
        match OpenOptions::new().read(true).write(true).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
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

fn configure_tty_raw(path: &Path, baud: u32) -> io::Result<()> {
    let mut command = Command::new("stty");

    if cfg!(target_os = "macos") || cfg!(target_os = "ios") || cfg!(target_os = "freebsd") {
        command.arg("-f");
    } else {
        command.arg("-F");
    }

    let status = command
        .arg(path)
        .arg(baud.to_string())
        .args(["raw", "-echo", "min", "1", "time", "0"])
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "stty failed for {} with status {status}",
            path.display()
        )))
    }
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
                    "\n[esp-serial-mux] ch={} dir={} seq={} ts={} kind={} flags={} payload={}",
                    envelope.channel_id,
                    envelope.direction,
                    envelope.sequence,
                    envelope.timestamp_us,
                    envelope.kind,
                    envelope.flags,
                    printable_payload(&envelope.payload)
                )
            }
            Err(err) => writeln!(
                out,
                "\n[esp-serial-mux] frame version={} flags={} payload_len={} envelope_decode_error={:?} payload={}",
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
                    "\n[esp-serial-mux] crc_error version={version} flags={flags} payload_len={payload_len} expected=0x{expected_crc:08x} actual=0x{actual_crc:08x}"
                )?;
            }
            Ok(())
        }
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

fn parse_args<I>(args: I) -> Result<Option<Args>, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().peekable();
    if matches!(args.peek().map(String::as_str), Some("listen")) {
        args.next();
    }

    let mut port = None;
    let mut baud = 115_200;
    let mut max_payload_len = DEFAULT_MAX_PAYLOAD_LEN;
    let mut reconnect_delay_ms = 500;
    let mut channel = None;

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
                channel = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --channel value: {value}"))?,
                );
            }
            "-h" | "--help" => return Ok(None),
            unknown => return Err(format!("unknown argument: {unknown}\n{}", usage())),
        }
    }

    Ok(Some(Args {
        port: port.ok_or_else(usage)?,
        baud,
        max_payload_len,
        reconnect_delay_ms,
        channel,
    }))
}

fn usage() -> String {
    "usage: esp-serial-mux [listen] --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id]".to_string()
}

#[cfg(test)]
mod tests {
    use super::{paired_tty_cu_path, parse_args, printable_payload, usbmodem_fragment};
    use super::{port_candidates, requested_file_name_starts_with};
    use std::path::PathBuf;

    #[test]
    fn parses_required_port_with_defaults() {
        let args = parse_args(["--port", "/dev/tty.usbmodem2101"].map(String::from))
            .expect("args parse")
            .expect("valid args");

        assert_eq!(args.port, PathBuf::from("/dev/tty.usbmodem2101"));
        assert_eq!(args.baud, 115_200);
        assert_eq!(
            args.max_payload_len,
            esp_serial_mux::frame::DEFAULT_MAX_PAYLOAD_LEN
        );
        assert_eq!(args.reconnect_delay_ms, 500);
        assert_eq!(args.channel, None);
    }

    #[test]
    fn parses_listen_subcommand_and_overrides() {
        let args = parse_args(
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
            ]
            .map(String::from),
        )
        .expect("args parse")
        .expect("valid args");

        assert_eq!(args.port, PathBuf::from("/tmp/capture.bin"));
        assert_eq!(args.baud, 921_600);
        assert_eq!(args.max_payload_len, 4096);
        assert_eq!(args.reconnect_delay_ms, 100);
        assert_eq!(args.channel, Some(3));
    }

    #[test]
    fn requires_port() {
        let err = parse_args(["--baud", "115200"].map(String::from)).expect_err("missing port");

        assert!(err.contains("usage: esp-serial-mux"));
    }

    #[test]
    fn rejects_invalid_baud() {
        let err = parse_args(["--port", "/tmp/fake", "--baud", "fast"].map(String::from))
            .expect_err("invalid baud");

        assert!(err.contains("invalid --baud value"));
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
}
