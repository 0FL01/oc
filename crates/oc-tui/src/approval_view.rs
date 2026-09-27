//! Invocation-keyed lower permission surface (pinned donor permission.tsx).
//! Only owner snapshots remove requests; replies never clear the composer.
use crate::{
    app::{KeyOutcome, PanelIntent, TuiState},
    editor::Editor,
    events::KeyAction,
    theme::Theme,
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_core::approval::{ApprovalDecision, ApprovalPreview, ApprovalReply, ApprovalRequest};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ApprovalView {
    requests: Vec<(ApprovalRequest, bool)>,
    selected: usize,
    pub fullscreen: bool,
    rejecting: bool,
    feedback: String,
    editor: Editor,
    scroll: usize,
    pub error: Option<String>,
    painted: RefCell<Option<Painted>>,
    down: Option<(u64, usize, Rect)>,
    key_prefix: Option<(String, Instant)>,
    feedback_drag: bool,
}
struct Painted {
    id: u64,
    area: Rect,
    targets: Vec<Rect>,
    max_scroll: usize,
    input: Option<(Rect, usize)>,
    feedback: String,
}
pub(crate) fn binding(event: crossterm::event::KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let key = match event.code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_ascii_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Esc => "esc".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pgup".into(),
        KeyCode::PageDown => "pgdown".into(),
        _ => String::new(),
    };
    let mut binding = String::new();
    for (flag, prefix) in [
        (KeyModifiers::CONTROL, "ctrl+"),
        (KeyModifiers::ALT, "alt+"),
        (KeyModifiers::SHIFT, "shift+"),
    ] {
        if event.modifiers.contains(flag) {
            binding.push_str(prefix);
        }
    }
    binding.push_str(&key);
    binding
}
impl ApprovalView {
    fn feedback_offset(&self, rect: Rect, top: usize, x: u16, y: u16) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        use unicode_width::UnicodeWidthStr;
        let (rows, _) = self
            .editor
            .layout_words(&self.feedback, usize::from(rect.width.max(1)));
        let target = top + usize::from(y.saturating_sub(rect.y).min(rect.height.saturating_sub(1)));
        let mut offset = 0;
        for row in rows.iter().take(target) {
            offset += row.text.len();
            if self.feedback.as_bytes().get(offset) == Some(&b'\n') {
                offset += 1;
            }
        }
        if let Some(row) = rows.get(target) {
            let mut cells = 0;
            for grapheme in row.text.graphemes(true) {
                let width = grapheme.width();
                if cells + width
                    > usize::from(x.saturating_sub(rect.x).min(rect.width.saturating_sub(1)))
                {
                    break;
                }
                cells += width;
                offset += grapheme.len();
            }
        }
        offset.min(self.feedback.len())
    }
    fn feedback_vertical(&mut self, down: bool, select: bool) {
        let input = self.painted.borrow().as_ref().and_then(|p| p.input);
        if let Some((rect, _)) = input {
            let (rows, (row, col)) = self
                .editor
                .layout_words(&self.feedback, usize::from(rect.width.max(1)));
            let next = if down {
                (row + 1).min(rows.len() - 1)
            } else {
                row.saturating_sub(1)
            };
            let target = self.feedback_offset(
                Rect::new(0, 0, rect.width, rows.len().min(u16::MAX as usize) as u16),
                0,
                col as u16,
                next as u16,
            );
            self.editor.move_to(target, select);
        } else {
            self.editor.vertical(&self.feedback, down, select);
        }
    }
    /// Effective owner-resolved bindings, including comma alternatives and leader sequences.
    pub(crate) fn terminal_key(
        &mut self,
        event: crossterm::event::KeyEvent,
        shortcuts: &oc_core::queries::PermissionShortcuts,
    ) -> Option<KeyAction> {
        use crossterm::event::{KeyCode, KeyEventKind};
        if event.kind == KeyEventKind::Release {
            return None;
        }
        if event.kind == KeyEventKind::Repeat
            && (self.key_prefix.is_some() || !event.modifiers.is_empty())
        {
            return None;
        }
        let stroke = binding(event);
        if event.code == KeyCode::Esc {
            self.key_prefix = None;
            return Some(KeyAction::Cancel);
        }
        let prior = self
            .key_prefix
            .take()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(2));
        let sequence = prior.as_ref().map_or_else(
            || stroke.clone(),
            |(prefix, _)| format!("{prefix} {stroke}"),
        );
        let normalize = |s: &str| {
            s.split_whitespace()
                .map(|stroke| {
                    stroke
                        .to_ascii_lowercase()
                        .split('+')
                        .map(|key| match key {
                            "escape" => "esc",
                            "return" => "enter",
                            "pagedown" => "pgdown",
                            "pageup" => "pgup",
                            _ => key,
                        })
                        .collect::<Vec<_>>()
                        .join("+")
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let match_binding = |bindings: &str| bindings.split(',').any(|s| normalize(s) == sequence);
        if !self.rejecting && match_binding(&shortcuts.fullscreen) {
            self.toggle_fullscreen();
            return None;
        }
        if match_binding(&shortcuts.exit) {
            return Some(if stroke == "ctrl+c" {
                KeyAction::Interrupt
            } else {
                KeyAction::Quit
            });
        }
        let prefix = format!("{sequence} ");
        if shortcuts
            .exit
            .split(',')
            .chain(
                (!self.rejecting)
                    .then_some(shortcuts.fullscreen.as_str())
                    .into_iter()
                    .flat_map(|s| s.split(',')),
            )
            .any(|s| normalize(s).starts_with(&prefix))
        {
            self.key_prefix = Some((sequence, Instant::now()));
            return None;
        }
        if prior.is_some() || matches!(stroke.as_str(), "ctrl+c" | "ctrl+d") {
            return None;
        }
        crate::events::map_key(event).filter(|action| *action != KeyAction::Leader)
    }
    fn max_scroll(&self) -> usize {
        self.painted
            .borrow()
            .as_ref()
            .map_or(4096, |paint| paint.max_scroll)
    }
    pub fn active(&self) -> Option<&ApprovalRequest> {
        self.requests.first().map(|r| &r.0)
    }
    pub(crate) fn request_for_call(
        &self,
        session: &str,
        turn: &str,
        call: &str,
    ) -> Option<&ApprovalRequest> {
        self.requests.iter().map(|r| &r.0).find(|r| {
            r.binding.session == session && r.binding.turn == turn && r.binding.call == call
        })
    }
    pub fn reconcile(&mut self, requests: Vec<(ApprovalRequest, bool)>) {
        if self.requests.first() != requests.first() {
            *self = Self {
                requests,
                ..Default::default()
            };
        } else {
            self.requests = requests;
        }
    }
    pub fn invalidate_paint(&mut self) {
        self.painted.borrow_mut().take();
        self.down = None;
        self.feedback_drag = false;
    }
    pub(crate) fn cancel_pointer(&mut self) {
        self.down = None;
        self.feedback_drag = false;
    }
    pub fn toggle_fullscreen(&mut self) -> bool {
        if self.rejecting {
            return false;
        }
        self.fullscreen = !self.fullscreen;
        self.invalidate_paint();
        true
    }
    fn options(&self) -> Vec<(&'static str, ApprovalDecision)> {
        let mut out = vec![("Allow once", ApprovalDecision::Once)];
        if self.active().is_some_and(|r| !r.save_patterns.is_empty()) {
            out.push(("Always allow", ApprovalDecision::Always));
        }
        out.push(("Reject", ApprovalDecision::Reject { feedback: None }));
        out
    }
    fn reply(&mut self, decision: ApprovalDecision) -> KeyOutcome {
        let Some(request) = self.active() else {
            return KeyOutcome::default();
        };
        if matches!(decision, ApprovalDecision::Reject { .. })
            && !self.rejecting
            && self.requests[0].1
        {
            self.rejecting = true;
            self.key_prefix = None;
            self.invalidate_paint();
            return KeyOutcome::default();
        }
        KeyOutcome {
            intent: Some(PanelIntent::ReplyApproval(ApprovalReply {
                id: request.id,
                binding: request.binding.clone(),
                decision,
            })),
            ..Default::default()
        }
    }
    pub fn paste(&mut self, text: &str) {
        if self.rejecting {
            self.editor.replace(&mut self.feedback, text, 16 * 1024);
        }
    }
    pub fn key(&mut self, action: KeyAction) -> KeyOutcome {
        self.scroll = self.scroll.min(self.max_scroll());
        if self.rejecting {
            match action {
                KeyAction::Enter => {
                    return self.reply(ApprovalDecision::Reject {
                        feedback: (!self.feedback.is_empty()).then(|| self.feedback.clone()),
                    });
                }
                KeyAction::Interrupt if !self.feedback.is_empty() => {
                    self.feedback.clear();
                    self.editor.clear();
                }
                KeyAction::Cancel
                | KeyAction::Interrupt
                | KeyAction::Quit
                | KeyAction::DeleteOrQuit => {
                    self.rejecting = false;
                    self.key_prefix = None;
                    self.feedback.clear();
                    self.editor.clear();
                    self.invalidate_paint();
                }
                KeyAction::Char(c) => self.paste(&c.to_string()),
                KeyAction::Newline => self.paste("\n"),
                KeyAction::Backspace | KeyAction::WordBackspace => {
                    self.editor.delete(
                        &mut self.feedback,
                        true,
                        action == KeyAction::WordBackspace,
                    );
                }
                KeyAction::Delete | KeyAction::WordDelete => {
                    self.editor
                        .delete(&mut self.feedback, false, action == KeyAction::WordDelete);
                }
                KeyAction::Left
                | KeyAction::Right
                | KeyAction::WordLeft
                | KeyAction::WordRight
                | KeyAction::SelectLeft
                | KeyAction::SelectRight
                | KeyAction::SelectWordLeft
                | KeyAction::SelectWordRight => {
                    self.editor.horizontal(
                        &self.feedback,
                        matches!(
                            action,
                            KeyAction::Right
                                | KeyAction::WordRight
                                | KeyAction::SelectRight
                                | KeyAction::SelectWordRight
                        ),
                        matches!(
                            action,
                            KeyAction::WordLeft
                                | KeyAction::WordRight
                                | KeyAction::SelectWordLeft
                                | KeyAction::SelectWordRight
                        ),
                        matches!(
                            action,
                            KeyAction::SelectLeft
                                | KeyAction::SelectRight
                                | KeyAction::SelectWordLeft
                                | KeyAction::SelectWordRight
                        ),
                    );
                }
                KeyAction::Up | KeyAction::Down | KeyAction::SelectUp | KeyAction::SelectDown => {
                    self.feedback_vertical(
                        matches!(action, KeyAction::Down | KeyAction::SelectDown),
                        matches!(action, KeyAction::SelectUp | KeyAction::SelectDown),
                    );
                }
                KeyAction::Undo | KeyAction::Redo => {
                    self.editor
                        .undo(&mut self.feedback, action == KeyAction::Redo);
                }
                KeyAction::Home
                | KeyAction::CtrlA
                | KeyAction::End
                | KeyAction::SelectHome
                | KeyAction::SelectEnd => self.editor.line_edge(
                    &self.feedback,
                    matches!(action, KeyAction::End | KeyAction::SelectEnd),
                    matches!(action, KeyAction::SelectHome | KeyAction::SelectEnd),
                ),
                _ => {}
            }
            return KeyOutcome::default();
        }
        let options = self.options();
        match action {
            KeyAction::Left | KeyAction::Char('h') => {
                self.selected = (self.selected + options.len() - 1) % options.len();
                self.scroll = 0;
            }
            KeyAction::Right | KeyAction::Char('l') => {
                self.selected = (self.selected + 1) % options.len();
                self.scroll = 0;
            }
            KeyAction::Enter => return self.reply(options[self.selected].1.clone()),
            KeyAction::Cancel
            | KeyAction::Interrupt
            | KeyAction::Quit
            | KeyAction::DeleteOrQuit => {
                if self.fullscreen {
                    self.fullscreen = false;
                    self.invalidate_paint();
                } else {
                    return self.reply(ApprovalDecision::Reject { feedback: None });
                }
            }
            KeyAction::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyAction::Down => self.scroll = self.scroll.saturating_add(1).min(self.max_scroll()),
            KeyAction::PageUp => self.scroll = self.scroll.saturating_sub(8),
            KeyAction::PageDown => {
                self.scroll = self.scroll.saturating_add(8).min(self.max_scroll())
            }
            _ => {}
        }
        KeyOutcome::default()
    }
    pub fn mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        self.scroll = self.scroll.min(self.max_scroll());
        let hit = self
            .painted
            .borrow()
            .as_ref()
            .filter(|p| {
                p.area == area
                    && p.feedback == self.feedback
                    && self.active().is_some_and(|r| r.id == p.id)
            })
            .and_then(|p| {
                p.targets
                    .iter()
                    .position(|r| r.contains((event.column, event.row).into()))
                    .map(|i| (p.id, i, area))
            });
        let input = self
            .painted
            .borrow()
            .as_ref()
            .filter(|p| {
                p.area == area
                    && p.feedback == self.feedback
                    && self.active().is_some_and(|r| r.id == p.id)
            })
            .and_then(|p| p.input);
        if self.rejecting
            && let Some((rect, top)) = input
        {
            if event.kind == MouseEventKind::Down(MouseButton::Left)
                && rect.contains((event.column, event.row).into())
            {
                let target = self.feedback_offset(rect, top, event.column, event.row);
                self.editor.move_to(target, false);
                self.feedback_drag = true;
                self.down = None;
                return KeyOutcome::default();
            }
            if self.feedback_drag && event.kind == MouseEventKind::Drag(MouseButton::Left) {
                let target = self.feedback_offset(rect, top, event.column, event.row);
                self.editor.move_to(target, true);
                return KeyOutcome::default();
            }
        }
        if event.kind == MouseEventKind::Up(MouseButton::Left) && self.feedback_drag {
            self.feedback_drag = false;
            return KeyOutcome::default();
        }
        match event.kind {
            MouseEventKind::Moved if !self.rejecting => {
                if let Some((_, i, _)) = hit {
                    self.selected = i;
                    self.scroll = 0;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => self.down = hit,
            MouseEventKind::Up(MouseButton::Left) => {
                if self.down.take().is_some_and(|down| Some(down) == hit)
                    && let Some((_, i, _)) = hit
                {
                    if self.rejecting {
                        return self.key(if i == 0 {
                            KeyAction::Enter
                        } else {
                            KeyAction::Cancel
                        });
                    }
                    self.selected = i;
                    return self.key(KeyAction::Enter);
                }
            }
            MouseEventKind::ScrollUp => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                self.scroll = self.scroll.saturating_add(1).min(self.max_scroll());
            }
            MouseEventKind::Drag(_) => self.down = None,
            _ => {}
        }
        KeyOutcome::default()
    }
}

pub(crate) fn inline_height(state: &TuiState, width: u16, terminal_width: u16) -> u16 {
    let view = &state.approvals;
    let Some(request) = view.active() else {
        return 0;
    };
    let controls = if terminal_width < 80 { 5 } else { 3 };
    if view.rejecting {
        let rows = view
            .editor
            .layout_words(
                &view.feedback,
                usize::from(feedback_width(width, terminal_width).max(1)),
            )
            .0
            .len();
        return (5 + 2 + rows + usize::from(terminal_width < 80) * 2).min(u16::MAX as usize) as u16;
    }
    let always = view.options()[view.selected].1 == ApprovalDecision::Always;
    // EditBody's 100%-height scrollbox consumes the available question budget.
    if matches!(&request.preview, ApprovalPreview::Patch { files, .. } if files.first().is_some_and(|file| !file.hunks.is_empty()))
        && !always
    {
        return 15;
    }
    let body_width = width.saturating_sub(
        if matches!(request.preview, ApprovalPreview::Patch { .. }) && !always {
            5
        } else {
            6
        },
    );
    let rows = wrapped(body_lines(state, body_width, terminal_width), body_width).len();
    let title_rows = title_lines(request, width, 15).map_or(0, |(_, lines)| lines.len());
    (4 + title_rows + rows + controls).min(15) as u16
}

fn title_lines(
    request: &ApprovalRequest,
    width: u16,
    limit: usize,
) -> Option<(&'static str, Vec<Line<'static>>)> {
    presentation_title(request).map(|(icon, title)| {
        (
            icon,
            wrapped_limited(vec![Line::raw(title)], width.saturating_sub(10), limit),
        )
    })
}

fn feedback_width(width: u16, terminal_width: u16) -> u16 {
    use unicode_width::UnicodeWidthStr;
    let inner = width.saturating_sub(1 + 2 + 3); // left border and footer padding
    let actions = "enter confirm".width() + 2 + "esc cancel".width();
    if terminal_width < 80 {
        inner
    } else {
        inner.saturating_sub(actions as u16 + 1)
    }
}

fn presentation_title(request: &ApprovalRequest) -> Option<(&'static str, String)> {
    match &request.preview {
        ApprovalPreview::Shell { .. } => None,
        ApprovalPreview::Patch { files, .. } => Some((
            "→",
            format!(
                "Edit {}",
                files
                    .first()
                    .map(|f| f.destination.as_ref().unwrap_or(&f.path).as_str())
                    .or_else(|| request.resources.first().map(String::as_str))
                    .unwrap_or("")
            ),
        )),
        ApprovalPreview::Search { pattern, .. } => Some((
            "✱",
            format!(
                "{} \"{pattern}\"",
                if request.action == "glob" {
                    "Glob"
                } else {
                    "Grep"
                }
            ),
        )),
        ApprovalPreview::Resource { values } => {
            let value = values.first().map_or("", String::as_str);
            Some(match request.action.as_str() {
                "read" => ("→", format!("Read {value}")),
                "webfetch" => ("%", format!("WebFetch {value}")),
                _ => ("⚙", format!("Call tool {}", request.action)),
            })
        }
    }
}

fn body_lines(state: &TuiState, width: u16, terminal_width: u16) -> Vec<Line<'static>> {
    let view = &state.approvals;
    let Some(request) = view.active() else {
        return vec![];
    };
    if view.options()[view.selected].1 != ApprovalDecision::Always {
        return preview(
            request,
            width,
            terminal_width,
            state.chrome.diffs.view,
            Theme::dark(),
        );
    }
    let muted = Style::default().fg(Theme::dark().text_muted());
    if request.save_patterns == ["*"] {
        return vec![Line::styled(
            format!(
                "This will always allow {} for this project.",
                request.action
            ),
            muted,
        )];
    }
    let mut lines = vec![Line::styled(
        "This will always allow the following patterns for this project.",
        muted,
    )];
    for pattern in &request.save_patterns {
        lines.push(Line::default());
        lines.push(Line::raw(format!("- {pattern}")));
    }
    lines
}

