//! Direct user commands have one receipt, not a synthetic model turn.
use super::*;
use oc_core::core_app::{FreshSelection, UserShellReceipt, UserShellSelection};

// Home supplies these examples; a session prompt has no Shell example list.
// Pinned routes/home.tsx:20–23 and component/prompt/index.tsx:965–973.
const SHELL_EXAMPLES: [&str; 3] = ["ls -la", "git status", "pwd"];

#[derive(Default)]
pub(super) struct UserShellState {
    mode: bool,
    example: usize,
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
    cancelling: bool,
}

impl TuiState {
    pub(crate) fn prompt_shell_mode(&self) -> bool {
        self.user_shell.mode
    }

    pub(crate) fn prompt_shell_example(&self) -> Option<&'static str> {
        self.home.then_some(SHELL_EXAMPLES[self.user_shell.example])
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
        if mode && !self.user_shell.mode && self.home {
            self.user_shell.example = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |now| now.subsec_nanos() as usize % SHELL_EXAMPLES.len());
            // OC2 keeps one placeholder index across mode changes, so normal
            // Home uses this selection again after Shell mode is dismissed.
            self.home_example = HOME_EXAMPLES[self.user_shell.example % HOME_EXAMPLES.len()];
        }
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
                self.user_shell.pending = Some(PendingUserShell {
                    session,
                    fresh,
                    selection,
                    command: self.input.clone(),
                    revision: self.input_revision,
                    generation: self.generation,
                    receipt,
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
            Ok(operation) => {
                if pending.fresh {
                    self.session = Some(pending.session.clone());
                    self.new_session_tab = true;
                }
                self.model_submission_accepted(&pending.session, pending.selection.as_ref());
                self.home = false;
                // Receipt identifies the real admitted operation. This echo
                // never fabricates a model call/result or durable message ID.
                self.window.push_user_shell(operation, &pending.command);
                self.refresh_running_user_shell_output();
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
