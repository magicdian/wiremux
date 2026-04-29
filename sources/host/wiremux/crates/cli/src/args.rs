use std::path::PathBuf;

use host_session::DEFAULT_MAX_PAYLOAD_LEN;
use interactive::InteractiveBackendMode;

#[derive(Debug)]
pub enum CliCommand {
    Listen(ListenArgs),
    Send(SendArgs),
    Passthrough(PassthroughArgs),
    Tui(tui::TuiArgs),
}

#[derive(Debug)]
pub struct ListenArgs {
    pub port: PathBuf,
    pub baud: u32,
    pub max_payload_len: usize,
    pub reconnect_delay_ms: u64,
    pub channel: Option<u32>,
    pub send_channel: Option<u8>,
    pub line: Option<String>,
}

#[derive(Debug)]
pub struct SendArgs {
    pub port: PathBuf,
    pub baud: u32,
    pub max_payload_len: usize,
    pub channel: u8,
    pub line: String,
}

#[derive(Debug)]
pub struct PassthroughArgs {
    pub port: PathBuf,
    pub baud: u32,
    pub max_payload_len: usize,
    pub channel: u8,
    pub interactive_backend: InteractiveBackendMode,
}

pub fn parse_args<I>(args: I) -> Result<Option<CliCommand>, String>
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
    let mut interactive_backend = InteractiveBackendMode::Auto;
    let mut tui_fps = None;

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
            "--interactive-backend" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--interactive-backend requires a value".to_string())?;
                interactive_backend = InteractiveBackendMode::parse(&value).ok_or_else(|| {
                    format!(
                        "invalid --interactive-backend value: {value}; expected auto, compat, or mio"
                    )
                })?;
            }
            "--tui-fps" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--tui-fps requires a value".to_string())?;
                let fps = value
                    .parse()
                    .map_err(|_| format!("invalid --tui-fps value: {value}"))?;
                if !matches!(fps, 60 | 120) {
                    return Err(format!(
                        "invalid --tui-fps value: {value}; expected 60 or 120"
                    ));
                }
                tui_fps = Some(fps);
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
        "listen" => {
            if tui_fps.is_some() || interactive_backend != InteractiveBackendMode::Auto {
                return Err(format!(
                    "listen does not accept --tui-fps or --interactive-backend\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Listen(ListenArgs {
                port,
                baud,
                max_payload_len,
                reconnect_delay_ms,
                channel: channel.map(u32::from),
                send_channel: line.as_ref().map(|_| send_channel.or(channel).unwrap_or(1)),
                line,
            })))
        }
        "send" => {
            if tui_fps.is_some()
                || interactive_backend != InteractiveBackendMode::Auto
                || reconnect_delay_ms != 500
            {
                return Err(format!(
                    "send does not accept --tui-fps, --interactive-backend, or --reconnect-delay-ms\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Send(SendArgs {
                port,
                baud,
                max_payload_len,
                channel: channel.ok_or_else(|| "send requires --channel <id>".to_string())?,
                line: line.ok_or_else(|| "send requires --line <text>".to_string())?,
            })))
        }
        "passthrough" => {
            if send_channel.is_some() || line.is_some() || tui_fps.is_some() {
                return Err(format!(
                    "passthrough does not accept --send-channel, --line, or --tui-fps\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Passthrough(PassthroughArgs {
                port,
                baud,
                max_payload_len,
                channel: channel
                    .ok_or_else(|| "passthrough requires --channel <id>".to_string())?,
                interactive_backend,
            })))
        }
        "tui" => {
            if channel.is_some() || send_channel.is_some() || line.is_some() {
                return Err(format!(
                    "tui does not accept --channel, --send-channel, or --line\n{}",
                    usage()
                ));
            }
            Ok(Some(CliCommand::Tui(tui::TuiArgs {
                port,
                baud,
                max_payload_len,
                reconnect_delay_ms,
                interactive_backend,
                tui_fps,
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

pub fn usage() -> String {
    "usage:\n  wiremux listen --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--channel id] [--line text] [--send-channel id]\n  wiremux send --port <path> --channel <id> --line <text> [--baud 115200] [--max-payload bytes]\n  wiremux passthrough --port <path> --channel <id> [--baud 115200] [--max-payload bytes] [--interactive-backend auto|compat|mio]\n  wiremux tui --port <path> [--baud 115200] [--max-payload bytes] [--reconnect-delay-ms 500] [--interactive-backend auto|compat|mio] [--tui-fps 60|120]".to_string()
}
