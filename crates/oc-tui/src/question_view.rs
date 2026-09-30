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
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
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
    custom: Vec<String>,
    editing: bool,
    editor: Editor,
    submitting: bool,
    error: Option<String>,
    scroll: usize,
}
impl Draft {
    fn new(request: QuestionRequest) -> Self {
        let count = request.input.questions.len();
        Self {
            request,
            tab: 0,
            selected: vec![0; count],
            answers: vec![vec![]; count],
            custom: vec![String::new(); count],
            editing: false,
            editor: Editor::default(),
            submitting: false,
            error: None,
            scroll: 0,
        }
    }
    fn single(&self) -> bool {
        self.request.input.questions.len() == 1 && !self.request.input.questions[0].multiple
    }
    fn reply(&mut self, decision: QuestionDecision) -> KeyOutcome {
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
            self.editing = true;
            self.editor.move_to(self.custom[self.tab].len(), false);
            return KeyOutcome::default();
        }
        let label = q.options[selection].label.clone();
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
            self.tab += 1;
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
}
impl QuestionView {
    pub fn reconcile(&mut self, all: &[QuestionRequest], visible: Vec<QuestionRequest>) {
        let mut drafts = self.drafts.lock().expect("question drafts");
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
        self.drafts = owner.drafts.clone();
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
        }
    }
    pub fn terminal_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        use crossterm::event::{KeyCode, KeyEventKind};
        if event.kind != KeyEventKind::Press {
            return None;
        }
        if event.code == KeyCode::BackTab {
            return Some(KeyAction::Left);
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
            && d.editing
            && !d.submitting
        {
            let clean = text.chars().filter(|c| !c.is_control()).collect::<String>();
            if d.custom[d.tab].len() + clean.len() <= oc_core::question::ANSWER_BYTES_CAP {
                d.editor.replace(
                    &mut d.custom[d.tab],
                    &clean,
                    oc_core::question::ANSWER_BYTES_CAP,
                );
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
        if d.editing {
            match action {
                KeyAction::Cancel => d.editing = false,
                KeyAction::Interrupt => {
                    if d.custom[d.tab].is_empty() {
                        d.editing = false;
                    } else {
                        d.custom[d.tab].clear();
                        d.editor.move_to(0, false);
                    }
                }
                KeyAction::Enter => {
                    let text = d.custom[d.tab].trim().to_string();
                    if text.is_empty() {
                        d.error = Some("Enter an answer".into());
                        return KeyOutcome::default();
                    }
                    let options = &d.request.input.questions[d.tab].options;
                    if d.request.input.questions[d.tab].multiple {
                        d.answers[d.tab].retain(|a| options.iter().any(|o| &o.label == a));
                        if !d.answers[d.tab].contains(&text) {
                            d.answers[d.tab].push(text);
                        }
                        d.order_answers();
                    } else {
                        d.answers[d.tab] = vec![text];
                    }
                    d.editing = false;
                    if d.single() {
                        return d.reply(QuestionDecision::Answers(d.answers.clone()));
                    }
                    if !d.request.input.questions[d.tab].multiple {
                        d.tab += 1;
                    }
                }
                KeyAction::Char(c) if !c.is_control() => {
                    if d.custom[d.tab].len() + c.len_utf8() <= oc_core::question::ANSWER_BYTES_CAP {
                        d.editor.replace(
                            &mut d.custom[d.tab],
                            &c.to_string(),
                            oc_core::question::ANSWER_BYTES_CAP,
                        );
                    }
                }
                KeyAction::Backspace => {
                    d.editor.delete(&mut d.custom[d.tab], true, false);
                }
                KeyAction::Delete => {
                    d.editor.delete(&mut d.custom[d.tab], false, false);
                }
                KeyAction::Left => d.editor.horizontal(&d.custom[d.tab], false, false, false),
                KeyAction::Right => d.editor.horizontal(&d.custom[d.tab], true, false, false),
                KeyAction::Home | KeyAction::CtrlA => d.editor.move_to(0, false),
                KeyAction::End => d.editor.move_to(d.custom[d.tab].len(), false),
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
            KeyAction::Tab | KeyAction::Right | KeyAction::Char('l') if !d.single() => {
                d.tab = (d.tab + 1) % (d.answers.len() + 1)
            }
            KeyAction::Left | KeyAction::Char('h') if !d.single() => {
                d.tab = (d.tab + d.answers.len()) % (d.answers.len() + 1)
            }
            KeyAction::Enter | KeyAction::Char(' ') => return d.select(),
            KeyAction::Up | KeyAction::Char('k') if d.tab < d.answers.len() => {
                let count = d.request.input.questions[d.tab].options.len() + 1;
                d.selected[d.tab] = (d.selected[d.tab] + count - 1) % count;
            }
            KeyAction::Down | KeyAction::Char('j') if d.tab < d.answers.len() => {
                let count = d.request.input.questions[d.tab].options.len() + 1;
                d.selected[d.tab] = (d.selected[d.tab] + 1) % count;
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
}
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn lines(state: &TuiState) -> Vec<Line<'static>> {
    let drafts = state.questions.drafts.lock().expect("question drafts");
    let Some(d) = drafts
        .iter()
        .find(|d| Some(&d.request) == state.questions.active())
    else {
        return vec![];
    };
    let theme = Theme::dark();
    let normal = Style::default()
        .fg(theme.text())
        .bg(theme.background_raised());
    let muted = Style::default()
        .fg(theme.text_muted())
        .bg(theme.background_raised());
    let mut out = vec![Line::styled(
        format!(
            "Question · {} · {}",
            safe(&d.request.binding.session),
            if d.submitting {
                "Submitting…"
            } else {
                "Awaiting answer"
            }
        ),
        muted,
    )];
    if !d.single() {
        let tabs = d
            .request
            .input
            .questions
            .iter()
            .map(|q| safe(&q.header))
            .chain(std::iter::once("Submit".into()))
            .enumerate()
            .map(|(i, t)| if i == d.tab { format!("[{t}]") } else { t })
            .collect::<Vec<_>>()
            .join("  ");
        out.push(Line::styled(tabs, normal));
    }
    if d.tab == d.answers.len() {
        for (q, a) in d.request.input.questions.iter().zip(&d.answers) {
            out.push(Line::styled(
                format!(
                    "{}: {}",
                    safe(&q.header),
                    a.iter().map(|s| safe(s)).collect::<Vec<_>>().join(", ")
                ),
                normal,
            ));
        }
        out.push(Line::styled("Submit answers", normal));
    } else {
        let q = &d.request.input.questions[d.tab];
        out.push(Line::styled(safe(&q.question), normal));
        for (i, option) in q.options.iter().enumerate() {
            let selected = d.answers[d.tab].contains(&option.label);
            out.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{} {} {}. {}",
                        if d.selected[d.tab] == i { "›" } else { " " },
                        if selected { "[✓]" } else { "[ ]" },
                        i + 1,
                        safe(&option.label)
                    ),
                    normal,
                ),
                Span::styled(format!("  {}", safe(&option.description)), muted),
            ]));
        }
        out.push(Line::styled(
            format!(
                "{} {}. Type your own answer{}",
                if d.selected[d.tab] == q.options.len() {
                    "›"
                } else {
                    " "
                },
                q.options.len() + 1,
                if d.custom[d.tab].is_empty() {
                    String::new()
                } else {
                    format!(": {}", safe(&d.custom[d.tab]))
                }
            ),
            normal,
        ));
        if d.editing {
            out.push(Line::styled(
                format!("> {}", safe(&d.custom[d.tab])),
                normal,
            ));
        }
    }
    if let Some(error) = &d.error {
        out.push(Line::styled(
            safe(error),
            Style::default()
                .fg(theme.error())
                .bg(theme.background_raised()),
        ));
    }
    out.push(Line::styled(
        if d.editing {
            "Enter confirm · Esc back"
        } else if d.single() {
            "↑↓ select · Enter answer · Esc dismiss"
        } else {
            "↑↓ select · Space/Enter toggle · Tab review · Enter Submit · Esc dismiss"
        },
        muted,
    ));
    out
}
fn wrapped(state: &TuiState, width: u16) -> Vec<Line<'static>> {
    lines(state)
        .into_iter()
        .flat_map(|line| {
            let styled = crate::styled::Line::new(
                line.spans
                    .into_iter()
                    .map(|s| {
                        crate::styled::Span::styled(
                            s.content.into_owned(),
                            line.style.patch(s.style),
                        )
                    })
                    .collect(),
            );
            crate::styled::wrap_line_limited(&styled, usize::from(width.max(1)), 1024)
                .into_iter()
                .map(crate::styled::Line::into_ratatui)
        })
        .take(1024)
        .collect()
}
pub(crate) fn height(state: &TuiState, width: u16) -> u16 {
    (wrapped(state, width).len() as u16 + 2).min(24)
}
pub fn render(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    if state.approvals.active().is_some() || state.questions.active().is_none() {
        return;
    }
    let rows = wrapped(state, area.width.saturating_sub(2));
    let height = (rows.len() as u16 + 2).min(24).min(area.height);
    let rect = Rect::new(
        area.x + 1,
        area.bottom().saturating_sub(height),
        area.width.saturating_sub(2),
        height,
    );
    frame.render_widget(Clear, rect);
    let scroll = state
        .questions
        .drafts
        .lock()
        .expect("question drafts")
        .iter()
        .find(|d| Some(&d.request) == state.questions.active())
        .map_or(0, |d| d.scroll)
        .min(rows.len().saturating_sub(rect.height as usize));
    frame.render_widget(
        Paragraph::new(rows)
            .scroll((scroll as u16, 0))
            .style(Style::default().bg(Theme::dark().background_raised())),
        rect,
    );
}

#[cfg(test)]
mod tests;
