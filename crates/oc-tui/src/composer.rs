//! One lower surface and input scope; feature views retain their inventories,
//! selections, captured identities and effect intents.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::events::KeyAction;
use crate::theme::Theme;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use oc_core::queries::TerminalRef;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::cell::RefCell;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Subagents,
    Shell,
    Terminals,
}
impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Subagents => "Subagents",
            Self::Shell => "Shell",
            Self::Terminals => "Terminals",
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Item {
    Child(String),
    Shell(String),
    Terminal(Option<TerminalRef>),
}
#[derive(Clone, PartialEq, Eq)]
enum Hit {
    Close,
    Row(Item),
}
struct Paint {
    frame: Rect,
    area: Rect,
    tab: Tab,
    close: Rect,
    rows: Vec<(Rect, Item)>,
}
#[derive(Default)]
pub(crate) struct Composer {
    pub(super) active: Option<Tab>,
    painted: RefCell<Option<Paint>>,
    down: Option<Hit>,
}
pub(super) struct Row {
    pub item: Item,
    pub line: Line<'static>,
}
impl Row {
    pub(super) fn new(
        item: Item,
        text: &str,
        selected: bool,
        current: bool,
        status: &str,
        theme: &Theme,
        width: u16,
    ) -> Self {
        let phase = if selected {
            "$focused"
        } else if current {
            "$selected"
        } else {
            "base"
        };
        let fg = theme
            .color(&format!("text.action.primary.{phase}"))
            .unwrap_or(theme.text());
        let bg = theme
            .color(&format!("background.action.primary.{phase}"))
            .unwrap_or(theme.background_panel());
        let padding = Style::default().bg(bg);
        let style = padding.fg(fg).add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let room = usize::from(width.saturating_sub(2)).saturating_sub(status.width());
        let mut cells = 0;
        let mut end = 0;
        for (index, grapheme) in text.grapheme_indices(true) {
            if cells + grapheme.width() > room {
                break;
            }
            cells += grapheme.width();
            end = index + grapheme.len();
        }
        let caption: String = text[..end]
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let space = room.saturating_sub(caption.width());
        Self {
            item,
            line: Line::from(vec![
                Span::styled(" ", padding),
                Span::styled(caption, style),
                Span::styled(" ".repeat(space), padding),
                Span::styled(
                    status.to_owned(),
                    padding.fg(if selected { fg } else { theme.text_muted() }),
                ),
                Span::styled(" ", padding),
            ])
            .style(padding),
        }
    }
}

