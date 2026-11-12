//! Source-aware UI projection. No OpenCode identity is a native turn, journal
//! cursor or permission grant. Existing widgets/rendering remain the presenter.

use super::*;

// OpenCode reads an omitted session variant back as "default". Keep that raw
// reference, but compare it like stock's default picker choice, not a new setter.
fn same_model(
    left: &Option<oc_core::queries::ModelRef>,
    right: &Option<oc_core::queries::ModelRef>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left.provider == right.provider
                && left.id == right.id
                && left
                    .variant
                    .as_deref()
                    .filter(|variant| *variant != "default")
                    == right
                        .variant
                        .as_deref()
                        .filter(|variant| *variant != "default")
        }
        _ => left == right,
    }
}
use oc_core::opencode::{
    self, Event, Message, Page, Part, Receipt, Reply, Request, Selection, View,
};

#[derive(Default)]
pub(super) struct State {
    catalog: Option<std::sync::Arc<opencode::Catalog>>,
    selection: Option<Selection>,
    messages: Vec<Live>,
    older: Option<String>,
    newer: Option<String>,
    detached: bool,
    end: Option<usize>,
    visible: (usize, usize, usize),
    dirty: bool,
    running: bool,
    pending: Option<Pending>,
    refresh: Option<Receipt>,
    again: bool,
    page: Option<(Receipt, bool)>,
    permissions: Vec<opencode::Permission>,
    forms: Vec<opencode::Form>,
}
struct Live {
    info: Message,
    parts: Vec<(PartKey, Part)>,
    live: bool,
    epoch: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum PartKey {
    Text(usize),
    Reasoning(usize),
    Tool(String),
}
impl PartKey {
    fn text(reasoning: bool, ordinal: usize) -> Self {
        if reasoning {
            Self::Reasoning(ordinal)
        } else {
            Self::Text(ordinal)
        }
    }
}

fn ordered_parts(parts: Vec<Part>) -> Vec<(PartKey, Part)> {
    let (mut text, mut reasoning) = (0, 0);
    parts
        .into_iter()
        .map(|part| {
            let key = match &part {
                Part::Text { .. } => {
                    let key = PartKey::Text(text);
                    text += 1;
                    key
                }
                Part::Reasoning { .. } => {
                    let key = PartKey::Reasoning(reasoning);
                    reasoning += 1;
                    key
                }
                Part::Tool { id, .. } => PartKey::Tool(id.clone()),
            };
            (key, part)
        })
        .collect()
}
struct Pending {
    receipt: Receipt,
    phase: Phase,
    desired: Selection,
    text: Option<String>,
    revision: u64,
    delivery: oc_core::queries::InboxDelivery,
}
enum Phase {
    Create,
    Agent,
    Model,
    Prompt,
}

impl State {
    pub(super) fn with_catalog(catalog: opencode::Catalog) -> Self {
        Self {
            catalog: Some(std::sync::Arc::new(catalog)),
            ..Default::default()
        }
    }
    fn bounded(&self) -> bool {
        if self.messages.len() > 128
            || self
                .messages
                .iter()
                .map(|item| item.parts.len())
                .sum::<usize>()
                > 8192
        {
            return false;
        }
        let bytes: usize = self
            .messages
            .iter()
            .map(|item| {
                item.info.id.len()
                    + item.info.text.len()
                    + item
                        .parts
                        .iter()
                        .map(|(_, part)| match part {
                            Part::Text { text } | Part::Reasoning { text } => text.len(),
                            Part::Tool {
                                id,
                                name,
                                input,
                                output,
                                status,
                            } => id.len() + name.len() + input.len() + output.len() + status.len(),
                        })
                        .sum::<usize>()
            })
            .sum();
        bytes <= crate::history::WINDOW_BYTES
    }
    pub(super) fn pending(&self) -> bool {
        self.pending.is_some() || self.refresh.is_some() || self.page.is_some()
    }
    pub(super) fn submitting(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn busy(&self) -> bool {
        self.running || self.pending.is_some()
    }
    pub(super) fn running(&self) -> bool {
        self.running
    }
    fn merge(&mut self, mut info: Message, epoch: u64) {
        let parts = ordered_parts(std::mem::take(&mut info.parts));
        if let Some(existing) = self
            .messages
            .iter_mut()
            .find(|item| item.info.id == info.id)
        {
            if epoch < existing.epoch {
                return;
            }
            if !existing.live || info.status != "running" {
                existing.parts = parts;
                existing.live = false;
            } else {
                for (key, part) in parts {
                    if let Some((_, old)) = existing.parts.iter_mut().find(|(old, _)| *old == key) {
                        if matches!(part, Part::Tool { .. }) {
                            *old = part;
                        }
                    } else {
                        existing.parts.push((key, part));
                    }
                }
            }
            existing.info = info;
            existing.epoch = epoch;
        } else if !self.detached {
            self.messages.push(Live {
                info,
                parts,
                live: false,
                epoch,
            });
        }
        self.dirty = true;
    }
    fn page(&mut self, page: Page, replace: bool) {
        if replace {
            self.messages.clear();
            self.detached = false;
        }
        let order: Vec<_> = page
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
        self.messages
            .retain(|item| order.contains(&item.info.id) || item.live && item.epoch > page.epoch);
        for message in page.messages {
            self.merge(message, page.epoch);
        }
        self.messages.sort_by_key(|item| {
            order
                .iter()
                .position(|id| id == &item.info.id)
                .unwrap_or(usize::MAX)
        });
        self.older = page.older;
        self.newer = page.newer;
        self.end = None;
        self.dirty = true;
    }
}

impl TuiState {
    pub fn opencode_catalog(&self) -> Option<std::sync::Arc<opencode::Catalog>> {
        self.opencode.as_ref()?.catalog.clone()
    }
    pub fn is_opencode(&self) -> bool {
        self.app.is_opencode()
    }
    pub fn opencode_selection(&self) -> Option<Selection> {
        self.opencode
            .as_ref()
            .and_then(|source| source.selection.clone())
            .or_else(|| {
                self.app
                    .opencode_init()
                    .map(|init| init.catalog.selection.clone())
            })
    }

