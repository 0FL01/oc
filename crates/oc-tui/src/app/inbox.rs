//! OC2 steer/queue: input submitted while the agent works. Enter steers it
//! into the running turn, `<leader>Enter` queues it for after the turn. The
//! owner holds the items; this view only mirrors its pending inbox.
use super::*;
use oc_core::queries::{InboxAction, InboxDelivery, InboxItem};

impl TuiState {
    pub(super) fn failed_submission_selected(&self) -> bool {
        self.panel == TuiPanel::Settings
            && self.failed_submission_note().is_some()
            && self
                .modal_options()
                .get(self.select.cursor)
                .is_some_and(|option| option.value == "submission-refusal")
    }

    pub(super) fn recover_submission(&mut self, restore: bool) -> KeyOutcome {
        if self.failed_submission_note().is_none() {
            return KeyOutcome::default();
        }
        if restore
            && (!self.input.is_empty() || self.editor.has_images() || self.prompt_shell_mode())
        {
            return KeyOutcome {
                note: Some(
                    "Empty the editor and leave shell mode before restoring the retained request"
                        .into(),
                ),
                ..Default::default()
            };
        }
        let pending = self.pending.take().expect("failed admission");
        if restore && !matches!(pending.purpose, SubmissionPurpose::Empty) {
            self.restore_composer_draft(pending.draft);
            self.clear_mentions();
            self.slash_selected = 0;
            self.slash_dismissed = None;
        }
        self.refused_submission = None;
        self.invalidate_transcript();
        self.close_panel();
        KeyOutcome {
            note: Some(
                if restore {
                    "Draft restored; nothing resent"
                } else {
                    "Retained request discarded; nothing resent"
                }
                .into(),
            ),
            ..Default::default()
        }
    }

    /// Owner snapshot or event of this session's pending input.
    pub fn apply_inbox(&mut self, session: &SessionId, items: Vec<InboxItem>) {
        if self.session.as_ref() != Some(session) || self.inbox == items {
            return;
        }
        self.inbox = items;
        let queued = self.queued().count();
        self.queued_cursor = self.queued_cursor.min(queued.saturating_sub(1));
        if self.panel == TuiPanel::QueuedPrompts && queued == 0 {
            self.panel = TuiPanel::None;
        }
        self.invalidate_transcript();
        self.sync_modal_cursor();
    }

    pub fn inbox_items(&self) -> &[InboxItem] {
        &self.inbox
    }

    pub(super) fn queued(&self) -> impl Iterator<Item = &InboxItem> {
        self.inbox
            .iter()
            .filter(|item| item.delivery == InboxDelivery::Queue)
    }

    /// OC2 `submit(delivery)` shares the same captured admission as idle input.
    pub(super) fn enqueue_input(&mut self, delivery: InboxDelivery) -> KeyOutcome {
        self.request_composer_submission(SubmissionPurpose::Inbox(delivery))
    }

    /// `<leader>Enter` (OC2 `prompt.queue`).
    pub(super) async fn queue_prompt(&mut self) -> KeyOutcome {
        let text = self.input.trim();
        if self.prompt_shell_mode()
            || text.starts_with('/') && !self.is_workspace_command(text)
            || matches!(text, "exit" | "quit" | ":q")
        {
            return KeyOutcome {
                note: Some("This prompt cannot be queued".into()),
                ..KeyOutcome::default()
            };
        }
        if self.has_occupied_submission() {
            return KeyOutcome {
                note: Some(self.submission_occupied_note().into()),
                ..Default::default()
            };
        }
        if text.is_empty() {
            return KeyOutcome::default();
        }
        if self.active_turn.is_some() {
            return self.enqueue_input(InboxDelivery::Queue);
        }
        // Idle: a queued prompt starts at once, exactly like Enter.
        self.handle_enter().await
    }

    /// Blank Enter promotes the first queued prompt, while running or idle.
    pub(super) fn promote_first_queued(&self) -> Option<PanelIntent> {
        let first = self.queued().next()?;
        Some(PanelIntent::Inbox {
            action: InboxAction::Deliver {
                id: first.id.clone(),
                delivery: InboxDelivery::Steer,
            },
            restore: false,
        })
    }

    pub(super) fn open_queued_prompts(&mut self) -> KeyOutcome {
        self.panel = TuiPanel::QueuedPrompts;
        self.select.reset();
        self.queued_cursor = 0;
        self.sync_modal_cursor();
        KeyOutcome::default()
    }