impl TuiState {
    pub(crate) fn composer_open(&self) -> bool {
        self.composer.active.is_some()
    }
    pub(crate) fn inherit_composer_tab(&mut self, previous: &mut Self, tab: Tab) {
        if previous.composer.active == Some(tab) {
            previous.composer.active = None;
            previous.composer.down = None;
            previous.composer.painted.get_mut().take();
            if self.composer.active.is_none() {
                self.composer.active = Some(tab);
            }
            self.composer.down = None;
            self.composer.painted.get_mut().take();
        }
    }
    pub(crate) fn open_composer(&mut self, tab: Tab) -> KeyOutcome {
        if self.attached_session().is_none() {
            return KeyOutcome::default();
        }
        if tab == Tab::Terminals && !self.chrome.session_terminal {
            return KeyOutcome::default();
        }
        self.composer.active = Some(tab);
        self.composer.down = None;
        self.composer.painted.get_mut().take();
        self.focus_terminal(false);
        match tab {
            Tab::Subagents => {
                self.children.enter(self.attached_session().cloned());
                KeyOutcome {
                    intent: Some(PanelIntent::LoadChildren),
                    ..Default::default()
                }
            }
            Tab::Shell => KeyOutcome {
                intent: Some(PanelIntent::LoadShells),
                ..Default::default()
            },
            Tab::Terminals => self.enter_terminal_composer(),
        }
    }
    pub(crate) fn close_composer(&mut self) -> KeyOutcome {
        self.composer.active = None;
        self.composer.down = None;
        self.composer.painted.get_mut().take();
        KeyOutcome {
            intent: self.linked_child().map(|_| PanelIntent::ReturnParent),
            ..Default::default()
        }
    }
    fn composer_tabs(&self) -> &'static [Tab] {
        if self.chrome.session_terminal {
            &[Tab::Subagents, Tab::Shell, Tab::Terminals]
        } else {
            &[Tab::Subagents, Tab::Shell]
        }
    }
    pub(crate) fn composer_key(&mut self, key: KeyAction) -> KeyOutcome {
        self.composer.down = None;
        if self.shell_viewer().is_some() {
            return self.shells.key(key);
        }
        if matches!(key, KeyAction::Cancel | KeyAction::Interrupt) {
            return self.close_composer();
        }
        if matches!(key, KeyAction::Left | KeyAction::Right) {
            let tabs = self.composer_tabs();
            let i = tabs
                .iter()
                .position(|tab| Some(*tab) == self.composer.active)
                .unwrap_or(0);
            let next = if key == KeyAction::Left {
                (i + tabs.len() - 1) % tabs.len()
            } else {
                (i + 1) % tabs.len()
            };
            return self.open_composer(tabs[next]);
        }
        match self.composer.active {
            Some(Tab::Subagents) if key == KeyAction::Up && self.children.at_first() => {
                self.close_composer()
            }
            Some(Tab::Shell)
                if key == KeyAction::Up && self.shells.at_first(self.attached_session()) =>
            {
                self.close_composer()
            }
            Some(Tab::Subagents) => self.children.key(key),
            Some(Tab::Shell) => self.shells.key(key),
            Some(Tab::Terminals) => self.terminal_composer_key(key),
            None => KeyOutcome::default(),
        }
    }
    /// Scoped row bindings precede root editor/global commands. Leader matching
    /// uses the existing deadline and parser, never a second input owner.
    pub(crate) fn composer_terminal_key(&mut self, event: KeyEvent) -> Option<KeyAction> {
        if !self.composer_open() || self.shell_viewer().is_some() {
            return None;
        }
        if event.kind == KeyEventKind::Release {
            return None;
        }
        if event.kind == KeyEventKind::Repeat
            && (matches!(event.code, KeyCode::Enter | KeyCode::Esc)
                || event.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Some(KeyAction::ComposerNoop);
        }
        let raw = crate::events::dialog_binding(&crate::events::binding(event)?);
        let candidate = self.composer_sequence_binding(&raw);
        let direct = candidate == raw;
        if direct && event.modifiers.is_empty() {
            match event.code {
                KeyCode::Left => return Some(KeyAction::Left),
                KeyCode::Right => return Some(KeyAction::Right),
                KeyCode::Esc => return Some(KeyAction::Cancel),
                _ => {}
            }
        }
        if direct && event.code == KeyCode::Char('c') && event.modifiers == KeyModifiers::CONTROL {
            return Some(KeyAction::Interrupt);
        }
        if direct
            && self.composer.active == Some(Tab::Subagents)
            && event.code == KeyCode::Char('a')
            && event.modifiers == KeyModifiers::CONTROL
        {
            return Some(KeyAction::CtrlA);
        }
        let range = match self.composer.active? {
            Tab::Subagents => 0..4,
            Tab::Shell => 4..8,
            Tab::Terminals => 8..11,
        };
        for index in range.clone() {
            if self.chrome.composer_shortcuts.bindings[index]
                .split(',')
                .any(|binding| {
                    !binding.trim().is_empty()
                        && crate::events::dialog_binding(binding) == candidate
                })
            {
                self.clear_composer_sequence();
                if event.kind == KeyEventKind::Repeat && matches!(index, 2 | 3 | 6 | 7 | 10) {
                    return Some(KeyAction::ComposerNoop);
                }
                return Some(match index {
                    0 | 4 | 8 => KeyAction::Up,
                    1 | 5 | 9 => KeyAction::Down,
                    2 | 6 | 10 => KeyAction::Enter,
                    _ => KeyAction::DeleteOrQuit,
                });
            }
        }
        if range.into_iter().any(|i| {
            self.chrome.composer_shortcuts.bindings[i]
                .split(',')
                .any(|binding| {
                    crate::events::dialog_binding(binding).starts_with(&format!("{candidate} "))
                })
        }) {
            self.start_composer_sequence(candidate);
            return Some(KeyAction::Leader);
        }
        // Disable default row bindings too when remapped. Keep real background
        // conversion and common native panel shortcuts on their existing path.
        if direct
            && (matches!(event.code, KeyCode::Up | KeyCode::Down | KeyCode::Enter)
                || event.modifiers.is_empty() && matches!(event.code, KeyCode::Char('j' | 'k'))
                || event.code == KeyCode::Char('d') && event.modifiers == KeyModifiers::CONTROL)
        {
            return Some(KeyAction::ComposerNoop);
        }
        None
    }
    pub(crate) fn composer_mouse(&mut self, event: MouseEvent, frame: Rect) -> Option<KeyOutcome> {
        if !self.composer_open() || self.shell_viewer().is_some() {
            return None;
        }
        if crate::shell::toast_rect(self, frame)
            .is_some_and(|rect| rect.contains((event.column, event.row).into()))
        {
            self.composer.down = None;
            return None;
        }
        let (inside, hit) = {
            let paint = self.composer.painted.borrow();
            let paint = paint
                .as_ref()
                .filter(|paint| paint.frame == frame && Some(paint.tab) == self.composer.active)?;
            (
                paint.area.contains((event.column, event.row).into()),
                if paint.close.contains((event.column, event.row).into()) {
                    Some(Hit::Close)
                } else {
                    paint
                        .rows
                        .iter()
                        .find(|(rect, _)| rect.contains((event.column, event.row).into()))
                        .map(|(_, item)| Hit::Row(item.clone()))
                },
            )
        };
        match event.kind {
            MouseEventKind::Moved => {
                if let Some(Hit::Row(item)) = hit {
                    self.select_composer_row(&item);
                }
            }
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.composer.down = hit.clone();
                if let Some(Hit::Row(item)) = hit {
                    self.select_composer_row(&item);
                }
            }
            MouseEventKind::Up(MouseButton::Left) if event.modifiers.is_empty() => {
                if let Some(hit) = hit
                    && self.composer.down.take() == Some(hit.clone())
                {
                    return Some(match hit {
                        Hit::Close => self.close_composer(),
                        Hit::Row(item) if self.select_composer_row(&item) => {
                            self.composer_key(KeyAction::Enter)
                        }
                        _ => KeyOutcome::default(),
                    });
                }
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                self.composer.down = None
            }
            _ => {}
        }
        inside.then(KeyOutcome::default)
    }
    fn select_composer_row(&mut self, item: &Item) -> bool {
        match item {
            Item::Child(id) => self.children.select_row(id),
            Item::Shell(id) => self.shells.select_row(id, self.attached_session().cloned()),
            Item::Terminal(target) => self.select_terminal_composer_row(target.as_ref()),
        }
    }
}