pub fn render(frame: &mut Frame<'_>, state: &TuiState, main: Rect) {
    let view = &state.approvals;
    let Some(request) = view.active() else { return };
    let theme = Theme::dark();
    let terminal = frame.area();
    let main = crate::layout::content_box(main);
    let narrow = terminal.width < 80;
    let controls = if view.rejecting {
        2 + view
            .editor
            .layout_words(
                &view.feedback,
                usize::from(feedback_width(main.width, terminal.width).max(1)),
            )
            .0
            .len()
            .min(u16::MAX as usize) as u16
            + if narrow { 2 } else { 0 }
    } else if narrow {
        5
    } else {
        3
    };
    let area = if view.fullscreen && !view.rejecting {
        Rect::new(
            2,
            1,
            terminal.width.saturating_sub(4),
            terminal.height.saturating_sub(2),
        )
    } else {
        let height = main
            .height
            .min(inline_height(state, main.width, terminal.width));
        Rect::new(
            main.x,
            main.bottom().saturating_sub(height),
            main.width,
            height,
        )
    };
    let controls = controls.min(area.height.saturating_sub(5));
    let base = Style::default()
        .fg(theme.text())
        .bg(theme.background_raised());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_set(ratatui::symbols::border::THICK)
            .border_style(base.fg(if view.rejecting {
                theme.error()
            } else {
                theme.primary()
            }))
            .style(base),
        area,
    );
    let title = if view.rejecting {
        "Reject permission"
    } else {
        "Permission required"
    };
    let header = Rect::new(area.x + 3, area.y + 1, area.width.saturating_sub(6), 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "△ ",
                base.fg(if view.rejecting {
                    theme.error()
                } else {
                    theme.warning()
                }),
            ),
            Span::raw(title),
        ]))
        .style(base),
        header,
    );
    let subtitle = (!view.rejecting)
        .then(|| {
            title_lines(
                request,
                area.width,
                usize::from(area.height.saturating_sub(5 + controls)),
            )
        })
        .flatten();
    let title_height = subtitle.as_ref().map_or(0, |(_, lines)| lines.len() as u16);
    if let Some((icon, lines)) = &subtitle {
        frame.render_widget(
            Paragraph::new(*icon).style(base.fg(theme.text_muted())),
            Rect::new(area.x + 5, area.y + 2, 1, title_height.min(1)),
        );
        frame.render_widget(
            Paragraph::new(lines.clone()).style(base),
            Rect::new(
                area.x + 7,
                area.y + 2,
                area.width.saturating_sub(10),
                title_height,
            ),
        );
    }
    let options = view.options();
    let body_top = if view.rejecting { 3 } else { 3 + title_height };
    let body_indent = if !view.rejecting
        && matches!(request.preview, ApprovalPreview::Patch { .. })
        && options[view.selected].1 != ApprovalDecision::Always
    {
        2
    } else {
        3
    };
    let body = Rect::new(
        area.x + body_indent,
        area.y + body_top,
        area.width.saturating_sub(body_indent + 3),
        area.height.saturating_sub(body_top + 1 + controls),
    );
    let lines = if view.rejecting {
        vec![Line::styled(
            "Tell OpenCode what to do differently",
            base.fg(theme.text_muted()),
        )]
    } else {
        body_lines(state, body.width, terminal.width)
    };
    let lines = wrapped(lines, body.width);
    let max_scroll = lines.len().saturating_sub(usize::from(body.height));
    frame.render_widget(
        Paragraph::new(lines)
            .style(base)
            .scroll((view.scroll.min(max_scroll).min(u16::MAX as usize) as u16, 0)),
        body,
    );
    let footer = Rect::new(
        area.x + 1,
        area.bottom().saturating_sub(controls),
        area.width.saturating_sub(1),
        controls,
    );
    let footer_style = base.bg(theme.decrease(theme.background_raised()));
    frame.render_widget(Block::default().style(footer_style), footer);
    let footer_content = Rect::new(
        footer.x + 2,
        footer.y + 1,
        footer.width.saturating_sub(2 + 3),
        footer.height.saturating_sub(2),
    );
    let actions_width = Line::raw("enter confirm  esc cancel").width() as u16;
    let mut x = if view.rejecting && !narrow {
        footer_content
            .right()
            .saturating_sub(actions_width)
            .max(footer_content.x)
    } else {
        footer_content.x
    };
    let mut targets = Vec::new();
    let labels: Vec<_> = if view.rejecting {
        vec!["enter confirm", "esc cancel"]
    } else {
        options.iter().map(|o| o.0).collect()
    };
    for (i, label) in labels.iter().enumerate() {
        let width = (label.len() as u16 + if view.rejecting { 0 } else { 2 })
            .min(area.right().saturating_sub(x));
        let target = Rect::new(
            x,
            footer_content.y
                + if view.rejecting {
                    if narrow {
                        footer_content.height.saturating_sub(1)
                    } else {
                        footer_content.height / 2
                    }
                } else {
                    0
                },
            width,
            1,
        );
        targets.push(target);
        let label = if view.rejecting {
            let (key, hint) = label.split_once(' ').unwrap_or((label, ""));
            Line::from(vec![
                Span::styled(format!("{key} "), footer_style),
                Span::styled(hint, footer_style.fg(theme.text_muted())),
            ])
        } else {
            Line::raw(format!(" {label} "))
        };
        frame.render_widget(
            Paragraph::new(label).style(if i == view.selected && !view.rejecting {
                base.bg(theme.primary()).fg(theme.background())
            } else {
                footer_style
            }),
            target,
        );
        x = x.saturating_add(width + if view.rejecting { 2 } else { 1 });
    }
    let mut painted_input = None;
    if view.rejecting {
        let input = Rect::new(
            footer_content.x,
            footer_content.y,
            feedback_width(area.width, terminal.width),
            footer.height.saturating_sub(2 + if narrow { 2 } else { 0 }),
        );
        let (rows, (row, col)) = view
            .editor
            .layout_words(&view.feedback, usize::from(input.width.max(1)));
        let top = row.saturating_sub(usize::from(input.height.saturating_sub(1)));
        painted_input = Some((input, top));
        let text: Vec<_> = rows
            .iter()
            .skip(top)
            .take(usize::from(input.height))
            .map(|r| {
                Line::from(
                    r.spans
                        .iter()
                        .map(|(s, selected, _)| {
                            Span::styled(
                                s.clone(),
                                if *selected {
                                    footer_style.add_modifier(ratatui::style::Modifier::REVERSED)
                                } else {
                                    footer_style
                                },
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(text).style(footer_style), input);
        if input.width > 0 && input.height > 0 && input.y < area.bottom() {
            frame.set_cursor_position((
                input.x + (col as u16).min(input.width - 1),
                input.y + (row - top) as u16,
            ));
        }
    }
    if !view.rejecting {
        let hint = format!(
            "{} {}  ⇆ select  enter confirm",
            state.chrome.permission_shortcuts.fullscreen,
            if view.fullscreen {
                "minimize"
            } else {
                "fullscreen"
            }
        );
        let y = footer_content.y + if narrow { 2 } else { 0 };
        let hint_width = Line::raw(hint.as_str()).width() as u16;
        let hx = if narrow {
            footer_content.x
        } else {
            footer_content.right().saturating_sub(hint_width).max(x)
        };
        frame.render_widget(
            Paragraph::new(hint).style(footer_style.fg(theme.text_muted())),
            Rect::new(hx, y, footer_content.right().saturating_sub(hx), 1),
        );
    }
    if let Some(error) = &view.error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(base.fg(theme.error())),
            Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(5), 1),
        );
    }
    *view.painted.borrow_mut() = Some(Painted {
        id: request.id,
        area: terminal,
        targets,
        max_scroll,
        input: painted_input,
        feedback: view.feedback.clone(),
    });
}

fn wrapped(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    const PREVIEW_ROWS: usize = 4096;
    let mut lines = wrapped_limited(lines, width, PREVIEW_ROWS + 1);
    if lines.len() > PREVIEW_ROWS {
        lines.truncate(PREVIEW_ROWS - 1);
        lines.push(Line::styled(
            "[Permission preview truncated]",
            Style::default().fg(Theme::dark().text_muted()),
        ));
    }
    lines
}

fn wrapped_limited(lines: Vec<Line<'static>>, width: u16, limit: usize) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    for line in lines {
        if rows.len() >= limit {
            break;
        }
        let line = crate::styled::Line::new(
            line.spans
                .into_iter()
                .map(|span| crate::styled::Span::styled(span.content.into_owned(), span.style))
                .collect(),
        );
        rows.extend(
            crate::styled::wrap_permission_line_limited(
                &line,
                usize::from(width.max(1)),
                limit - rows.len(),
            )
            .into_iter()
            .map(|line| {
                Line::from(
                    line.spans()
                        .iter()
                        .map(|s| Span::styled(s.content().to_string(), s.style()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        );
    }
    rows
}

fn preview(
    request: &ApprovalRequest,
    width: u16,
    terminal_width: u16,
    diff_view: oc_core::queries::DiffView,
    theme: &Theme,
) -> Vec<Line<'static>> {
    match &request.preview {
        ApprovalPreview::Shell { command, .. } => vec![Line::raw(format!("$ {command}"))],
        ApprovalPreview::Search { pattern, .. } => vec![Line::styled(
            format!("Pattern: {pattern}"),
            Style::default().fg(theme.text_muted()),
        )],
        ApprovalPreview::Resource { values } => values
            .iter()
            .map(|v| {
                Line::styled(
                    format!(
                        "{}: {v}",
                        match request.action.as_str() {
                            "read" => "Path",
                            "webfetch" => "URL",
                            _ => "Resource",
                        }
                    ),
                    Style::default().fg(theme.text_muted()),
                )
            })
            .collect(),
        ApprovalPreview::Patch {
            files,
            total_files,
            truncated,
        } => {
            let Some(file) = files.first() else {
                return vec![Line::styled(
                    "No diff provided",
                    Style::default().fg(theme.text_muted()),
                )];
            };
            let mut out = Vec::new();
            let split = match diff_view {
                oc_core::queries::DiffView::Auto => terminal_width > 120,
                oc_core::queries::DiffView::Split => true,
                oc_core::queries::DiffView::Unified => false,
            };
            out.extend(
                crate::patch_view::preview(file, theme, width, split)
                    .into_iter()
                    .map(|line| {
                        Line::from(
                            line.spans()
                                .iter()
                                .map(|s| Span::styled(s.content().to_string(), s.style()))
                                .collect::<Vec<_>>(),
                        )
                    }),
            );
            if *total_files > 1 {
                out.push(Line::raw(format!(
                    "{} files requested; showing first file",
                    total_files
                )));
            }
            if *truncated {
                out.push(Line::raw("Proposed preview truncated"));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::{
        approval::ApprovalBinding,
        core_app::{CoreApp, MockProvider},
        domain::SessionId,
    };
    fn request(id: u64, save: bool) -> ApprovalRequest {
        ApprovalRequest {
            id,
            binding: ApprovalBinding {
                session: "root".into(),
                turn: "t".into(),
                call: "c".into(),
                operation: "op".into(),
                input_digest: "input".into(),
                location: "location".into(),
                generation: 1,
                agent: None,
                agent_digest: None,
            },
            project: "p".into(),
            action: "read".into(),
            resources: vec!["src/main.rs".into()],
            save_patterns: if save { vec!["src/*".into()] } else { vec![] },
            preview: ApprovalPreview::Resource {
                values: vec!["src/main.rs".into()],
            },
        }
    }
    fn decision(outcome: KeyOutcome) -> ApprovalDecision {
        let Some(PanelIntent::ReplyApproval(reply)) = outcome.intent else {
            panic!("reply intent")
        };
        reply.decision
    }
    #[test]
    fn permission_resource_wrapping_is_grapheme_safe_and_bounded() {
        let title = crate::styled::Line::plain(format!("WebFetch {}", "界🦊".repeat(8000)));
        let rows = crate::styled::wrap_permission_line_limited(&title, 17, 15);
        assert!(
            rows.len() <= 15
                && rows
                    .iter()
                    .all(|row| row.clone().into_ratatui().width() <= 17)
        );
        let rows = wrapped(vec![Line::raw("x".repeat(8192))], 1);
        assert_eq!(rows.len(), 4096);
        assert!(
            rows.last()
                .unwrap()
                .to_string()
                .contains("preview truncated")
        );
    }
    #[tokio::test]
    async fn growing_feedback_paints_whole_capture_text_and_keeps_selection_in_textarea() {
        use crossterm::event::KeyModifiers;
        use ratatui::{
            Terminal,
            backend::{Backend, TestBackend},
        };
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut state = TuiState::new(app.clone(), SessionId("root".into()));
        state.handle_paste("draftguard");
        state.handle_key(KeyAction::Home).await;
        state.handle_key(KeyAction::Right).await;
        state.handle_key(KeyAction::Right).await;
        let feedback = "VIS36-FEEDBACK: do not read; continue with this correction.";
        for width in [79, 80, 120, 121] {
            state
                .approvals
                .reconcile(vec![(request(u64::from(width), true), true)]);
            state.approvals.key(KeyAction::Left);
            state.approvals.key(KeyAction::Enter);
            state.handle_paste(feedback);
            let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
            terminal
                .draw(|frame| crate::views::render_frame(frame, &state))
                .unwrap();
            let cold = terminal.backend().buffer().clone();
            let cursor = terminal.backend_mut().get_cursor_position().unwrap();
            terminal
                .draw(|frame| crate::views::render_frame(frame, &state))
                .unwrap();
            assert_eq!(&cold, terminal.backend().buffer());
            let (input, top) = state
                .approvals
                .painted
                .borrow()
                .as_ref()
                .unwrap()
                .input
                .unwrap();
            assert_eq!(
                top, 0,
                "short complete feedback must not scroll to its suffix"
            );
            let (rows, (row, col)) = state
                .approvals
                .editor
                .layout_words(feedback, input.width as usize);
            assert!(input.height >= rows.len() as u16);
            assert_eq!(cursor, (input.x + col as u16, input.y + row as u16).into());
            let actions = state.approvals.painted.borrow().as_ref().unwrap().targets[0];
            assert_eq!(
                actions.y,
                if width < 80 {
                    input.bottom() + 1
                } else {
                    input.y + input.height / 2
                }
            );
            if width == 80 {
                assert_eq!(actions.y, 37);
                assert_eq!(input.y, 36);
            }
            let painted: Vec<_> = (input.y..input.bottom())
                .map(|y| {
                    (input.x..input.right())
                        .map(|x| cold[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            for (actual, expected) in painted.iter().zip(&rows) {
                assert!(
                    actual.starts_with(&expected.text),
                    "{actual:?} != {:?}",
                    expected.text
                );
            }
            assert_eq!(
                painted.join(" ").split_whitespace().collect::<Vec<_>>(),
                feedback.split_whitespace().collect::<Vec<_>>()
            );
            state.approvals.error = Some("owner ACK refused".into());
            state
                .approvals
                .reconcile(vec![(request(u64::from(width), true), true)]);
            assert_eq!(state.approvals.feedback, feedback);
            state.handle_key(KeyAction::Interrupt).await;
            assert!(state.approvals.rejecting && state.approvals.feedback.is_empty());
            state.handle_paste("first 🦊 line\nsecond selected line\nthird");
            terminal
                .draw(|frame| crate::views::render_frame(frame, &state))
                .unwrap();
            let (input, _) = state
                .approvals
                .painted
                .borrow()
                .as_ref()
                .unwrap()
                .input
                .unwrap();
            let event = |kind, x, y| MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            };
            state.handle_mouse(
                event(MouseEventKind::Down(MouseButton::Left), input.x, input.y),
                Rect::new(0, 0, width, 40),
            );
            state.handle_mouse(
                event(
                    MouseEventKind::Drag(MouseButton::Left),
                    input.x + 6,
                    input.y + 1,
                ),
                Rect::new(0, 0, width, 40),
            );
            state.handle_mouse(
                event(
                    MouseEventKind::Up(MouseButton::Left),
                    input.x + 6,
                    input.y + 1,
                ),
                Rect::new(0, 0, width, 40),
            );
            assert!(state.approvals.editor.selected().is_some());
            assert_eq!(state.input(), "draftguard");
            state.handle_key(KeyAction::Up).await;
            let (_, (caret_row, _)) = state
                .approvals
                .editor
                .layout_words(&state.approvals.feedback, input.width as usize);
            assert_eq!(caret_row, 0);
        }
        state.approvals.reconcile(vec![]);
        state.handle_key(KeyAction::Char('!')).await;
        assert_eq!(state.input(), "dr!aftguard");
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
    #[tokio::test]
    async fn effective_multistroke_bindings_and_reject_footer_textarea() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut state = TuiState::new(app.clone(), SessionId("root".into()));
        state.chrome.permission_shortcuts = oc_core::queries::PermissionShortcuts {
            fullscreen: "ctrl+x f,alt+f".into(),
            exit: "alt+x q,ctrl+c".into(),
        };
        state.approvals.reconcile(vec![(request(1, true), true)]);
        assert!(
            state
                .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
                .is_none()
        );
        assert!(
            state
                .terminal_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
                .is_none()
        );
        assert!(state.approvals.fullscreen);
        let escape = state
            .terminal_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        state.handle_key(escape).await;
        assert!(!state.approvals.fullscreen);
        state.handle_key(KeyAction::Left).await;
        state.handle_key(KeyAction::Enter).await;
        state.handle_paste("reason 🦊");
        for width in [79, 80, 120, 121] {
            let rows = crate::views::render_test(&state, width, 32);
            let actions_y =
                state.approvals.painted.borrow().as_ref().unwrap().targets[0].y as usize;
            let input_y = rows
                .iter()
                .position(|row| row.contains("reason 🦊"))
                .unwrap();
            assert!(rows[input_y].contains("reason 🦊"));
            assert!(rows[actions_y].contains("enter confirm"));
            assert!(
                rows.iter()
                    .position(|r| r.contains("Tell OpenCode"))
                    .unwrap()
                    < input_y
            );
        }
        let clear = state
            .terminal_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();
        state.handle_key(clear).await;
        assert!(state.approvals.feedback.is_empty());
        assert!(state.approvals.rejecting);
        state.handle_paste("retained on error");
        state.approvals.error = Some("owner refusal".into());
        state.approvals.reconcile(vec![(request(1, true), true)]);
        assert_eq!(state.approvals.feedback, "retained on error");
        assert!(
            state
                .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT))
                .is_none()
        );
        let cancel = state
            .terminal_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .unwrap();
        state.handle_key(cancel).await;
        assert!(!state.approvals.rejecting);
        assert!(state.approvals.feedback.is_empty());
        state.terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        state.approvals.reconcile(vec![(request(2, true), false)]);
        assert_eq!(
            state.terminal_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)),
            Some(KeyAction::Char('f'))
        );
        assert!(!state.approvals.fullscreen);
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
    #[test]
    fn proposed_diff_uses_terminal_auto_breakpoint_and_word_wrap_without_effect_labels() {
        use oc_core::patch::*;
        let mut request = request(1, true);
        request.action = "edit".into();
        request.preview = ApprovalPreview::Patch {
            files: vec![FileEffect {
                algorithm: DiffAlgorithm::Minimal,
                operation: PatchOperation::Update,
                path: "src/main.rs".into(),
                destination: None,
                additions: 1,
                deletions: 1,
                truncated: false,
                hunks: vec![PatchHunk {
                    old: LineRange { start: 1, count: 1 },
                    new: LineRange { start: 1, count: 1 },
                    truncated: false,
                    lines: vec![
                        PatchLine {
                            lossy: false,
                            kind: PatchLineKind::Removed,
                            old_line: Some(1),
                            new_line: None,
                            text: "old 🦊 ".repeat(20),
                            ending: LineEnding::Lf,
                            truncated: false,
                        },
                        PatchLine {
                            lossy: false,
                            kind: PatchLineKind::Added,
                            old_line: None,
                            new_line: Some(1),
                            text: "new 🦊 ".repeat(20),
                            ending: LineEnding::Lf,
                            truncated: false,
                        },
                    ],
                }],
            }],
            total_files: 1,
            truncated: false,
        };
        let unified = preview(
            &request,
            110,
            120,
            oc_core::queries::DiffView::Auto,
            Theme::dark(),
        );
        let split = preview(
            &request,
            110,
            121,
            oc_core::queries::DiffView::Auto,
            Theme::dark(),
        );
        assert_ne!(unified, split);
        assert_eq!(
            unified,
            preview(
                &request,
                110,
                121,
                oc_core::queries::DiffView::Unified,
                Theme::dark()
            )
        );
        assert_eq!(
            split,
            preview(
                &request,
                110,
                79,
                oc_core::queries::DiffView::Split,
                Theme::dark()
            )
        );
        for lines in [unified, split] {
            assert!(lines.iter().all(|line| line.width() <= 110));
            let text: String = lines
                .iter()
                .flat_map(|l| &l.spans)
                .map(|s| s.content.as_ref())
                .collect();
            assert!(text.contains("old 🦊"));
            assert!(text.contains("new 🦊"));
            assert!(!text.contains("Patched"));
        }
    }
    #[test]
    fn request_key_reset_no_optimistic_remove_and_child_feedback_editor() {
        let mut view = ApprovalView::default();
        let first = request(1, true);
        view.reconcile(vec![(first.clone(), true)]);
        view.key(KeyAction::Left);
        assert!(view.key(KeyAction::Enter).intent.is_none());
        view.paste("retry a narrower path 🦊");
        view.key(KeyAction::Left);
        view.key(KeyAction::Char('!'));
        assert!(matches!(
            decision(view.key(KeyAction::Enter)),
            ApprovalDecision::Reject { feedback: Some(_) }
        ));
        assert_eq!(view.active(), Some(&first));
        view.error = Some("owner busy".into());
        view.reconcile(vec![(first, true)]);
        assert!(view.rejecting);
        assert!(view.error.is_some());
        view.key(KeyAction::Interrupt);
        assert!(view.feedback.is_empty());
        assert!(view.rejecting);
        view.key(KeyAction::Interrupt);
        assert!(!view.rejecting);
        view.fullscreen = true;
        assert!(view.key(KeyAction::Cancel).intent.is_none());
        assert!(!view.fullscreen);
        view.reconcile(vec![(request(2, false), false)]);
        assert_eq!(view.selected, 0);
        assert!(view.error.is_none());
        view.key(KeyAction::Left);
        assert_eq!(
            decision(view.key(KeyAction::Enter)),
            ApprovalDecision::Reject { feedback: None }
        );
        assert!(view.active().is_some());
    }
    #[tokio::test]
    async fn painted_mouse_request_guard_geometry_and_composer_preservation() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut state = TuiState::new(app.clone(), SessionId("root".into()));
        state.handle_paste("draft 🦊\nwith cursor");
        let draft = state.input().to_string();
        for width in [79, 80, 120, 121] {
            state.approvals.reconcile(vec![(request(1, true), false)]);
            let rows = crate::views::render_test(&state, width, 32);
            assert!(rows.iter().any(|r| r.contains("Permission required")));
            assert!(rows.iter().any(|r| r.contains("src/main.rs")));
            for _ in 0..20 {
                state.approvals.key(KeyAction::PageDown);
            }
            assert_eq!(
                state.approvals.scroll, 0,
                "short proposed preview cannot overscroll"
            );
            let (target, area) = {
                let paint = state.approvals.painted.borrow();
                let paint = paint.as_ref().unwrap();
                (paint.targets[0], paint.area)
            };
            let event = |kind| MouseEvent {
                kind,
                column: target.x,
                row: target.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            };
            state
                .approvals
                .mouse(event(MouseEventKind::Down(MouseButton::Left)), area);
            state.approvals.reconcile(vec![(request(2, true), false)]);
            assert!(
                state
                    .approvals
                    .mouse(event(MouseEventKind::Up(MouseButton::Left)), area)
                    .intent
                    .is_none()
            );
            state.approvals.fullscreen = true;
            let rows = crate::views::render_test(&state, width, 32);
            assert!(rows[2].contains("Permission required"));
            state.handle_key(KeyAction::Interrupt).await;
            assert!(!state.approvals.fullscreen);
            assert_eq!(state.input(), draft);
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}