    pub fn apply_opencode_view(&mut self, view: View) {
        self.chrome.location = Some(view.session.location.clone());
        self.opencode.as_mut().expect("OpenCode view").catalog =
            Some(std::sync::Arc::new(view.catalog));
        self.session = Some(view.session.id.clone());
        self.home = false;
        self.session_title = view.session.title;
        self.parent_id = view.session.parent;
        self.apply_opencode_selection(view.session.selection);
        self.apply_inbox(&view.session.id, view.inbox);
        let source = self.opencode.as_mut().expect("OpenCode view");
        source.running = view.running;
        if !source.detached {
            source.page(view.page, false);
        }
        source.permissions = view.permissions;
        source.forms = view.forms;
        self.reconcile_opencode_prompts();
        self.opencode_status();
    }

    fn apply_opencode_selection(&mut self, selection: Selection) {
        let metadata = self.opencode_catalog().expect("OpenCode metadata");
        let provider = selection
            .model
            .as_ref()
            .map(|model| model.provider.clone())
            .unwrap_or_default();
        let catalog = CatalogSnapshot {
            provider: provider.clone(),
            model_id: selection
                .model
                .as_ref()
                .map(|model| model.id.clone())
                .unwrap_or_default(),
            variant: selection
                .model
                .as_ref()
                .and_then(|model| model.variant.clone())
                .filter(|variant| variant != "default"),
            models: metadata
                .models
                .iter()
                .filter(|model| model.provider == provider)
                .map(|model| model.entry.clone())
                .collect(),
            agents: metadata.agents.clone(),
            agent_id: selection.agent.clone(),
            chrome: self.chrome.clone(),
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            commands: Vec::new(),
            command_descriptions: Default::default(),
        };
        self.opencode.as_mut().expect("OpenCode view").selection = Some(selection);
        self.apply_catalog(catalog);
    }

    fn opencode_status(&mut self) {
        if self.status == TuiStatus::Quit {
            return;
        }
        let source = self.opencode.as_ref().expect("OpenCode view");
        self.status = if source.running {
            TuiStatus::Streaming
        } else if source.pending.is_some() {
            TuiStatus::PendingSubmission
        } else {
            TuiStatus::Idle
        };
    }

