//! Durable pre-session failures are distinct from queued/deliverable input.
use super::*;
use oc_core::queries::{CommandRecovery, CommandRecoveryAction};

pub(super) struct Request {
    location: Option<String>,
    reply: tokio::sync::oneshot::Receiver<Result<CommandRecovery, CoreError>>,
}

impl TuiState {
    pub(super) fn request_command_recovery(&mut self, action: CommandRecoveryAction) -> KeyOutcome {
        if self.command_recovery_job.is_some() {
            return KeyOutcome::default();
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let app = self.app.clone();
        oc_core::wake::spawn(async move {
            let _ = tx.send(app.command_recovery(action).await);
        });
        self.command_recovery_job = Some(Request {
            location: self.chrome.location.clone(),
            reply: rx,
        });
        KeyOutcome::default()
    }

    pub(super) fn poll_command_recovery(&mut self) {
        if self.panel == TuiPanel::Settings
            && self.chrome.location.is_some()
            && !self.command_recovery_loaded
            && self.command_recovery_job.is_none()
        {
            self.command_recovery_loaded = true;
            self.request_command_recovery(CommandRecoveryAction::List);
        }
        let Some(request) = &mut self.command_recovery_job else {
            return;
        };
        let result = match request.reply.try_recv() {
            Ok(result) => result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return,
            Err(_) => Err(CoreError::Shutdown),
        };
        let location = self
            .command_recovery_job
            .take()
            .expect("recovery request")
            .location;
        if self.chrome.location != location {
            self.command_recovery_loaded = false;
            return;
        }
        match result {
            Ok(recovery) => {
                let appeared = !recovery.items.is_empty() && self.interrupted_commands.is_empty();
                self.interrupted_commands = recovery.items;
                if appeared {
                    self.push_note("Interrupted command effects may have occurred. Settings → Interrupted commands; nothing is replayed.");
                }
                if let Some(original) = recovery.original {
                    if self.input.is_empty()
                        && !self.editor.has_images()
                        && !self.prompt_shell_mode()
                    {
                        self.editor.clear();
                        self.input = original;
                        self.editor.cursor = self.input.len();
                        self.close_panel();
                        self.push_note("Command restored, not submitted. Submitting again can repeat effects; the recovery record is retained.");
                    } else {
                        self.push_note("Empty the editor before restoring. The interrupted command remains retained.");
                    }
                }
                self.sync_modal_cursor();
                self.invalidate_transcript();
            }
            Err(error) => self.push_note(&format!(
                "Command recovery unavailable; nothing resent: {error}"
            )),
        }
    }

    pub(super) fn selected_command_recovery(&self) -> Option<String> {
        if self.panel != TuiPanel::Settings {
            return None;
        }
        self.modal_options()
            .get(self.select.cursor)?
            .value
            .strip_prefix("command-recovery:")
            .map(str::to_owned)
    }

    pub(super) fn recover_command_key(&mut self, restore: bool) -> Option<KeyOutcome> {
        let id = self.selected_command_recovery()?;
        if restore
            && (!self.input.is_empty() || self.editor.has_images() || self.prompt_shell_mode())
        {
            return Some(KeyOutcome {
                note: Some("Empty the editor before restoring an interrupted command".into()),
                ..Default::default()
            });
        }
        Some(self.request_command_recovery(if restore {
            CommandRecoveryAction::Read { id }
        } else {
            CommandRecoveryAction::Dismiss { id }
        }))
    }
}