fn rows(state: &TuiState, theme: &Theme, width: u16) -> Vec<Row> {
    match state.composer.active {
        Some(Tab::Subagents) => {
            state
                .children
                .composer_rows(state.attached_session(), theme, width)
        }
        Some(Tab::Shell) => state
            .shells
            .composer_rows(state.attached_session(), theme, width),
        Some(Tab::Terminals) => state.terminal_composer_rows(theme, width),
        None => Vec::new(),
    }
}
pub(crate) fn height(state: &TuiState) -> u16 {
    if !state.composer_open()
        || state.approvals.active().is_some()
        || state.questions.active().is_some()
    {
        return 0;
    }
    // The pinned scrollbox retains a five-row viewport even for one entry.
    // Inventories are still capped to five painted rows, not duplicated here.
    5 + 6
}
fn shortcut(state: &TuiState, index: usize) -> String {
    state.chrome.composer_shortcuts.bindings[index]
        .split(',')
        .next()
        .unwrap_or("")
        .replace("return", "enter")
}
pub(crate) fn render(frame: &mut Frame<'_>, state: &TuiState, main: Rect, theme: &Theme) {
    state.composer.painted.borrow_mut().take();
    let bounds = crate::layout::content_box(main);
    let h = height(state).min(bounds.height);
    let Some(tab) = state.composer.active.filter(|_| h > 0) else {
        return;
    };
    let area = Rect::new(bounds.x, bounds.bottom().saturating_sub(h), bounds.width, h);
    let content = Rect::new(
        area.x + area.width.min(2),
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background_panel())),
        area,
    );
    for y in area.y..area.bottom() {
        frame.render_widget(
            Paragraph::new("┃").style(Style::default().fg(theme.border()).bg(theme.background())),
            Rect::new(area.x, y, area.width.min(1), 1),
        );
    }
    let mut labels = Vec::new();
    for (i, item) in state.composer_tabs().iter().enumerate() {
        if i > 0 {
            labels.push(Span::raw("  "));
        }
        labels.push(Span::styled(
            item.label(),
            Style::default()
                .fg(if *item == tab {
                    theme.text()
                } else {
                    theme.text_muted()
                })
                .add_modifier(if *item == tab {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
    }
    let close = Rect::new(
        content.right().saturating_sub(3).max(content.x),
        content.y,
        content.width.min(3),
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(labels)),
        Rect::new(
            content.x + content.width.min(1),
            content.y,
            content.width.saturating_sub(5),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new("esc").style(Style::default().fg(theme.text_muted())),
        close,
    );
    let list = rows(state, theme, content.width);
    let count = list
        .len()
        .max(1)
        .min(usize::from(area.height.saturating_sub(6)));
    let mut painted = Vec::new();
    if list.is_empty() {
        let empty = match tab {
            Tab::Subagents => state.children.empty_label(),
            Tab::Shell => "No shell commands",
            Tab::Terminals => "",
        };
        frame.render_widget(
            Paragraph::new(format!(" {empty}")).style(Style::default().fg(theme.text_muted())),
            Rect::new(content.x, content.y + 2, content.width, count as u16),
        );
    }
    for (i, row) in list.into_iter().take(count).enumerate() {
        let rect = Rect::new(content.x, content.y + 2 + i as u16, content.width, 1);
        frame.render_widget(Paragraph::new(row.line), rect);
        painted.push((rect, row.item));
    }
    let mut hints = Vec::new();
    let actions = match tab {
        Tab::Subagents => {
            let mut hints = Vec::new();
            if state.children.selected_running() {
                hints.push(("interrupt", shortcut(state, 3)));
            }
            hints.push((state.children.filter_hint(), "ctrl+a".into()));
            hints
        }
        Tab::Shell if state.shells.has_selected(state.attached_session()) => {
            vec![("output", shortcut(state, 6)), ("kill", shortcut(state, 7))]
        }
        _ => Vec::new(),
    };
    for (label, key) in actions
        .into_iter()
        .filter(|(_, key)| !key.is_empty())
        .chain(std::iter::once(("tabs", "←/→".into())))
    {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        hints.push(Span::styled(
            format!("{label} "),
            Style::default()
                .fg(theme.text())
                .add_modifier(Modifier::BOLD),
        ));
        hints.push(Span::styled(key, Style::default().fg(theme.text_muted())));
    }
    frame.render_widget(
        Paragraph::new(Line::from(hints)),
        Rect::new(
            content.x + content.width.min(1),
            area.bottom().saturating_sub(2),
            content.width.saturating_sub(1),
            1,
        ),
    );
    *state.composer.painted.borrow_mut() = Some(Paint {
        frame: frame.area(),
        area,
        tab,
        close,
        rows: painted,
    });
}

#[cfg(test)]
mod tests;
