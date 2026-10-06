//! Disposable composer and cell projection over the single application VT owner.
//! No descriptors, process effects, raw host escapes or second emulator live here.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::events::KeyAction;
use crate::theme::Theme;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use oc_core::domain::SessionId;
use oc_core::queries::{
    TERMINAL_INPUT_BYTES, TERMINAL_MAX_COLS, TERMINAL_MAX_ROWS, TerminalAction, TerminalColor,
    TerminalEntry, TerminalInventory, TerminalRef, TerminalReplay, TerminalSize, TerminalSnapshot,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::cell::Cell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalIntent {
    Refresh,
    Toggle(TerminalAction),
    Create(TerminalAction),
    Select(TerminalRef),
    Hide,
    Remove(TerminalRef),
    Control(TerminalAction),
    CloseComposer,
}

#[derive(Default)]
pub(crate) struct TerminalView {
    pub(super) open: bool,
    loaded: Option<SessionId>,
    rows: Vec<TerminalEntry>,
    visible: Option<TerminalRef>,
    cursor: Option<usize>,
    snapshot: Option<TerminalSnapshot>,
    focused: bool,
    ready: bool,
    gap: bool,
    painted: Cell<Option<Rect>>,
    release_guard: Option<MouseButton>,
    row_down: Option<usize>,
}

fn safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

impl TuiState {
    pub fn terminals_open(&self) -> bool {
        self.terminals.open
    }
    pub fn terminal_visible(&self) -> Option<&TerminalRef> {
        self.terminals.visible.as_ref()
    }
    pub fn terminal_focused(&self) -> bool {
        self.terminals.focused
            && self.terminals.visible.is_some()
            && *self.panel() == crate::app::TuiPanel::None
            && !self.terminals.open
    }
    pub fn terminal_snapshot(&self) -> Option<&TerminalSnapshot> {
        self.terminals.snapshot.as_ref()
    }
    pub fn terminals_need_refresh(&self) -> bool {
        self.chrome.session_terminal
            && self.attached_session().is_some()
            && self.terminals.loaded.as_ref() != self.attached_session()
    }
    pub fn terminal_cursor(&self) -> Option<u64> {
        self.terminals.snapshot.as_ref().map(|s| s.output_cursor)
    }
    pub fn terminal_ready(&self) -> bool {
        self.terminals.ready
    }
    pub fn terminal_inventory(&self) -> &[TerminalEntry] {
        &self.terminals.rows
    }

    pub fn apply_terminal_inventory(&mut self, session: &SessionId, inventory: TerminalInventory) {
        if self.attached_session() != Some(session) {
            return;
        }
        self.terminals.loaded = Some(session.clone());
        self.terminals.rows = inventory.entries;
        if self.terminals.visible != inventory.selected {
            self.terminals.snapshot = None;
            self.terminals.ready = false;
            self.terminals.gap = false;
        }
        self.terminals.visible = inventory.selected;
        if self.terminals.open && self.terminals.cursor.is_none() {
            self.terminals.cursor = self
                .terminals
                .rows
                .iter()
                .position(|r| Some(&r.target) == self.terminals.visible.as_ref());
        }
        if self.terminals.visible.is_none() {
            self.terminals.focused = false;
        }
        if self
            .terminals
            .cursor
            .is_some_and(|i| i > self.terminals.rows.len())
        {
            self.terminals.cursor = None;
        }
    }

    pub fn apply_terminal_snapshot(&mut self, snapshot: TerminalSnapshot) {
        if self.terminals.visible.as_ref() != Some(&snapshot.entry.target) {
            return;
        }
        self.terminals.ready = false; // Snapshot precedes output-cursor attachment.
        self.terminals.snapshot = Some(snapshot);
    }
    pub fn apply_terminal_replay(&mut self, replay: TerminalReplay) -> bool {
        if self.terminals.visible.as_ref() != Some(&replay.screen.entry.target)
            || self.terminal_cursor() != Some(replay.from)
            || replay.screen.output_cursor != replay.next
        {
            return false;
        }
        let changed = self
            .terminals
            .snapshot
            .as_ref()
            .is_none_or(|s| s.revision != replay.screen.revision);
        self.terminals.gap |= replay.reset.is_some();
        self.terminals.ready = replay.screen.ready;
        self.terminals.snapshot = Some(*replay.screen);
        changed
    }
    pub fn focus_terminal(&mut self, focused: bool) {
        self.terminals.focused = focused && self.terminals.visible.is_some();
        self.clear_transcript_selection();
    }
    pub fn close_terminal_composer(&mut self) {
        self.terminals.open = false;
        self.terminals.row_down = None;
    }
    pub fn inherit_terminal_view(&mut self, previous: &mut Self) {
        if self.attached_session().is_some()
            && self.attached_session() == previous.attached_session()
        {
            self.terminals = std::mem::take(&mut previous.terminals);
        }
    }
    pub fn terminal_failure(&mut self, composer: bool) {
        self.terminals.loaded = self.attached_session().cloned();
        self.terminals.ready = false;
        self.terminals.focused = false;
        if composer {
            self.push_transient_note("Unable to load terminal", crate::app::NoteVariant::Error);
        }
    }

    fn terminal_intent(&self, action: TerminalIntent, close_composer: bool) -> KeyOutcome {
        KeyOutcome {
            intent: self
                .attached_session()
                .cloned()
                .map(|session| PanelIntent::Terminal {
                    session,
                    action,
                    close_composer,
                }),
            ..Default::default()
        }
    }
    pub(crate) fn terminal_command(
        &mut self,
        action: crate::commands::CommandAction,
    ) -> KeyOutcome {
        use crate::commands::CommandAction;
        match action {
            CommandAction::SelectTerminal => {
                self.shells.open = false;
                self.children.open = false;
                self.close_panel();
                self.terminals.open = true;
                self.terminals.focused = false;
                self.terminals.cursor = self
                    .terminals
                    .rows
                    .iter()
                    .position(|r| Some(&r.target) == self.terminals.visible.as_ref());
                self.terminal_intent(TerminalIntent::Refresh, false)
            }
            CommandAction::FocusSessionPane => {
                self.focus_terminal(false);
                KeyOutcome::default()
            }
            CommandAction::FocusTerminalPane => {
                self.focus_terminal(true);
                KeyOutcome::default()
            }
            CommandAction::CloseTerminal => self.terminal_intent(TerminalIntent::Hide, false),
            CommandAction::ToggleTerminal => {
                self.terminal_intent(TerminalIntent::Toggle(self.terminal_creation()), false)
            }
            CommandAction::CreateTerminal => {
                self.terminal_intent(TerminalIntent::Create(self.terminal_creation()), false)
            }
            _ => unreachable!("terminal command"),
        }
    }
    fn terminal_creation(&self) -> TerminalAction {
        if let Some(source) = self.linked_child() {
            TerminalAction::CreateChild {
                source: Box::new(source.clone()),
                generation: self.chrome.terminal_generation,
                size: TerminalSize::default(),
            }
        } else {
            TerminalAction::Create {
                location: self.chrome.location.clone().unwrap_or_default(),
                generation: self.chrome.terminal_generation,
                size: TerminalSize::default(),
            }
        }
    }
    fn activate_terminal_row(&mut self) -> KeyOutcome {
        let Some(index) = self.terminals.cursor else {
            return KeyOutcome::default();
        };
        let action = if let Some(row) = self.terminals.rows.get(index) {
            TerminalIntent::Select(row.target.clone())
        } else {
            TerminalIntent::Create(self.terminal_creation())
        };
        self.close_terminal_composer();
        self.terminal_intent(action, true)
    }
    pub(crate) fn terminal_composer_key(&mut self, key: KeyAction) -> KeyOutcome {
        match key {
            KeyAction::Up | KeyAction::Char('k') | KeyAction::Down | KeyAction::Char('j') => {
                let len = self.terminals.rows.len() + 1;
                let up = matches!(key, KeyAction::Up | KeyAction::Char('k'));
                self.terminals.cursor = Some(match self.terminals.cursor {
                    None => {
                        if up {
                            len - 1
                        } else {
                            0
                        }
                    }
                    Some(i) => {
                        if up {
                            (i + len - 1) % len
                        } else {
                            (i + 1) % len
                        }
                    }
                });
                KeyOutcome::default()
            }
            KeyAction::Enter => self.activate_terminal_row(),
            KeyAction::Cancel | KeyAction::Interrupt => {
                self.close_terminal_composer();
                self.terminal_intent(TerminalIntent::CloseComposer, true)
            }
            KeyAction::DeleteOrQuit => self
                .terminals
                .cursor
                .and_then(|i| self.terminals.rows.get(i))
                .map(|r| r.target.clone())
                .map_or_else(KeyOutcome::default, |target| {
                    self.terminal_intent(TerminalIntent::Remove(target), false)
                }),
            _ => KeyOutcome::default(),
        }
    }

    /// Called by the binary before ANY editor/global/approval key mapping.
    pub fn raw_terminal_key(&mut self, event: KeyEvent) -> Option<KeyOutcome> {
        if !self.terminal_focused() || self.terminal_leader_bypass(event) {
            return None;
        }
        if event.kind == KeyEventKind::Release {
            return Some(KeyOutcome::default());
        }
        let bytes = encode_key(
            event,
            self.terminals
                .snapshot
                .as_ref()
                .is_some_and(|s| s.application_cursor),
        );
        Some(self.terminal_input(bytes))
    }
    pub fn raw_terminal_paste(&self, text: &str) -> Option<KeyOutcome> {
        if !self.terminal_focused() {
            return None;
        }
        if text.len() > TERMINAL_INPUT_BYTES - 12 {
            return Some(KeyOutcome {
                note: Some("Terminal input too large".into()),
                ..Default::default()
            });
        }
        let bracketed = self
            .terminals
            .snapshot
            .as_ref()
            .is_some_and(|s| s.bracketed_paste);
        let mut bytes = Vec::new();
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
        }
        bytes.extend_from_slice(text.as_bytes());
        if bracketed {
            bytes.extend_from_slice(b"\x1b[201~");
        }
        Some(self.terminal_input(bytes))
    }
    fn terminal_input(&self, bytes: Vec<u8>) -> KeyOutcome {
        if bytes.is_empty() {
            return KeyOutcome::default();
        }
        if !self.terminals.ready {
            return KeyOutcome {
                note: Some("Terminal is attaching".into()),
                ..Default::default()
            };
        }
        self.terminals
            .visible
            .as_ref()
            .map_or_else(KeyOutcome::default, |target| {
                self.terminal_intent(
                    TerminalIntent::Control(TerminalAction::Input {
                        target: target.clone(),
                        bytes,
                    }),
                    false,
                )
            })
    }

    /// Focus transfer consumes both halves of the first click, even over approval
    /// controls. Transcript wheels deliberately do not change pane focus.
    pub fn terminal_mouse(&mut self, event: MouseEvent, frame: Rect) -> Option<KeyOutcome> {
        if let MouseEventKind::Up(button) = event.kind
            && self.terminals.release_guard == Some(button)
        {
            self.terminals.release_guard = None;
            return Some(KeyOutcome::default());
        }
        let (session, pane) = regions(self, frame);
        if pane.contains((event.column, event.row).into()) {
            if matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) && let Some(target) = self.terminal_visible().cloned()
            {
                return Some(self.terminal_intent(
                    TerminalIntent::Control(TerminalAction::Scroll {
                        target,
                        lines: if event.kind == MouseEventKind::ScrollUp {
                            3
                        } else {
                            -3
                        },
                    }),
                    false,
                ));
            }
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.focus_terminal(true);
            }
            return Some(KeyOutcome::default());
        }
        if self.terminal_focused()
            && session.contains((event.column, event.row).into())
            && let MouseEventKind::Down(button) = event.kind
        {
            self.focus_terminal(false);
            self.terminals.release_guard = Some(button);
            return Some(KeyOutcome::default());
        }
        if self.terminals.open {
            let row = self
                .terminals
                .painted
                .get()
                .filter(|r| r.contains((event.column, event.row).into()))
                .and_then(|r| event.row.checked_sub(r.y + 1))
                .map(usize::from)
                .filter(|i| *i <= self.terminals.rows.len());
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                    self.terminals.row_down = row;
                    self.terminals.cursor = row;
                }
                MouseEventKind::Up(MouseButton::Left) if event.modifiers.is_empty() => {
                    if row.is_some() && self.terminals.row_down.take() == row {
                        self.terminals.cursor = row;
                        return Some(self.activate_terminal_row());
                    }
                }
                MouseEventKind::Drag(_) | MouseEventKind::Down(_) | MouseEventKind::Up(_) => {
                    self.terminals.row_down = None
                }
                _ => {}
            }
            return Some(KeyOutcome::default());
        }
        None
    }
    pub fn terminal_size_for_frame(&self, frame: Rect) -> TerminalSize {
        let (_, pane) = regions(self, frame);
        TerminalSize {
            rows: pane.height.saturating_sub(2).clamp(1, TERMINAL_MAX_ROWS),
            cols: pane.width.saturating_sub(2).clamp(1, TERMINAL_MAX_COLS),
        }
    }
}

