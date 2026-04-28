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
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Terminal;
use wiremux::host_session::{
    display_channel_name, DeviceManifest, HostDecodeStage, HostEvent, HostSession, MuxEnvelope,
    ProtocolCompatibilityKind, DIRECTION_INPUT,
};

use super::{
    build_frame_error_to_io, build_input_frame, build_manifest_request_frame,
    channel_supports_passthrough, create_diagnostics_file, is_passthrough_escape_exit_suffix,
    is_passthrough_exit_key, is_passthrough_meta_exit_key, open_available_port_with_timeout,
    passthrough_key_payload, passthrough_policy_for_channel, printable_payload,
    write_envelope_diagnostics, TuiArgs, INTERACTIVE_SERIAL_READ_TIMEOUT,
    PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS,
};

const MAX_LINES: usize = 1000;
const WHEEL_SCROLL_LINES: usize = 3;

struct OutputLine {
    channel: Option<u32>,
    text: String,
    complete: bool,
}

struct RenderOutputLine<'a> {
    channel: Option<u32>,
    text: &'a str,
}

struct RenderOutputRow {
    channel: Option<u32>,
    prefix: Option<String>,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputState {
    ReadOnly,
    Line(u8),
    Passthrough(u8),
}

impl InputState {
    fn channel(self) -> Option<u8> {
        match self {
            Self::ReadOnly => None,
            Self::Line(channel) | Self::Passthrough(channel) => Some(channel),
        }
    }

    fn is_passthrough(self) -> bool {
        matches!(self, Self::Passthrough(_))
    }
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
    exit_escape_started_at: Option<Instant>,
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
            exit_escape_started_at: None,
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
            if self
                .active_input_channel()
                .is_some_and(|channel| u32::from(channel) == envelope.channel_id)
            {
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

    fn active_input_channel(&self) -> Option<u8> {
        self.filter.and_then(|channel| u8::try_from(channel).ok())
    }

    fn active_input_state(&self) -> InputState {
        let Some(channel) = self.active_input_channel() else {
            return InputState::ReadOnly;
        };
        let channel_id = u32::from(channel);
        let Some(manifest) = self.manifest.as_ref() else {
            return InputState::ReadOnly;
        };
        if !channel_supports_input(manifest, channel_id) {
            return InputState::ReadOnly;
        }
        if channel_supports_passthrough(manifest, channel_id) {
            InputState::Passthrough(channel)
        } else {
            InputState::Line(channel)
        }
    }

    fn active_input_is_passthrough(&self) -> bool {
        self.active_input_state().is_passthrough()
    }

    fn clear_input_if_read_only(&mut self) {
        if self.active_input_state() == InputState::ReadOnly {
            self.input.clear();
        }
    }

    fn input_label(&self) -> String {
        match self.active_input_state() {
            InputState::ReadOnly => "read-only".to_string(),
            InputState::Line(channel) => format!("ch{channel} line"),
            InputState::Passthrough(channel) => format!("ch{channel} passthrough"),
        }
    }

    fn input_title(&self) -> &'static str {
        match self.active_input_state() {
            InputState::ReadOnly => "input: read-only, Ctrl-C/Ctrl-]/Esc x quits",
            InputState::Line(_) => "input: Enter sends, Esc clears, Ctrl-C/Ctrl-]/Esc x quits",
            InputState::Passthrough(_) => "input: passthrough, Ctrl-C/Ctrl-]/Esc x quits",
        }
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

    #[cfg(test)]
    fn filtered_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| self.line_matches_filter(line))
            .count()
    }

    fn line_matches_filter(&self, line: &OutputLine) -> bool {
        self.filter.is_none() || line.channel == self.filter
    }

    fn max_scroll_offset(&self, output_area: Rect) -> usize {
        let output_height = output_content_height(output_area);
        let output_width = output_content_width(output_area);
        max_scroll_offset(self.filtered_visual_row_count(output_width), output_height)
    }

