//! Disposable bounded child consumer; all controls carry the original owner fence.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::events::KeyAction;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_core::queries::{ChildJob, ChildState};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::cell::Cell;

#[derive(Default)]
pub(crate) struct ChildView {
    pub(super) open: bool,
    rows: Vec<ChildJob>,
    selected: Option<String>,
    linked: Option<ChildJob>,
    filter: String,
    painted: Cell<Option<Rect>>,
    pointer_down: Option<String>,
}

impl ChildView {
    fn visible(&self) -> impl Iterator<Item = &ChildJob> {
        self.rows.iter().filter(|j| {
            format!("{} {} {}", j.description, j.agent, j.child.0)
                .to_lowercase()
                .contains(&self.filter.to_lowercase())
        })
    }
    fn selected(&self) -> Option<&ChildJob> {
        if self.open {
            self.visible()
                .find(|j| Some(&j.operation) == self.selected.as_ref())
        } else {
            self.linked.as_ref()
        }
    }

    pub(super) fn key(&mut self, key: KeyAction) -> KeyOutcome {
        let mut out = KeyOutcome::default();
        match key {
            KeyAction::Children => {
                self.open = !self.open;
                out.intent = self.open.then_some(PanelIntent::LoadChildren);
            }
            KeyAction::Cancel => {
                if self.open {
                    self.open = false;
                } else if self.linked.is_some() {
                    out.intent = Some(PanelIntent::ReturnParent);
                }
            }
            KeyAction::Up | KeyAction::Down if self.open => {
                let rows = self.visible().collect::<Vec<_>>();
                if !rows.is_empty() {
                    let i = rows
                        .iter()
                        .position(|j| Some(&j.operation) == self.selected.as_ref())
                        .unwrap_or(0);
                    let next = if key == KeyAction::Up {
                        (i + rows.len() - 1) % rows.len()
                    } else {
                        (i + 1) % rows.len()
                    };
                    self.selected = Some(rows[next].operation.clone());
                }
            }
            KeyAction::Char(c)
                if self.open && !c.is_control() && self.filter.len() + c.len_utf8() <= 128 =>
            {
                self.filter.push(c);
                let selected = self.visible().next().map(|j| j.operation.clone());
                self.selected = selected;
            }
            KeyAction::Backspace if self.open => {
                self.filter.pop();
                let selected = self.visible().next().map(|j| j.operation.clone());
                self.selected = selected;
            }
            KeyAction::Enter if self.open => {
                out.intent = self
                    .selected()
                    .cloned()
                    .map(|selected| PanelIntent::OpenChild { selected });
            }
            KeyAction::ShellBackground => {
                out.intent = self
                    .selected()
                    .cloned()
                    .map(|selected| PanelIntent::BackgroundChild { selected });
            }
            KeyAction::Interrupt | KeyAction::DeleteOrQuit => {
                out.intent = self
                    .selected()
                    .cloned()
                    .map(|selected| PanelIntent::InterruptChild { selected });
            }
            _ => {}
        }
        out
    }

    pub(super) fn mouse(&mut self, event: MouseEvent) -> KeyOutcome {
        let mut out = KeyOutcome::default();
        let job = self
            .painted
            .get()
            .filter(|r| self.open && r.contains((event.column, event.row).into()))
            .and_then(|r| event.row.checked_sub(r.y + 2))
            .and_then(|row| self.visible().nth(row as usize))
            .cloned();
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.pointer_down = job.as_ref().map(|j| j.operation.clone());
                if let Some(job) = job {
                    self.selected = Some(job.operation);
                }
            }
            MouseEventKind::Up(MouseButton::Left) if event.modifiers.is_empty() => {
                if let Some(job) = job
                    && self.pointer_down.take().as_deref() == Some(&job.operation)
                {
                    out.intent = Some(PanelIntent::OpenChild { selected: job });
                }
            }
            MouseEventKind::Drag(_) | MouseEventKind::Down(_) | MouseEventKind::Up(_) => {
                self.pointer_down = None
            }
            _ => {}
        }
        out
    }
}

impl TuiState {
    pub fn children_open(&self) -> bool {
        self.children.open
    }
    pub fn hide_children(&mut self) {
        self.children.open = false;
    }
    pub fn linked_child(&self) -> Option<&ChildJob> {
        self.children.linked.as_ref()
    }
    pub fn apply_child_jobs(&mut self, rows: Vec<ChildJob>) {
        if let Some(linked) = &mut self.children.linked
            && let Some(current) = rows.iter().find(|j| j.operation == linked.operation)
        {
            *linked = current.clone();
        }
        self.children.rows = rows;
        if !self
            .children
            .rows
            .iter()
            .any(|j| Some(&j.operation) == self.children.selected.as_ref())
        {
            self.children.selected = self.children.rows.first().map(|j| j.operation.clone());
        }
    }
    pub fn attach_linked_child(&mut self, selected: ChildJob) {
        self.chrome.location = Some(selected.location.clone());
        self.chrome.selection_generation = selected.generation;
        self.children.linked = Some(selected);
    }
}

pub(crate) fn height(state: &TuiState) -> u16 {
    if state.children.open
        && state.approvals.active().is_none()
        && state.questions.active().is_none()
    {
        (state.children.visible().count() as u16 + 3).max(4)
    } else {
        0
    }
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &TuiState, main: Rect) {
    state.children.painted.set(None);
    if let Some(job) = state.linked_child() {
        let area = Rect::new(main.x, main.y, main.width, 1);
        frame.render_widget(
            Paragraph::new(format!(
                "Linked child · {} · {:?} · {} · esc parent",
                safe(&job.description),
                job.state,
                if job.background { "BG" } else { "FG" }
            )),
            area,
        );
    }
    let h = height(state).min(main.height);
    if h == 0 {
        return;
    }
    let area = Rect::new(
        main.x + 1,
        main.bottom().saturating_sub(h),
        main.width.saturating_sub(2),
        h,
    );
    state.children.painted.set(Some(area));
    let mut lines = vec![Line::from(format!("Filter: {}", state.children.filter))];
    if state.children.visible().next().is_none() {
        lines.push(Line::from("No child jobs"));
    }
    for job in state.children.visible() {
        let status = match job.state {
            ChildState::Admitted => "ADMITTED",
            ChildState::Running => "RUNNING",
            ChildState::Completed => "COMPLETED",
            ChildState::Cancelled => "CANCELLED",
            ChildState::Error => "ERROR",
            ChildState::Unknown => "UNKNOWN",
        };
        lines.push(Line::from(format!(
            "{} {} {} {} · {}",
            if Some(&job.operation) == state.children.selected.as_ref() {
                "›"
            } else {
                " "
            },
            status,
            if job.background { "BG" } else { "FG" },
            safe(&job.description),
            safe(&job.agent)
        )));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Subagents · enter open · ctrl+b background · ctrl+c interrupt · esc hide"),
        ),
        area,
    );
}
