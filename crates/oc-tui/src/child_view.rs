//! Bounded child-family consumer; every control retains its original owner fence.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::composer::{Item, Row, Tab};
use crate::events::KeyAction;
use crate::theme::Theme;
use oc_core::domain::SessionId;
use oc_core::queries::{ChildJob, ChildState};

#[derive(Default)]
pub(crate) struct ChildView {
    rows: Vec<ChildJob>,
    selected: Option<String>,
    linked: Option<ChildJob>,
    inactive: bool,
}
impl ChildView {
    fn visible(&self) -> impl Iterator<Item = &ChildJob> {
        self.rows
            .iter()
            .filter(|job| (job.state == ChildState::Running) != self.inactive)
    }
    fn selected(&self) -> Option<&ChildJob> {
        self.visible()
            .find(|job| Some(&job.operation) == self.selected.as_ref())
    }
    pub(super) fn enter(&mut self, session: Option<SessionId>) {
        self.inactive = false;
        let selected = self
            .visible()
            .find(|job| Some(&job.child) == session.as_ref())
            .or_else(|| self.visible().next())
            .map(|job| job.operation.clone());
        self.selected = selected;
    }
    pub(super) fn at_first(&self) -> bool {
        self.visible()
            .next()
            .is_none_or(|job| Some(&job.operation) == self.selected.as_ref())
    }
    pub(super) fn selected_running(&self) -> bool {
        self.selected()
            .is_some_and(|job| job.state == ChildState::Running)
    }
    pub(super) fn select_row(&mut self, id: &str) -> bool {
        if !self.visible().any(|job| job.operation == id) {
            return false;
        }
        self.selected = Some(id.into());
        true
    }
    pub(super) fn empty_label(&self) -> &'static str {
        if self.inactive {
            "No inactive subagents"
        } else {
            "No active subagents"
        }
    }
    pub(super) fn filter_hint(&self) -> &'static str {
        if self.inactive {
            "show active"
        } else {
            "show inactive"
        }
    }
    pub(super) fn key(&mut self, key: KeyAction) -> KeyOutcome {
        let mut out = KeyOutcome::default();
        match key {
            KeyAction::CtrlA => {
                self.inactive = !self.inactive;
                let selected = self.visible().next().map(|job| job.operation.clone());
                self.selected = selected;
            }
            KeyAction::Up | KeyAction::Down => {
                let rows = self.visible().collect::<Vec<_>>();
                if !rows.is_empty() {
                    let i = rows
                        .iter()
                        .position(|job| Some(&job.operation) == self.selected.as_ref())
                        .unwrap_or(0);
                    let next = if key == KeyAction::Up {
                        i.saturating_sub(1)
                    } else {
                        (i + 1) % rows.len()
                    };
                    self.selected = Some(rows[next].operation.clone());
                }
            }
            KeyAction::Enter => {
                out.intent = self
                    .selected()
                    .cloned()
                    .map(|selected| PanelIntent::OpenChild { selected })
            }
            KeyAction::ShellBackground => {
                out.intent = self
                    .selected()
                    .or(self.linked.as_ref())
                    .filter(|job| job.state == ChildState::Running)
                    .cloned()
                    .map(|selected| PanelIntent::BackgroundChild { selected })
            }
            KeyAction::DeleteOrQuit => {
                out.intent = self
                    .selected()
                    .filter(|job| job.state == ChildState::Running)
                    .cloned()
                    .map(|selected| PanelIntent::InterruptChild { selected })
            }
            _ => {}
        }
        out
    }
    pub(super) fn composer_rows(
        &self,
        session: Option<&SessionId>,
        theme: &Theme,
        width: u16,
    ) -> Vec<Row> {
        let selected = self
            .visible()
            .position(|job| Some(&job.operation) == self.selected.as_ref())
            .unwrap_or(0);
        let start = selected
            .saturating_sub(2)
            .min(self.visible().count().saturating_sub(5));
        self.visible()
            .skip(start)
            .take(5)
            .map(|job| {
                let agent = job
                    .agent
                    .chars()
                    .enumerate()
                    .flat_map(|(i, c)| {
                        if i == 0 {
                            c.to_uppercase().collect::<Vec<_>>()
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>();
                Row::new(
                    Item::Child(job.operation.clone()),
                    &format!("{agent}: {}", job.description),
                    Some(&job.operation) == self.selected.as_ref(),
                    Some(&job.child) == session,
                    if job.state == ChildState::Running {
                        "Running"
                    } else {
                        ""
                    },
                    theme,
                    width,
                )
            })
            .collect()
    }
}
impl TuiState {
    pub fn children_open(&self) -> bool {
        self.composer.active == Some(Tab::Subagents)
    }
    pub fn hide_children(&mut self) {
        if self.children_open() {
            self.close_composer();
        }
    }
    pub fn linked_child(&self) -> Option<&ChildJob> {
        self.children.linked.as_ref()
    }
    pub fn apply_child_jobs(&mut self, rows: Vec<ChildJob>) {
        if let Some(linked) = &mut self.children.linked
            && let Some(current) = rows.iter().find(|job| job.operation == linked.operation)
        {
            *linked = current.clone();
        }
        self.children.rows = rows;
        if self.children.selected().is_none() {
            let selected = self
                .children
                .visible()
                .find(|job| self.attached_session() == Some(&job.child))
                .or_else(|| self.children.visible().next())
                .map(|job| job.operation.clone());
            self.children.selected = selected;
        }
    }
    pub fn attach_linked_child(&mut self, selected: ChildJob) {
        self.chrome.location = Some(selected.location.clone());
        self.chrome.selection_generation = selected.generation;
        self.children.linked = Some(selected);
        self.composer.active = Some(Tab::Subagents);
        self.children.enter(self.attached_session().cloned());
    }
    /// A live owner refresh changes facts, not the user's lower tab, filter or
    /// captured output dialog. Explicit navigation uses attach_linked_child.
    pub fn refresh_linked_child(&mut self, current: ChildJob) {
        if self.children.linked.as_ref().is_some_and(|old| {
            old.operation == current.operation
                && old.parent == current.parent
                && old.child == current.child
                && old.generation == current.generation
                && old.location == current.location
        }) {
            self.children.linked = Some(current);
        }
    }
}
