//! Current request handoff over the existing captured Shell/Child inventories.
use super::*;
use oc_core::queries::{ChildJob, ChildState, ShellJob};

const HINT_DELAY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForegroundWork {
    Shell(Box<ShellJob>),
    Child(Box<ChildJob>),
}

enum WorkRef<'a> {
    Shell(&'a ShellJob),
    Child(&'a ChildJob),
}
impl WorkRef<'_> {
    fn captured(&self) -> ForegroundWork {
        match self {
            Self::Shell(job) => ForegroundWork::Shell(Box::new((*job).clone())),
            Self::Child(job) => ForegroundWork::Child(Box::new((*job).clone())),
        }
    }
}

pub(super) struct Hint {
    operation: String,
    due: Instant,
    visible: bool,
}

impl TuiState {
    fn foreground_work(&self, card: &ToolCard) -> Option<WorkRef<'_>> {
        let session = self.session.as_ref()?;
        let turn = self.active_turn.as_ref()?;
        if !matches!(card.state.as_str(), "started" | "running") {
            return None;
        }
        match &card.render {
            crate::tools::ToolRender::Subagent(_) => {
                // Native logical `running` may already be a published background
                // handle. A late foreground inventory cannot restore its blocker.
                if card.state == "running" && card.output_bytes > 0 {
                    return None;
                }
                let captured = card.child_job.as_deref()?;
                if !matches!(captured.state, ChildState::Admitted | ChildState::Running) {
                    return None;
                }
                self.children
                    .foreground_job(session, captured)
                    .map(WorkRef::Child)
            }
            crate::tools::ToolRender::Shell(shell)
                if !shell.direct_user
                    && shell.process_state.is_none()
                    && shell.exit.is_none()
                    && !shell.signal
                    && !shell.cancelled
                    && !shell.timed_out =>
            {
                self.shells
                    .foreground_job(session, &turn.0, &card.op)
                    .map(WorkRef::Shell)
            }
            _ => None,
        }
    }

    pub(super) fn foreground_available(&self) -> bool {
        self.live_parts.iter().any(|part| {
            matches!(part,
            LivePart::Tool { card, .. } if self.foreground_work(card).is_some())
        })
    }

    pub(super) fn background_session(&self) -> KeyOutcome {
        let Some((session, turn)) = self.session.as_ref().zip(self.active_turn.as_ref()) else {
            return KeyOutcome::default();
        };
        let work: Vec<_> = self
            .live_parts
            .iter()
            .filter_map(|part| match part {
                LivePart::Tool { card, .. } => {
                    self.foreground_work(card).map(|work| work.captured())
                }
                _ => None,
            })
            .collect();
        KeyOutcome {
            intent: (!work.is_empty()).then(|| PanelIntent::BackgroundSession {
                session: session.clone(),
                turn: turn.clone(),
                work,
            }),
            ..Default::default()
        }
    }

    pub(super) fn background_binding(&self) -> &str {
        self.chrome
            .background_shortcut
            .as_deref()
            .unwrap_or("ctrl+b")
    }

    fn background_shortcut(&self) -> Option<&str> {
        self.background_binding()
            .split(',')
            .map(str::trim)
            .find(|key| !key.is_empty() && !key.eq_ignore_ascii_case("none"))
    }

    pub(super) fn sync_foreground_hint(&mut self, now: Instant) -> bool {
        let next = self.background_shortcut().and_then(|_| {
            self.live_parts.iter().find_map(|part| {
                let LivePart::Tool { card, .. } = part else {
                    return None;
                };
                self.foreground_work(card)?;
                Some((card.op.clone(), card.started_at?.checked_add(HINT_DELAY)?))
            })
        });
        let was_visible = self
            .foreground_hint
            .as_ref()
            .is_some_and(|hint| hint.visible);
        self.foreground_hint = next.map(|(operation, due)| Hint {
            operation,
            due,
            visible: now >= due,
        });
        let changed = was_visible
            != self
                .foreground_hint
                .as_ref()
                .is_some_and(|hint| hint.visible);
        if changed {
            self.invalidate_transcript();
        }
        changed
    }

    pub(super) fn foreground_hint_deadline(&self) -> Option<Instant> {
        self.foreground_hint
            .as_ref()
            .filter(|hint| !hint.visible && self.foreground_available())
            .map(|hint| hint.due)
    }

    pub(super) fn background_hint_key(&self) -> Option<&str> {
        let hint = self.foreground_hint.as_ref().filter(|hint| hint.visible)?;
        self.live_parts.iter().any(|part| matches!(part,
            LivePart::Tool { card, .. } if card.op == hint.operation && self.foreground_work(card).is_some()))
            .then(|| self.background_shortcut()).flatten()
    }
}