    pub(super) fn opencode_enter(&mut self) -> KeyOutcome {
        self.opencode_enter_delivery(oc_core::queries::InboxDelivery::Steer)
    }

    pub(super) fn opencode_enter_delivery(
        &mut self,
        delivery: oc_core::queries::InboxDelivery,
    ) -> KeyOutcome {
        if self.opencode.as_ref().is_some_and(State::submitting) {
            return KeyOutcome {
                note: Some("OpenCode admission pending; input was not sent again".into()),
                ..Default::default()
            };
        }
        if self.editor.has_images() || self.editor.has_prompt_parts() {
            return KeyOutcome {
                note: Some(
                    "Attachments are unavailable in the OpenCode bridge; use stock TUI".into(),
                ),
                ..Default::default()
            };
        }
        let current = self
            .opencode
            .as_ref()
            .and_then(|state| state.selection.clone())
            .unwrap_or_else(|| {
                self.app
                    .opencode_init()
                    .expect("OpenCode metadata")
                    .catalog
                    .selection
                    .clone()
            });
        let desired = Selection {
            agent: self
                .staged_agent()
                .or_else(|| self.composer_agent().map(str::to_owned))
                .or_else(|| current.agent.clone()),
            model: self
                .captured_model_commit()
                .map(|draft| oc_core::queries::ModelRef {
                    provider: draft.binding.provider,
                    id: draft.model_id,
                    variant: draft.variant,
                })
                .or_else(|| current.model.clone()),
        };
        let text = (!self.input.trim().is_empty()).then(|| self.input.trim().to_owned());
        let (phase, request) = if let Some(session) = self.session.clone() {
            if desired.agent != current.agent && desired.agent.is_some() {
                (
                    Phase::Agent,
                    Request::SelectAgent {
                        session,
                        agent: desired.agent.clone().expect("agent"),
                    },
                )
            } else if !same_model(&desired.model, &current.model) && desired.model.is_some() {
                (
                    Phase::Model,
                    Request::SelectModel {
                        session,
                        model: desired.model.clone().expect("model"),
                    },
                )
            } else if let Some(text) = &text {
                (
                    Phase::Prompt,
                    Request::Prompt {
                        session,
                        text: text.clone(),
                        delivery,
                    },
                )
            } else {
                return KeyOutcome::default();
            }
        } else if text.is_some() {
            (
                Phase::Create,
                Request::Create {
                    selection: desired.clone(),
                },
            )
        } else {
            return KeyOutcome::default();
        };
        match self.app.request_opencode(request) {
            Ok(receipt) => {
                self.opencode.as_mut().expect("OpenCode view").pending = Some(Pending {
                    receipt,
                    phase,
                    desired,
                    text,
                    revision: self.input_revision,
                    delivery,
                });
                self.opencode_status();
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(error.to_string()),
                ..Default::default()
            },
        }
    }

