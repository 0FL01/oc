//! Direct user commands have one receipt, not a synthetic model turn.
use super::*;
use oc_core::core_app::{FreshSelection, UserShellReceipt, UserShellSelection};

#[derive(Default)]
pub(super) struct UserShellState {
    mode: bool,
    pending: Option<PendingUserShell>,
}

struct PendingUserShell {
    session: SessionId,
    fresh: bool,
    selection: Option<oc_core::queries::ModelCommit>,
    command: String,
    revision: u64,
    generation: u64,
    receipt: UserShellReceipt,
    agent: Option<String>,
    color: Option<usize>,
    cancelling: bool,
}

impl TuiState {
    pub(crate) fn prompt_shell_mode(&self) -> bool {
        self.user_shell.mode
    }

    /// An Ask precedes durable fresh-root creation. The pending receipt still
    /// owns that exact session; the approval router must not query its history
    /// or make it an attached/admitted conversation before permission resolves.
    pub fn user_shell_admission_session(&self) -> Option<&SessionId> {
        let pending = self.user_shell.pending.as_ref()?;
        (pending.generation == self.generation
            && pending.fresh == self.session.is_none()
            && (pending.fresh || self.session.as_ref() == Some(&pending.session)))
        .then_some(&pending.session)
    }

    pub(super) fn change_prompt_shell_mode(&mut self, mode: bool) {
        self.user_shell.mode = mode;
        self.painted_prompt.replace(None);
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.clear_mentions();
    }

    pub(super) fn cancel_pending_user_shell(&mut self) -> bool {
        let Some(pending) = self.user_shell.pending.as_mut() else {
            return false;
        };
        pending.cancelling = true;
        pending.receipt.cancel();
        true
    }

    pub(super) fn user_shell_pending(&self) -> bool {
        self.user_shell.pending.is_some()
    }

    pub(super) fn submit_user_shell(&mut self) -> KeyOutcome {
        if self.input.trim().is_empty() {
            return KeyOutcome::default();
        }
        if self.pending.is_some() || self.user_shell_pending() || self.active_turn.is_some() {
            return KeyOutcome {
                note: Some("submission pending; Esc to cancel".into()),
                ..Default::default()
            };
        }
        let selection = self.captured_model_commit();
        let fresh = self.session.is_none();
        let session = self.session.clone().unwrap_or_else(fresh_session_id);
        let choice = if fresh {
            UserShellSelection::Fresh(selection.as_ref().map(|commit| FreshSelection {
                binding: Some(commit.binding.clone()),
                agent_id: commit.binding.agent_id.clone(),
                model_id: commit.model_id.clone(),
                variant: commit.variant.clone(),
            }))
        } else {
            UserShellSelection::Existing(selection.clone())
        };
        match self
            .app
            .request_user_shell(session.clone(), self.input.clone(), choice)
        {
            Ok(receipt) => {
                let agent = self.active_agent.clone();
                let color = agent.as_deref().and_then(|id| {
                    self.agents
                        .iter()
                        .find(|a| a.id == id)
                        .map(|a| a.color_index)
                });
                self.user_shell.pending = Some(PendingUserShell {
                    session,
                    fresh,
                    selection,
                    command: self.input.clone(),
                    revision: self.input_revision,
                    generation: self.generation,
                    receipt,
                    agent,
                    color,
                    cancelling: false,
                });
                self.status = TuiStatus::PendingSubmission;
                self.push_note("shell admission pending; Esc to cancel");
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(self.submission_note(&error)),
                ..Default::default()
            },
        }
    }

    pub(super) fn poll_user_shell(&mut self) {
        let Some(result) = self
            .user_shell
            .pending
            .as_mut()
            .and_then(|p| p.receipt.try_result())
        else {
            return;
        };
        let pending = self
            .user_shell
            .pending
            .take()
            .expect("polled user shell receipt");
        self.reconcile_user_shell(pending, result, false);
    }

    pub(super) async fn reconcile_user_shell_quit(&mut self) -> Result<(), CoreError> {
        let Some(mut pending) = self.user_shell.pending.take() else {
            return Ok(());
        };
        pending.receipt.cancel();
        pending.cancelling = true;
        let result = pending.receipt.wait().await;
        if result == Err(CoreError::Shutdown) {
            return Err(CoreError::Shutdown);
        }
        self.reconcile_user_shell(pending, result, true);
        Ok(())
    }

    fn reconcile_user_shell(
        &mut self,
        pending: PendingUserShell,
        result: Result<String, CoreError>,
        exiting: bool,
    ) {
        if pending.generation != self.generation
            || pending.fresh != self.session.is_none()
            || (!pending.fresh && self.session.as_ref() != Some(&pending.session))
        {
            return;
        }
        if !exiting {
            self.status = TuiStatus::Idle;
        }
        match result {
            Ok(_) => {
                if pending.fresh {
                    self.session = Some(pending.session.clone());
                }
                self.model_submission_accepted(&pending.session, pending.selection.as_ref());
                self.home = false;
                // Mirrors the durable USER command, never ToolCallResult or
                // an accepted LLM turn. Jobs owns all process/outcome events.
                self.window
                    .push_synthetic("user", &pending.command, pending.agent, pending.color);
                self.invalidate_transcript();
                self.conversation_available = None;
                if self.input_revision == pending.revision && !pending.cancelling {
                    self.input.clear();
                    self.editor.clear_submitted_draft();
                }
                self.change_prompt_shell_mode(false);
                self.note = None;
            }
            Err(error) => {
                let note = self.submission_note(&error);
                self.push_note(&note);
            }
        }
    }
}
