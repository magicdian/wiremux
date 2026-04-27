use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use wiremux::batch::{decode_batch, decode_batch_records, BATCH_PAYLOAD_TYPE, COMPRESSION_NONE};
use wiremux::codec::decompress;
use wiremux::envelope::{decode_envelope, MuxEnvelope};
use wiremux::frame::{FrameError, FrameScanner, StreamEvent};
use wiremux::manifest::{decode_manifest, DeviceManifest, MANIFEST_PAYLOAD_TYPE};

use super::{
    build_frame_error_to_io, build_input_frame, build_manifest_request_frame,
    create_diagnostics_file, open_available_port, printable_payload, write_envelope_diagnostics,
    TuiArgs,
};

const MAX_LINES: usize = 1000;

struct OutputLine {
    channel: Option<u32>,
    text: String,
}

struct App {
    lines: VecDeque<OutputLine>,
    input: String,
    filter: Option<u32>,
    prefix_pending: bool,
    status: String,
    connected_port: Option<String>,
    diagnostics_path: String,
    manifest: Option<DeviceManifest>,
    should_quit: bool,
}

impl App {
    fn new(diagnostics_path: String) -> Self {
        Self {
            lines: VecDeque::new(),
            input: String::new(),
            filter: None,
            prefix_pending: false,
            status: "connecting".to_string(),
            connected_port: None,
            diagnostics_path,
            manifest: None,
            should_quit: false,
        }
    }

    fn push_marker(&mut self, message: impl Into<String>) {
        self.push_line(None, format!("wiremux> {}", message.into()));
    }

    fn push_terminal(&mut self, bytes: &[u8]) {
        self.push_line(None, String::from_utf8_lossy(bytes).into_owned());
    }

    fn push_record(&mut self, envelope: &MuxEnvelope) {
        let text = String::from_utf8_lossy(&envelope.payload).into_owned();
        self.push_line(Some(envelope.channel_id), text);
    }

    fn push_line(&mut self, channel: Option<u32>, text: String) {
        for line in text.split_inclusive(['\n', '\r']) {
            let line = line.trim_end_matches(['\n', '\r']).to_string();
            self.lines.push_back(OutputLine {
                channel,
                text: line,
            });
        }
        if text.is_empty() || !text.ends_with(['\n', '\r']) {
            if let Some(last) = self.lines.back() {
                if !last.text.is_empty() {
                    // The split above already pushed the partial line.
                }
            }
        }
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    fn active_input_channel(&self) -> u8 {
        self.filter
            .and_then(|channel| u8::try_from(channel).ok())
            .unwrap_or(1)
    }

    fn filter_label(&self) -> String {
        match self.filter {
            Some(channel) => format!("ch{channel}"),
            None => "all".to_string(),
        }
    }

    fn manifest_label(&self) -> String {
        match &self.manifest {
            Some(manifest) => format!(
                "{} {} channels={} max_payload={}",
                empty_as_dash(&manifest.device_name),
                empty_as_dash(&manifest.firmware_version),
                manifest.channels.len(),
                manifest.max_payload_len
            ),
            None => "manifest pending".to_string(),
        }
    }
}

pub fn run(args: TuiArgs) -> io::Result<()> {
    let (diagnostics_path, diagnostics) = create_diagnostics_file(&args.port)?;
    let diagnostics_path_label = diagnostics_path.display().to_string();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(args, &mut terminal, diagnostics, diagnostics_path_label);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    args: TuiArgs,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut diagnostics: File,
    diagnostics_path: String,
) -> io::Result<()> {
    let reconnect_delay = Duration::from_millis(args.reconnect_delay_ms);
    let mut app = App::new(diagnostics_path);
    app.push_marker(format!(
        "diagnostics: {}; Ctrl-B 0..9 filters; Enter sends; Ctrl-C quits",
        app.diagnostics_path
    ));

    let mut serial = None;
    let mut scanner = FrameScanner::new(args.max_payload_len);
    let mut last_connect_attempt = Instant::now() - reconnect_delay;
    let mut buf = [0u8; 4096];

    loop {
        if serial.is_none() && last_connect_attempt.elapsed() >= reconnect_delay {
            last_connect_attempt = Instant::now();
            match open_available_port(&args.port, args.baud) {
                Ok((path, mut port)) => {
                    app.connected_port = Some(path.display().to_string());
                    app.status = format!("connected {}", path.display());
                    writeln!(diagnostics, "[wiremux] connected: {}", path.display())?;
                    let request = build_manifest_request_frame(args.max_payload_len)
                        .map_err(build_frame_error_to_io)?;
                    port.write_all(&request)?;
                    port.flush()?;
                    serial = Some(port);
                    scanner = FrameScanner::new(args.max_payload_len);
                    app.push_marker("manifest requested");
                }
                Err(err) => {
                    app.status = format!("waiting for {}: {err}", args.port.display());
                    writeln!(
                        diagnostics,
                        "[wiremux] waiting for {}: {err}",
                        args.port.display()
                    )?;
                }
            }
        }

        if let Some(port) = serial.as_mut() {
            match port.read(&mut buf) {
                Ok(0) => {
                    app.push_marker("disconnected: EOF");
                    serial = None;
                }
                Ok(read_len) => {
                    for event in scanner.push(&buf[..read_len]) {
                        handle_stream_event(&mut app, &mut diagnostics, event)?;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::TimedOut => {}
                Err(err) => {
                    app.push_marker(format!("disconnected: {err}"));
                    writeln!(diagnostics, "[wiremux] disconnected: {err}")?;
                    serial = None;
                }
            }
        }

        while event::poll(Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                handle_key(&mut app, serial.as_mut(), &args, key)?;
            }
        }

        terminal.draw(|frame| render(frame, &app))?;
        diagnostics.flush()?;

        if app.should_quit {
            break;
        }

        thread::sleep(Duration::from_millis(16));
    }

    Ok(())
}

fn handle_key(
    app: &mut App,
    serial: Option<&mut Box<dyn serialport::SerialPort>>,
    args: &TuiArgs,
    key: KeyEvent,
) -> io::Result<()> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
        app.prefix_pending = true;
        app.status = "prefix: press 0 for all or 1..9 for channel".to_string();
        return Ok(());
    }