    pub(super) fn poll_opencode(&mut self) {
        let mut source = self.opencode.take().expect("OpenCode view");
        if let Some(mut pending) = source.pending.take() {
            if let Some(result) = pending.receipt.try_result() {
                self.opencode = Some(source);
                match result {
                    Ok(Reply::Created { session }) => {
                        // This ACK precedes prompt admission. Never undo the root
                        // or its editable draft after a later prompt refusal.
                        self.session = Some(session.id);
                        self.home = false;
                        self.session_title = session.title;
                        self.apply_opencode_selection(session.selection);
                        self.next_opencode(pending, false);
                    }
                    Ok(Reply::Selection { selection }) => {
                        let model = matches!(pending.phase, Phase::Agent)
                            && !same_model(&pending.desired.model, &selection.model)
                            && pending.desired.model.is_some();
                        self.apply_opencode_selection(selection);
                        self.next_opencode(pending, model);
                    }
                    Ok(Reply::Admitted { .. }) => {
                        // Admission is not execution-start. Actual running and
                        // pending-input state come from their own public reads.
                        if self.input_revision == pending.revision {
                            self.input.clear();
                            self.editor.clear();
                            self.input_revision += 1;
                        }
                        if self
                            .agent_draft
                            .as_ref()
                            .and_then(|draft| draft.agent_id.as_ref())
                            == pending.desired.agent.as_ref()
                        {
                            self.agent_draft = None;
                        }
                        self.request_opencode_refresh();
                    }
                    Ok(_) => self.push_note("Unexpected OpenCode acknowledgement; input retained"),
                    Err(error) => self.push_note(&format!("{error}; session and draft retained")),
                }
                source = self.opencode.take().expect("OpenCode view");
            } else {
                source.pending = Some(pending);
            }
        }
        if let Some(mut receipt) = source.refresh.take() {
            if let Some(result) = receipt.try_result() {
                self.opencode = Some(source);
                match result {
                    Ok(Reply::View { view }) => self.apply_opencode_view(*view),
                    Err(error) => self.push_note(&error.to_string()),
                    _ => self.push_note("Unexpected OpenCode projection"),
                }
                source = self.opencode.take().expect("OpenCode view");
            } else {
                source.refresh = Some(receipt);
            }
        }
        if let Some((mut receipt, older)) = source.page.take() {
            if let Some(result) = receipt.try_result() {
                match result {
                    Ok(Reply::History { page }) if page.messages.is_empty() => {
                        if older {
                            source.older = None;
                        } else {
                            source.newer = None;
                            source.detached = false;
                        }
                    }
                    Ok(Reply::History { page }) => {
                        source.page(page, true);
                        source.detached = true;
                        self.scroll = if older { 0 } else { usize::MAX };
                    }
                    Err(error) => self.push_note(&error.to_string()),
                    _ => self.push_note("Unexpected OpenCode history page"),
                }
            } else {
                source.page = Some((receipt, older));
            }
        }
        let again = source.again && source.refresh.is_none();
        if again {
            source.again = false;
        }
        self.opencode = Some(source);
        if again {
            self.request_opencode_refresh();
        }
        self.opencode_status();
    }

    fn next_opencode(&mut self, mut pending: Pending, model: bool) {
        let session = self.session.clone().expect("created or existing session");
        let request = if model {
            pending.phase = Phase::Model;
            Request::SelectModel {
                session,
                model: pending.desired.model.clone().expect("selected model"),
            }
        } else if let Some(text) = &pending.text {
            pending.phase = Phase::Prompt;
            Request::Prompt {
                session,
                text: text.clone(),
                delivery: pending.delivery,
            }
        } else {
            self.agent_draft = None;
            return;
        };
        match self.app.request_opencode(request) {
            Ok(receipt) => {
                pending.receipt = receipt;
                self.opencode.as_mut().expect("OpenCode view").pending = Some(pending);
            }
            Err(error) => self.push_note(&format!("{error}; session and draft retained")),
        }
    }

    pub fn request_opencode_refresh(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let source = self.opencode.as_mut().expect("OpenCode view");
        if source.refresh.is_some() {
            source.again = true;
            return;
        }
        match self.app.request_opencode(Request::Open { session }) {
            Ok(receipt) => source.refresh = Some(receipt),
            Err(error) => self.push_note(&error.to_string()),
        }
    }