    fn scroll_up(&mut self, output_area: Rect) {
        self.empty_enter_restore_count = 0;
        let max_offset = self.max_scroll_offset(output_area);
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

    fn scroll_down(&mut self, output_area: Rect) {
        self.empty_enter_restore_count = 0;
        let max_offset = self.max_scroll_offset(output_area);
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

    fn set_scroll_offset(&mut self, scroll_offset: usize, output_area: Rect) {
        self.empty_enter_restore_count = 0;
        self.scroll_offset = scroll_offset.min(self.max_scroll_offset(output_area));
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

    fn filtered_visual_row_count(&self, output_width: usize) -> usize {
        let filtered = self
            .lines
            .iter()
            .filter(|line| self.line_matches_filter(line))
            .collect::<Vec<_>>();
        let append_prompt = should_append_passthrough_prompt(self, &filtered);
        let total_logical_lines = filtered.len() + usize::from(append_prompt);
        let logical = rendered_output_lines(self, &filtered, append_prompt, 0, total_logical_lines);
        output_rows(self, &logical, output_width).len()
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
        "diagnostics: {}; Ctrl-B 0..9 filters; Enter sends; Ctrl-C/Ctrl-]/Esc x quits",
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
            match open_available_port_with_timeout(
                &args.port,
                args.baud,
                INTERACTIVE_SERIAL_READ_TIMEOUT,
            ) {
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

        handle_exit_escape_timeout(&mut app, serial.as_mut(), &args)?;

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
    let mut serial = serial;
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return Ok(());
    }

    if is_passthrough_exit_key(key) || is_passthrough_meta_exit_key(key) {
        app.should_quit = true;
        return Ok(());
    }

    if app.exit_escape_started_at.take().is_some() {
        if is_passthrough_escape_exit_suffix(key) {
            app.should_quit = true;
            return Ok(());
        }
        apply_pending_escape(app, serial.as_mut().map(|port| &mut **port), args)?;
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
                app.clear_input_if_read_only();
                app.status = "filter: all".to_string();
            }
            KeyCode::Char(ch @ '1'..='9') => {
                let channel = ch.to_digit(10).unwrap_or(0);
                app.filter = Some(channel);
                app.restore_auto_follow();
                app.clear_input_if_read_only();
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

    if key.code == KeyCode::Esc {
        app.exit_escape_started_at = Some(Instant::now());
        app.status = "exit prefix: press x to quit".to_string();
        return Ok(());
    }

    match app.active_input_state() {
        InputState::ReadOnly => match key.code {
            KeyCode::Enter if app.input.is_empty() => app.handle_empty_enter_restore(),
            KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Enter => {
                app.input.clear();
                app.reset_empty_enter_restore();
                app.status = "input is read-only".to_string();
            }
            _ => {}
        },
        InputState::Passthrough(_) => {
            send_tui_passthrough_key(app, serial.as_mut().map(|port| &mut **port), args, key)?;
        }
        InputState::Line(channel) => match key.code {
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
            _ => {}
        },
    }

    Ok(())
}

fn handle_exit_escape_timeout(
    app: &mut App,
    serial: Option<&mut Box<dyn serialport::SerialPort>>,
    args: &TuiArgs,
) -> io::Result<()> {
    if app.exit_escape_started_at.is_some_and(|started_at| {
        started_at.elapsed() >= Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
    }) {
        app.exit_escape_started_at = None;
        apply_pending_escape(app, serial, args)?;
    }
    Ok(())
}

fn apply_pending_escape(
    app: &mut App,
    serial: Option<&mut Box<dyn serialport::SerialPort>>,
    args: &TuiArgs,
) -> io::Result<()> {
    if app.active_input_is_passthrough() {
        send_tui_passthrough_key(
            app,
            serial,
            args,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        )?;
    } else if app.active_input_state() == InputState::ReadOnly {
        app.input.clear();
        app.status = "input is read-only".to_string();
    } else {
        app.reset_empty_enter_restore();
        app.input.clear();
        app.status = "input cleared".to_string();
    }
    Ok(())
}

fn send_tui_passthrough_key(
    app: &mut App,
    serial: Option<&mut Box<dyn serialport::SerialPort>>,
    args: &TuiArgs,
    key: KeyEvent,
) -> io::Result<()> {
    let Some(channel) = app.active_input_state().channel() else {
        app.input.clear();
        app.status = "input is read-only".to_string();
        return Ok(());
    };
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
    Ok(())
}

fn handle_mouse(app: &mut App, output_area: Rect, mouse: MouseEvent) {
    let output_height = output_content_height(output_area);
    let output_width = output_content_width(output_area);
    let total_rows = app.filtered_visual_row_count(output_width);
    match mouse.kind {
        MouseEventKind::ScrollUp => app.scroll_up(output_area),
        MouseEventKind::ScrollDown => app.scroll_down(output_area),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(offset) = scrollbar_offset_from_mouse(
                output_area,
                total_rows,
                output_height,
                mouse.column,
                mouse.row,
            ) {
                app.dragging_scrollbar = true;
                app.set_scroll_offset(offset, output_area);
            } else {
                app.dragging_scrollbar = false;
                app.reset_empty_enter_restore();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_scrollbar => {
            let offset =
                scrollbar_offset_from_drag_row(output_area, total_rows, output_height, mouse.row);
            app.set_scroll_offset(offset, output_area);
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
            app.clear_input_if_read_only();
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

fn channel_supports_input(manifest: &DeviceManifest, channel_id: u32) -> bool {
    manifest
        .channels
        .iter()
        .find(|channel| channel.channel_id == channel_id)
        .is_some_and(|channel| channel.directions.contains(&DIRECTION_INPUT))
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
    let width = output_content_width(output_area);
    let filtered = app
        .lines
        .iter()
        .filter(|line| app.line_matches_filter(line))
        .collect::<Vec<_>>();
    let append_prompt = should_append_passthrough_prompt(app, &filtered);
    let total_logical_lines = filtered.len() + usize::from(append_prompt);
    let logical = rendered_output_lines(app, &filtered, append_prompt, 0, total_logical_lines);
    let rows = output_rows(app, &logical, width);
    let total_rendered_lines = rows.len();
    let (start, end) = visible_window(total_rendered_lines, height, app.scroll_offset);
    let visible = rows[start..end].iter().collect::<Vec<_>>();
    let lines = visible
        .iter()
        .map(|line| {
            if let Some(prefix) = line.prefix.as_deref() {
                Line::from(vec![
                    Span::styled(
                        prefix,
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

    let output = Paragraph::new(lines).block(
        Block::default()
            .title(output_title(app.scroll_offset))
            .borders(Borders::ALL),
    );
    frame.render_widget(output, output_area);

    let max_offset = max_scroll_offset(total_rendered_lines, height);
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
            Span::raw(app.input_label()),
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

    let input_line = match app.active_input_state() {
        InputState::ReadOnly => Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            Span::styled("read-only", Style::default().fg(Color::DarkGray)),
        ]),
        InputState::Line(_) => Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Green)),
            Span::raw(app.input.as_str()),
        ]),
        InputState::Passthrough(_) => Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "passthrough: type in output pane",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    };
    let input = Paragraph::new(input_line).block(
        Block::default()
            .title(app.input_title())
            .borders(Borders::ALL),
    );
    frame.render_widget(input, chunks[2]);
    set_cursor_position(frame, app, output_area, &visible, chunks[2]);
}

fn should_append_passthrough_prompt(app: &App, filtered: &[&OutputLine]) -> bool {
    let InputState::Passthrough(active_channel) = app.active_input_state() else {
        return false;
    };
    if app.scroll_offset > 0 {
        return false;
    }

    let active_channel = u32::from(active_channel);
    match filtered
        .iter()
        .rev()
        .find(|line| line.channel == Some(active_channel))
    {
        Some(line) => line.complete,
        None => true,
    }
}

fn rendered_output_lines<'a>(
    app: &App,
    filtered: &'a [&'a OutputLine],
    append_prompt: bool,
    start: usize,
    end: usize,
) -> Vec<RenderOutputLine<'a>> {
    let active_channel = app
        .active_input_state()
        .channel()
        .map(u32::from)
        .unwrap_or_default();
    (start..end)
        .filter_map(|index| {
            if index < filtered.len() {
                let line = filtered[index];
                Some(RenderOutputLine {
                    channel: line.channel,
                    text: line.text.as_str(),
                })
            } else if append_prompt {
                Some(RenderOutputLine {
                    channel: Some(active_channel),
                    text: "",
                })
            } else {
                None
            }
        })
        .collect()
}

fn output_rows(
    app: &App,
    lines: &[RenderOutputLine<'_>],
    output_width: usize,
) -> Vec<RenderOutputRow> {
    let output_width = output_width.max(1);
    let mut rows = Vec::new();
    for line in lines {
        let prefix = line
            .channel
            .map(|channel| format!("{}> ", app.channel_prefix(channel)));
        append_output_rows(&mut rows, line.channel, prefix, line.text, output_width);
    }
    rows
}

fn append_output_rows(
    rows: &mut Vec<RenderOutputRow>,
    channel: Option<u32>,
    prefix: Option<String>,
    text: &str,
    output_width: usize,
) {
    let Some(prefix) = prefix else {
        append_continuation_rows(rows, channel, text, output_width);
        return;
    };

    let prefix_width = display_width_usize(&prefix);
    let first_text_width = output_width.saturating_sub(prefix_width);
    if first_text_width == 0 {
        rows.push(RenderOutputRow {
            channel,
            prefix: Some(prefix),
            text: String::new(),
        });
        append_continuation_rows(rows, channel, text, output_width);
        return;
    }

    let mut chars = text.chars();
    let mut first_text = String::new();
    for _ in 0..first_text_width {
        let Some(ch) = chars.next() else {
            break;
        };
        first_text.push(ch);
    }
    rows.push(RenderOutputRow {
        channel,
        prefix: Some(prefix),
        text: first_text,
    });

    let rest = chars.collect::<String>();
    if !rest.is_empty() {
        append_continuation_rows(rows, channel, &rest, output_width);
    }
}

fn append_continuation_rows(
    rows: &mut Vec<RenderOutputRow>,
    channel: Option<u32>,
    text: &str,
    output_width: usize,
) {
    if text.is_empty() {
        rows.push(RenderOutputRow {
            channel,
            prefix: None,
            text: String::new(),
        });
        return;
    }

    let mut row = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        if width == output_width {
            rows.push(RenderOutputRow {
                channel,
                prefix: None,
                text: row,
            });
            row = String::new();
            width = 0;
        }
        row.push(ch);
        width = width.saturating_add(1);
    }
    rows.push(RenderOutputRow {
        channel,
        prefix: None,
        text: row,
    });
}

fn set_cursor_position(
    frame: &mut ratatui::Frame<'_>,
    app: &App,
    output_area: Rect,
    visible: &[&RenderOutputRow],
    input_area: Rect,
) {
    match app.active_input_state() {
        InputState::ReadOnly => {}
        InputState::Line(_) => set_line_input_cursor(frame, app, input_area),
        InputState::Passthrough(_) => set_passthrough_cursor(frame, app, output_area, visible),
    }
}

fn set_line_input_cursor(frame: &mut ratatui::Frame<'_>, app: &App, input_area: Rect) {
    if input_area.width <= 2 || input_area.height <= 2 {
        return;
    }

    let content_width = input_area.width.saturating_sub(2);
    let max_offset = content_width.saturating_sub(1);
    let cursor_offset = 2u16
        .saturating_add(display_width(&app.input))
        .min(max_offset);

    frame.set_cursor_position(Position::new(
        input_area.x.saturating_add(1).saturating_add(cursor_offset),
        input_area.y.saturating_add(1),
    ));
}

fn set_passthrough_cursor(
    frame: &mut ratatui::Frame<'_>,
    app: &App,
    output_area: Rect,
    visible: &[&RenderOutputRow],
) {
    if output_area.width <= 2 || output_area.height <= 2 {
        return;
    }

    let content_width = output_area.width.saturating_sub(2);
    let max_offset = content_width.saturating_sub(1);
    let InputState::Passthrough(active_channel) = app.active_input_state() else {
        return;
    };
    let active_channel = u32::from(active_channel);
    let cursor = passthrough_cursor_position(output_area, visible, active_channel, max_offset)
        .unwrap_or_else(|| {
            Position::new(
                output_area.x.saturating_add(1),
                output_area.y.saturating_add(1),
            )
        });

    frame.set_cursor_position(cursor);
}

fn passthrough_cursor_position(
    output_area: Rect,
    visible: &[&RenderOutputRow],
    active_channel: u32,
    max_offset: u16,
) -> Option<Position> {
    let mut cursor = None;
    for (visual_row, line) in visible.iter().enumerate() {
        if line.channel == Some(active_channel) {
            let line_col = render_row_width(line).min(max_offset as usize);
            cursor = Some(Position::new(
                output_area
                    .x
                    .saturating_add(1)
                    .saturating_add(line_col as u16),
                output_area
                    .y
                    .saturating_add(1)
                    .saturating_add(visual_row.min(u16::MAX as usize) as u16),
            ));
        }
    }
    cursor
}

fn render_row_width(line: &RenderOutputRow) -> usize {
    let prefix_width = line
        .prefix
        .as_deref()
        .map(display_width_usize)
        .unwrap_or_default();
    prefix_width.saturating_add(display_width_usize(&line.text))
}

#[cfg(test)]
fn wrapped_visual_height(width: usize, content_width: usize) -> usize {
    if content_width == 0 {
        return 0;
    }
    width.max(1).div_ceil(content_width)
}

fn display_width(input: &str) -> u16 {
    display_width_usize(input).min(u16::MAX as usize) as u16
}

fn display_width_usize(input: &str) -> usize {
    input.chars().count()
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

fn output_content_width(output_area: Rect) -> usize {
    output_area.width.saturating_sub(2) as usize
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
    use ratatui::backend::TestBackend;
    use wiremux::host_session::{
        ChannelDescriptor, CHANNEL_INTERACTION_PASSTHROUGH, DIRECTION_OUTPUT,
    };

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
        let output_area = output_area_for_content(38, 4);

        app.scroll_up(output_area);
        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.empty_enter_restore_count, 0);

        app.scroll_down(output_area);
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

        app.scroll_up(output_area_for_content(38, 4));
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn wrapped_output_creates_visual_scrollback_even_when_logical_lines_fit() {
        let mut app = App::new("diag.log".to_string());
        for index in 0..3 {
            app.push_line(None, format!("line {index}: abcdefghijklmnopqrstuvwxyz\n"));
        }
        let output_area = output_area_for_content(16, 4);

        assert_eq!(max_scroll_offset(app.filtered_line_count(), 4), 0);
        assert!(app.max_scroll_offset(output_area) > 0);
    }

    #[test]
    fn two_empty_enters_restore_tail_follow() {
        let mut app = app_with_lines(10);
        app.scroll_up(output_area_for_content(38, 4));

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
        app.scroll_up(output_area_for_content(38, 4));
        app.handle_empty_enter_restore();

        app.reset_empty_enter_restore();
        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.empty_enter_restore_count, 0);
    }

    #[test]
    fn appended_matching_lines_do_not_move_scrolled_view() {
        let mut app = app_with_lines(10);
        app.scroll_up(output_area_for_content(38, 4));
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
        app.scroll_up(output_area_for_content(38, 1));
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
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));

        assert!(app.active_input_is_passthrough());
    }

    #[test]
    fn unfiltered_input_is_read_only_even_when_channel_one_supports_passthrough() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        assert_eq!(app.active_input_state(), InputState::ReadOnly);

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()),
        )
        .expect("handle read-only key");

        assert!(app.input.is_empty());
        assert_eq!(app.status, "input is read-only");
    }