    if app.prefix_pending {
        app.prefix_pending = false;
        match key.code {
            KeyCode::Char('0') => {
                app.filter = None;
                app.status = "filter: all".to_string();
            }
            KeyCode::Char(ch @ '1'..='9') => {
                let channel = ch.to_digit(10).unwrap_or(0);
                app.filter = Some(channel);
                app.status = format!("filter: ch{channel}");
            }
            KeyCode::Esc => {
                app.status = "prefix cancelled".to_string();
            }
            _ => {
                app.status = "unknown prefix command".to_string();
            }
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Char(ch) => app.input.push(ch),
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Enter => {
            if app.input.is_empty() {
                return Ok(());
            }
            let channel = app.active_input_channel();
            let frame = build_input_frame(channel, app.input.as_bytes(), args.max_payload_len)
                .map_err(build_frame_error_to_io)?;
            if let Some(port) = serial {
                port.write_all(&frame)?;
                port.flush()?;
                app.status = format!("sent {} bytes to ch{channel}", app.input.len());
                app.input.clear();
            } else {
                app.status = "not connected; input not sent".to_string();
            }
        }
        KeyCode::Esc => {
            app.input.clear();
        }
        _ => {}
    }

    Ok(())
}

fn handle_stream_event(
    app: &mut App,
    diagnostics: &mut File,
    event: StreamEvent,
) -> io::Result<()> {
    match event {
        StreamEvent::Terminal(bytes) => app.push_terminal(&bytes),
        StreamEvent::Frame(frame) => match decode_envelope(&frame.payload) {
            Ok(envelope) => handle_envelope(app, diagnostics, &envelope)?,
            Err(err) => {
                writeln!(
                    diagnostics,
                    "[wiremux] frame version={} flags={} payload_len={} envelope_decode_error={:?} payload={}",
                    frame.version,
                    frame.flags,
                    frame.payload.len(),
                    err,
                    printable_payload(&frame.payload)
                )?;
                app.push_marker("envelope decode error; details in diagnostics");
            }
        },
        StreamEvent::FrameError(FrameError::CrcMismatch {
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
            app.push_marker("crc error; details in diagnostics");
        }
    }

    Ok(())
}

fn handle_envelope(
    app: &mut App,
    diagnostics: &mut File,
    envelope: &MuxEnvelope,
) -> io::Result<()> {
    if envelope.payload_type == MANIFEST_PAYLOAD_TYPE {
        match decode_manifest(&envelope.payload) {
            Ok(manifest) => {
                app.push_marker(format!(
                    "manifest received: {} channels",
                    manifest.channels.len()
                ));
                app.manifest = Some(manifest);
            }
            Err(err) => {
                writeln!(diagnostics, "[wiremux] manifest_decode_error={err:?}")?;
                app.push_marker("manifest decode error; details in diagnostics");
            }
        }
        return Ok(());
    }

    if envelope.payload_type == BATCH_PAYLOAD_TYPE {
        return handle_batch(app, diagnostics, envelope);
    }

    write_envelope_diagnostics(diagnostics, envelope)?;
    app.push_record(envelope);
    Ok(())
}

fn handle_batch(app: &mut App, diagnostics: &mut File, envelope: &MuxEnvelope) -> io::Result<()> {
    let batch = match decode_batch(&envelope.payload) {
        Ok(batch) => batch,
        Err(err) => {
            writeln!(diagnostics, "[wiremux] batch_decode_error={err:?}")?;
            app.push_marker("batch decode error; details in diagnostics");
            return Ok(());
        }
    };
    let uncompressed_len = if batch.compression == COMPRESSION_NONE {
        batch.records.len()
    } else {
        batch.uncompressed_len as usize
    };
    let records_payload = match decompress(batch.compression, &batch.records, uncompressed_len) {
        Ok(records_payload) => records_payload,
        Err(err) => {
            writeln!(
                diagnostics,
                "[wiremux] batch compression={} raw_len={} decode_error={err:?}",
                batch.compression, batch.uncompressed_len
            )?;
            app.push_marker("batch payload decode error; details in diagnostics");
            return Ok(());
        }
    };
    let records = match decode_batch_records(&records_payload) {
        Ok(records) => records,
        Err(err) => {
            writeln!(diagnostics, "[wiremux] batch_records_decode_error={err:?}")?;
            app.push_marker("batch records decode error; details in diagnostics");
            return Ok(());
        }
    };
    writeln!(
        diagnostics,
        "[wiremux] batch records={} compression={} encoded_bytes={} raw_bytes={}",
        records.len(),
        batch.compression,
        batch.records.len(),
        records_payload.len()
    )?;
    for record in records {
        if record.payload_type == MANIFEST_PAYLOAD_TYPE {
            handle_envelope(app, diagnostics, &record)?;
        } else {
            write_envelope_diagnostics(diagnostics, &record)?;
            app.push_record(&record);
        }
    }
    Ok(())
}

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let height = chunks[0].height.saturating_sub(2) as usize;
    let visible = app
        .lines
        .iter()
        .filter(|line| app.filter.is_none() || line.channel == app.filter)
        .rev()
        .take(height)
        .collect::<Vec<_>>();
    let lines = visible
        .into_iter()
        .rev()
        .map(|line| {
            if let Some(channel) = line.channel {
                Line::from(vec![
                    Span::styled(
                        format!("ch{channel}> "),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(line.text.as_str()),
                ])
            } else {
                Line::from(line.text.as_str())
            }
        })
        .collect::<Vec<_>>();

    let output = Paragraph::new(lines)
        .block(Block::default().title("wiremux").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(output, chunks[0]);

    let status = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("filter ", Style::default().fg(Color::Yellow)),
            Span::raw(app.filter_label()),
            Span::raw("  "),
            Span::styled("input ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("ch{}", app.active_input_channel())),
            Span::raw("  "),
            Span::styled("device ", Style::default().fg(Color::Yellow)),
            Span::raw(app.manifest_label()),
        ]),
        Line::from(vec![
            Span::styled("status ", Style::default().fg(Color::Yellow)),
            Span::raw(app.status.as_str()),
        ]),
    ])
    .block(Block::default().title("status").borders(Borders::ALL));
    frame.render_widget(status, chunks[1]);

    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Green)),
        Span::raw(app.input.as_str()),
    ]))
    .block(
        Block::default()
            .title("input: Enter sends, Esc clears, Ctrl-C quits")
            .borders(Borders::ALL),
    );
    frame.render_widget(input, chunks[2]);
}

fn empty_as_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}
