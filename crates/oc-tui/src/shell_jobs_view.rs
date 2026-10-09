//! Disposable lower composer over the existing application's shell facts.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::composer::{Item, Row, Tab};
use crate::events::KeyAction;
use crate::theme::Theme;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_core::domain::SessionId;
use oc_core::queries::{ShellJob, ToolOutputPage};
use ratatui::layout::Rect;
mod output;
pub(crate) use output::render;

#[derive(Default)]
pub(crate) struct ShellView {
    rows: Vec<ShellJob>,
    inventory_loaded: bool,
    selected: Option<String>,
    // A viewer holds the original source identity, never a running-list index.
    viewer: Option<ShellJob>,
    output: Option<ToolOutputPage>,
    output_state: String,
    output_exit: Option<i32>,
    output_omitted: bool,
    output_error: bool,
    output_scroll: std::cell::Cell<usize>,
    output_follow: std::cell::Cell<bool>,
    output_paint: std::cell::RefCell<Option<output::Paint>>,
    output_down: Option<bool>,
    pub(crate) footer_hit: std::cell::Cell<Option<(Rect, [Rect; 2])>>,
    footer_down: bool,
}

impl ShellView {
    pub(crate) fn refresh_transcript(
        &self,
        session: &oc_core::domain::SessionId,
        window: &mut crate::history::HistoryWindow,
    ) -> bool {
        window.refresh_running_user_shell_output(session, &self.rows)
    }

    fn selected(&self) -> Option<&ShellJob> {
        self.viewer.as_ref().or_else(|| {
            self.rows
                .iter()
                .find(|row| Some(&row.shell_id) == self.selected.as_ref())
        })
    }
    pub(super) fn at_first(&self, session: Option<&SessionId>) -> bool {
        self.rows
            .iter()
            .find(|row| Some(&row.session) == session)
            .is_none_or(|row| Some(&row.shell_id) == self.selected.as_ref())
    }
    pub(super) fn has_selected(&self, session: Option<&SessionId>) -> bool {
        self.selected()
            .is_some_and(|row| Some(&row.session) == session)
    }
    pub(super) fn select_row(&mut self, id: &str, session: Option<SessionId>) -> bool {
        if !self
            .rows
            .iter()
            .any(|row| row.shell_id == id && Some(&row.session) == session.as_ref())
        {
            return false;
        }
        self.selected = Some(id.into());
        true
    }
    pub(super) fn composer_rows(
        &self,
        session: Option<&SessionId>,
        theme: &Theme,
        width: u16,
    ) -> Vec<Row> {
        let rows = || self.rows.iter().filter(|row| Some(&row.session) == session);
        let selected = rows()
            .position(|row| Some(&row.shell_id) == self.selected.as_ref())
            .unwrap_or(0);
        let start = selected
            .saturating_sub(2)
            .min(rows().count().saturating_sub(5));
        rows()
            .skip(start)
            .take(5)
            .map(|row| {
                Row::new(
                    Item::Shell(row.shell_id.clone()),
                    row.command.lines().next().unwrap_or_default(),
                    Some(&row.shell_id) == self.selected.as_ref(),
                    false,
                    "",
                    theme,
                    width,
                )
            })
            .collect()
    }

    pub(super) fn key(&mut self, key: KeyAction) -> KeyOutcome {
        if self.viewer.is_some()
            && let Some(result) = self.output_key(&key)
        {
            return result;
        }
        let mut result = KeyOutcome::default();
        match key {
            KeyAction::Cancel | KeyAction::Interrupt => {
                if self.viewer.take().is_some() {
                    self.output = None;
                    self.output_paint.get_mut().take();
                    self.output_down = None;
                }
            }
            KeyAction::Up | KeyAction::Down if self.viewer.is_none() && !self.rows.is_empty() => {
                let i = self
                    .rows
                    .iter()
                    .position(|r| Some(&r.shell_id) == self.selected.as_ref())
                    .unwrap_or(0);
                let next = if key == KeyAction::Up {
                    i.saturating_sub(1)
                } else {
                    (i + 1) % self.rows.len()
                };
                self.selected = Some(self.rows[next].shell_id.clone());
            }
            KeyAction::Enter if self.viewer.is_none() => {
                self.viewer = self.selected().cloned();
                self.output = None;
                self.output_state = "running".into();
                self.output_exit = None;
                self.output_omitted = false;
                self.output_error = false;
                self.output_scroll.set(0);
                self.output_follow.set(true);
                self.output_down = None;
                self.output_paint.get_mut().take();
                result.intent = self.viewer.as_ref().map(|_| PanelIntent::LoadShells);
            }
            KeyAction::DeleteOrQuit | KeyAction::ShellBackground => {
                if let Some(job) = self.selected() {
                    result.intent = Some(if key == KeyAction::DeleteOrQuit {
                        PanelIntent::CancelShell {
                            session: job.session.clone(),
                            shell_id: job.shell_id.clone(),
                        }
                    } else {
                        PanelIntent::BackgroundShell {
                            session: job.session.clone(),
                            shell_id: job.shell_id.clone(),
                        }
                    });
                }
            }
            _ => {}
        }
        result
    }
}