    pub fn opencode_event(&mut self, event: &Event) -> Result<(), String> {
        let session = match event {
            Event::RequestsChanged => {
                if self.session.is_none() {
                    self.push_note(
                        "OpenCode requests changed; open their session or return to stock TUI",
                    );
                }
                self.request_opencode_refresh();
                return Ok(());
            }
            Event::Started { session, .. }
            | Event::Message { session, .. }
            | Event::Part { session, .. }
            | Event::PartStarted { session, .. }
            | Event::ToolStarted { session, .. }
            | Event::Delta { session, .. }
            | Event::Running { session, .. }
            | Event::Refresh { session } => session,
            Event::Unavailable { reason } => return Err(reason.clone()),
        };
        if self.session.as_ref() != Some(session) {
            return Ok(());
        }
        if matches!(event, Event::Refresh { .. }) {
            self.request_opencode_refresh();
            return Ok(());
        }
        let source = self.opencode.as_mut().expect("OpenCode view");
        match event {
            Event::Started { message, epoch, .. } => {
                source.merge(message.clone(), *epoch);
                if let Some(item) = source
                    .messages
                    .iter_mut()
                    .find(|item| item.info.id == message.id && item.epoch == *epoch)
                {
                    item.live = true;
                }
            }
            Event::Message { message, epoch, .. } => source.merge(message.clone(), *epoch),
            Event::PartStarted {
                message,
                ordinal,
                reasoning,
                epoch,
                ..
            } => {
                if let Some(item) = source
                    .messages
                    .iter_mut()
                    .find(|item| &item.info.id == message && item.live)
                {
                    let key = PartKey::text(*reasoning, *ordinal);
                    if !item.parts.iter().any(|(old, _)| *old == key) {
                        let part = if *reasoning {
                            Part::Reasoning {
                                text: String::new(),
                            }
                        } else {
                            Part::Text {
                                text: String::new(),
                            }
                        };
                        item.parts.push((key, part));
                    }
                    item.epoch = *epoch;
                    source.dirty = true;
                }
            }
            Event::ToolStarted {
                message,
                id,
                name,
                epoch,
                ..
            } => {
                if let Some(item) = source
                    .messages
                    .iter_mut()
                    .find(|item| &item.info.id == message && item.live)
                {
                    let key = PartKey::Tool(id.clone());
                    if !item.parts.iter().any(|(old, _)| *old == key) {
                        item.parts.push((
                            key,
                            Part::Tool {
                                id: id.clone(),
                                name: name.clone(),
                                input: String::new(),
                                output: String::new(),
                                status: "streaming".into(),
                            },
                        ));
                    }
                    item.epoch = *epoch;
                    source.dirty = true;
                }
            }
            Event::Delta {
                message,
                ordinal,
                reasoning,
                delta,
                epoch,
                ..
            } => {
                // A history read has no public delta watermark. Only append for
                // a step whose start this view actually observed; already-active
                // partial content waits for full ended parts/settled hydration.
                let Some(item) = source
                    .messages
                    .iter_mut()
                    .find(|item| &item.info.id == message && item.live)
                else {
                    return Ok(());
                };
                let key = PartKey::text(*reasoning, *ordinal);
                let Some((_, part)) = item.parts.iter_mut().find(|(old, _)| *old == key) else {
                    return Ok(());
                };
                if let Part::Text { text } | Part::Reasoning { text } = part {
                    let remaining = (32 * 1024_usize).saturating_sub(text.len());
                    text.push_str(crate::truncate_utf8(delta, remaining));
                    item.info.truncated |= delta.len() > remaining;
                }
                item.live = true;
                item.epoch = *epoch;
                source.dirty = true;
            }
            Event::Part {
                message,
                ordinal,
                part,
                epoch,
                ..
            } => {
                if let Some(item) = source
                    .messages
                    .iter_mut()
                    .find(|item| &item.info.id == message && item.live)
                {
                    let key = match part {
                        Part::Text { .. } => PartKey::Text(*ordinal),
                        Part::Reasoning { .. } => PartKey::Reasoning(*ordinal),
                        Part::Tool { id, .. } => PartKey::Tool(id.clone()),
                    };
                    if let Some((_, old)) = item.parts.iter_mut().find(|(old, _)| *old == key) {
                        *old = part.clone();
                    }
                    item.epoch = *epoch;
                    source.dirty = true;
                }
            }
            Event::Running { running, .. } => source.running = *running,
            _ => {}
        }
        if !source.bounded() {
            return Err(
                "OpenCode live projection limit exceeded; nothing resent or interrupted".into(),
            );
        }
        self.opencode_status();
        Ok(())
    }

    /// Called once before a paint, not once per token or on a polling timer.
    pub fn flush_opencode(&mut self) {
        let Some(source) = self.opencode.as_mut().filter(|source| source.dirty) else {
            return;
        };
        source.dirty = false;
        let rows: Vec<_> = source.messages.iter().flat_map(rows).collect();
        let end = source.end.unwrap_or(rows.len()).min(rows.len());
        let start = end.saturating_sub(crate::history::WINDOW_ROWS);
        source.visible = (start, end, rows.len());
        self.window.set_opencode_rows(
            rows[start..end].to_vec(),
            start > 0 || source.older.is_some(),
            end < rows.len() || source.detached && source.newer.is_some(),
        );
        self.invalidate_transcript();
    }