    #[test]
    fn output_only_channel_input_is_read_only() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(2);
        app.manifest = Some(output_only_manifest(2));

        assert_eq!(app.active_input_state(), InputState::ReadOnly);

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        )
        .expect("handle output-only key");

        assert!(app.input.is_empty());
        assert_eq!(app.status, "input is read-only");
    }

    #[test]
    fn input_channel_without_passthrough_uses_line_mode() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(line_manifest(1));

        assert_eq!(app.active_input_state(), InputState::Line(1));
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
    fn passthrough_empty_newline_completes_prompt_line() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"\r\n".to_vec(),
            flags: 0,
        });
        assert_eq!(app.lines.len(), 1);
        assert!(app.lines[0].text.is_empty());
        assert!(app.lines[0].complete);
    }

    #[test]
    fn passthrough_repeated_empty_newlines_stack_prompt_history() {
        let mut app = App::new("diag.log".to_string());
        app.manifest = Some(passthrough_manifest(1));

        for sequence in 0..3 {
            app.push_record(&MuxEnvelope {
                channel_id: 1,
                direction: 2,
                sequence,
                timestamp_us: 0,
                kind: 1,
                payload_type: String::new(),
                payload: b"\r\n".to_vec(),
                flags: 0,
            });
        }

        assert_eq!(app.lines.len(), 3);
        assert!(app.lines.iter().all(|line| line.text.is_empty()));
        assert!(app.lines.iter().all(|line| line.complete));
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
        let mut app = app_with_channel_lines(1, 10);
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.scroll_up(output_area_for_content(38, 4));
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
        let mut app = app_with_channel_lines(1, 10);
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.scroll_up(output_area_for_content(38, 4));
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
            &tui_args(),
            KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
        )
        .expect("handle passthrough exit key");

        assert!(app.should_quit);
    }

    #[test]
    fn meta_x_quits_tui() {
        let mut app = App::new("diag.log".to_string());

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
        )
        .expect("handle meta exit key");

        assert!(app.should_quit);
    }

    #[test]
    fn escape_then_x_quits_tui() {
        let mut app = App::new("diag.log".to_string());

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        )
        .expect("start escape exit prefix");
        assert!(!app.should_quit);
        assert!(app.exit_escape_started_at.is_some());

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        )
        .expect("complete escape exit prefix");

        assert!(app.should_quit);
    }

    #[test]
    fn escape_timeout_clears_line_input() {
        let mut app = App::new("diag.log".to_string());
        app.input = "help".to_string();
        app.exit_escape_started_at = Some(
            Instant::now()
                - Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS.saturating_add(1)),
        );

        handle_exit_escape_timeout(&mut app, None, &tui_args()).expect("handle escape timeout");

        assert!(app.input.is_empty());
        assert!(app.exit_escape_started_at.is_none());
    }

    #[test]
    fn render_sets_input_cursor_after_prompt_and_text() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(line_manifest(1));
        app.input = "help".to_string();
        let area = Rect::new(0, 0, 80, 12);
        let input_area = main_layout(area)[2];
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(input_area.x + 7, input_area.y + 1)
        );
    }

    #[test]
    fn render_shows_passthrough_hint_in_bottom_input() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.input = "stale line input".to_string();
        let area = Rect::new(0, 0, 80, 12);
        let input_area = main_layout(area)[2];
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        let input_row = buffer_row(terminal.backend().buffer(), input_area.y + 1, area.width);
        assert!(input_row.contains("> passthrough: type in output pane"));
        assert!(!input_row.contains("stale line input"));
    }

    #[test]
    fn render_keeps_passthrough_prompt_for_empty_stream_line() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"\r\n".to_vec(),
            flags: 0,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert!(buffer_row(terminal.backend().buffer(), 1, 80).contains("ch1(console)>"));
    }

    #[test]
    fn render_sets_passthrough_cursor_in_output_pane_after_echo() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"hel".to_vec(),
            flags: 0,
        });
        let area = Rect::new(0, 0, 80, 12);
        let output_area = main_layout(area)[0];
        let prompt = "ch1(console)> ";
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(
                output_area.x + 1 + display_width(prompt) + display_width("hel"),
                output_area.y + 1
            )
        );
    }

    #[test]
    fn render_appends_virtual_passthrough_prompt_after_completed_output() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"available commands\n".to_vec(),
            flags: 0,
        });
        let area = Rect::new(0, 0, 80, 12);
        let output_area = main_layout(area)[0];
        let prompt = "ch1(console)> ";
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert!(buffer_row(terminal.backend().buffer(), 2, 80).contains(prompt));
        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(output_area.x + 1 + display_width(prompt), output_area.y + 2)
        );
    }

    #[test]
    fn render_sets_passthrough_cursor_after_wrapped_completed_output() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        let response = "available commands: help hello mux_log";
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: format!("{response}\n").into_bytes(),
            flags: 0,
        });
        let area = Rect::new(0, 0, 34, 12);
        let output_area = main_layout(area)[0];
        let content_width = output_area.width.saturating_sub(2) as usize;
        let prompt = "ch1(console)> ";
        let wrapped_response_height = wrapped_visual_height(
            display_width_usize(prompt).saturating_add(display_width_usize(response)),
            content_width,
        ) as u16;
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert!(buffer_row(
            terminal.backend().buffer(),
            output_area.y + 1 + wrapped_response_height,
            area.width,
        )
        .contains(prompt));
        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(
                output_area.x + 1 + display_width(prompt),
                output_area.y + 1 + wrapped_response_height
            )
        );
    }

    #[test]
    fn render_sets_passthrough_cursor_after_wrapped_output_and_echo() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        let response = "available commands: help hello mux_log";
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: format!("{response}\n").into_bytes(),
            flags: 0,
        });
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 2,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"hel".to_vec(),
            flags: 0,
        });
        let area = Rect::new(0, 0, 34, 12);
        let output_area = main_layout(area)[0];
        let content_width = output_area.width.saturating_sub(2) as usize;
        let prompt = "ch1(console)> ";
        let wrapped_response_height = wrapped_visual_height(
            display_width_usize(prompt).saturating_add(display_width_usize(response)),
            content_width,
        ) as u16;
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert!(buffer_row(
            terminal.backend().buffer(),
            output_area.y + 1 + wrapped_response_height,
            area.width,
        )
        .contains("ch1(console)> hel"));
        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(
                output_area.x + 1 + display_width(prompt) + display_width("hel"),
                output_area.y + 1 + wrapped_response_height
            )
        );
    }

    #[test]
    fn render_keeps_passthrough_cursor_inside_output_after_wrapped_scrollback() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        for sequence in 0..6 {
            app.push_record(&MuxEnvelope {
                channel_id: 1,
                direction: 2,
                sequence,
                timestamp_us: 0,
                kind: 1,
                payload_type: String::new(),
                payload: b"available commands: help hello mux_manifest mux_console_mode mux_hello mux_log mux_utf8 mux_stress mux_diag\n".to_vec(),
                flags: 0,
            });
        }
        let area = Rect::new(0, 0, 34, 12);
        let chunks = main_layout(area);
        let output_area = chunks[0];
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        let cursor = terminal.get_cursor_position().expect("cursor position");
        assert!(app.max_scroll_offset(output_area) > 0);
        assert!(cursor.y > output_area.y);
        assert!(cursor.y < output_area.y + output_area.height - 1);
        assert!(cursor.x > output_area.x);
        assert!(cursor.x < output_area.x + output_area.width - 1);
    }

    #[test]
    fn render_appends_virtual_passthrough_prompt_after_empty_enter() {
        let mut app = App::new("diag.log".to_string());
        app.filter = Some(1);
        app.manifest = Some(passthrough_manifest(1));
        app.push_record(&MuxEnvelope {
            channel_id: 1,
            direction: 2,
            sequence: 1,
            timestamp_us: 0,
            kind: 1,
            payload_type: String::new(),
            payload: b"\r\n".to_vec(),
            flags: 0,
        });
        let area = Rect::new(0, 0, 80, 12);
        let output_area = main_layout(area)[0];
        let prompt = "ch1(console)> ";
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert!(buffer_row(terminal.backend().buffer(), 1, 80).contains(prompt));
        assert!(buffer_row(terminal.backend().buffer(), 2, 80).contains(prompt));
        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(output_area.x + 1 + display_width(prompt), output_area.y + 2)
        );
    }

    fn buffer_row(buffer: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|column| buffer[(column, row)].symbol())
            .collect()
    }

    fn output_area_for_content(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width.saturating_add(2), height.saturating_add(2))
    }

    fn tui_args() -> TuiArgs {
        TuiArgs {
            port: "/tmp/fake".into(),
            baud: 115200,
            max_payload_len: 512,
            reconnect_delay_ms: 500,
        }
    }

    fn passthrough_manifest(channel_id: u32) -> DeviceManifest {
        manifest_with_channel(
            channel_id,
            vec![DIRECTION_INPUT],
            vec![CHANNEL_INTERACTION_PASSTHROUGH],
            CHANNEL_INTERACTION_PASSTHROUGH,
        )
    }

    fn line_manifest(channel_id: u32) -> DeviceManifest {
        manifest_with_channel(channel_id, vec![DIRECTION_INPUT], Vec::new(), 0)
    }

    fn output_only_manifest(channel_id: u32) -> DeviceManifest {
        manifest_with_channel(channel_id, vec![DIRECTION_OUTPUT], Vec::new(), 0)
    }

    fn manifest_with_channel(
        channel_id: u32,
        directions: Vec<u32>,
        interaction_modes: Vec<u32>,
        default_interaction_mode: u32,
    ) -> DeviceManifest {
        DeviceManifest {
            device_name: String::new(),
            firmware_version: String::new(),
            protocol_version: 2,
            max_channels: 8,
            channels: vec![ChannelDescriptor {
                channel_id,
                name: "console".to_string(),
                description: String::new(),
                directions,
                payload_kinds: Vec::new(),
                payload_types: Vec::new(),
                flags: 0,
                default_payload_kind: 0,
                interaction_modes,
                default_interaction_mode,
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

    fn app_with_channel_lines(channel: u32, line_count: usize) -> App {
        let mut app = App::new("diag.log".to_string());
        for index in 0..line_count {
            app.push_line(Some(channel), format!("line {index}\n"));
        }
        app
    }
}