impl TuiState {
    /// Consumer visibility; approval/question/modal focus remains authoritative.
    pub fn shells_open(&self) -> bool {
        self.composer.active == Some(Tab::Shell) || self.shells.viewer.is_some()
    }

    /// Reconcile bounded current running rows without retargeting an open viewer.
    pub fn apply_shell_jobs(&mut self, rows: Vec<ShellJob>) {
        self.shells.inventory_loaded = true;
        self.shells.rows = rows
            .into_iter()
            .filter(|row| self.attached_session() == Some(&row.session))
            .collect();
        self.refresh_running_user_shell_output();
        if !self.shells.rows.iter().any(|r| {
            Some(&r.shell_id) == self.shells.selected.as_ref()
                && self.attached_session() == Some(&r.session)
        }) {
            self.shells.selected = self
                .shells
                .rows
                .iter()
                .find(|row| self.attached_session() == Some(&row.session))
                .map(|r| r.shell_id.clone());
        }
    }

    /// Footer facts are source-session scoped, unlike the family inventory panel.
    pub(crate) fn running_shell_count(&self) -> usize {
        self.shells
            .rows
            .iter()
            .filter(|job| self.attached_session() == Some(&job.session))
            .count()
    }

    pub(crate) fn live_shell_binding(&self) -> &str {
        self.chrome
            .child_first_shortcut
            .as_deref()
            .unwrap_or("down")
    }

    pub(crate) fn cancel_shell_footer_pointer(&mut self) {
        self.shells.footer_down = false;
    }

    pub(crate) fn shell_footer_mouse(
        &mut self,
        event: MouseEvent,
        area: Rect,
    ) -> Option<KeyOutcome> {
        let hit = self.running_shell_count() > 0
            && self
                .shells
                .footer_hit
                .get()
                .is_some_and(|(painted, rects)| {
                    painted == area
                        && rects
                            .iter()
                            .any(|rect| rect.contains((event.column, event.row).into()))
                });
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.shells.footer_down = hit;
                hit.then(KeyOutcome::default)
            }
            MouseEventKind::Up(MouseButton::Left) if event.modifiers.is_empty() => {
                let accepted = std::mem::take(&mut self.shells.footer_down) && hit;
                accepted.then(|| self.open_composer(Tab::Subagents))
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                self.shells.footer_down = false;
                None
            }
            _ => None,
        }
    }

    /// Original viewer address, retained after the job leaves the running list.
    pub fn shell_viewer(&self) -> Option<&ShellJob> {
        self.shells.viewer.as_ref()
    }

    /// Replace only the exact selected capture's disposable bounded page.
    pub fn apply_shell_output(&mut self, job: &ShellJob, page: ToolOutputPage) {
        if self.shells.viewer.as_ref().is_some_and(|v| {
            v.shell_id == job.shell_id && v.session == job.session && v.generation == job.generation
        }) && page.text.len() <= 65536
        {
            self.shells.output = Some(page);
        }
    }

    /// Same-ID Location adoption carries only a capture's original identity,
    /// never the old Location's model, authority, catalog or ordinary editor.
    pub fn inherit_shell_view(&mut self, previous: &mut Self) {
        if self.attached_session().is_some()
            && self.attached_session() == previous.attached_session()
        {
            let current = self
                .shells
                .inventory_loaded
                .then(|| std::mem::take(&mut self.shells.rows));
            self.shells = std::mem::take(&mut previous.shells);
            self.inherit_composer_tab(previous, Tab::Shell);
            self.shells.footer_hit.set(None);
            self.shells.footer_down = false;
            if let Some(rows) = current {
                self.apply_shell_jobs(rows);
            }
        }
    }
}

#[cfg(test)]
mod tests;