    pub fn opencode_history(&mut self, older: bool) {
        let source = self.opencode.as_mut().expect("OpenCode view");
        if source.page.is_some() {
            return;
        }
        let (start, end, total) = source.visible;
        if older && start > 0 {
            source.end = Some(start);
            source.dirty = true;
            self.scroll = 0;
            return;
        }
        if !older && end < total {
            source.end = Some((end + crate::history::WINDOW_ROWS).min(total));
            source.dirty = true;
            self.scroll = usize::MAX;
            return;
        }
        let cursor = if older {
            source.older.clone()
        } else {
            source.newer.clone()
        };
        let Some(cursor) = cursor else {
            return;
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        match self.app.request_opencode(Request::History {
            session,
            cursor: Some(cursor),
        }) {
            Ok(receipt) => source.page = Some((receipt, older)),
            Err(error) => self.push_note(&error.to_string()),
        }
    }

    fn reconcile_opencode_prompts(&mut self) {
        let source = self.opencode.as_ref().expect("OpenCode view");
        // These are presentation DTOs for the existing widgets, not native
        // invocation authority: no turn/call/digest is fabricated. Replies are
        // reconstructed below from the retained typed OpenCode requests only.
        let binding = |session: &SessionId, id: &str| oc_core::approval::ApprovalBinding {
            session: session.0.clone(),
            operation: id.into(),
            turn: String::new(),
            call: String::new(),
            input_digest: String::new(),
            location: String::new(),
            generation: 0,
            agent: None,
            agent_digest: None,
        };
        self.approvals.reconcile(
            source
                .permissions
                .iter()
                .enumerate()
                .map(|(id, request)| {
                    (
                        oc_core::approval::ApprovalRequest {
                            id: id as u64,
                            binding: binding(&request.session, &request.id),
                            project: "OpenCode".into(),
                            action: request.action.clone(),
                            resources: request.resources.clone(),
                            save_patterns: request.save.clone(),
                            preview: oc_core::approval::ApprovalPreview::Tool,
                        },
                        true,
                    )
                })
                .collect(),
        );
        let questions: Vec<_> = source
            .forms
            .iter()
            .enumerate()
            .filter_map(|(id, form)| {
                let questions = form.questions.as_ref()?;
                Some(oc_core::question::QuestionRequest {
                    id: id as u64,
                    binding: binding(&form.session, &form.id),
                    images: false,
                    input: oc_core::question::QuestionInput {
                        questions: questions
                            .iter()
                            .map(|question| oc_core::question::QuestionPrompt {
                                question: question.label.clone(),
                                header: form.title.clone(),
                                multiple: question.multiple,
                                options: question
                                    .options
                                    .iter()
                                    .map(|option| oc_core::question::QuestionOption {
                                        label: option.label.clone(),
                                        description: option.description.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    },
                })
            })
            .collect();
        self.questions.reconcile(&questions, questions.clone());
    }

    pub fn opencode_permission(
        &self,
        reply: &oc_core::approval::ApprovalReply,
    ) -> Result<Request, CoreError> {
        let permission = self
            .opencode
            .as_ref()
            .and_then(|source| source.permissions.get(reply.id as usize))
            .filter(|request| {
                request.id == reply.binding.operation && request.session.0 == reply.binding.session
            })
            .ok_or(CoreError::SessionNotFound)?;
        let (decision, message) = match &reply.decision {
            oc_core::approval::ApprovalDecision::Once => (opencode::PermissionDecision::Once, None),
            oc_core::approval::ApprovalDecision::Always => {
                (opencode::PermissionDecision::Always, None)
            }
            oc_core::approval::ApprovalDecision::Reject { feedback } => {
                (opencode::PermissionDecision::Reject, feedback.clone())
            }
            _ => return Err(CoreError::SessionNotFound),
        };
        Ok(Request::ReplyPermission {
            session: permission.session.clone(),
            id: permission.id.clone(),
            decision,
            message,
        })
    }

    pub fn opencode_form(
        &self,
        reply: &oc_core::question::QuestionReply,
    ) -> Result<Request, CoreError> {
        let form = self
            .opencode
            .as_ref()
            .and_then(|source| source.forms.get(reply.id as usize))
            .filter(|request| {
                request.id == reply.binding.operation && request.session.0 == reply.binding.session
            })
            .ok_or(CoreError::SessionNotFound)?;
        let questions = form.questions.as_ref().ok_or(CoreError::SessionNotFound)?;
        match &reply.decision {
            oc_core::question::QuestionDecision::Cancelled => Ok(Request::CancelForm {
                session: form.session.clone(),
                id: form.id.clone(),
            }),
            oc_core::question::QuestionDecision::Answers { answers, images }
                if images.is_empty() && answers.len() == questions.len() =>
            {
                let answer = questions
                    .iter()
                    .zip(answers)
                    .map(|(question, answer)| {
                        let values: Vec<_> = answer
                            .iter()
                            .map(|value| {
                                question
                                    .options
                                    .iter()
                                    .find(|option| &option.label == value)
                                    .map_or_else(|| value.clone(), |option| option.value.clone())
                            })
                            .collect();
                        let value = if question.multiple {
                            opencode::Answer::Many(values)
                        } else {
                            opencode::Answer::One(values.first().cloned().unwrap_or_default())
                        };
                        (question.key.clone(), value)
                    })
                    .collect();
                Ok(Request::ReplyForm {
                    session: form.session.clone(),
                    id: form.id.clone(),
                    answer,
                })
            }
            _ => Err(CoreError::SessionNotFound),
        }
    }
}

#[cfg(test)]
fn empty_message(id: &str) -> Message {
    Message {
        id: id.into(),
        role: "assistant".into(),
        text: String::new(),
        agent: None,
        model: None,
        parts: Vec::new(),
        status: "running".into(),
        truncated: false,
    }
}

fn rows(message: &Live) -> Vec<crate::history::HistoryRow> {
    use crate::history::{HistoryPosition, HistoryRow};
    use crate::messages::{AssistantMeta, ReasoningBlock, ReasoningIdentity};
    let id = std::sync::Arc::new(oc_core::session::MessageId(message.info.id.clone()));
    let base = || HistoryRow {
        message_id: Some(id.clone()),
        position: Some(HistoryPosition::local(&message.info.id, None, false)),
        role: message.info.role.clone(),
        agent: message.info.agent.clone(),
        ..Default::default()
    };
    if message.info.role != "assistant" {
        return vec![HistoryRow {
            text: message.info.text.clone().into(),
            ..base()
        }];
    }
    let mut result = Vec::new();
    for (ordinal, (key, part)) in message.parts.iter().enumerate() {
        let mut row = base();
        row.position = Some(HistoryPosition::local(
            &message.info.id,
            Some(ordinal),
            false,
        ));
        match part {
            Part::Text { text } => row.text = text.clone().into(),
            Part::Reasoning { text } => {
                row.reasoning = Some(ReasoningBlock {
                    text: text.clone().into(),
                    duration_ms: None,
                    running: message.info.status == "running",
                    expanded: false,
                    toggleable: true,
                    spinner: None,
                    identity: Some(ReasoningIdentity::OpenCode(
                        message.info.id.clone(),
                        match key {
                            PartKey::Reasoning(ordinal) => *ordinal,
                            _ => unreachable!("reasoning key"),
                        },
                    )),
                })
            }
            Part::Tool {
                id,
                name,
                input,
                output,
                status,
            } => {
                row.role = "tool".into();
                row.tool = Some(std::sync::Arc::new(crate::history::card_from_row(
                    &oc_core::queries::ToolOpView {
                        op: id.clone(),
                        rowid: 0,
                        name: name.clone(),
                        state: if status == "error" { "failed" } else { status }.into(),
                        input: Some(input.clone()),
                        output: Some(output.clone()),
                        output_bytes: output.len() as i64,
                        output_truncated: message.info.truncated,
                        child_job: None,
                        output_presentation: None,
                        question: None,
                        dcp_topic: None,
                        dcp: None,
                        patch_effects: None,
                    },
                )));
            }
        }
        result.push(row);
    }
    if let Some(row) = result.last_mut() {
        row.meta = Some(AssistantMeta {
            model: message.info.model.as_ref().map(|model| {
                format!(
                    "{}/{}{}",
                    model.provider,
                    model.id,
                    model
                        .variant
                        .as_ref()
                        .map_or(String::new(), |variant| format!(" · {variant}"))
                )
            }),
            status: Some(message.info.status.clone()),
            preview_limited: message.info.truncated,
            ..Default::default()
        });
    }
    result
}

#[cfg(test)]
mod tests;
