//! Lower FormPrompt consumer, based on pinned session/form.tsx behavior.
//! Request-bound drafts live for the process, including root/tab remounts.
use crate::{
    app::{KeyOutcome, PanelIntent, TuiState},
    editor::Editor,
    events::KeyAction,
    theme::Theme,
};
use oc_core::question::{QuestionDecision, QuestionReply, QuestionRequest};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

#[derive(Default)]
pub struct QuestionView {
    drafts: std::sync::Arc<std::sync::Mutex<Vec<Draft>>>,
    visible: Vec<QuestionRequest>,
}
struct Draft {
    request: QuestionRequest,
    tab: usize,
    selected: Vec<usize>,
    answers: Vec<Vec<String>>,
    answered: Vec<bool>,
    custom: Vec<String>,
    editing: bool,
    editor: Editor,
    submitting: bool,
    error: Option<String>,
    scroll: usize,
    painted: Option<Painted>,
    pointer_down: Option<(Rect, Control)>,
    hover_tab: Option<usize>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Tab(usize),
    Option(usize),
    Submit,
    Cancel,
}
struct Painted {
    frame: Rect,
    targets: Vec<(Rect, Control)>,
    input: Option<(Rect, usize, String)>,
}
impl Draft {
    fn new(request: QuestionRequest) -> Self {
        let count = request.input.questions.len();
        Self {
            request,
            tab: 0,
            selected: vec![0; count],
            answers: vec![vec![]; count],
            answered: vec![false; count],
            custom: vec![String::new(); count],
            editing: false,
            editor: Editor::default(),
            submitting: false,
            error: None,
            scroll: 0,
            painted: None,
            pointer_down: None,
            hover_tab: None,
        }
    }
    fn single(&self) -> bool {
        self.request.input.questions.len() == 1 && !self.request.input.questions[0].multiple
    }
    fn select_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.editing = false;
        self.error = None;
        self.scroll = 0;
        if let Some(q) = self.request.input.questions.get(tab) {
            self.selected[tab] = if q.multiple || self.answers[tab].is_empty() {
                0
            } else {
                q.options
                    .iter()
                    .position(|o| self.answers[tab].contains(&o.label))
                    .unwrap_or(q.options.len())
            };
        }
    }
    fn reply(&mut self, decision: QuestionDecision) -> KeyOutcome {
        if let QuestionDecision::Answers(answers) = &decision
            && let Err(error) = self.request.input.validate_answers(answers)
        {
            self.error = Some(error.to_string());
            return KeyOutcome::default();
        }
        self.submitting = true;
        self.error = None;
        KeyOutcome {
            intent: Some(PanelIntent::ReplyQuestion(QuestionReply {
                id: self.request.id,
                binding: self.request.binding.clone(),
                decision,
            })),
            ..KeyOutcome::default()
        }
    }
    fn select(&mut self) -> KeyOutcome {
        if self.tab == self.answers.len() {
            return self.reply(QuestionDecision::Answers(self.answers.clone()));
        }
        let q = &self.request.input.questions[self.tab];
        let selection = self.selected[self.tab];
        if selection == q.options.len() {
            if q.multiple
                && !self.custom[self.tab].is_empty()
                && self.answers[self.tab].contains(&self.custom[self.tab])
            {
                self.answers[self.tab].retain(|a| a != &self.custom[self.tab]);
                self.answered[self.tab] = true;
                return KeyOutcome::default();
            }
            self.editing = true;
            self.editor.move_to(self.custom[self.tab].len(), false);
            return KeyOutcome::default();
        }
        let label = q.options[selection].label.clone();
        self.answered[self.tab] = true;
        if q.multiple {
            let a = &mut self.answers[self.tab];
            if a.contains(&label) {
                a.retain(|s| s != &label);
            } else {
                a.push(label);
            }
            self.order_answers();
        } else {
            self.answers[self.tab] = vec![label];
            if self.single() {
                return self.reply(QuestionDecision::Answers(self.answers.clone()));
            }
            self.select_tab(self.tab + 1);
        }
        KeyOutcome::default()
    }
    fn order_answers(&mut self) {
        let q = &self.request.input.questions[self.tab];
        self.answers[self.tab].sort_by_key(|a| {
            q.options
                .iter()
                .position(|o| &o.label == a)
                .unwrap_or(q.options.len())
        });
    }
    // The pinned textarea updates its owned answer on content change, including
    // before Esc closes editing. This is draft state, never an owner reply.
    fn update_custom_answer(&mut self) {
        let q = &self.request.input.questions[self.tab];
        let text = &self.custom[self.tab];
        if q.multiple {
            self.answers[self.tab].retain(|a| q.options.iter().any(|o| &o.label == a));
            if !text.is_empty() && !self.answers[self.tab].contains(text) {
                self.answers[self.tab].push(text.clone());
            }
            self.order_answers();
        } else {
            self.answers[self.tab] = if text.is_empty() {
                vec![]
            } else {
                vec![text.clone()]
            };
        }
        self.answered[self.tab] = !self.answers[self.tab].is_empty();
        self.error = None;
    }
    fn commit_custom(&mut self) -> bool {
        let text = self.custom[self.tab].trim().to_string();
        let q = &self.request.input.questions[self.tab];
        let multiple = q.multiple;
        // QuestionTool::toField does not mark its string field required. A
        // blank commit clears that answer; no new required-field restriction.
        if q.multiple {
            self.answers[self.tab].retain(|a| q.options.iter().any(|o| &o.label == a));
            if !text.is_empty() && !self.answers[self.tab].contains(&text) {
                self.answers[self.tab].push(text.clone());
            }
            self.order_answers();
        } else {
            self.answers[self.tab] = if text.is_empty() {
                vec![]
            } else {
                vec![text.clone()]
            };
        }
        self.answered[self.tab] = multiple || !text.is_empty();
        self.custom[self.tab] = text;
        self.editing = false;
        self.error = None;
        true
    }
}
impl QuestionView {
    pub fn reconcile(&mut self, all: &[QuestionRequest], visible: Vec<QuestionRequest>) {
        let mut drafts = self.drafts.lock().expect("question drafts");
        if self.visible != visible {
            for d in drafts.iter_mut() {
                d.painted = None;
                d.pointer_down = None;
                d.hover_tab = None;
            }
        }
        drafts.retain(|d| all.contains(&d.request));
        for request in &visible {
            if !drafts.iter().any(|d| d.request == *request) && drafts.len() < 64 {
                drafts.push(Draft::new(request.clone()));
            }
        }
        self.visible = visible;
    }
    pub fn active(&self) -> Option<&QuestionRequest> {
        self.visible.first()
    }
    /// The frontend loop owns process-memory drafts; remounted views share them.
    pub fn share_drafts(&mut self, owner: &Self) {
        // Synchronization happens before every input, including between mouse
        // down and up. Invalidate transient ownership only on a real remount.
        if std::sync::Arc::ptr_eq(&self.drafts, &owner.drafts) {
            return;
        }
        self.drafts = owner.drafts.clone();
        for d in self.drafts.lock().expect("question drafts").iter_mut() {
            d.painted = None;
            d.pointer_down = None;
            d.hover_tab = None;
        }
    }
    pub fn reply_result(&mut self, reply: &QuestionReply, error: Option<String>) {
        if let Some(d) = self
            .drafts
            .lock()
            .expect("question drafts")
            .iter_mut()
            .find(|d| d.request.id == reply.id && d.request.binding == reply.binding)
        {
            d.submitting = error.is_none();
            d.error = error;
            d.painted = None;
            d.pointer_down = None;
        }
    }
    pub fn terminal_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if event.kind != KeyEventKind::Press {
            return None;
        }
        if event.code == KeyCode::BackTab
            || event.code == KeyCode::Tab && event.modifiers == KeyModifiers::SHIFT
        {
            return Some(KeyAction::DialogActionPrevious);
        }
        crate::events::map_key(event)
    }
    pub fn paste(&mut self, text: &str) {
        let active = self.active().cloned();
        if let Some(d) = self
            .drafts
            .lock()
            .expect("question drafts")
            .iter_mut()
            .find(|d| Some(&d.request) == active.as_ref())
            && d.tab < d.answers.len()
            && !d.submitting
        {
            d.painted = None;
            d.pointer_down = None;
            let clean = text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .collect::<String>();
            if d.custom[d.tab].len() + clean.len() <= oc_core::question::ANSWER_BYTES_CAP {
                if !d.editing {
                    d.selected[d.tab] = d.request.input.questions[d.tab].options.len();
                    d.editor.move_to(d.custom[d.tab].len(), false);
                    d.editing = true;
                }
                d.editor.replace(
                    &mut d.custom[d.tab],
                    &clean,
                    oc_core::question::ANSWER_BYTES_CAP,
                );
                d.update_custom_answer();
            } else {
                d.error = Some("Answer exceeds 4096 bytes".into());
            }
        }
    }
    pub fn key(&mut self, action: KeyAction) -> KeyOutcome {
        let active = self.active().cloned();
        let mut drafts = self.drafts.lock().expect("question drafts");
        let Some(d) = drafts
            .iter_mut()
            .find(|d| Some(&d.request) == active.as_ref())
        else {
            return KeyOutcome::default();
        };
        if d.submitting {
            return KeyOutcome::default();
        }
        d.painted = None;
        d.pointer_down = None;
        d.hover_tab = None;
        if !d.editing
            && d.tab < d.answers.len()
            && d.selected[d.tab] == d.request.input.questions[d.tab].options.len()
            && matches!(&action, KeyAction::Char(c) if !c.is_control() && *c != ' ')
        {
            d.editing = true;
            d.editor.move_to(d.custom[d.tab].len(), false);
        }
        if matches!(action, KeyAction::Tab | KeyAction::DialogActionPrevious) {
            let previous = action == KeyAction::DialogActionPrevious;
            if d.editing && !d.commit_custom() && !previous {
                return KeyOutcome::default();
            }
            if !d.single() {
                d.select_tab(
                    (d.tab + if previous { d.answers.len() } else { 1 }) % (d.answers.len() + 1),
                );
            }
            d.editing = false;
            d.error = None;
            d.scroll = 0;
            return KeyOutcome::default();
        }
        if d.editing {
            match action {
                KeyAction::Cancel => d.editing = false,
                KeyAction::Interrupt => {
                    if d.custom[d.tab].is_empty() {
                        d.editing = false;
                    } else {
                        d.custom[d.tab].clear();
                        d.editor.move_to(0, false);
                        d.update_custom_answer();
                    }
                }
                KeyAction::Enter => {
                    let empty = d.custom[d.tab].trim().is_empty();
                    if !d.commit_custom() {
                        return KeyOutcome::default();
                    }
                    if empty {
                        return KeyOutcome::default();
                    }
                    if d.single() {
                        return d.reply(QuestionDecision::Answers(d.answers.clone()));
                    }
                    if !d.request.input.questions[d.tab].multiple {
                        d.select_tab(d.tab + 1);
                    }
                }
                KeyAction::Char(c) if !c.is_control() => {
                    if d.custom[d.tab].len() + c.len_utf8() <= oc_core::question::ANSWER_BYTES_CAP {
                        d.editor.replace(
                            &mut d.custom[d.tab],
                            &c.to_string(),
                            oc_core::question::ANSWER_BYTES_CAP,
                        );
                        d.update_custom_answer();
                    }
                }
                KeyAction::Backspace => {
                    d.editor.delete(&mut d.custom[d.tab], true, false);
                    d.update_custom_answer();
                }
                KeyAction::Delete => {
                    d.editor.delete(&mut d.custom[d.tab], false, false);
                    d.update_custom_answer();
                }
                KeyAction::Left => d.editor.horizontal(&d.custom[d.tab], false, false, false),
                KeyAction::Right => d.editor.horizontal(&d.custom[d.tab], true, false, false),
                KeyAction::Home | KeyAction::CtrlA => d.editor.move_to(0, false),
                KeyAction::End => d.editor.move_to(d.custom[d.tab].len(), false),
                KeyAction::Up => {
                    if !d.editor.vertical(&d.custom[d.tab], false, false) && d.selected[d.tab] > 0 {
                        d.editing = false;
                        d.selected[d.tab] -= 1;
                    }
                }
                KeyAction::Down => {
                    d.editor.vertical(&d.custom[d.tab], true, false);
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Cancel | KeyAction::Interrupt | KeyAction::Quit => {
                return d.reply(QuestionDecision::Cancelled);
            }
            KeyAction::PageUp => d.scroll = d.scroll.saturating_sub(8),
            KeyAction::PageDown => d.scroll = d.scroll.saturating_add(8).min(1024),
            KeyAction::Up if d.tab == d.answers.len() => d.scroll = d.scroll.saturating_sub(1),
            KeyAction::Down if d.tab == d.answers.len() => {
                d.scroll = d.scroll.saturating_add(1).min(1024)
            }
            KeyAction::Right | KeyAction::Char('l') if !d.single() => {
                d.select_tab((d.tab + 1) % (d.answers.len() + 1));
            }
            KeyAction::Left | KeyAction::Char('h') if !d.single() => {
                d.select_tab((d.tab + d.answers.len()) % (d.answers.len() + 1));
            }
            KeyAction::Enter | KeyAction::Char(' ') => return d.select(),
            KeyAction::Up | KeyAction::Char('k') if d.tab < d.answers.len() => {
                let count = d.request.input.questions[d.tab].options.len() + 1;
                d.selected[d.tab] = (d.selected[d.tab] + count - 1) % count;
                d.scroll = 0;
            }
            KeyAction::Down | KeyAction::Char('j') if d.tab < d.answers.len() => {
                let count = d.request.input.questions[d.tab].options.len() + 1;
                d.selected[d.tab] = (d.selected[d.tab] + 1) % count;
                d.scroll = 0;
            }
            KeyAction::Char(c @ '1'..='9') if d.tab < d.answers.len() => {
                let index = c as usize - '1' as usize;
                if index <= d.request.input.questions[d.tab].options.len() {
                    d.selected[d.tab] = index;
                    return d.select();
                }
            }
            _ => {}
        }
        KeyOutcome::default()
    }

    pub(crate) fn cancel_pointer(&self) {
        for draft in self.drafts.lock().expect("question drafts").iter_mut() {
            draft.painted = None;
            draft.pointer_down = None;
            draft.hover_tab = None;
        }
    }

    pub(crate) fn mouse(&mut self, event: crossterm::event::MouseEvent, frame: Rect) -> KeyOutcome {
        use crossterm::event::{MouseButton, MouseEventKind};
        let active = self.active().cloned();
        let mut drafts = self.drafts.lock().expect("question drafts");
        let Some(d) = drafts
            .iter_mut()
            .find(|d| Some(&d.request) == active.as_ref())
        else {
            return KeyOutcome::default();
        };
        let Some(paint) = d
            .painted
            .as_ref()
            .filter(|p| p.frame == frame && !d.submitting)
        else {
            d.pointer_down = None;
            return KeyOutcome::default();
        };
        let point = (event.column, event.row).into();
        if d.editing
            && event.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some((rect, top, raw)) = &paint.input
            && rect.contains(point)
            && *raw == d.custom[d.tab]
        {
            let rows = d.editor.layout_words(raw, usize::from(rect.width.max(1))).0;
            let offset = rows
                .get(top + usize::from(event.row - rect.y))
                .and_then(|row| row.offset_at(usize::from(event.column - rect.x)));
            if let Some(offset) = offset {
                d.editor.move_to(offset, false);
            }
            d.pointer_down = None;
            return KeyOutcome::default();
        }
        let hit = paint
            .targets
            .iter()
            .find(|(rect, _)| rect.contains(point))
            .map(|(_, target)| *target);
        match event.kind {
            MouseEventKind::Moved => {
                d.hover_tab = match hit {
                    Some(Control::Tab(index)) => Some(index),
                    _ => None,
                };
                if !d.editing
                    && let Some(Control::Option(index)) = hit
                {
                    d.selected[d.tab] = index;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                d.pointer_down = hit.map(|target| (frame, target));
                if !d.editing
                    && let Some(Control::Option(index)) = hit
                {
                    d.selected[d.tab] = index;
                }
            }
            MouseEventKind::Drag(_) => d.pointer_down = None,
            MouseEventKind::Up(MouseButton::Left) => {
                if d.pointer_down.take() != hit.map(|target| (frame, target)) || hit.is_none() {
                    return KeyOutcome::default();
                }
                d.painted = None;
                match hit.unwrap() {
                    Control::Option(index) if !d.editing => {
                        d.selected[d.tab] = index;
                        return d.select();
                    }
                    Control::Tab(index) => {
                        if d.editing && !d.commit_custom() && index >= d.tab {
                            return KeyOutcome::default();
                        }
                        d.select_tab(index);
                    }
                    Control::Submit if d.tab == d.answers.len() => {
                        return d.reply(QuestionDecision::Answers(d.answers.clone()));
                    }
                    Control::Cancel => return d.reply(QuestionDecision::Cancelled),
                    _ => {}
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if d.tab == d.answers.len() => {
                d.scroll = if event.kind == MouseEventKind::ScrollUp {
                    d.scroll.saturating_sub(3)
                } else {
                    d.scroll.saturating_add(3).min(1024)
                };
                d.painted = None;
                d.pointer_down = None;
            }
            _ => {}
        }
        KeyOutcome::default()
    }
}
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn label(text: &str, limit: usize) -> String {
    let text = safe(text);
    if text.encode_utf16().count() <= limit {
        return text;
    }
    use unicode_segmentation::UnicodeSegmentation;
    let mut units = 0;
    let prefix: String = text
        .graphemes(true)
        .take_while(|s| {
            units += s.encode_utf16().count();
            units < limit
        })
        .collect();
    format!("{}…", prefix.trim_end())
}

struct FormLayout {
    rows: Vec<Line<'static>>,
    cursor: Option<(usize, usize)>,
    targets: Vec<(Rect, Control)>,
    input: Option<(Rect, usize)>,
}

fn wrapped_text(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    text.split('\n')
        .flat_map(|text| {
            crate::styled::wrap_source_space_line_limited(
                &crate::styled::Line::new(vec![crate::styled::Span::styled(safe(text), style)]),
                width.max(1),
                1024,
            )
            .into_iter()
            .map(crate::styled::Line::into_ratatui)
        })
        .take(1024)
        .collect()
}

fn layout(d: &Draft, width: u16, terminal_height: u16) -> FormLayout {
    let theme = Theme::dark();
    let slot = |name| theme.color(name).expect("question form theme role");
    let base = Style::default().fg(theme.text());
    let muted = Style::default().fg(theme.text_muted());
    let focused = slot("text.formfield.$focused");
    let selected = slot("text.formfield.$selected");
    // The pinned palette supplies only the formfield base background; absent
    // state overrides inherit that same semantic role in the donor resolver.
    let form_bg = slot("background.formfield.base");
    let focus_bg = theme
        .color("background.formfield.$focused")
        .unwrap_or(form_bg);
    let content_width = usize::from(width.saturating_sub(10).max(1));
    let mut out = vec![
        Line::default(),
        Line::from(Span::styled("Questions", muted)),
    ];
    let mut targets = Vec::new();
    if !d.single() {
        out.push(Line::default());
        let headers: Vec<_> = d
            .request
            .input
            .questions
            .iter()
            .map(|q| label(&q.header, 24))
            .collect();
        let tabbed = headers
            .iter()
            .map(|s| s.encode_utf16().count() + 3)
            .sum::<usize>()
            + 9
            <= usize::from(width.saturating_sub(8));
        if tabbed {
            let mut spans = Vec::new();
            let mut x = 0;
            for (i, text) in headers
                .into_iter()
                .chain(std::iter::once("Submit".into()))
                .enumerate()
            {
                let active = i == d.tab;
                let hover = d.hover_tab == Some(i);
                let bg = if active {
                    theme
                        .color("background.formfield.$selected")
                        .unwrap_or(form_bg)
                } else if hover {
                    focus_bg
                } else {
                    theme.background_raised()
                };
                let length = Line::raw(&text).width() + if i < d.answers.len() { 2 } else { 0 };
                targets.push((
                    Rect::new(x as u16, out.len() as u16, length as u16, 1),
                    Control::Tab(i),
                ));
                x += length;
                spans.push(Span::styled(
                    text,
                    if active {
                        base.bg(bg).add_modifier(Modifier::BOLD)
                    } else if hover {
                        Style::default().fg(focused).bg(bg)
                    } else {
                        muted.bg(bg)
                    },
                ));
                if i < d.answers.len() {
                    spans.push(Span::styled("  ", Style::default().bg(bg)));
                }
            }
            out.push(Line::from(spans));
        } else {
            out.push(Line::from(Span::styled(
                format!(
                    "{}   · {}/{} completed",
                    if d.tab == d.answers.len() {
                        "Review".into()
                    } else {
                        format!("Field {} of {}", d.tab + 1, d.answers.len())
                    },
                    d.answered.iter().filter(|a| **a).count(),
                    d.answers.len()
                ),
                muted,
            )));
        }
    }
    out.push(Line::default());
    let body_start = out.len();
    let mut cursor = None;
    let mut input = None;
    if d.tab == d.answers.len() {
        let start = out.len();
        for (i, (q, a)) in d.request.input.questions.iter().zip(&d.answers).enumerate() {
            let answer = if !d.answered[i] {
                "(not answered)".into()
            } else if a.is_empty() {
                "(none)".into()
            } else {
                a.iter().map(|s| safe(s)).collect::<Vec<_>>().join(", ")
            };
            let line = crate::styled::Line::new(vec![
                crate::styled::Span::styled(format!("{}:", label(&q.header, 40)), muted),
                crate::styled::Span::plain(" "),
                crate::styled::Span::styled(answer, if d.answered[i] { base } else { muted }),
            ]);
            out.extend(
                crate::styled::wrap_source_space_line_limited(&line, content_width, 1024)
                    .into_iter()
                    .map(crate::styled::Line::into_ratatui),
            );
        }
        let max = usize::from(terminal_height.saturating_sub(14).max(3));
        let count = out.len() - start;
        let scroll = d.scroll.min(count.saturating_sub(max));
        let review: Vec<_> = out.drain(start..).skip(scroll).take(max).collect();
        out.extend(review);
    } else {
        let q = &d.request.input.questions[d.tab];
        out.extend(wrapped_text(&q.question, content_width, base));
        out.push(Line::default());
        for (i, option) in q.options.iter().enumerate() {
            let start = out.len();
            let active = d.selected[d.tab] == i;
            let picked = d.answers[d.tab].contains(&option.label);
            let bg = if active {
                focus_bg
            } else {
                theme.background_raised()
            };
            let mut prefix = vec![
                Span::styled(
                    format!("{}.", i + 1),
                    Style::default()
                        .fg(if active { focused } else { theme.text_muted() })
                        .bg(bg),
                ),
                Span::styled(" ", Style::default().bg(bg)),
            ];
            if q.multiple {
                prefix.push(Span::styled(
                    if picked { "[✓]" } else { "[ ]" },
                    Style::default()
                        .fg(if active {
                            focused
                        } else if picked {
                            selected
                        } else {
                            theme.text_muted()
                        })
                        .bg(bg),
                ));
                prefix.push(Span::styled(" ", Style::default().bg(bg)));
            }
            let indent: usize = prefix.iter().map(Span::width).sum();
            let label_rows = wrapped_text(
                &option.label,
                content_width.saturating_sub(indent),
                Style::default()
                    .fg(if active {
                        focused
                    } else {
                        slot("text.formfield.base")
                    })
                    .bg(bg),
            );
            for (n, mut row) in label_rows.into_iter().enumerate() {
                let mut spans = if n == 0 {
                    prefix.clone()
                } else {
                    vec![Span::styled(" ".repeat(indent), Style::default().bg(bg))]
                };
                spans.append(&mut row.spans);
                if n == 0 && !q.multiple && picked {
                    spans.push(Span::styled(" ✓", Style::default().fg(selected)));
                }
                out.push(Line::from(spans));
            }
            if !option.description.is_empty() {
                let indent = if q.multiple { 7 } else { 3 };
                for mut row in wrapped_text(
                    &option.description,
                    content_width.saturating_sub(indent),
                    muted,
                ) {
                    row.spans.insert(0, Span::raw(" ".repeat(indent)));
                    out.push(row);
                }
            }
            targets.push((
                Rect::new(
                    0,
                    start as u16,
                    content_width as u16,
                    (out.len() - start) as u16,
                ),
                Control::Option(i),
            ));
        }
        let start = out.len();
        let active = d.selected[d.tab] == q.options.len();
        let bg = if active {
            focus_bg
        } else {
            theme.background_raised()
        };
        let picked = d.answers[d.tab].contains(&d.custom[d.tab]);
        let mut prefix = vec![
            Span::styled(
                format!("{}.", q.options.len() + 1),
                Style::default()
                    .fg(if active { focused } else { theme.text_muted() })
                    .bg(bg),
            ),
            Span::styled(" ", Style::default().bg(bg)),
        ];
        if q.multiple {
            prefix.push(Span::styled(
                if picked || active && d.editing {
                    "[✓]"
                } else {
                    "[ ]"
                },
                Style::default()
                    .fg(if active {
                        focused
                    } else if picked {
                        selected
                    } else {
                        theme.text_muted()
                    })
                    .bg(bg),
            ));
            prefix.push(Span::styled(" ", Style::default().bg(bg)));
        }
        let indent: usize = prefix.iter().map(Span::width).sum();
        let text_width = content_width.saturating_sub(indent).max(1);
        if d.editing {
            let (rows, (row, col)) = d.editor.layout_words(&d.custom[d.tab], text_width);
            let top = row.saturating_sub(5);
            cursor = Some((out.len() + row - top, indent + col));
            input = Some((
                Rect::new(
                    indent as u16,
                    out.len() as u16,
                    text_width as u16,
                    rows.len().saturating_sub(top).min(6) as u16,
                ),
                top,
            ));
            for (n, row) in rows.into_iter().skip(top).take(6).enumerate() {
                let mut spans = if n == 0 {
                    prefix.clone()
                } else {
                    vec![Span::styled(" ".repeat(indent), Style::default().bg(bg))]
                };
                spans.push(Span::styled(
                    if d.custom[d.tab].is_empty() {
                        "Type your own answer".into()
                    } else {
                        row.text
                    },
                    Style::default()
                        .fg(if d.custom[d.tab].is_empty() {
                            theme.text_muted()
                        } else {
                            focused
                        })
                        .bg(bg),
                ));
                let used: usize = spans.iter().map(Span::width).sum();
                spans.push(Span::styled(
                    " ".repeat(content_width.saturating_sub(used)),
                    Style::default().bg(bg),
                ));
                out.push(Line::from(spans));
            }
        } else {
            let display = if d.custom[d.tab].is_empty() {
                "Type your own answer"
            } else {
                &d.custom[d.tab]
            };
            let style = Style::default()
                .fg(if active {
                    focused
                } else {
                    slot("text.formfield.base")
                })
                .bg(bg);
            let offset = prefix.iter().map(Span::width).sum::<usize>();
            let rows = wrapped_text(display, content_width.saturating_sub(offset), style);
            let last = rows.len().saturating_sub(1);
            for (row, line) in rows.into_iter().enumerate() {
                let mut spans = if row == 0 {
                    std::mem::take(&mut prefix)
                } else {
                    vec![Span::styled(" ".repeat(offset), Style::default().bg(bg))]
                };
                spans.extend(line.spans);
                if row == last && !q.multiple && picked {
                    spans.push(Span::styled("✓", Style::default().fg(selected).bg(bg)));
                }
                let used = spans.iter().map(Span::width).sum::<usize>();
                spans.push(Span::styled(
                    " ".repeat(content_width.saturating_sub(used)),
                    Style::default().bg(bg),
                ));
                out.push(Line::from(spans));
            }
        }
        targets.push((
            Rect::new(
                0,
                start as u16,
                content_width as u16,
                (out.len() - start) as u16,
            ),
            Control::Option(q.options.len()),
        ));
    }
    // Keep the owned heading and actions visible even when a long option's
    // description exceeds the viewport. Only body rows scroll; no answer,
    // request binding or source metadata is discarded by this projection.
    let body_len = out.len() - body_start;
    let available = usize::from(terminal_height)
        .saturating_sub(body_start + 5)
        .max(1);
    if body_len > available {
        let focus = if let Some((row, _)) = cursor {
            row
        } else {
            targets
                .iter()
                .find(|(_, c)| d.tab < d.answers.len() && *c == Control::Option(d.selected[d.tab]))
                .map_or(body_start, |(r, _)| usize::from(r.y))
        };
        let top = if d.scroll > 0 {
            d.scroll.min(body_len - available)
        } else {
            focus
                .saturating_sub(body_start + available - 1)
                .min(body_len - available)
        };
        let visible = Rect::new(
            0,
            (body_start + top) as u16,
            content_width as u16,
            available as u16,
        );
        for (r, _) in &mut targets {
            if usize::from(r.y) >= body_start {
                *r = r.intersection(visible);
                r.y = r.y.saturating_sub(top as u16);
            }
        }
        if let Some((r, source_top)) = &mut input {
            let old_y = r.y;
            *r = r.intersection(visible);
            *source_top += usize::from(r.y.saturating_sub(old_y));
            r.y = r.y.saturating_sub(top as u16);
        }
        cursor = cursor
            .filter(|(row, _)| *row >= body_start + top && *row < body_start + top + available)
            .map(|(row, col)| (row - top, col));
        let body: Vec<_> = out.drain(body_start..).skip(top).take(available).collect();
        out.extend(body);
    }
    out.push(Line::default());
    let mut hints = Vec::new();
    if !d.single() {
        hints.extend([
            Span::styled("⇆", base),
            Span::styled(" ", base),
            Span::styled("tab", muted),
            Span::raw("  "),
        ]);
    }
    if d.tab < d.answers.len() && !d.editing {
        hints.extend([
            Span::styled("↑↓", base),
            Span::styled(" ", base),
            Span::styled("select", muted),
            Span::raw("  "),
        ]);
    }
    let action = if d.tab == d.answers.len() || d.single() {
        "submit"
    } else if d.request.input.questions[d.tab].multiple {
        if d.editing {
            "done"
        } else if d.selected[d.tab] == d.request.input.questions[d.tab].options.len()
            && d.custom[d.tab].is_empty()
        {
            "edit"
        } else {
            "toggle"
        }
    } else {
        "confirm"
    };
    let action_x: usize = hints.iter().map(Span::width).sum();
    if d.tab == d.answers.len() {
        targets.push((
            Rect::new(
                action_x as u16,
                out.len() as u16,
                (6 + action.len()) as u16,
                1,
            ),
            Control::Submit,
        ));
    }
    hints.extend([
        Span::styled("enter", base),
        Span::styled(" ", base),
        Span::styled(action, muted),
        Span::raw("  "),
        Span::styled("esc", base),
        Span::styled(" ", base),
        Span::styled(if d.editing { "close" } else { "dismiss" }, muted),
    ]);
    let hints_width: usize = hints.iter().map(Span::width).sum();
    let cancel_width = if d.editing { 9 } else { 11 };
    targets.push((
        Rect::new(
            hints_width.saturating_sub(cancel_width) as u16,
            out.len() as u16,
            cancel_width as u16,
            1,
        ),
        Control::Cancel,
    ));
    if let Some(error) = &d.error {
        let used: usize = hints.iter().map(Span::width).sum();
        let error = safe(error);
        hints.push(Span::raw(
            " ".repeat(
                content_width
                    .saturating_sub(used + Line::raw(&error).width())
                    .max(1),
            ),
        ));
        hints.push(Span::styled(error, Style::default().fg(theme.error())));
    }
    out.extend([Line::from(hints), Line::default()]);
    FormLayout {
        rows: out,
        cursor,
        targets,
        input,
    }
}
pub(crate) fn height(state: &TuiState, width: u16, terminal_height: u16) -> u16 {
    let drafts = state.questions.drafts.lock().expect("question drafts");
    drafts
        .iter()
        .find(|d| Some(&d.request) == state.questions.active())
        .map_or(0, |d| layout(d, width, terminal_height).rows.len() as u16)
}
pub fn render(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    if state.approvals.active().is_some() || state.questions.active().is_none() {
        state.questions.cancel_pointer();
        return;
    }
    let mut drafts = state.questions.drafts.lock().expect("question drafts");
    let Some(d) = drafts
        .iter_mut()
        .find(|d| Some(&d.request) == state.questions.active())
    else {
        return;
    };
    let form = layout(d, area.width, frame.area().height);
    let content = crate::layout::content_box(area);
    let height = (form.rows.len() as u16).min(content.height);
    let rect = Rect::new(
        content.x,
        content.bottom().saturating_sub(height),
        content.width,
        height,
    );
    let theme = Theme::dark();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background_raised())),
        rect,
    );
    frame.render_widget(
        Paragraph::new(vec![Line::raw("┃"); usize::from(rect.height)]).style(
            Style::default().fg(theme
                .color("background.action.primary.$focused")
                .expect("question border role")),
        ),
        Rect::new(rect.x, rect.y, rect.width.min(1), rect.height),
    );
    let text = Rect::new(
        rect.x.saturating_add(3),
        rect.y,
        rect.width.saturating_sub(6),
        rect.height,
    );
    frame.render_widget(Paragraph::new(form.rows), text);
    if state.panel() != &crate::app::TuiPanel::None {
        d.painted = None;
        d.pointer_down = None;
        d.hover_tab = None;
        return;
    }
    if let Some((row, col)) = form.cursor
        && row < usize::from(text.height)
        && text.width > 0
    {
        frame.set_cursor_position((
            text.x + (col as u16).min(text.width - 1),
            text.y + row as u16,
        ));
    }
    d.painted = Some(Painted {
        frame: frame.area(),
        targets: form
            .targets
            .into_iter()
            .map(|(r, target)| {
                (
                    Rect::new(
                        text.x.saturating_add(r.x),
                        text.y.saturating_add(r.y),
                        r.width,
                        r.height,
                    )
                    .intersection(text),
                    target,
                )
            })
            .collect(),
        input: form.input.map(|(r, top)| {
            (
                Rect::new(
                    text.x.saturating_add(r.x),
                    text.y.saturating_add(r.y),
                    r.width,
                    r.height,
                )
                .intersection(text),
                top,
                d.custom[d.tab].clone(),
            )
        }),
    });
}

#[cfg(test)]
mod tests;
