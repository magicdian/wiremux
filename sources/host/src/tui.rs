use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Terminal;
use wiremux::host_session::{
    display_channel_name, DeviceManifest, HostDecodeStage, HostEvent, HostSession, MuxEnvelope,
    ProtocolCompatibilityKind,
};

use super::{
    build_frame_error_to_io, build_input_frame, build_manifest_request_frame,
    channel_supports_passthrough, create_diagnostics_file, is_passthrough_exit_key,
    open_available_port, passthrough_key_payload, passthrough_policy_for_channel,
    printable_payload, write_envelope_diagnostics, TuiArgs,
};

const MAX_LINES: usize = 1000;
const WHEEL_SCROLL_LINES: usize = 3;

struct OutputLine {
    channel: Option<u32>,
    text: String,
    complete: bool,
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
    scroll_offset: usize,
    empty_enter_restore_count: u8,
    dragging_scrollbar: bool,
    stream_cr_pending_channel: Option<u32>,
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
            scroll_offset: 0,
            empty_enter_restore_count: 0,
            dragging_scrollbar: false,
            stream_cr_pending_channel: None,
        }
    }

    fn push_marker(&mut self, message: impl Into<String>) {
        self.push_line(None, format!("wiremux> {}", message.into()));
    }

    fn push_terminal(&mut self, bytes: &[u8]) {
        self.push_line(None, String::from_utf8_lossy(bytes).into_owned());
    }

    fn push_record(&mut self, envelope: &MuxEnvelope) {
        if self.channel_is_passthrough(envelope.channel_id) {
            if u32::from(self.active_input_channel()) == envelope.channel_id {
                self.follow_live_output();
            }
            let text = String::from_utf8_lossy(&envelope.payload).into_owned();
            self.push_stream(Some(envelope.channel_id), &text);
        } else {
            let text = String::from_utf8_lossy(&envelope.payload).into_owned();
            self.push_line(Some(envelope.channel_id), text);
        }
    }

    fn push_line(&mut self, channel: Option<u32>, text: String) {
        for line in text.split_inclusive(['\n', '\r']) {
            let complete = line.ends_with(['\n', '\r']);
            let line = line.trim_end_matches(['\n', '\r']).to_string();
            let output_line = OutputLine {
                channel,
                text: line,
                complete,
            };
            if self.scroll_offset > 0 && self.line_matches_filter(&output_line) {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            self.lines.push_back(output_line);
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

    fn push_stream(&mut self, channel: Option<u32>, text: &str) {
        if text.is_empty() {
            return;
        }

        for ch in text.chars() {
            self.push_stream_char(channel, ch);
        }

        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    fn push_stream_char(&mut self, channel: Option<u32>, ch: char) {
        if ch == '\n' && channel.is_some() && self.stream_cr_pending_channel == channel {
            self.stream_cr_pending_channel = None;
            return;
        }

        if ch == '\r' || ch == '\n' {
            self.stream_cr_pending_channel = if ch == '\r' { channel } else { None };
            self.complete_stream_line(channel);
            return;
        }

        self.stream_cr_pending_channel = None;
        if ch == '\u{8}' || ch == '\u{7f}' {
            self.backspace_stream_line(channel);
            return;
        }
        if ch.is_control() && ch != '\t' {
            return;
        }

        let mut buf = [0; 4];
        self.append_stream_segment(channel, ch.encode_utf8(&mut buf), false);
    }

    fn complete_stream_line(&mut self, channel: Option<u32>) {
        if let Some(line) = self.incomplete_stream_line_mut(channel) {
            line.complete = true;
            return;
        }
        self.append_stream_segment(channel, "", true);
    }

    fn backspace_stream_line(&mut self, channel: Option<u32>) {
        if let Some(line) = self.incomplete_stream_line_mut(channel) {
            line.text.pop();
        }
    }

    fn append_stream_segment(&mut self, channel: Option<u32>, segment: &str, complete: bool) {
        if let Some(line) = self.incomplete_stream_line_mut(channel) {
            line.text.push_str(segment);
            line.complete = complete;
            return;
        }

        let output_line = OutputLine {
            channel,
            text: segment.to_string(),
            complete,
        };
        if self.scroll_offset > 0 && self.line_matches_filter(&output_line) {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
        self.lines.push_back(output_line);
    }

    fn incomplete_stream_line_mut(&mut self, channel: Option<u32>) -> Option<&mut OutputLine> {
        self.lines
            .iter_mut()
            .rev()
            .find(|line| line.channel == channel && !line.complete)
    }

    fn active_input_channel(&self) -> u8 {
        self.filter
            .and_then(|channel| u8::try_from(channel).ok())
            .unwrap_or(1)
    }

    fn active_input_is_passthrough(&self) -> bool {
        self.manifest.as_ref().is_some_and(|manifest| {
            channel_supports_passthrough(manifest, u32::from(self.active_input_channel()))
        })
    }

    fn channel_is_passthrough(&self, channel_id: u32) -> bool {
        self.manifest
            .as_ref()
            .is_some_and(|manifest| channel_supports_passthrough(manifest, channel_id))
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

    fn channel_prefix(&self, channel_id: u32) -> String {
        let name = self.manifest.as_ref().and_then(|manifest| {
            manifest
                .channels
                .iter()
                .find(|channel| channel.channel_id == channel_id)
                .and_then(|channel| display_channel_name(&channel.name))
        });
        match name {
            Some(name) => format!("ch{channel_id}({name})"),
            None => format!("ch{channel_id}"),
        }
    }

    fn filtered_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| self.line_matches_filter(line))
            .count()
    }

    fn line_matches_filter(&self, line: &OutputLine) -> bool {
        self.filter.is_none() || line.channel == self.filter
    }

    fn max_scroll_offset(&self, output_height: usize) -> usize {
        max_scroll_offset(self.filtered_line_count(), output_height)
    }

    fn scroll_up(&mut self, output_height: usize) {
        self.empty_enter_restore_count = 0;
        let max_offset = self.max_scroll_offset(output_height);
        if max_offset == 0 {
            self.scroll_offset = 0;
            return;
        }
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(WHEEL_SCROLL_LINES)
            .min(max_offset);
        self.status = format!(
            "scrollback paused: {} lines from bottom",
            self.scroll_offset
        );
    }

    fn scroll_down(&mut self, output_height: usize) {
        self.empty_enter_restore_count = 0;
        let max_offset = self.max_scroll_offset(output_height);
        self.scroll_offset = self.scroll_offset.min(max_offset);
        self.scroll_offset = self.scroll_offset.saturating_sub(WHEEL_SCROLL_LINES);
        if self.scroll_offset == 0 {
            self.status = "scrollback: following live output".to_string();
        } else {
            self.status = format!(
                "scrollback paused: {} lines from bottom",
                self.scroll_offset
            );
        }
    }

    fn restore_auto_follow(&mut self) {
        self.follow_live_output();
        self.status = "scrollback: following live output".to_string();
    }

    fn follow_live_output(&mut self) {
        self.scroll_offset = 0;
        self.empty_enter_restore_count = 0;
        self.dragging_scrollbar = false;
    }

    fn set_scroll_offset(&mut self, scroll_offset: usize, output_height: usize) {
        self.empty_enter_restore_count = 0;
        self.scroll_offset = scroll_offset.min(self.max_scroll_offset(output_height));
        if self.scroll_offset == 0 {
            self.status = "scrollbar: following live output".to_string();
        } else {
            self.status = format!("scrollbar: {} lines from bottom", self.scroll_offset);
        }
    }

    fn reset_empty_enter_restore(&mut self) {
        self.empty_enter_restore_count = 0;
    }

    fn handle_empty_enter_restore(&mut self) {
        if self.scroll_offset == 0 {
            self.empty_enter_restore_count = 0;
            return;
        }
        self.empty_enter_restore_count = self.empty_enter_restore_count.saturating_add(1);
        if self.empty_enter_restore_count >= 2 {
            self.restore_auto_follow();
        } else {
            self.status = "scrollback paused: press Enter again to follow live output".to_string();
        }
    }
}

pub fn run(args: TuiArgs) -> io::Result<()> {
    let (diagnostics_path, diagnostics) = create_diagnostics_file(&args.port)?;
    let diagnostics_path_label = diagnostics_path.display().to_string();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(args, &mut terminal, diagnostics, diagnostics_path_label);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
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
    let mut host_session = HostSession::new(args.max_payload_len).map_err(|status| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("host session init failed: {status}"),
        )
    })?;
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
                    host_session = HostSession::new(args.max_payload_len).map_err(|status| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("host session init failed: {status}"),
                        )
                    })?;
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
                    for event in host_session.feed(&buf[..read_len]).map_err(|status| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("host session feed failed: {status}"),
                        )
                    })? {
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
            match event::read()? {
                Event::Key(key) => handle_key(&mut app, serial.as_mut(), &args, key)?,
                Event::Mouse(mouse) => {
                    let output_area = output_area_from_terminal_area(terminal.size()?.into());
                    handle_mouse(&mut app, output_area, mouse);
                }
                _ => {}
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

    if is_passthrough_exit_key(key) {
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
                app.restore_auto_follow();
                app.status = "filter: all".to_string();
            }
            KeyCode::Char(ch @ '1'..='9') => {
                let channel = ch.to_digit(10).unwrap_or(0);
                app.filter = Some(channel);
                app.restore_auto_follow();
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

    if app.active_input_is_passthrough() {
        let channel = app.active_input_channel();
        let policy = app
            .manifest
            .as_ref()
            .and_then(|manifest| passthrough_policy_for_channel(manifest, u32::from(channel)))
            .unwrap_or_default();
        if let Some(payload) = passthrough_key_payload(key, policy) {
            app.restore_auto_follow();
            let frame = build_input_frame(channel, &payload, args.max_payload_len)
                .map_err(build_frame_error_to_io)?;
            if let Some(port) = serial {
                port.write_all(&frame)?;
                port.flush()?;
                app.status = format!("passthrough sent {} bytes to ch{channel}", payload.len());
                app.input.clear();
            } else {
                app.status = "not connected; passthrough input not sent".to_string();
            }
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Char(ch) => {
            app.reset_empty_enter_restore();
            app.input.push(ch);
        }
        KeyCode::Backspace => {
            app.reset_empty_enter_restore();
            app.input.pop();
        }
        KeyCode::Enter => {
            if app.input.is_empty() {
                app.handle_empty_enter_restore();
                return Ok(());
            }
            app.reset_empty_enter_restore();
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
            app.reset_empty_enter_restore();
            app.input.clear();
        }
        _ => {}
    }

    Ok(())
}

fn handle_mouse(app: &mut App, output_area: Rect, mouse: MouseEvent) {
    let output_height = output_content_height(output_area);
    match mouse.kind {
        MouseEventKind::ScrollUp => app.scroll_up(output_height),
        MouseEventKind::ScrollDown => app.scroll_down(output_height),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(offset) = scrollbar_offset_from_mouse(
                output_area,
                app.filtered_line_count(),
                output_height,
                mouse.column,
                mouse.row,
            ) {
                app.dragging_scrollbar = true;
                app.set_scroll_offset(offset, output_height);
            } else {
                app.dragging_scrollbar = false;
                app.reset_empty_enter_restore();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_scrollbar => {
            let offset = scrollbar_offset_from_drag_row(
                output_area,
                app.filtered_line_count(),
                output_height,
                mouse.row,
            );
            app.set_scroll_offset(offset, output_height);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging_scrollbar = false;
            app.reset_empty_enter_restore();
        }
        _ => app.reset_empty_enter_restore(),
    }
}

fn handle_stream_event(app: &mut App, diagnostics: &mut File, event: HostEvent) -> io::Result<()> {
    match event {
        HostEvent::Terminal(bytes) => app.push_terminal(&bytes),
        HostEvent::Record(envelope) => handle_envelope(app, diagnostics, &envelope)?,
        HostEvent::Manifest(manifest) => {
            app.push_marker(format!(
                "manifest received: {} channels",
                manifest.channels.len()
            ));
            app.manifest = Some(manifest);
        }
        HostEvent::ProtocolCompatibility(compatibility) => match compatibility.compatibility {
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
                app.push_marker("device protocol is newer; upgrade host SDK/tool");
            }
            ProtocolCompatibilityKind::UnsupportedOld => {
                writeln!(
                    diagnostics,
                    "[wiremux] protocol_api unsupported_old device={} host_min={}",
                    compatibility.device_api_version, compatibility.host_min_api_version
                )?;
                app.push_marker("device protocol is too old for this host");
            }
            ProtocolCompatibilityKind::Unknown(value) => {
                writeln!(
                    diagnostics,
                    "[wiremux] protocol_api unknown compatibility={value}"
                )?;
            }
        },
        HostEvent::BatchSummary(summary) => {
            writeln!(
                diagnostics,
                "[wiremux] batch records={} compression={} encoded_bytes={} raw_bytes={}",
                summary.record_count, summary.compression, summary.encoded_bytes, summary.raw_bytes
            )?;
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
            app.push_marker(decode_error_marker(err.stage));
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
    write_envelope_diagnostics(diagnostics, envelope)?;
    app.push_record(envelope);
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

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let chunks = main_layout(frame.area());

    let output_area = chunks[0];
    let height = output_content_height(output_area);
    let filtered = app
        .lines
        .iter()
        .filter(|line| app.line_matches_filter(line))
        .collect::<Vec<_>>();
    let (start, end) = visible_window(filtered.len(), height, app.scroll_offset);
    let visible = &filtered[start..end];
    let lines = visible
        .iter()
        .map(|line| {
            if let Some(channel) = line.channel {
                if app.channel_is_passthrough(channel) && line.text.is_empty() {
                    return Line::from("");
                }
                Line::from(vec![
                    Span::styled(
                        format!("{}> ", app.channel_prefix(channel)),
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
        .block(
            Block::default()
                .title(output_title(app.scroll_offset))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(output, output_area);

    let max_offset = max_scroll_offset(filtered.len(), height);
    if max_offset > 0 {
        let mut scrollbar_state = ScrollbarState::new(max_offset + 1)
            .position(scrollbar_position(max_offset, app.scroll_offset))
            .viewport_content_length(1);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            output_area,
            &mut scrollbar_state,
        );
    }

    let status = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("filter ", Style::default().fg(Color::Yellow)),
            Span::raw(app.filter_label()),
            Span::raw("  "),
            Span::styled("input ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "ch{}{}",
                app.active_input_channel(),
                if app.active_input_is_passthrough() {
                    " passthrough"
                } else {
                    " line"
                }
            )),
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
            .title(if app.active_input_is_passthrough() {
                "input: passthrough, Ctrl-C or Ctrl-] quits"
            } else {
                "input: Enter sends, Esc clears, Ctrl-C quits"
            })
            .borders(Borders::ALL),
    );
    frame.render_widget(input, chunks[2]);
}

fn main_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area)
}

fn output_title(scroll_offset: usize) -> String {
    if scroll_offset == 0 {
        "wiremux".to_string()
    } else {
        format!("wiremux - scrollback +{scroll_offset}")
    }
}

fn output_area_from_terminal_area(area: Rect) -> Rect {
    main_layout(area)[0]
}

fn output_content_height(output_area: Rect) -> usize {
    output_area.height.saturating_sub(2) as usize
}

fn max_scroll_offset(total_lines: usize, output_height: usize) -> usize {
    if output_height == 0 {
        return 0;
    }
    total_lines.saturating_sub(output_height)
}

fn visible_window(
    total_lines: usize,
    output_height: usize,
    scroll_offset: usize,
) -> (usize, usize) {
    if output_height == 0 || total_lines == 0 {
        return (0, 0);
    }
    let offset = scroll_offset.min(max_scroll_offset(total_lines, output_height));
    let end = total_lines.saturating_sub(offset);
    let start = end.saturating_sub(output_height);
    (start, end)
}

fn scrollbar_position(max_offset: usize, scroll_offset: usize) -> usize {
    max_offset.saturating_sub(scroll_offset.min(max_offset))
}

fn scrollbar_offset_from_mouse(
    output_area: Rect,
    total_lines: usize,
    output_height: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    if output_area.width == 0 || output_area.height == 0 {
        return None;
    }

    let scrollbar_column = output_area.x + output_area.width - 1;
    let row_end = output_area.y + output_area.height;
    if column != scrollbar_column || row < output_area.y || row >= row_end {
        return None;
    }

    Some(scrollbar_offset_from_drag_row(
        output_area,
        total_lines,
        output_height,
        row,
    ))
}

fn scrollbar_offset_from_drag_row(
    output_area: Rect,
    total_lines: usize,
    output_height: usize,
    row: u16,
) -> usize {
    let max_offset = max_scroll_offset(total_lines, output_height);
    if max_offset == 0 || output_height == 0 {
        return 0;
    }

    let track_len = output_area.height.saturating_sub(2);
    if track_len <= 1 {
        return 0;
    }

    let first_track_row = output_area.y.saturating_add(1);
    let last_track_row = first_track_row.saturating_add(track_len - 1);
    let clamped_row = row.clamp(first_track_row, last_track_row);
    let track_row = clamped_row.saturating_sub(first_track_row) as usize;
    let max_position = max_offset;
    let position =
        (track_row * max_position + (track_len as usize - 1) / 2) / (track_len as usize - 1);
    max_position.saturating_sub(position)
}

fn empty_as_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremux::host_session::{ChannelDescriptor, CHANNEL_INTERACTION_PASSTHROUGH};

    #[test]
    fn visible_window_follows_tail_when_offset_is_zero() {
        assert_eq!(visible_window(10, 4, 0), (6, 10));
    }

    #[test]
    fn visible_window_moves_back_by_scroll_offset() {
        assert_eq!(visible_window(10, 4, 3), (3, 7));
    }

    #[test]
    fn visible_window_clamps_to_oldest_lines() {
        assert_eq!(visible_window(10, 4, 50), (0, 4));
    }

    #[test]
    fn visible_window_handles_empty_or_zero_height() {
        assert_eq!(visible_window(0, 4, 0), (0, 0));
        assert_eq!(visible_window(10, 0, 0), (0, 0));
    }

    #[test]
    fn scrollbar_position_reaches_last_position_at_tail() {
        assert_eq!(scrollbar_position(20, 0), 20);
        assert_eq!(scrollbar_position(20, 11), 9);
        assert_eq!(scrollbar_position(20, 20), 0);
        assert_eq!(scrollbar_position(20, 50), 0);
    }

    #[test]
    fn scrollbar_mouse_maps_top_middle_and_bottom_to_offsets() {
        let output_area = Rect::new(0, 0, 40, 12);

        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 1),
            Some(20)
        );
        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 10),
            Some(0)
        );
        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 5),
            Some(11)
        );
    }

    #[test]
    fn scrollbar_mouse_ignores_non_scrollbar_cells() {
        let output_area = Rect::new(0, 0, 40, 12);

        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 38, 1),
            None
        );
        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 12),
            None
        );
    }

    #[test]
    fn scrollbar_drag_row_clamps_outside_output_area() {
        let output_area = Rect::new(0, 10, 40, 12);

        assert_eq!(scrollbar_offset_from_drag_row(output_area, 30, 10, 0), 20);
        assert_eq!(scrollbar_offset_from_drag_row(output_area, 30, 10, 99), 0);
    }

    #[test]
    fn mouse_scroll_up_pauses_and_scroll_down_restores_tail_follow() {
        let mut app = app_with_lines(10);

        app.scroll_up(4);
        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.empty_enter_restore_count, 0);

        app.scroll_down(4);
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.empty_enter_restore_count, 0);
    }

    #[test]
    fn scrollbar_drag_sets_offset_until_mouse_release() {
        let mut app = app_with_lines(30);
        let output_area = Rect::new(0, 0, 40, 12);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 39,
                row: 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert!(app.dragging_scrollbar);
        assert_eq!(app.scroll_offset, 20);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 20,
                row: 10,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.scroll_offset, 0);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: 39,
                row: 10,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert!(!app.dragging_scrollbar);
    }

    #[test]
    fn scroll_up_is_noop_when_everything_fits() {
        let mut app = app_with_lines(2);

        app.scroll_up(4);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn two_empty_enters_restore_tail_follow() {
        let mut app = app_with_lines(10);
        app.scroll_up(4);

        app.handle_empty_enter_restore();
        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.empty_enter_restore_count, 1);

        app.handle_empty_enter_restore();
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.empty_enter_restore_count, 0);
    }

    #[test]
    fn non_empty_input_actions_reset_empty_enter_restore_counter() {
        let mut app = app_with_lines(10);
        app.scroll_up(4);
        app.handle_empty_enter_restore();

        app.reset_empty_enter_restore();
        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.empty_enter_restore_count, 0);
    }

    #[test]
    fn appended_matching_lines_do_not_move_scrolled_view() {
        let mut app = app_with_lines(10);
        app.scroll_up(4);
        assert_eq!(
            visible_window(app.filtered_line_count(), 4, app.scroll_offset),
            (3, 7)
        );

        app.push_line(None, "new\n".to_string());
        assert_eq!(
            visible_window(app.filtered_line_count(), 4, app.scroll_offset),
            (3, 7)
        );
    }

    #[test]
    fn filtered_scroll_uses_matching_lines_only() {
        let mut app = App::new("diag.log".to_string());
        app.push_line(Some(1), "one\n".to_string());
        app.push_line(Some(2), "two\n".to_string());
        app.push_line(Some(1), "three\n".to_string());
        app.filter = Some(1);

        assert_eq!(app.filtered_line_count(), 2);
        app.scroll_up(1);
        assert_eq!(app.scroll_offset, 1);
    }

    #[test]
    fn channel_prefix_uses_manifest_name_and_clamps_utf8() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(DeviceManifest {
            device_name: String::new(),
            firmware_version: String::new(),
            protocol_version: 1,
            max_channels: 8,
            channels: vec![ChannelDescriptor {
                channel_id: 4,
                name: "🚗🎒😄🔥".to_string(),
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
        });

        assert_eq!(app.channel_prefix(4), "ch4(🚗🎒😄)");
        assert_eq!(app.channel_prefix(5), "ch5");
    }

    #[test]
    fn active_input_mode_uses_manifest_passthrough_metadata() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        assert!(app.active_input_is_passthrough());
    }

    #[test]
    fn passthrough_records_append_to_stream_line_until_newline() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"m".to_vec(),
            flags: 0,
        });
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 2,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"ux\r\n".to_vec(),
            flags: 0,
        });
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 3,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"help".to_vec(),
            flags: 0,
        });

        assert_eq!(app.lines.len(), 2);
        assert_eq!(app.lines[0].text, "mux");
        assert!(app.lines[0].complete);
        assert_eq!(app.lines[1].text, "help");
        assert!(!app.lines[1].complete);
    }

    #[test]
    fn passthrough_stream_applies_backspace_echo() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"hel\x08 \x08lp\r\n".to_vec(),
            flags: 0,
        });

        assert_eq!(app.lines.len(), 1);
        assert_eq!(app.lines[0].text, "help");
        assert!(app.lines[0].complete);
    }

    #[test]
    fn passthrough_stream_applies_split_backspace_echo() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        for (sequence, payload) in [b"hel".as_slice(), b"\x08", b" ", b"\x08", b"lp\r\n"]
            .into_iter()
            .enumerate()
        {
            app.push_record(&MuxEnvelope {
                channel_id: 1,
                direction: 2,
                sequence: sequence as u32,
                timestamp_us: 0,
                kind: 1,
                payload_type: String::new(),
                payload: payload.to_vec(),
                flags: 0,
            });
        }

        assert_eq!(app.lines.len(), 1);
        assert_eq!(app.lines[0].text, "help");
        assert!(app.lines[0].complete);
    }

    #[test]
    fn passthrough_stream_continues_after_interleaved_channel_records() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        for (sequence, payload) in [b"h".as_slice(), b"e", b"l", b"p"].into_iter().enumerate() {
            app.push_record(&MuxEnvelope {
                channel_id: 1,
                direction: 2,
                sequence: sequence as u32,
                timestamp_us: 0,
                kind: 1,
                payload_type: String::new(),
                payload: payload.to_vec(),
                flags: 0,
            });
        }

        app.push_record(&MuxEnvelope {
            channel_id: 3,
            direction: 2,
            sequence: 10,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"demo telemetry sample\n".to_vec(),
            flags: 0,
        });
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 11,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"\x08 \x08".to_vec(),
            flags: 0,
        });
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 12,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"p\r\n".to_vec(),
            flags: 0,
        });

        let channel_one_lines = app
            .lines
            .iter()
            .filter(|line| line.channel == Some(1))
            .collect::<Vec<_>>();

        assert_eq!(channel_one_lines.len(), 1);
        assert_eq!(channel_one_lines[0].text, "help");
        assert!(channel_one_lines[0].complete);
    }

    #[test]
    fn passthrough_input_restores_live_tail() {
        let mut app = app_with_lines(10);
        app.manifest = Some(passthrough_manifest(1));
        app.scroll_up(4);
        assert!(app.scroll_offset > 0);

        handle_key(
            &mut app,
            None,
            &TuiArgs {
                port: "/tmp/fake".into(),
                baud: 115200,
                max_payload_len: 512,
                reconnect_delay_ms: 500,
            },
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()),
        )
        .expect("handle passthrough key");

        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn passthrough_output_on_active_input_channel_restores_live_tail() {
        let mut app = app_with_lines(10);
        app.manifest = Some(passthrough_manifest(1));
        app.scroll_up(4);
        assert!(app.scroll_offset > 0);

        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"h".to_vec(),
            flags: 0,
        });

        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn passthrough_exit_key_quits_tui() {
        let mut app = App::new("diag.log".to_string());

        handle_key(
            &mut app,
            None,
            &TuiArgs {
                port: "/tmp/fake".into(),
                baud: 115200,
                max_payload_len: 512,
                reconnect_delay_ms: 500,
            },
            KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
        )
        .expect("handle passthrough exit key");

        assert!(app.should_quit);
    }

    fn passthrough_manifest(channel_id: u32) -> DeviceManifest {
        DeviceManifest {
            device_name: String::new(),
            firmware_version: String::new(),
            protocol_version: 2,
            max_channels: 8,
            channels: vec![ChannelDescriptor {
                channel_id,
                name: "console".to_string(),
                description: String::new(),
                directions: Vec::new(),
                payload_kinds: Vec::new(),
                payload_types: Vec::new(),
                flags: 0,
                default_payload_kind: 0,
                interaction_modes: vec![CHANNEL_INTERACTION_PASSTHROUGH],
                default_interaction_mode: CHANNEL_INTERACTION_PASSTHROUGH,
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

    fn app_with_lines(line_count: usize) -> App {
        let mut app = App::new("diag.log".to_string());
        for index in 0..line_count {
            app.push_line(None, format!("line {index}\n"));
        }
        app
    }
}