    /// Queued-prompts dialog: Enter steers, ctrl+d deletes, ctrl+u undoes.
    pub(super) fn queued_prompt_key(&mut self, action: &KeyAction) -> Option<KeyOutcome> {
        if self.panel != TuiPanel::QueuedPrompts {
            return None;
        }
        let id = self.queued().nth(self.queued_cursor)?.id.clone();
        let (action, restore) = match action {
            KeyAction::Enter => (
                InboxAction::Deliver {
                    id,
                    delivery: InboxDelivery::Steer,
                },
                false,
            ),
            KeyAction::DeleteOrQuit => (InboxAction::Remove { id }, false),
            KeyAction::QueuedUndo => {
                if self.prompt_shell_mode() && !self.input.trim().is_empty() {
                    return Some(KeyOutcome {
                        note: Some("Leave shell mode before undoing a queued prompt".into()),
                        ..KeyOutcome::default()
                    });
                }
                (InboxAction::Remove { id }, true)
            }
            _ => return None,
        };
        Some(KeyOutcome {
            intent: Some(PanelIntent::Inbox { action, restore }),
            ..KeyOutcome::default()
        })
    }

    /// Preflight before withdrawing an item. Renumber only appended image chips;
    /// an existing draft and its tracked attachments remain byte-for-byte intact.
    pub fn prepare_inbox_restore(
        &self,
        prompt: oc_core::prompt::PromptInput,
    ) -> Result<oc_core::prompt::PromptInput, String> {
        let mut draft = self.editor.prompt(&self.input);
        if draft.text.trim().is_empty() && draft.images.is_empty() {
            return Ok(prompt);
        }
        let mut ordinal = draft
            .images
            .iter()
            .map(|i| i.mark.ordinal)
            .max()
            .unwrap_or(0);
        draft.text.push('\n');
        let mut end = 0;
        for mut image in prompt.images {
            draft.text.push_str(
                prompt
                    .text
                    .get(end..image.mark.start)
                    .ok_or("invalid image range")?,
            );
            end = image.mark.end;
            ordinal = ordinal.checked_add(1).ok_or("image ordinal limit")?;
            image.mark.ordinal = ordinal;
            image.mark.start = draft.text.len();
            draft.text.push_str(&image.mark.label());
            image.mark.end = draft.text.len();
            draft.images.push(image);
        }
        draft
            .text
            .push_str(prompt.text.get(end..).ok_or("invalid image range")?);
        draft.check_budget().map_err(|error| error.to_string())?;
        Ok(draft)
    }

    /// Owner ACK of a dialog action; restoration was preflighted before removal.
    pub fn applied_inbox(
        &mut self,
        session: &SessionId,
        snapshot: oc_core::queries::InboxSnapshot,
        restore: bool,
    ) {
        if restore
            && snapshot.removed.is_some()
            && let Some(prompt) = snapshot.prompt
        {
            self.restore_prompt_input(prompt);
            self.panel = TuiPanel::None;
        }
        self.apply_inbox(session, snapshot.items);
    }

    /// A steer delivered into the running turn becomes its durable user row.
    pub fn insert_steered_rows(
        &mut self,
        page: &oc_core::queries::HistoryPage,
        ids: &[oc_core::session::MessageId],
    ) {
        if self.window.insert_steered(page, ids) {
            self.invalidate_transcript();
        }
    }

    /// Pending steers use neutral user rows; only queued prompts join the dock.
    pub(super) fn inbox_rows(&self) -> Vec<HistoryRow> {
        let row = |role: &str, text: String, delivery| HistoryRow {
            position: None,
            message_id: None,
            seq: i64::MAX,
            role: role.into(),
            pending_delivery: Some(delivery),
            text: text.into(),
            agent: None,
            agent_color_index: None,
            chips: Vec::new(),
            reasoning: None,
            meta: None,
            tool: None,
            child_notice: None,
            shell_notice: None,
        };
        let mut rows = self
            .inbox
            .iter()
            .filter(|item| item.delivery == InboxDelivery::Steer)
            .map(|item| {
                row(
                    "user",
                    match &item.error {
                        Some(error) => format!("{}\nNot delivered: {error}", item.text),
                        None => item.text.clone(),
                    },
                    InboxDelivery::Steer,
                )
            })
            .collect::<Vec<_>>();
        let queued = self.queued().collect::<Vec<_>>();
        if let Some(first) = queued.first() {
            rows.push(row(
                "queued",
                format!(
                    "{} queued · {}",
                    queued.len(),
                    first.text.replace(['\n', '\r'], " ")
                ),
                InboxDelivery::Queue,
            ));
        }
        rows
    }

    pub(super) fn queued_prompt_options(&self) -> Vec<crate::dialog::SelectOption> {
        let count = self.queued().count();
        self.queued()
            .enumerate()
            .map(|(index, item)| crate::dialog::SelectOption {
                value: item.id.clone(),
                title: item.text.replace(['\n', '\r'], " "),
                footer: item.error.as_ref().map_or_else(
                    || format!("{}/{count} · {} images", index + 1, item.images),
                    |error| format!("Not delivered: {error}"),
                ),
                ..crate::dialog::SelectOption::default()
            })
            .collect()
    }
}