pub(crate) fn split(state: &TuiState, area: Rect) -> (Rect, Rect) {
    if state.terminal_visible().is_none() {
        return (area, Rect::default());
    }
    let width = (area.width / 2).min(TERMINAL_MAX_COLS + 2);
    (
        Rect {
            width: area.width - width,
            ..area
        },
        Rect::new(area.right() - width, area.y, width, area.height),
    )
}
pub(crate) fn regions(state: &TuiState, frame: Rect) -> (Rect, Rect) {
    split(
        state,
        crate::layout::configured_shell_regions(
            frame,
            state.chrome.devtools_visible(),
            state.chrome.vertical_tabs_width,
        )
        .session,
    )
}
pub fn pane_hit(state: &TuiState, frame: Rect, x: u16, y: u16) -> bool {
    regions(state, frame).1.contains((x, y).into())
}
pub(crate) fn height(state: &TuiState) -> u16 {
    if state.terminals.open {
        state.terminals.rows.len() as u16 + 3
    } else {
        0
    }
}

pub(crate) fn render_composer(frame: &mut Frame<'_>, state: &TuiState, main: Rect, theme: &Theme) {
    state.terminals.painted.set(None);
    let h = height(state).min(main.height);
    if h == 0 || state.approvals.active().is_some() || state.questions.active().is_some() {
        return;
    }
    let area = Rect::new(
        main.x + 1,
        main.bottom().saturating_sub(h),
        main.width.saturating_sub(2),
        h,
    );
    state.terminals.painted.set(Some(area));
    let mut lines = Vec::new();
    for index in 0..=state.terminals.rows.len() {
        let row = state.terminals.rows.get(index);
        let title = row.map_or_else(
            || "+ New terminal".into(),
            |r| {
                safe(
                    r.foreground
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&r.title),
                )
            },
        );
        let current = row.is_some_and(|r| Some(&r.target) == state.terminal_visible());
        let selected = state.terminals.cursor == Some(index);
        lines.push(Line::from(Span::styled(
            format!(
                "{} {}{}",
                if selected { "›" } else { " " },
                title,
                if current { " · visible" } else { "" }
            ),
            Style::default()
                .fg(if selected {
                    theme.text()
                } else {
                    theme.text_muted()
                })
                .bg(if selected {
                    theme.background_panel()
                } else {
                    theme.background()
                }),
        )));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Terminals · ↑/k ↓/j · enter open · ctrl+d remove · esc hide"),
        ),
        area,
    );
}

