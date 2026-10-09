//! Disposable lower composer over the existing application's shell facts.
use crate::app::{KeyOutcome, PanelIntent, TuiState};
use crate::events::KeyAction;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_core::queries::{ShellJob, ToolOutputPage};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[derive(Default)]
pub(crate) struct ShellView {
    pub(super) open: bool,
    rows: Vec<ShellJob>,
    inventory_loaded: bool,
    selected: Option<String>,
    // A viewer holds the original source identity, never a running-list index.
    viewer: Option<ShellJob>,
    output: Option<ToolOutputPage>,
    pub(crate) footer_hit: std::cell::Cell<Option<(Rect, [Rect; 2])>>,
    footer_down: bool,
}

impl ShellView {
    fn selected(&self) -> Option<&ShellJob> {
        self.viewer.as_ref().or_else(|| {
            self.rows
                .iter()
                .find(|row| Some(&row.shell_id) == self.selected.as_ref())
        })
    }

    pub(super) fn key(&mut self, key: KeyAction) -> KeyOutcome {
        let mut result = KeyOutcome::default();
        match key {
            KeyAction::Shells => {
                self.open = !self.open;
                result.intent = self.open.then_some(PanelIntent::LoadShells);
            }
            KeyAction::Cancel | KeyAction::Interrupt => {
                if self.viewer.take().is_some() {
                    self.output = None;
                } else {
                    self.open = false;
                }
            }
            KeyAction::Up | KeyAction::Down if self.viewer.is_none() && !self.rows.is_empty() => {
                let i = self
                    .rows
                    .iter()
                    .position(|r| Some(&r.shell_id) == self.selected.as_ref())
                    .unwrap_or(0);
                let next = if key == KeyAction::Up {
                    (i + self.rows.len() - 1) % self.rows.len()
                } else {
                    (i + 1) % self.rows.len()
                };
                self.selected = Some(self.rows[next].shell_id.clone());
            }
            KeyAction::Enter if self.viewer.is_none() => {
                self.viewer = self.selected().cloned();
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
        self.shells.open
    }

    /// Reconcile bounded current running rows without retargeting an open viewer.
    pub fn apply_shell_jobs(&mut self, rows: Vec<ShellJob>) {
        self.shells.inventory_loaded = true;
        self.shells.rows = rows;
        if !self
            .shells
            .rows
            .iter()
            .any(|r| Some(&r.shell_id) == self.shells.selected.as_ref())
        {
            self.shells.selected = self.shells.rows.first().map(|r| r.shell_id.clone());
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
                accepted.then(|| {
                    self.children.open = false;
                    self.close_terminal_composer();
                    self.shells.key(KeyAction::Shells)
                })
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
        }) {
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
            self.shells.footer_hit.set(None);
            self.shells.footer_down = false;
            if let Some(rows) = current {
                self.apply_shell_jobs(rows);
            }
        }
    }
}

pub(crate) fn height(state: &TuiState) -> u16 {
    if state.shells.open && state.approvals.active().is_none() && state.questions.active().is_none()
    {
        if state.shells.viewer.is_some() {
            12
        } else {
            (state.shells.rows.len() as u16 + 4).max(5)
        }
    } else {
        0
    }
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &TuiState, main: Rect) {
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
    let mut lines = Vec::new();
    if let Some(job) = state.shells.viewer.as_ref() {
        lines.push(Line::from(format!(
            "{} · {} · generation {}",
            job.shell_id, job.session.0, job.generation
        )));
        lines.push(Line::from(safe(&format!(
            "{} · {}/{}",
            job.location, job.provider, job.model
        ))));
        lines.extend(
            state
                .shells
                .output
                .as_ref()
                .map(|p| {
                    p.text
                        .lines()
                        .map(|s| Line::from(safe(s)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![Line::from("Loading live output…")]),
        );
    } else {
        if state.shells.rows.is_empty() {
            lines.push(Line::from("No running shells"));
        }
        for job in &state.shells.rows {
            lines.push(Line::from(format!(
                "{} RUNNING {} PID {} {}",
                if Some(&job.shell_id) == state.shells.selected.as_ref() {
                    "›"
                } else {
                    " "
                },
                if job.background { "BG" } else { "FG" },
                job.pid.map_or_else(|| "?".into(), |p| p.to_string()),
                safe(&job.command)
            )));
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Shell · enter output · ctrl+b background · ctrl+d kill · esc back"),
        ),
        area,
    );
}

#[cfg(test)]
mod tests;
