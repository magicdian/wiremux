use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(test)]
use clipboard::base64_encode;
use clipboard::write_osc52_copy;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use host_session::{
    build_frame_error_to_io, build_input_frame, build_manifest_request_frame,
    channel_supports_passthrough, decode_error_marker, display_channel_name,
    passthrough_policy_for_channel, printable_payload, write_envelope_diagnostics, DeviceManifest,
    HostCrcError, HostEvent, HostSession, MuxEnvelope, ProtocolCompatibilityKind, DIRECTION_INPUT,
};
#[cfg(test)]
use interactive::InteractiveBackendMode;
use interactive::{
    self, is_passthrough_escape_exit_suffix, is_passthrough_exit_key, is_passthrough_meta_exit_key,
    passthrough_key_payload, ConnectedInteractiveBackend, HostConfig, InteractiveEvent,
    SerialFlowControl, SerialParity, SerialProfile, INTERACTIVE_SERIAL_READ_TIMEOUT,
    PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Terminal;

mod args;
mod clipboard;

pub use args::TuiArgs;

const MAX_LINES: usize = 1000;
const WHEEL_SCROLL_LINES: usize = 1;
const TERMINAL_BURST_DRAIN_LIMIT: usize = 256;
const SELECTION_EDGE_SCROLL_LINES: usize = 1;

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

struct OutputRenderModel {
    rows: Vec<RenderOutputRow>,
    visible_start: usize,
    visible_end: usize,
    content_height: usize,
}

struct StyledSegment {
    text: String,
    style: Style,
}

struct StatusRow {
    segments: Vec<StyledSegment>,
}

#[cfg(test)]
fn test_serial_profile() -> SerialProfile {
    SerialProfile {
        port: PathBuf::from("/dev/tty.usbmodem2101"),
        baud: 115_200,
        data_bits: 8,
        stop_bits: 1,
        parity: SerialParity::None,
        flow_control: SerialFlowControl::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionPane {
    Output,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionAutoScroll {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectionPosition {
    row: usize,
    col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextSelection {
    pane: SelectionPane,
    anchor: SelectionPosition,
    cursor: SelectionPosition,
    active: bool,
}

impl TextSelection {
    fn new(pane: SelectionPane, position: SelectionPosition) -> Self {
        Self {
            pane,
            anchor: position,
            cursor: position,
            active: true,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsField {
    Port,
    Baud,
    DataBits,
    StopBits,
    Parity,
    FlowControl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsAction {
    Apply,
    SaveDefaults,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsRow {
    Field(SettingsField),
    Action(SettingsAction),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsPopup {
    TextInput {
        field: SettingsField,
        value: String,
        cursor: usize,
    },
    ChoiceList {
        field: SettingsField,
        selected: usize,
    },
    ConfirmExit {
        selected: usize,
    },
    Message(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettingsState {
    draft: SerialProfile,
    baseline: SerialProfile,
    selected: usize,
    popup: Option<SettingsPopup>,
}

impl SettingsState {
    fn new(profile: SerialProfile) -> Self {
        Self {
            draft: profile.clone(),
            baseline: profile,
            selected: 0,
            popup: None,
        }
    }

    fn is_dirty(&self) -> bool {
        self.draft != self.baseline
    }

    fn rows(&self) -> [SettingsRow; 9] {
        [
            SettingsRow::Field(SettingsField::Port),
            SettingsRow::Field(SettingsField::Baud),
            SettingsRow::Field(SettingsField::DataBits),
            SettingsRow::Field(SettingsField::StopBits),
            SettingsRow::Field(SettingsField::Parity),
            SettingsRow::Field(SettingsField::FlowControl),
            SettingsRow::Action(SettingsAction::Apply),
            SettingsRow::Action(SettingsAction::SaveDefaults),
            SettingsRow::Action(SettingsAction::Discard),
        ]
    }

    fn selected_row(&self) -> SettingsRow {
        self.rows()[self.selected.min(self.rows().len().saturating_sub(1))]
    }
}

struct App {
    lines: VecDeque<OutputLine>,
    input: String,
    filter: Option<u32>,
    prefix_pending: bool,
    status: String,
    backend_label: String,
    target_fps: u16,
    connected_port: Option<String>,
    diagnostics_path: String,
    manifest: Option<DeviceManifest>,
    should_quit: bool,
    scroll_offset: usize,
    scroll_target_offset: Option<usize>,
    empty_enter_restore_count: u8,
    dragging_scrollbar: bool,
    selection: Option<TextSelection>,
    selection_auto_scroll: Option<SelectionAutoScroll>,
    pending_clipboard: Option<String>,
    stream_cr_pending_channel: Option<u32>,
    exit_escape_started_at: Option<Instant>,
    serial: SerialProfile,
    config_path: PathBuf,
    settings: Option<SettingsState>,
    reconnect_requested: bool,
}

impl App {
    fn with_serial(diagnostics_path: String, serial: SerialProfile, config_path: PathBuf) -> Self {
        Self {
            lines: VecDeque::new(),
            input: String::new(),
            filter: None,
            prefix_pending: false,
            status: "connecting".to_string(),
            backend_label: "disconnected".to_string(),
            target_fps: 60,
            connected_port: None,
            diagnostics_path,
            manifest: None,
            should_quit: false,
            scroll_offset: 0,
            scroll_target_offset: None,
            empty_enter_restore_count: 0,
            dragging_scrollbar: false,
            selection: None,
            selection_auto_scroll: None,
            pending_clipboard: None,
            stream_cr_pending_channel: None,
            exit_escape_started_at: None,
            serial,
            config_path,
            settings: None,
            reconnect_requested: false,
        }
    }

    #[cfg(test)]
    fn new(diagnostics_path: String) -> Self {
        Self::with_serial(
            diagnostics_path,
            test_serial_profile(),
            PathBuf::from("/tmp/wiremux-config.toml"),
        )
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
            self.pin_scrolled_view_for_new_line(&output_line);
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
        self.pin_scrolled_view_for_new_line(&output_line);
        self.lines.push_back(output_line);
    }

    fn pin_scrolled_view_for_new_line(&mut self, output_line: &OutputLine) {
        if self.scroll_offset > 0 && self.line_matches_filter(output_line) {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
            if let Some(target) = self.scroll_target_offset.as_mut() {
                *target = target.saturating_add(1);
            }
        }
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
                "{} {} api={} channels={} max_payload={}",
                empty_as_dash(&manifest.device_name),
                empty_as_dash(&manifest.firmware_version),
                manifest.protocol_version,
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
        self.scroll_up_by(output_area, WHEEL_SCROLL_LINES);
    }

    fn scroll_up_by(&mut self, output_area: Rect, lines: usize) {
        if lines == 0 {
            return;
        }
        self.empty_enter_restore_count = 0;
        self.scroll_target_offset = None;
        let max_offset = self.max_scroll_offset(output_area);
        if max_offset == 0 {
            self.scroll_offset = 0;
            return;
        }
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(lines.saturating_mul(WHEEL_SCROLL_LINES))
            .min(max_offset);
        self.status = format!(
            "scrollback paused: {} lines from bottom",
            self.scroll_offset
        );
    }

    fn scroll_down(&mut self, output_area: Rect) {
        self.scroll_down_by(output_area, WHEEL_SCROLL_LINES);
    }

    fn scroll_down_by(&mut self, output_area: Rect, lines: usize) {
        if lines == 0 {
            return;
        }
        self.empty_enter_restore_count = 0;
        self.scroll_target_offset = None;
        if self.scroll_offset == 0 {
            self.status = "scrollback: following live output".to_string();
            return;
        }
        let max_offset = self.max_scroll_offset(output_area);
        self.scroll_offset = self.scroll_offset.min(max_offset);
        self.scroll_offset = self
            .scroll_offset
            .saturating_sub(lines.saturating_mul(WHEEL_SCROLL_LINES));
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

    fn jump_to_oldest_visible(&mut self, output_area: Rect) {
        self.empty_enter_restore_count = 0;
        self.scroll_target_offset = None;
        self.dragging_scrollbar = false;

        let max_offset = self.max_scroll_offset(output_area);
        self.scroll_offset = max_offset;
        if self.scroll_offset == 0 {
            self.status = "scrollback: following live output".to_string();
        } else {
            self.status = format!(
                "scrollback paused: {} lines from bottom",
                self.scroll_offset
            );
        }
    }

    fn follow_live_output(&mut self) {
        self.scroll_offset = 0;
        self.scroll_target_offset = None;
        self.empty_enter_restore_count = 0;
        self.dragging_scrollbar = false;
    }

    fn animate_to_scroll_offset(&mut self, scroll_offset: usize, output_area: Rect) {
        self.empty_enter_restore_count = 0;
        let target = scroll_offset.min(self.max_scroll_offset(output_area));
        if target == self.scroll_offset {
            self.scroll_target_offset = None;
        } else {
            self.scroll_target_offset = Some(target);
        }
    }

    fn advance_scroll_animation(&mut self) -> bool {
        let Some(target) = self.scroll_target_offset else {
            return false;
        };
        if self.scroll_offset == target {
            self.scroll_target_offset = None;
            return false;
        }

        let distance = self.scroll_offset.abs_diff(target);
        let step = ((distance + 4) / 5).clamp(1, 12);
        if self.scroll_offset < target {
            self.scroll_offset = self.scroll_offset.saturating_add(step).min(target);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub(step).max(target);
        }

        if self.scroll_offset == target {
            self.scroll_target_offset = None;
        }
        self.scroll_target_offset.is_some()
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

    fn clear_selection(&mut self) {
        self.selection = None;
        self.selection_auto_scroll = None;
        self.pending_clipboard = None;
    }

    fn has_selection(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|selection| !selection_is_empty(selection))
    }

    fn request_copy_selection(&mut self, output_area: Rect, status_area: Rect) {
        let Some(selection) = self.selection.as_ref() else {
            self.status = "copy: no selection".to_string();
            return;
        };

        let selected = match selection.pane {
            SelectionPane::Output => {
                let model = output_render_model(self, output_area);
                selected_output_text(selection, &model.rows)
            }
            SelectionPane::Status => {
                let rows = status_rows(self);
                selected_status_text(selection, &rows, status_area)
            }
        };

        if selected.is_empty() {
            self.status = "copy: no selection".to_string();
            return;
        }

        self.status = format!("copy: selected {} chars", selected.chars().count());
        self.pending_clipboard = Some(selected);
    }

    fn advance_selection_auto_scroll(&mut self, output_area: Rect) -> bool {
        let Some(direction) = self.selection_auto_scroll else {
            return false;
        };
        let before = self.scroll_offset;
        match direction {
            SelectionAutoScroll::Up => self.scroll_up_by(output_area, SELECTION_EDGE_SCROLL_LINES),
            SelectionAutoScroll::Down => {
                self.scroll_down_by(output_area, SELECTION_EDGE_SCROLL_LINES)
            }
        }
        let moved = self.scroll_offset != before;
        if moved {
            self.advance_selection_cursor_for_auto_scroll(output_area, direction);
        }
        if !moved {
            self.selection_auto_scroll = None;
        }
        moved
    }

    fn advance_selection_cursor_for_auto_scroll(
        &mut self,
        output_area: Rect,
        direction: SelectionAutoScroll,
    ) {
        let output_width = output_content_width(output_area);
        let max_row = self
            .filtered_visual_row_count(output_width)
            .saturating_sub(1);
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        if selection.pane != SelectionPane::Output {
            return;
        }
        match direction {
            SelectionAutoScroll::Up => {
                selection.cursor.row = selection.cursor.row.saturating_sub(1);
            }
            SelectionAutoScroll::Down => {
                selection.cursor.row = selection.cursor.row.saturating_add(1).min(max_row);
            }
        }
    }
}

pub fn run(args: TuiArgs, diagnostics_path: String, diagnostics: File) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(args, &mut terminal, diagnostics, diagnostics_path);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        PopKeyboardEnhancementFlags,
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
    let target_fps = resolve_tui_fps(args.tui_fps);
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(target_fps));
    let mut app = App::with_serial(
        diagnostics_path,
        args.serial.clone(),
        args.config_path.clone(),
    );
    app.target_fps = target_fps;
    app.push_marker(format!(
        "diagnostics: {}; Ctrl-B 0..9 filters; Ctrl-B s settings; Enter sends; Ctrl-C/Ctrl-]/Esc x quits",
        app.diagnostics_path
    ));

    let mut backend: Option<ConnectedInteractiveBackend> = None;
    let mut host_session = HostSession::new(args.max_payload_len).map_err(|status| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("host session init failed: {status}"),
        )
    })?;
    let mut last_connect_attempt = Instant::now() - reconnect_delay;
    let mut dirty = true;
    let mut next_render_at = Instant::now();

    loop {
        if backend.is_none() && last_connect_attempt.elapsed() >= reconnect_delay {
            last_connect_attempt = Instant::now();
            match interactive::open_interactive_backend(
                &app.serial,
                args.interactive_backend,
                INTERACTIVE_SERIAL_READ_TIMEOUT,
            ) {
                Ok((path, mut connected_backend)) => {
                    app.connected_port = Some(path.display().to_string());
                    app.backend_label = connected_backend.label().to_string();
                    app.status = format!("connected {}", path.display());
                    writeln!(
                        diagnostics,
                        "[wiremux] connected: {} backend={}",
                        path.display(),
                        connected_backend.label()
                    )?;
                    let request = build_manifest_request_frame(args.max_payload_len)
                        .map_err(build_frame_error_to_io)?;
                    connected_backend.write_all(&request)?;
                    connected_backend.flush()?;
                    backend = Some(connected_backend);
                    host_session = HostSession::new(args.max_payload_len).map_err(|status| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("host session init failed: {status}"),
                        )
                    })?;
                    app.push_marker("manifest requested");
                    dirty = true;
                }
                Err(err) => {
                    app.status = format!("waiting for {}: {err}", app.serial.port.display());
                    app.backend_label = "disconnected".to_string();
                    writeln!(
                        diagnostics,
                        "[wiremux] waiting for {}: {err}",
                        app.serial.port.display()
                    )?;
                    dirty = true;
                }
            }
        }

        if let Some(serial) = backend.as_mut().map(|backend| backend as &mut dyn Write) {
            if handle_exit_escape_timeout(&mut app, Some(serial), &args)? {
                dirty = true;
            }
        } else if handle_exit_escape_timeout(&mut app, None, &args)? {
            dirty = true;
        }

        let now = Instant::now();
        if (dirty || app.selection_auto_scroll.is_some()) && now >= next_render_at {
            let keep_auto_scrolling = if app.selection_auto_scroll.is_some() {
                let terminal_area: Rect =
                    interactive::retry_interrupted(|| terminal.size())?.into();
                let output_area = main_layout(terminal_area)[0];
                app.advance_selection_auto_scroll(output_area)
            } else {
                false
            };
            let keep_animating = app.advance_scroll_animation();
            terminal.draw(|frame| render(frame, &app))?;
            next_render_at = Instant::now() + frame_interval;
            dirty = keep_animating || keep_auto_scrolling;
        }

        diagnostics.flush()?;

        if app.should_quit {
            break;
        }

        let deadline = next_deadline(
            backend.is_none(),
            last_connect_attempt + reconnect_delay,
            app.exit_escape_started_at.map(|started_at| {
                started_at + Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
            }),
            (dirty || app.selection_auto_scroll.is_some()).then_some(next_render_at),
        );

        let event = if let Some(backend) = backend.as_mut() {
            backend.next_event(deadline)?
        } else {
            interactive::wait_terminal_event(deadline)?
        };

        match event {
            InteractiveEvent::SerialBytes(bytes) => {
                for event in host_session.feed(&bytes).map_err(|status| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("host session feed failed: {status}"),
                    )
                })? {
                    handle_stream_event(&mut app, &mut diagnostics, event)?;
                }
                dirty = true;
            }
            InteractiveEvent::SerialEof => {
                app.push_marker("disconnected: EOF");
                app.connected_port = None;
                app.backend_label = "disconnected".to_string();
                backend = None;
                dirty = true;
            }
            InteractiveEvent::SerialError(err) => {
                if err.kind() == io::ErrorKind::TimedOut {
                    continue;
                }
                app.push_marker(format!("disconnected: {err}"));
                writeln!(diagnostics, "[wiremux] disconnected: {err}")?;
                app.connected_port = None;
                app.backend_label = "disconnected".to_string();
                backend = None;
                dirty = true;
            }
            InteractiveEvent::Terminal(event) => {
                let terminal_area: Rect =
                    interactive::retry_interrupted(|| terminal.size())?.into();
                let chunks = main_layout(terminal_area);
                let output_area = chunks[0];
                let status_area = chunks[1];
                let events = collect_terminal_burst(event)?;
                if let Some(serial) = backend.as_mut().map(|backend| backend as &mut dyn Write) {
                    handle_terminal_events_with_areas(
                        &mut app,
                        output_area,
                        status_area,
                        Some(serial),
                        &args,
                        events,
                    )?;
                } else {
                    handle_terminal_events_with_areas(
                        &mut app,
                        output_area,
                        status_area,
                        None,
                        &args,
                        events,
                    )?;
                }
                if let Some(text) = app.pending_clipboard.take() {
                    write_osc52_copy(terminal.backend_mut(), &text)?;
                }
                if app.reconnect_requested {
                    app.reconnect_requested = false;
                    app.connected_port = None;
                    app.backend_label = "disconnected".to_string();
                    app.manifest = None;
                    app.input.clear();
                    backend = None;
                    host_session = HostSession::new(args.max_payload_len).map_err(|status| {
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("host session init failed: {status}"),
                        )
                    })?;
                    last_connect_attempt = Instant::now() - reconnect_delay;
                    app.push_marker(format!("target changed: {}", app.serial.summary()));
                }
                dirty = true;
            }
            InteractiveEvent::Timeout => {
                if app.selection_auto_scroll.is_some() {
                    dirty = true;
                }
                if backend.is_none() && last_connect_attempt.elapsed() >= reconnect_delay {
                    continue;
                }
                if app.exit_escape_started_at.is_some_and(|started_at| {
                    started_at.elapsed()
                        >= Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
                }) {
                    dirty = true;
                }
            }
        }
    }

    Ok(())
}

fn collect_terminal_burst(first: Event) -> io::Result<Vec<Event>> {
    let mut events = vec![first];
    if !events.first().is_some_and(
        |event| matches!(event, Event::Mouse(mouse) if mouse_wheel_direction(mouse).is_some()),
    ) {
        return Ok(events);
    }

    while events.len() < TERMINAL_BURST_DRAIN_LIMIT {
        let Some(event) = interactive::drain_terminal_event()? else {
            break;
        };
        events.push(event);
    }
    Ok(events)
}

fn next_deadline(
    reconnect_pending: bool,
    reconnect_at: Instant,
    escape_at: Option<Instant>,
    render_at: Option<Instant>,
) -> Option<Instant> {
    [
        reconnect_pending.then_some(reconnect_at),
        escape_at,
        render_at,
    ]
    .into_iter()
    .flatten()
    .min()
}

fn resolve_tui_fps(override_fps: Option<u16>) -> u16 {
    if let Some(fps) = override_fps {
        return fps;
    }
    if std::env::var("TERM").is_ok_and(|value| value == "xterm-ghostty")
        || std::env::var("TERM_PROGRAM").is_ok_and(|value| value.eq_ignore_ascii_case("ghostty"))
    {
        120
    } else {
        60
    }
}

#[cfg(test)]
fn handle_key(
    app: &mut App,
    serial: Option<&mut dyn Write>,
    args: &TuiArgs,
    key: KeyEvent,
) -> io::Result<()> {
    handle_key_with_areas(
        app,
        Rect::new(0, 0, 0, 0),
        Rect::new(0, 0, 0, 0),
        serial,
        args,
        key,
    )
}

fn handle_key_with_areas(
    app: &mut App,
    output_area: Rect,
    status_area: Rect,
    serial: Option<&mut dyn Write>,
    args: &TuiArgs,
    key: KeyEvent,
) -> io::Result<()> {
    let mut serial = serial;
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
        && key.code == KeyCode::Char('c')
    {
        app.should_quit = true;
        return Ok(());
    }

    if app.settings.is_some() {
        handle_settings_key(app, key)?;
        return Ok(());
    }

    if app.has_selection() && is_copy_key(key) {
        app.request_copy_selection(output_area, status_area);
        return Ok(());
    }

    if key.code == KeyCode::Esc && app.selection.is_some() {
        app.clear_selection();
        app.status = "selection cleared".to_string();
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
        match serial.as_mut() {
            Some(port) => apply_pending_escape(app, Some(&mut **port), args)?,
            None => apply_pending_escape(app, None, args)?,
        }
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
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.settings = Some(SettingsState::new(app.serial.clone()));
                app.status = "settings".to_string();
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
        InputState::Passthrough(_) => match serial.as_mut() {
            Some(port) => send_tui_passthrough_key(app, Some(&mut **port), args, key)?,
            None => send_tui_passthrough_key(app, None, args, key)?,
        },
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
                if let Some(port) = serial.as_mut() {
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
    serial: Option<&mut dyn Write>,
    args: &TuiArgs,
) -> io::Result<bool> {
    if app.exit_escape_started_at.is_some_and(|started_at| {
        started_at.elapsed() >= Duration::from_millis(PASSTHROUGH_EXIT_ESCAPE_TIMEOUT_MS)
    }) {
        app.exit_escape_started_at = None;
        apply_pending_escape(app, serial, args)?;
        return Ok(true);
    }
    Ok(false)
}

fn apply_pending_escape(
    app: &mut App,
    serial: Option<&mut dyn Write>,
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
    serial: Option<&mut dyn Write>,
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

fn handle_settings_key(app: &mut App, key: KeyEvent) -> io::Result<()> {
    let Some(mut settings) = app.settings.take() else {
        return Ok(());
    };

    if let Some(popup) = settings.popup.take() {
        if handle_settings_popup_key(app, &mut settings, popup, key)? {
            app.settings = Some(settings);
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Up => {
            settings.selected = settings
                .selected
                .checked_sub(1)
                .unwrap_or_else(|| settings.rows().len().saturating_sub(1));
            app.settings = Some(settings);
        }
        KeyCode::Down => {
            settings.selected = (settings.selected + 1) % settings.rows().len();
            app.settings = Some(settings);
        }
        KeyCode::Enter => match settings.selected_row() {
            SettingsRow::Field(field) => {
                settings.popup = Some(open_settings_field_popup(field, &settings.draft));
                app.settings = Some(settings);
            }
            SettingsRow::Action(SettingsAction::Apply) => {
                apply_settings_profile(app, settings.draft);
            }
            SettingsRow::Action(SettingsAction::SaveDefaults) => {
                save_settings_profile(app, &mut settings)?;
                app.settings = Some(settings);
            }
            SettingsRow::Action(SettingsAction::Discard) => {
                app.status = "settings discarded".to_string();
                app.settings = None;
            }
        },
        KeyCode::Esc => {
            if settings.is_dirty() {
                settings.popup = Some(SettingsPopup::ConfirmExit { selected: 2 });
                app.settings = Some(settings);
            } else {
                app.status = "settings closed".to_string();
                app.settings = None;
            }
        }
        _ => {
            app.settings = Some(settings);
        }
    }
    Ok(())
}

fn handle_settings_popup_key(
    app: &mut App,
    settings: &mut SettingsState,
    popup: SettingsPopup,
    key: KeyEvent,
) -> io::Result<bool> {
    match popup {
        SettingsPopup::TextInput {
            field,
            mut value,
            mut cursor,
        } => match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => match commit_settings_text(field, &value, &mut settings.draft) {
                Ok(()) => app.status = format!("updated {}", settings_field_label(field)),
                Err(err) => settings.popup = Some(SettingsPopup::Message(err)),
            },
            KeyCode::Left => {
                cursor = cursor.saturating_sub(1);
                settings.popup = Some(SettingsPopup::TextInput {
                    field,
                    value,
                    cursor,
                });
            }
            KeyCode::Right => {
                cursor = (cursor + 1).min(value.len());
                settings.popup = Some(SettingsPopup::TextInput {
                    field,
                    value,
                    cursor,
                });
            }
            KeyCode::Backspace => {
                if cursor > 0 {
                    let remove_at = cursor - 1;
                    value.remove(remove_at);
                    cursor = remove_at;
                }
                settings.popup = Some(SettingsPopup::TextInput {
                    field,
                    value,
                    cursor,
                });
            }
            KeyCode::Char(ch) => {
                value.insert(cursor, ch);
                cursor += ch.len_utf8();
                settings.popup = Some(SettingsPopup::TextInput {
                    field,
                    value,
                    cursor,
                });
            }
            _ => {
                settings.popup = Some(SettingsPopup::TextInput {
                    field,
                    value,
                    cursor,
                });
            }
        },
        SettingsPopup::ChoiceList {
            field,
            mut selected,
        } => match key.code {
            KeyCode::Esc => {}
            KeyCode::Up => {
                let len = settings_choice_len(field);
                selected = selected.checked_sub(1).unwrap_or(len.saturating_sub(1));
                settings.popup = Some(SettingsPopup::ChoiceList { field, selected });
            }
            KeyCode::Down => {
                selected = (selected + 1) % settings_choice_len(field);
                settings.popup = Some(SettingsPopup::ChoiceList { field, selected });
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                apply_settings_choice(field, selected, &mut settings.draft);
                app.status = format!("updated {}", settings_field_label(field));
            }
            _ => settings.popup = Some(SettingsPopup::ChoiceList { field, selected }),
        },
        SettingsPopup::ConfirmExit { mut selected } => match key.code {
            KeyCode::Esc => {}
            KeyCode::Left => {
                selected = selected.saturating_sub(1);
                settings.popup = Some(SettingsPopup::ConfirmExit { selected });
            }
            KeyCode::Right => {
                selected = (selected + 1).min(2);
                settings.popup = Some(SettingsPopup::ConfirmExit { selected });
            }
            KeyCode::Enter => match selected {
                0 => {
                    save_settings_profile(app, settings)?;
                    apply_settings_profile(app, settings.draft.clone());
                    return Ok(false);
                }
                1 => {
                    app.status = "settings discarded".to_string();
                    return Ok(false);
                }
                _ => settings.popup = None,
            },
            _ => settings.popup = Some(SettingsPopup::ConfirmExit { selected }),
        },
        SettingsPopup::Message(_) => {
            if !matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                settings.popup = Some(popup);
            }
        }
    }
    Ok(true)
}

fn open_settings_field_popup(field: SettingsField, profile: &SerialProfile) -> SettingsPopup {
    match field {
        SettingsField::Port | SettingsField::Baud => {
            let value = settings_field_value(field, profile);
            SettingsPopup::TextInput {
                field,
                cursor: value.len(),
                value,
            }
        }
        SettingsField::DataBits
        | SettingsField::StopBits
        | SettingsField::Parity
        | SettingsField::FlowControl => SettingsPopup::ChoiceList {
            field,
            selected: current_choice_index(field, profile),
        },
    }
}

fn commit_settings_text(
    field: SettingsField,
    value: &str,
    profile: &mut SerialProfile,
) -> Result<(), String> {
    match field {
        SettingsField::Port => {
            if value.trim().is_empty() {
                return Err("port cannot be empty".to_string());
            }
            profile.port = PathBuf::from(value.trim());
            Ok(())
        }
        SettingsField::Baud => {
            let baud = value
                .trim()
                .parse()
                .map_err(|_| format!("invalid baud value: {value}"))?;
            if baud == 0 {
                return Err("baud must be greater than 0".to_string());
            }
            profile.baud = baud;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn settings_choice_len(field: SettingsField) -> usize {
    match field {
        SettingsField::DataBits => 4,
        SettingsField::StopBits => 2,
        SettingsField::Parity => SerialParity::VALUES.len(),
        SettingsField::FlowControl => SerialFlowControl::VALUES.len(),
        _ => 0,
    }
}

fn current_choice_index(field: SettingsField, profile: &SerialProfile) -> usize {
    match field {
        SettingsField::DataBits => [5, 6, 7, 8]
            .iter()
            .position(|value| *value == profile.data_bits)
            .unwrap_or(3),
        SettingsField::StopBits => [1, 2]
            .iter()
            .position(|value| *value == profile.stop_bits)
            .unwrap_or(0),
        SettingsField::Parity => SerialParity::VALUES
            .iter()
            .position(|value| *value == profile.parity)
            .unwrap_or(0),
        SettingsField::FlowControl => SerialFlowControl::VALUES
            .iter()
            .position(|value| *value == profile.flow_control)
            .unwrap_or(0),
        _ => 0,
    }
}

fn apply_settings_choice(field: SettingsField, selected: usize, profile: &mut SerialProfile) {
    match field {
        SettingsField::DataBits => profile.data_bits = [5, 6, 7, 8][selected.min(3)],
        SettingsField::StopBits => profile.stop_bits = [1, 2][selected.min(1)],
        SettingsField::Parity => profile.parity = SerialParity::VALUES[selected.min(2)],
        SettingsField::FlowControl => {
            profile.flow_control = SerialFlowControl::VALUES[selected.min(2)];
        }
        _ => {}
    }
}

fn apply_settings_profile(app: &mut App, profile: SerialProfile) {
    app.serial = profile;
    app.reconnect_requested = true;
    app.status = "settings applied; reconnecting".to_string();
    app.settings = None;
}

fn save_settings_profile(app: &mut App, settings: &mut SettingsState) -> io::Result<()> {
    HostConfig::from_serial_profile(&settings.draft).save(&app.config_path)?;
    settings.baseline = settings.draft.clone();
    app.status = format!("settings saved to {}", app.config_path.display());
    settings.popup = Some(SettingsPopup::Message("settings saved".to_string()));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollbarMouseTarget {
    UpButton,
    DownButton,
    Track(usize),
}

#[cfg(test)]
fn handle_terminal_events<I>(
    app: &mut App,
    output_area: Rect,
    serial: Option<&mut dyn Write>,
    args: &TuiArgs,
    events: I,
) -> io::Result<()>
where
    I: IntoIterator<Item = Event>,
{
    handle_terminal_events_with_areas(
        app,
        output_area,
        Rect::new(0, 0, 0, 0),
        serial,
        args,
        events,
    )
}

fn handle_terminal_events_with_areas<I>(
    app: &mut App,
    output_area: Rect,
    status_area: Rect,
    serial: Option<&mut dyn Write>,
    args: &TuiArgs,
    events: I,
) -> io::Result<()>
where
    I: IntoIterator<Item = Event>,
{
    let mut serial = serial;
    let mut wheel_run = None;
    let mut wheel_count = 0usize;

    for event in events {
        if let Event::Mouse(mouse) = event {
            if let Some(direction) = mouse_wheel_direction(&mouse) {
                if wheel_run == Some(direction) {
                    wheel_count = wheel_count.saturating_add(1);
                } else {
                    flush_wheel_run(app, output_area, wheel_run, wheel_count);
                    wheel_run = Some(direction);
                    wheel_count = 1;
                }
                continue;
            }

            flush_wheel_run(app, output_area, wheel_run, wheel_count);
            wheel_run = None;
            wheel_count = 0;
            handle_mouse_with_areas(app, output_area, status_area, mouse);
        } else {
            flush_wheel_run(app, output_area, wheel_run, wheel_count);
            wheel_run = None;
            wheel_count = 0;

            if let Event::Key(key) = event {
                let serial = serial.as_mut().map(|port| &mut **port as &mut dyn Write);
                handle_key_with_areas(app, output_area, status_area, serial, args, key)?;
                if app.should_quit {
                    return Ok(());
                }
            }
        }
    }

    flush_wheel_run(app, output_area, wheel_run, wheel_count);
    Ok(())
}

fn flush_wheel_run(
    app: &mut App,
    output_area: Rect,
    direction: Option<WheelDirection>,
    count: usize,
) {
    match direction {
        Some(WheelDirection::Up) => app.scroll_up_by(output_area, count),
        Some(WheelDirection::Down) => app.scroll_down_by(output_area, count),
        None => {}
    }
}

fn mouse_wheel_direction(mouse: &MouseEvent) -> Option<WheelDirection> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(WheelDirection::Up),
        MouseEventKind::ScrollDown => Some(WheelDirection::Down),
        _ => None,
    }
}

#[cfg(test)]
fn handle_mouse(app: &mut App, output_area: Rect, mouse: MouseEvent) {
    handle_mouse_with_areas(app, output_area, Rect::new(0, 0, 0, 0), mouse);
}

fn handle_mouse_with_areas(app: &mut App, output_area: Rect, status_area: Rect, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.scroll_up(output_area),
        MouseEventKind::ScrollDown => app.scroll_down(output_area),
        MouseEventKind::Down(MouseButton::Left) => {
            let output_height = output_content_height(output_area);
            let output_width = output_content_width(output_area);
            let total_rows = app.filtered_visual_row_count(output_width);
            match scrollbar_mouse_target(
                output_area,
                total_rows,
                output_height,
                mouse.column,
                mouse.row,
            ) {
                Some(ScrollbarMouseTarget::UpButton) => {
                    app.clear_selection();
                    app.jump_to_oldest_visible(output_area);
                }
                Some(ScrollbarMouseTarget::DownButton) => {
                    app.clear_selection();
                    app.restore_auto_follow();
                }
                Some(ScrollbarMouseTarget::Track(offset)) => {
                    app.clear_selection();
                    app.dragging_scrollbar = true;
                    app.animate_to_scroll_offset(offset, output_area);
                }
                None => {
                    app.dragging_scrollbar = false;
                    if start_text_selection(app, output_area, status_area, mouse) {
                        app.reset_empty_enter_restore();
                    } else {
                        app.clear_selection();
                        app.reset_empty_enter_restore();
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_scrollbar => {
            let output_height = output_content_height(output_area);
            let output_width = output_content_width(output_area);
            let total_rows = app.filtered_visual_row_count(output_width);
            let offset =
                scrollbar_offset_from_drag_row(output_area, total_rows, output_height, mouse.row);
            app.animate_to_scroll_offset(offset, output_area);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            update_text_selection(app, output_area, status_area, mouse);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging_scrollbar = false;
            app.selection_auto_scroll = None;
            finish_text_selection(app);
            app.reset_empty_enter_restore();
        }
        _ => app.reset_empty_enter_restore(),
    }
}

fn start_text_selection(
    app: &mut App,
    output_area: Rect,
    status_area: Rect,
    mouse: MouseEvent,
) -> bool {
    if let Some(position) = output_selection_position(app, output_area, mouse.column, mouse.row) {
        app.selection = Some(TextSelection::new(SelectionPane::Output, position));
        app.selection_auto_scroll = None;
        return true;
    }

    let status = status_rows(app);
    if let Some(position) = status_selection_position(&status, status_area, mouse.column, mouse.row)
    {
        app.selection = Some(TextSelection::new(SelectionPane::Status, position));
        app.selection_auto_scroll = None;
        return true;
    }

    false
}

fn update_text_selection(app: &mut App, output_area: Rect, status_area: Rect, mouse: MouseEvent) {
    let Some(pane) = app.selection.as_ref().map(|selection| selection.pane) else {
        return;
    };

    match pane {
        SelectionPane::Output => {
            apply_selection_edge_scroll(app, output_area, mouse.row);
            if let Some(position) =
                output_selection_position(app, output_area, mouse.column, mouse.row)
            {
                if let Some(selection) = app.selection.as_mut() {
                    selection.cursor = position;
                }
            }
        }
        SelectionPane::Status => {
            let rows = status_rows(app);
            if let Some(position) =
                status_selection_position(&rows, status_area, mouse.column, mouse.row)
            {
                if let Some(selection) = app.selection.as_mut() {
                    selection.cursor = position;
                }
            }
        }
    }
}

fn finish_text_selection(app: &mut App) {
    if let Some(selection) = app.selection.as_mut() {
        selection.active = false;
        if selection_is_empty(selection) {
            app.selection = None;
        }
    }
}

fn apply_selection_edge_scroll(app: &mut App, output_area: Rect, row: u16) {
    if output_area.height <= 2 {
        app.selection_auto_scroll = None;
        return;
    }

    let top = output_area.y.saturating_add(1);
    let bottom = output_area
        .y
        .saturating_add(output_area.height.saturating_sub(2));
    if row <= top {
        app.selection_auto_scroll = Some(SelectionAutoScroll::Up);
        app.scroll_up_by(output_area, SELECTION_EDGE_SCROLL_LINES);
    } else if row >= bottom {
        app.selection_auto_scroll = Some(SelectionAutoScroll::Down);
        app.scroll_down_by(output_area, SELECTION_EDGE_SCROLL_LINES);
    } else {
        app.selection_auto_scroll = None;
    }
}

fn output_selection_position(
    app: &App,
    output_area: Rect,
    column: u16,
    row: u16,
) -> Option<SelectionPosition> {
    if !inside_content(output_area, column, row) {
        return None;
    }

    let model = output_render_model(app, output_area);
    let visible_row = row.saturating_sub(output_area.y).saturating_sub(1) as usize;
    let row_index = model.visible_start.saturating_add(visible_row);
    let render_row = model.rows.get(row_index)?;
    let text = row_text(render_row);
    Some(SelectionPosition {
        row: row_index,
        col: content_col(output_area, column).min(char_len(&text)),
    })
}

fn status_selection_position(
    rows: &[StatusRow],
    status_area: Rect,
    column: u16,
    row: u16,
) -> Option<SelectionPosition> {
    if !inside_content(status_area, column, row) {
        return None;
    }

    let row_index = row.saturating_sub(status_area.y).saturating_sub(1) as usize;
    let status_row = rows.get(row_index)?;
    Some(SelectionPosition {
        row: row_index,
        col: content_col(status_area, column).min(char_len(&status_row_text(status_row))),
    })
}

fn inside_content(area: Rect, column: u16, row: u16) -> bool {
    area.width > 2
        && area.height > 2
        && column > area.x
        && column < area.x.saturating_add(area.width).saturating_sub(1)
        && row > area.y
        && row < area.y.saturating_add(area.height).saturating_sub(1)
}

fn content_col(area: Rect, column: u16) -> usize {
    column.saturating_sub(area.x).saturating_sub(1) as usize
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

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let chunks = main_layout(frame.area());

    let output_area = chunks[0];
    let output_model = output_render_model(app, output_area);
    let visible = output_model.rows[output_model.visible_start..output_model.visible_end]
        .iter()
        .collect::<Vec<_>>();
    let lines = visible
        .iter()
        .enumerate()
        .map(|(index, line)| {
            render_output_row(
                line,
                output_model.visible_start.saturating_add(index),
                app.selection.as_ref(),
            )
        })
        .collect::<Vec<_>>();

    let output = Paragraph::new(lines).block(
        Block::default()
            .title(output_title(app.scroll_offset))
            .borders(Borders::ALL),
    );
    frame.render_widget(output, output_area);

    let total_rendered_lines = output_model.rows.len();
    let max_offset = max_scroll_offset(total_rendered_lines, output_model.content_height);
    if max_offset > 0 {
        let mut scrollbar_state = ScrollbarState::new(total_rendered_lines)
            .position(scrollbar_position(
                total_rendered_lines,
                output_model.content_height,
                app.scroll_offset,
            ))
            .viewport_content_length(output_model.content_height);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            output_area,
            &mut scrollbar_state,
        );
    }

    let status_lines = status_rows(app)
        .iter()
        .enumerate()
        .map(|(index, row)| render_status_row(row, index, app.selection.as_ref()))
        .collect::<Vec<_>>();
    let status =
        Paragraph::new(status_lines).block(Block::default().title("status").borders(Borders::ALL));
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
    if app.settings.is_some() {
        render_settings(frame, app);
    } else {
        set_cursor_position(frame, app, output_area, &visible, chunks[2]);
    }
}

fn render_settings(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        let modal = centered_rect(area, 46, 5);
        frame.render_widget(Clear, modal);
        frame.render_widget(
            Paragraph::new("Resize terminal to at least 80x24")
                .block(Block::default().title("settings").borders(Borders::ALL)),
            modal,
        );
        return;
    }

    let Some(settings) = app.settings.as_ref() else {
        return;
    };
    let panel = centered_rect(area, 76, 18);
    frame.render_widget(Clear, panel);
    let title = if settings.is_dirty() {
        "wiremux settings *"
    } else {
        "wiremux settings"
    };
    let rows = settings
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| render_settings_row(*row, index == settings.selected, &settings.draft))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(rows).block(Block::default().title(title).borders(Borders::ALL)),
        panel,
    );

    if let Some(popup) = settings.popup.as_ref() {
        render_settings_popup(frame, panel, popup);
    }
}

fn render_settings_row(row: SettingsRow, selected: bool, profile: &SerialProfile) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let text = match row {
        SettingsRow::Field(field) => {
            format!(
                "{marker}  {:<18} ({}) --->",
                settings_field_label(field),
                settings_field_value(field, profile)
            )
        }
        SettingsRow::Action(action) => format!("{marker}  {} --->", settings_action_label(action)),
    };
    if selected {
        Line::from(Span::styled(
            text,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(text)
    }
}

fn render_settings_popup(frame: &mut ratatui::Frame<'_>, parent: Rect, popup: &SettingsPopup) {
    match popup {
        SettingsPopup::TextInput { field, value, .. } => {
            let area = centered_rect(parent, 52, 5);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!("> {value}")).block(
                    Block::default()
                        .title(settings_field_label(*field))
                        .borders(Borders::ALL),
                ),
                area,
            );
        }
        SettingsPopup::ChoiceList { field, selected } => {
            let area = centered_rect(parent, 42, 8);
            frame.render_widget(Clear, area);
            let rows = settings_choice_labels(*field)
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    let text = if index == *selected {
                        format!("> <*>{label}")
                    } else {
                        format!("  < >{label}")
                    };
                    if index == *selected {
                        Line::from(Span::styled(
                            text,
                            Style::default().fg(Color::Black).bg(Color::White),
                        ))
                    } else {
                        Line::from(text)
                    }
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(rows).block(
                    Block::default()
                        .title(settings_field_label(*field))
                        .borders(Borders::ALL),
                ),
                area,
            );
        }
        SettingsPopup::ConfirmExit { selected } => {
            let area = centered_rect(parent, 56, 6);
            frame.render_widget(Clear, area);
            let labels = ["Save + Apply", "Discard", "Cancel"];
            let buttons = labels
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    if index == *selected {
                        format!("[ {label} ]")
                    } else {
                        format!("  {label}  ")
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Unsaved serial settings"),
                    Line::from(buttons),
                ])
                .block(Block::default().title("confirm").borders(Borders::ALL)),
                area,
            );
        }
        SettingsPopup::Message(message) => {
            let area = centered_rect(parent, 42, 5);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(message.as_str())
                    .block(Block::default().title("message").borders(Borders::ALL)),
                area,
            );
        }
    }
}

fn centered_rect(parent: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(parent.width);
    let height = height.min(parent.height);
    Rect::new(
        parent.x + parent.width.saturating_sub(width) / 2,
        parent.y + parent.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn settings_field_label(field: SettingsField) -> &'static str {
    match field {
        SettingsField::Port => "Serial Device",
        SettingsField::Baud => "Baud Rate",
        SettingsField::DataBits => "Data Bits",
        SettingsField::StopBits => "Stop Bits",
        SettingsField::Parity => "Parity",
        SettingsField::FlowControl => "Flow Control",
    }
}

fn settings_action_label(action: SettingsAction) -> &'static str {
    match action {
        SettingsAction::Apply => "Apply And Reconnect",
        SettingsAction::SaveDefaults => "Save As Defaults",
        SettingsAction::Discard => "Discard And Close",
    }
}

fn settings_field_value(field: SettingsField, profile: &SerialProfile) -> String {
    match field {
        SettingsField::Port => profile.port.display().to_string(),
        SettingsField::Baud => profile.baud.to_string(),
        SettingsField::DataBits => profile.data_bits.to_string(),
        SettingsField::StopBits => profile.stop_bits.to_string(),
        SettingsField::Parity => profile.parity.to_string(),
        SettingsField::FlowControl => profile.flow_control.to_string(),
    }
}

fn settings_choice_labels(field: SettingsField) -> Vec<String> {
    match field {
        SettingsField::DataBits => [5, 6, 7, 8].iter().map(|value| value.to_string()).collect(),
        SettingsField::StopBits => [1, 2].iter().map(|value| value.to_string()).collect(),
        SettingsField::Parity => SerialParity::VALUES
            .iter()
            .map(ToString::to_string)
            .collect(),
        SettingsField::FlowControl => SerialFlowControl::VALUES
            .iter()
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn render_output_row(
    row: &RenderOutputRow,
    row_index: usize,
    selection: Option<&TextSelection>,
) -> Line<'static> {
    let selected = selection_range_for_row(
        selection,
        SelectionPane::Output,
        row_index,
        char_len(&row_text(row)),
    );
    let mut spans = Vec::new();
    let prefix_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let mut offset = 0usize;
    if let Some(prefix) = row.prefix.as_deref() {
        append_selectable_spans(&mut spans, prefix, prefix_style, selected, offset);
        offset = offset.saturating_add(char_len(prefix));
    }
    append_selectable_spans(
        &mut spans,
        row.text.as_str(),
        Style::default(),
        selected,
        offset,
    );
    Line::from(spans)
}

fn render_status_row(
    row: &StatusRow,
    row_index: usize,
    selection: Option<&TextSelection>,
) -> Line<'static> {
    let selected = selection_range_for_row(
        selection,
        SelectionPane::Status,
        row_index,
        char_len(&status_row_text(row)),
    );
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for segment in &row.segments {
        append_selectable_spans(
            &mut spans,
            segment.text.as_str(),
            segment.style,
            selected,
            offset,
        );
        offset = offset.saturating_add(char_len(&segment.text));
    }
    Line::from(spans)
}

fn append_selectable_spans(
    spans: &mut Vec<Span<'static>>,
    text: &str,
    style: Style,
    selected: Option<(usize, usize)>,
    offset: usize,
) {
    let Some((start, end)) = selected else {
        spans.push(Span::styled(text.to_string(), style));
        return;
    };

    let segment_start = offset;
    let text_len = char_len(text);
    let segment_end = offset.saturating_add(text_len);
    if end <= segment_start || start >= segment_end {
        spans.push(Span::styled(text.to_string(), style));
        return;
    }

    let local_start = start.saturating_sub(segment_start).min(text_len);
    let local_end = end.saturating_sub(segment_start).min(text_len);
    if local_start > 0 {
        spans.push(Span::styled(char_range(text, 0, local_start), style));
    }
    if local_end > local_start {
        spans.push(Span::styled(
            char_range(text, local_start, local_end),
            selection_style(style),
        ));
    }
    if local_end < text_len {
        spans.push(Span::styled(char_range(text, local_end, text_len), style));
    }
}

fn selection_style(style: Style) -> Style {
    style.bg(Color::Blue).fg(Color::White)
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

fn output_render_model(app: &App, output_area: Rect) -> OutputRenderModel {
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
    let (visible_start, visible_end) = visible_window(rows.len(), height, app.scroll_offset);
    OutputRenderModel {
        rows,
        visible_start,
        visible_end,
        content_height: height,
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

fn status_rows(app: &App) -> Vec<StatusRow> {
    let label = Style::default().fg(Color::Yellow);
    vec![
        StatusRow {
            segments: vec![
                segment("filter ", label),
                segment(app.filter_label(), Style::default()),
                segment("  ", Style::default()),
                segment("input ", label),
                segment(app.input_label(), Style::default()),
                segment("  ", Style::default()),
                segment("backend ", label),
                segment(app.backend_label.as_str(), Style::default()),
                segment("  ", Style::default()),
                segment("fps ", label),
                segment(app.target_fps.to_string(), Style::default()),
                segment("  ", Style::default()),
                segment("status ", label),
                segment(status_label(app), Style::default()),
            ],
        },
        StatusRow {
            segments: vec![
                segment("conn ", label),
                segment(
                    app.connected_port
                        .as_deref()
                        .unwrap_or("disconnected")
                        .to_string(),
                    Style::default(),
                ),
                segment("  ", Style::default()),
                segment("target ", label),
                segment(app.serial.port.display().to_string(), Style::default()),
                segment("  ", Style::default()),
                segment("device ", label),
                segment(app.manifest_label(), Style::default()),
            ],
        },
    ]
}

fn segment(text: impl Into<String>, style: Style) -> StyledSegment {
    StyledSegment {
        text: text.into(),
        style,
    }
}

fn row_text(row: &RenderOutputRow) -> String {
    match row.prefix.as_deref() {
        Some(prefix) => format!("{prefix}{}", row.text),
        None => row.text.clone(),
    }
}

fn status_row_text(row: &StatusRow) -> String {
    row.segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect()
}

fn selected_output_text(selection: &TextSelection, rows: &[RenderOutputRow]) -> String {
    selected_text_from_rows(selection, rows.len(), |index| row_text(&rows[index]))
}

fn selected_status_text(
    selection: &TextSelection,
    rows: &[StatusRow],
    _status_area: Rect,
) -> String {
    selected_text_from_rows(selection, rows.len(), |index| status_row_text(&rows[index]))
}

fn selected_text_from_rows<F>(
    selection: &TextSelection,
    row_count: usize,
    mut row_text_at: F,
) -> String
where
    F: FnMut(usize) -> String,
{
    if row_count == 0 {
        return String::new();
    }
    let Some((start, end)) = normalized_selection(selection) else {
        return String::new();
    };
    if start.row >= row_count {
        return String::new();
    }

    let mut selected = Vec::new();
    let last_row = end.row.min(row_count.saturating_sub(1));
    for row_index in start.row..=last_row {
        let text = row_text_at(row_index);
        let len = char_len(&text);
        let start_col = if row_index == start.row {
            start.col.min(len)
        } else {
            0
        };
        let end_col = if row_index == end.row {
            end.col.min(len)
        } else {
            len
        };
        if end_col >= start_col {
            selected.push(char_range(&text, start_col, end_col));
        }
    }
    selected.join("\n")
}

fn selection_range_for_row(
    selection: Option<&TextSelection>,
    pane: SelectionPane,
    row_index: usize,
    row_len: usize,
) -> Option<(usize, usize)> {
    let selection = selection?;
    if selection.pane != pane {
        return None;
    }
    let (start, end) = normalized_selection(selection)?;
    if row_index < start.row || row_index > end.row {
        return None;
    }
    let start_col = if row_index == start.row {
        start.col.min(row_len)
    } else {
        0
    };
    let end_col = if row_index == end.row {
        end.col.min(row_len)
    } else {
        row_len
    };
    (end_col > start_col).then_some((start_col, end_col))
}

fn normalized_selection(
    selection: &TextSelection,
) -> Option<(SelectionPosition, SelectionPosition)> {
    if selection_is_empty(selection) {
        return None;
    }
    if selection.anchor.row < selection.cursor.row
        || (selection.anchor.row == selection.cursor.row
            && selection.anchor.col <= selection.cursor.col)
    {
        Some((selection.anchor, selection.cursor))
    } else {
        Some((selection.cursor, selection.anchor))
    }
}

fn selection_is_empty(selection: &TextSelection) -> bool {
    selection.anchor == selection.cursor
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

fn char_len(input: &str) -> usize {
    input.chars().count()
}

fn char_range(input: &str, start: usize, end: usize) -> String {
    input
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn is_copy_key(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter => true,
        KeyCode::Char('y') if key.modifiers.is_empty() => true,
        KeyCode::Char('c' | 'C')
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            true
        }
        KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::SUPER) => true,
        _ => false,
    }
}

fn main_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(4),
            Constraint::Length(3),
        ])
        .split(area)
}

fn status_label(app: &App) -> String {
    if app.scroll_offset == 0 {
        return app.status.clone();
    }
    format!("scrollback: {} lines from bottom", app.scroll_offset)
}

fn output_title(scroll_offset: usize) -> String {
    if scroll_offset == 0 {
        "wiremux".to_string()
    } else {
        format!("wiremux - scrollback +{scroll_offset}")
    }
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

fn scrollbar_position(total_lines: usize, output_height: usize, scroll_offset: usize) -> usize {
    visible_window(total_lines, output_height, scroll_offset).0
}

#[cfg(test)]
fn scrollbar_offset_from_mouse(
    output_area: Rect,
    total_lines: usize,
    output_height: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    match scrollbar_mouse_target(output_area, total_lines, output_height, column, row)? {
        ScrollbarMouseTarget::UpButton => Some(max_scroll_offset(total_lines, output_height)),
        ScrollbarMouseTarget::DownButton => Some(0),
        ScrollbarMouseTarget::Track(offset) => Some(offset),
    }
}

fn scrollbar_mouse_target(
    output_area: Rect,
    total_lines: usize,
    output_height: usize,
    column: u16,
    row: u16,
) -> Option<ScrollbarMouseTarget> {
    if output_area.width == 0 || output_area.height == 0 {
        return None;
    }

    let scrollbar_column = output_area.x + output_area.width - 1;
    let row_end = output_area.y + output_area.height;
    if column != scrollbar_column || row < output_area.y || row >= row_end {
        return None;
    }

    if row == output_area.y {
        return Some(ScrollbarMouseTarget::UpButton);
    }

    if output_area.height > 1 && row == row_end.saturating_sub(1) {
        return Some(ScrollbarMouseTarget::DownButton);
    }

    Some(ScrollbarMouseTarget::Track(scrollbar_offset_from_drag_row(
        output_area,
        total_lines,
        output_height,
        row,
    )))
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
    use host_session::{ChannelDescriptor, CHANNEL_INTERACTION_PASSTHROUGH, DIRECTION_OUTPUT};
    use ratatui::backend::TestBackend;

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
        assert_eq!(scrollbar_position(24, 4, 0), 20);
        assert_eq!(scrollbar_position(24, 4, 11), 9);
        assert_eq!(scrollbar_position(24, 4, 20), 0);
        assert_eq!(scrollbar_position(24, 4, 50), 0);
    }

    #[test]
    fn status_label_uses_current_scroll_offset_while_scrolled() {
        let mut app = App::new("diag.log".to_string());
        app.status = "scrollbar: 796 lines from bottom".to_string();
        app.scroll_offset = 880;

        assert_eq!(status_label(&app), "scrollback: 880 lines from bottom");

        app.scroll_offset = 0;
        assert_eq!(status_label(&app), "scrollbar: 796 lines from bottom");
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
    fn scrollbar_mouse_maps_buttons_to_extreme_offsets() {
        let output_area = Rect::new(0, 0, 40, 12);

        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 0),
            Some(20)
        );
        assert_eq!(
            scrollbar_offset_from_mouse(output_area, 30, 10, 39, 11),
            Some(0)
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
    fn wheel_burst_down_to_tail_then_up_uses_latest_direction() {
        let mut app = app_with_lines(10);
        let output_area = output_area_for_content(38, 4);
        app.scroll_offset = 2;

        handle_terminal_events(
            &mut app,
            output_area,
            None,
            &tui_args(),
            vec![
                Event::Mouse(mouse_scroll(MouseEventKind::ScrollDown)),
                Event::Mouse(mouse_scroll(MouseEventKind::ScrollDown)),
                Event::Mouse(mouse_scroll(MouseEventKind::ScrollDown)),
                Event::Mouse(mouse_scroll(MouseEventKind::ScrollUp)),
            ],
        )
        .expect("handle terminal burst");

        assert_eq!(app.scroll_offset, WHEEL_SCROLL_LINES);
        assert_eq!(app.status, "scrollback paused: 1 lines from bottom");
    }

    #[test]
    fn wheel_burst_does_not_delay_quit_key_after_tail_follow() {
        let mut app = app_with_lines(10);
        let output_area = output_area_for_content(38, 4);
        app.scroll_offset = 2;
        let mut events = Vec::new();
        for _ in 0..100 {
            events.push(Event::Mouse(mouse_scroll(MouseEventKind::ScrollDown)));
        }
        events.push(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));

        handle_terminal_events(&mut app, output_area, None, &tui_args(), events)
            .expect("handle terminal burst");

        assert_eq!(app.scroll_offset, 0);
        assert!(app.should_quit);
    }

    #[test]
    fn scrollbar_drag_animates_toward_target_until_mouse_release() {
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
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.scroll_target_offset, Some(20));

        assert!(app.advance_scroll_animation());
        assert!(app.scroll_offset > 0);
        assert!(app.scroll_offset < 20);

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
        assert_eq!(app.scroll_target_offset, Some(0));

        while app.advance_scroll_animation() {}
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
    fn scrollbar_buttons_jump_without_animation_and_down_follows_appends() {
        let mut app = app_with_lines(30);
        let output_area = Rect::new(0, 0, 40, 12);
        let scrollbar_column = output_area.x + output_area.width - 1;
        let scrollbar_bottom_row = output_area.y + output_area.height - 1;

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: scrollbar_column,
                row: output_area.y,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.scroll_target_offset, None);
        assert!(!app.dragging_scrollbar);
        assert_eq!(app.status, "scrollback paused: 20 lines from bottom");

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: scrollbar_column,
                row: scrollbar_bottom_row,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.scroll_target_offset, None);
        assert!(!app.dragging_scrollbar);
        assert_eq!(app.status, "scrollback: following live output");

        app.push_line(None, "line 30\n".to_string());
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.scroll_target_offset, None);
    }

    #[test]
    fn output_mouse_selection_highlights_and_copies_visible_text() {
        let mut app = app_with_lines(2);
        let area = Rect::new(0, 0, 80, 12);
        let output_area = main_layout(area)[0];
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: output_area.x + 5,
                row: output_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: output_area.x + 5,
                row: output_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );

        assert!(app.has_selection());
        app.request_copy_selection(output_area, Rect::new(0, 0, 0, 0));
        assert_eq!(app.pending_clipboard.as_deref(), Some("line"));

        terminal.draw(|frame| render(frame, &app)).expect("draw");
        assert_eq!(
            terminal.backend().buffer()[(output_area.x + 1, output_area.y + 1)].bg,
            Color::Blue
        );
    }

    #[test]
    fn status_mouse_selection_copies_status_text() {
        let mut app = App::new("diag.log".to_string());
        let area = Rect::new(0, 0, 80, 12);
        let chunks = main_layout(area);
        let status_area = chunks[1];
        let output_area = chunks[0];

        handle_mouse_with_areas(
            &mut app,
            output_area,
            status_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: status_area.x + 1,
                row: status_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse_with_areas(
            &mut app,
            output_area,
            status_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: status_area.x + 7,
                row: status_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse_with_areas(
            &mut app,
            output_area,
            status_area,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: status_area.x + 7,
                row: status_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );

        app.request_copy_selection(output_area, status_area);

        assert_eq!(app.pending_clipboard.as_deref(), Some("filter"));
    }

    #[test]
    fn selection_edge_drag_scrolls_output_up_and_down() {
        let mut app = app_with_lines(30);
        let output_area = Rect::new(0, 0, 40, 8);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + 2,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.scroll_offset, 1);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + output_area.height - 2,
                modifiers: KeyModifiers::empty(),
            },
        );
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn selection_auto_scroll_continues_without_more_mouse_events() {
        let mut app = app_with_lines(30);
        let output_area = Rect::new(0, 0, 40, 8);

        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + 2,
                modifiers: KeyModifiers::empty(),
            },
        );
        handle_mouse(
            &mut app,
            output_area,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: output_area.x + 1,
                row: output_area.y + 1,
                modifiers: KeyModifiers::empty(),
            },
        );
        let first_offset = app.scroll_offset;
        let first_cursor = app.selection.as_ref().expect("selection").cursor.row;

        assert!(app.advance_selection_auto_scroll(output_area));

        assert!(app.scroll_offset > first_offset);
        assert!(app.selection.as_ref().expect("selection").cursor.row < first_cursor);
    }

    #[test]
    fn escape_clears_selection_before_exit_prefix() {
        let mut app = app_with_lines(2);
        app.selection = Some(TextSelection {
            pane: SelectionPane::Output,
            anchor: SelectionPosition { row: 0, col: 0 },
            cursor: SelectionPosition { row: 0, col: 4 },
            active: false,
        });

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        )
        .expect("clear selection");

        assert!(app.selection.is_none());
        assert!(app.exit_escape_started_at.is_none());
    }

    #[test]
    fn copy_key_writes_pending_clipboard_without_clearing_selection() {
        let mut app = app_with_lines(2);
        let output_area = output_area_for_content(38, 4);
        app.selection = Some(TextSelection {
            pane: SelectionPane::Output,
            anchor: SelectionPosition { row: 0, col: 0 },
            cursor: SelectionPosition { row: 0, col: 4 },
            active: false,
        });

        handle_key_with_areas(
            &mut app,
            output_area,
            Rect::new(0, 0, 0, 0),
            None,
            &tui_args(),
            KeyEvent::new(
                KeyCode::Char('C'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
        )
        .expect("copy selection");

        assert_eq!(app.pending_clipboard.as_deref(), Some("line"));
        assert!(app.selection.is_some());
    }

    #[test]
    fn ctrl_shift_c_without_selection_does_not_quit() {
        let mut app = App::new("diag.log".to_string());

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
        )
        .expect("handle copy key without selection");

        assert!(!app.should_quit);
        assert!(app.pending_clipboard.is_none());
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
        let expected_window = (
            10usize.saturating_sub(4 + WHEEL_SCROLL_LINES),
            10usize.saturating_sub(WHEEL_SCROLL_LINES),
        );
        assert_eq!(
            visible_window(app.filtered_line_count(), 4, app.scroll_offset),
            expected_window
        );

        app.push_line(None, "new\n".to_string());
        assert_eq!(
            visible_window(app.filtered_line_count(), 4, app.scroll_offset),
            expected_window
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
                serial: test_serial_profile(),
                config_path: PathBuf::from("/tmp/wiremux-config.toml"),
                max_payload_len: 512,
                reconnect_delay_ms: 500,
                interactive_backend: InteractiveBackendMode::Auto,
                tui_fps: None,
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
    fn render_status_shows_backend_and_fps() {
        let mut app = App::new("diag.log".to_string());
        app.backend_label = "mio".to_string();
        app.target_fps = 120;
        app.manifest = Some(line_manifest(1));
        let area = Rect::new(0, 0, 80, 12);
        let status_area = main_layout(area)[1];
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        let runtime_row = buffer_row(terminal.backend().buffer(), status_area.y + 1, area.width);
        let target_row = buffer_row(terminal.backend().buffer(), status_area.y + 2, area.width);
        assert!(runtime_row.contains("backend mio"));
        assert!(runtime_row.contains("fps 120"));
        assert!(target_row.contains("target /dev/tty.usbmodem2101"));
        assert!(target_row.contains("conn disconnected"));
    }

    #[test]
    fn ctrl_b_s_opens_settings_panel() {
        let mut app = App::new("diag.log".to_string());

        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
        )
        .expect("prefix");
        handle_key(
            &mut app,
            None,
            &tui_args(),
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty()),
        )
        .expect("settings");

        assert!(app.settings.is_some());
        assert_eq!(app.status, "settings");
    }

    #[test]
    fn settings_choice_apply_updates_serial_profile_and_reconnects() {
        let mut app = App::new("diag.log".to_string());
        app.settings = Some(SettingsState::new(app.serial.clone()));

        for _ in 0..2 {
            handle_settings_key(
                &mut app,
                KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            )
            .expect("move to data bits");
        }
        handle_settings_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        )
        .expect("open data bits");
        handle_settings_key(&mut app, KeyEvent::new(KeyCode::Up, KeyModifiers::empty()))
            .expect("select 7 data bits");
        handle_settings_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        )
        .expect("commit data bits");
        for _ in 0..4 {
            handle_settings_key(
                &mut app,
                KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            )
            .expect("move to apply");
        }
        handle_settings_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        )
        .expect("apply");

        assert_eq!(app.serial.data_bits, 7);
        assert!(app.reconnect_requested);
        assert!(app.settings.is_none());
    }

    #[test]
    fn settings_render_uses_resize_overlay_below_minimum() {
        let mut app = App::new("diag.log".to_string());
        app.settings = Some(SettingsState::new(app.serial.clone()));
        let area = Rect::new(0, 0, 60, 20);
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test terminal");

        terminal.draw(|frame| render(frame, &app)).expect("draw");

        let text = (0..area.height)
            .map(|row| buffer_row(terminal.backend().buffer(), row, area.width))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Resize terminal to at least 80x24"));
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

    #[test]
    fn base64_encode_handles_padding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn write_osc52_copy_emits_clipboard_sequence() {
        let mut output = Vec::new();

        write_osc52_copy(&mut output, "line").expect("write osc52");

        assert_eq!(
            String::from_utf8(output).expect("utf8"),
            "\x1b]52;c;bGluZQ==\x07"
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

    fn mouse_scroll(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn tui_args() -> TuiArgs {
        TuiArgs {
            serial: test_serial_profile(),
            config_path: PathBuf::from("/tmp/wiremux-config.toml"),
            max_payload_len: 512,
            reconnect_delay_ms: 500,
            interactive_backend: InteractiveBackendMode::Auto,
            tui_fps: None,
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