fn terminal_color(color: TerminalColor, background: bool, theme: &Theme) -> Color {
    let bg = theme
        .color("background.base")
        .unwrap_or_else(|| theme.background());
    match color {
        TerminalColor::Default => {
            if background {
                bg
            } else {
                theme.text()
            }
        }
        TerminalColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
        TerminalColor::Indexed(index) if index < 16 => {
            let paths = [
                "background.base",
                "text.feedback.error.base",
                "text.feedback.success.base",
                "text.feedback.warning.base",
                "$hue.blue.200",
                "$hue.purple.200",
                "text.feedback.info.base",
                "text.base",
                "text.muted",
                "text.feedback.error.muted",
                "text.feedback.success.muted",
                "text.feedback.warning.muted",
                "$hue.blue.100",
                "$hue.purple.100",
                "$hue.cyan.100",
                "$hue.neutral.100",
            ];
            theme
                .color(paths[index as usize])
                .unwrap_or_else(|| match index {
                    4 | 12 => theme
                        .hue("blue", if index < 8 { 200 } else { 100 })
                        .unwrap_or(theme.text()),
                    5 | 13 => theme
                        .hue("purple", if index < 8 { 200 } else { 100 })
                        .unwrap_or(theme.text()),
                    14 => theme.hue("cyan", 100).unwrap_or(theme.text()),
                    15 => theme.hue("neutral", 100).unwrap_or(theme.text()),
                    _ => theme.text(),
                })
        }
        TerminalColor::Indexed(index) => Color::Indexed(index),
    }
}
pub(crate) fn render_pane(frame: &mut Frame<'_>, state: &TuiState, area: Rect, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if state.terminals.gap {
            "Terminal · output gap → current screen"
        } else {
            "Terminal"
        })
        .border_style(Style::default().fg(if state.terminal_focused() {
            theme.border_active()
        } else {
            theme.text_muted()
        }));
    let inner = block.inner(area);
    frame.render_widget(
        block.style(Style::default().fg(theme.text()).bg(theme.background())),
        area,
    );
    let Some(screen) = state.terminals.snapshot.as_ref() else {
        frame.render_widget(Paragraph::new("Attaching terminal…"), inner);
        return;
    };
    for row in 0..inner.height.min(screen.size.rows) {
        for col in 0..inner.width.min(screen.size.cols) {
            let Some(cell) = screen
                .cells
                .get(usize::from(row) * usize::from(screen.size.cols) + usize::from(col))
            else {
                continue;
            };
            if cell.wide_continuation {
                continue;
            }
            let mut fg = terminal_color(cell.fg, false, theme);
            let mut bg = terminal_color(cell.bg, true, theme);
            if cell.inverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut style = Style::default().fg(fg).bg(bg);
            for (enabled, modifier) in [
                (cell.bold, Modifier::BOLD),
                (cell.dim, Modifier::DIM),
                (cell.italic, Modifier::ITALIC),
                (cell.underline, Modifier::UNDERLINED),
            ] {
                if enabled {
                    style = style.add_modifier(modifier);
                }
            }
            let text = safe(&cell.text);
            frame.buffer_mut().set_stringn(
                inner.x + col,
                inner.y + row,
                if text.is_empty() { " " } else { &text },
                usize::from(inner.width - col),
                style,
            );
        }
    }
    if state.terminal_focused()
        && state.terminals.ready
        && !screen.hide_cursor
        && screen.cursor.0 < inner.height
        && screen.cursor.1 < inner.width
    {
        frame.set_cursor_position((inner.x + screen.cursor.1, inner.y + screen.cursor.0));
    }
}

fn encode_key(event: KeyEvent, application_cursor: bool) -> Vec<u8> {
    let modifiers = event.modifiers;
    let parameter = 1
        + u8::from(modifiers.contains(KeyModifiers::SHIFT))
        + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
        + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL));
    let seq = |suffix: char| {
        if parameter > 1 {
            format!("\x1b[1;{parameter}{suffix}")
        } else {
            format!("\x1b{}{suffix}", if application_cursor { 'O' } else { '[' })
        }
    };
    let tilde = |code| {
        if parameter > 1 {
            format!("\x1b[{code};{parameter}~")
        } else {
            format!("\x1b[{code}~")
        }
    };
    let text = match event.code {
        KeyCode::Char(c) => {
            let mut s = String::new();
            if modifiers.contains(KeyModifiers::ALT) {
                s.push('\x1b');
            }
            if modifiers.contains(KeyModifiers::CONTROL) {
                let c = c.to_ascii_uppercase();
                if ('@'..='_').contains(&c) {
                    s.push(((c as u8) & 0x1f) as char);
                } else if c == '?' {
                    s.push('\x7f');
                } else if c == ' ' {
                    s.push('\0');
                }
            } else {
                s.push(c);
            }
            s
        }
        KeyCode::Enter => "\r".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Up => seq('A'),
        KeyCode::Down => seq('B'),
        KeyCode::Right => seq('C'),
        KeyCode::Left => seq('D'),
        KeyCode::Home => seq('H'),
        KeyCode::End => seq('F'),
        KeyCode::Insert => tilde(2),
        KeyCode::Delete => tilde(3),
        KeyCode::PageUp => tilde(5),
        KeyCode::PageDown => tilde(6),
        KeyCode::F(n @ 1..=4) => {
            if parameter > 1 {
                format!("\x1b[1;{parameter}{}", (b'P' + n - 1) as char)
            } else {
                format!("\x1bO{}", (b'P' + n - 1) as char)
            }
        }
        KeyCode::F(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]),
        _ => String::new(),
    };
    text.into_bytes()
}

#[cfg(test)]
mod tests;
