//! Composer drafts and bounded owner receipts. This state never dispatches a
//! generation or infers a committed choice from a menu/stream/footer.
use super::*;
use oc_core::queries::{ModelCommit, ModelPreferenceModel, ModelRef, SelectionBinding};
use std::collections::{BTreeMap, VecDeque};

#[cfg(test)]
#[path = "model_selection_tests.rs"]
mod tests;

/// A local model choice with its provider's already-loaded catalog. Captured
/// before asynchronous agent selection; never evidence of an owner commit.
#[derive(Clone)]
pub struct ComposerModelDraft {
    pub(super) model: ModelRef,
    pub(super) catalog: ModelCatalog,
}

pub(super) struct ComposerSelection {
    caller: u64,
    binding: Option<SelectionBinding>,
    picker_binding: Option<SelectionBinding>,
    picker_readiness: Option<oc_core::queries::ProviderReadiness>,
    picker_custom: bool,
    drafts: BTreeMap<Option<String>, ComposerModelDraft>,
    committed: Option<ModelRef>,
    revision: u64,
    acknowledged: u64,
    recent: VecDeque<(Option<String>, ModelRef)>,
    committed_recent: VecDeque<(Option<String>, ModelRef)>,
    pending: VecDeque<(
        SessionId,
        ModelCommit,
        oc_core::core_app::ModelCommitReceipt,
    )>,
}

impl Default for ComposerSelection {
    fn default() -> Self {
        static CALLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            caller: CALLER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            binding: None,
            picker_binding: None,
            picker_readiness: None,
            picker_custom: false,
            drafts: Default::default(),
            committed: None,
            revision: 0,
            acknowledged: 0,
            recent: Default::default(),
            committed_recent: Default::default(),
            pending: Default::default(),
        }
    }
}

impl TuiState {
    /// Canonical owner facts do not own an independently scoped browse dialog.
    /// Keep its binding and rows until its own fenced provider refresh arrives.
    pub fn apply_catalog_refresh(&mut self, snapshot: CatalogSnapshot) {
        let browse = self
            .model_selection
            .picker_binding
            .as_ref()
            .filter(|binding| {
                binding.location == snapshot.chrome.location
                    && self
                        .model_selection
                        .binding
                        .as_ref()
                        .is_some_and(|composer| composer.provider != binding.provider)
                    && binding.provider != snapshot.provider
            })
            .cloned()
            .map(|binding| {
                (
                    self.picker.take(),
                    binding,
                    self.model_selection.picker_readiness.clone(),
                    self.model_selection.picker_custom,
                    self.select.cursor,
                )
            });
        self.apply_catalog(snapshot);
        if let Some((picker, binding, readiness, custom, cursor)) = browse {
            self.picker = picker;
            self.model_selection.picker_binding = Some(binding);
            self.model_selection.picker_readiness = readiness;
            self.model_selection.picker_custom = custom;
            self.select.cursor = cursor;
            self.sync_modal_cursor();
        }
    }

    /// Carry only client-owned composer state across a freshly loaded native
    /// projection. Keep the owner's new committed choice and configuration.
    pub fn preserve_shared_composer(&mut self, mut previous: Self) {
        self.restore_composer_draft(previous.composer_draft());
        let same_route =
            self.chrome.location == previous.chrome.location && self.session == previous.session;
        if same_route {
            self.provisional_session = previous.provisional_session.take();
            self.pending = previous.pending.take();
            self.submission_intent = previous.submission_intent.take();
            self.compaction_admission_pending = previous.compaction_admission_pending;
            self.refused_submission = previous.refused_submission.take();
            self.agent_draft_revision = previous.agent_draft_revision;
            if self.active_turn.is_none() && self.has_pending_submission() {
                self.status = TuiStatus::PendingSubmission;
            }
            let _ = previous.execute_capture();
            // A canonical refresh updates facts, not this client's interaction
            // owner. Preserve complete modal state, not just its panel enum.
            self.inherit_terminal_view(&mut previous);
            self.inherit_shell_view(&mut previous);
            self.panel = previous.panel.clone();
            self.select = previous.select;
            self.rename_input = previous.rename_input;
            self.rename_editor = previous.rename_editor;
            self.rename_pending = previous.rename_pending;
            self.rename_selected = previous.rename_selected;
            self.rename_direct_pending = previous.rename_direct_pending;
            self.regenerate_pending = previous.regenerate_pending;
            self.session_delete_confirm = previous.session_delete_confirm;
            self.session_project_name = previous.session_project_name;
            self.session_scope_pending = previous.session_scope_pending;
            self.sessions = previous.sessions;
            self.session_entries = previous.session_entries;
            self.sessions_all_projects = previous.sessions_all_projects;
            self.sessions_cursor = previous.sessions_cursor;
            self.sessions_loaded = previous.sessions_loaded;
            self.agents_cursor = previous
                .agents_cursor
                .min(self.agents.len().saturating_sub(1));
            self.skills_cursor = previous
                .skills_cursor
                .min(self.skills.len().saturating_sub(1));
            self.exploration_expanded = previous.exploration_expanded;
            self.inline_errors_expanded = previous.inline_errors_expanded;
            self.instructions_expanded = previous.instructions_expanded;
            self.execute_calls_expanded = previous.execute_calls_expanded;
            self.execute_detail = previous.execute_detail;
            self.reasoning_expanded = previous.reasoning_expanded;
            self.thinking_expanded = previous.thinking_expanded;
            self.scroll = previous.scroll.min(self.max_scroll());
            self.leader = previous.leader;
            self.slash_selected = previous.slash_selected;
            self.slash_dismissed = previous.slash_dismissed;
            self.request_id = previous.request_id;
            self.generation = previous.generation;
            self.view_id = previous.view_id;
            self.tool_spinner_clocks
                .clone_from(&previous.tool_spinner_clocks);
            self.reasoning_spinner_clock
                .clone_from(&previous.reasoning_spinner_clock);
            if (self.chrome.animations != Some(false))
                != (previous.chrome.animations != Some(false))
            {
                for clock in self
                    .tool_spinner_clocks
                    .values_mut()
                    .chain(self.reasoning_spinner_clock.iter_mut())
                {
                    *clock = ToolSpinnerClock::default();
                }
            }
            self.sync_tool_spinner_clocks();
            self.input_revision = previous.input_revision;
            self.mention_selected = previous.mention_selected;
            self.mention_dismissed = previous.mention_dismissed;
            self.mention_result = previous.mention_result;
            self.mention_owner_epoch = previous.mention_owner_epoch;
            self.interrupt_armed_until = previous.interrupt_armed_until;
            if self.note.is_none() {
                self.note = previous.note;
                self.toast_expiry = previous.toast_expiry;
                self.service_note = previous.service_note;
            }
            if self.model_selection.binding == previous.model_selection.binding {
                self.accounts = previous.accounts;
                self.mcp_detail = previous.mcp_detail;
                self.mcp_focused = previous.mcp_focused;
            } else if matches!(
                self.panel,
                TuiPanel::Accounts | TuiPanel::Mcps | TuiPanel::Model | TuiPanel::Variant
            ) {
                self.panel = TuiPanel::None;
            }
        }
        // A staged next-send agent is client-owned composer state. Re-stage it
        // first so its per-agent model drafts below match the same binding.
        if let Some(draft) = previous.agent_draft.take()
            && self.agent_draft.is_none()
            && self.session == previous.session
            && self.active_agent.as_deref() != draft.agent_id.as_deref()
        {
            let _ = self.apply_agent_preview(draft);
        }
        if same_route {
            // Reconstructing a local preview is not a new user selection. Keep
            // the exact revision captured by a transferred preparation receipt.
            self.agent_draft_revision = previous.agent_draft_revision;
        }
        let draft_scope_matches = same_route
            && self
                .model_selection
                .binding
                .as_ref()
                .zip(previous.model_selection.binding.as_ref())
                .is_some_and(|(current, old)| {
                    current.location == old.location
                        && current.generation == old.generation
                        && current.agent_id == old.agent_id
                        && previous.model_selection.drafts.contains_key(&old.agent_id)
                });
        if self.model_selection.binding == previous.model_selection.binding || draft_scope_matches {
            let preserve_picker = !previous.model_selection.drafts.is_empty()
                || matches!(previous.panel, TuiPanel::Model | TuiPanel::Variant);
            let committed = self.model_selection.committed.take();
            self.model_selection = previous.model_selection;
            self.model_selection.committed = committed;
            if preserve_picker {
                self.picker = previous.picker.take();
                self.model_scope = previous.model_scope.take();
                self.model_directory = previous.model_directory.take();
            }
            if matches!(previous.panel, TuiPanel::Model | TuiPanel::Variant) {
                self.panel = previous.panel;
            }
        }
    }

    /// A shared canonical refresh follows these finite receipts, so a late ACK
    /// cannot overwrite a newer choice already read from the authority.
    pub fn has_pending_model_commit(&self) -> bool {
        !self.model_selection.pending.is_empty()
    }

    pub(super) fn account_provider(&self) -> String {
        self.model_selection
            .binding
            .as_ref()
            .map(|b| b.provider.clone())
            .unwrap_or_else(|| "opencode-go".into())
    }

    /// A filtered picker is a browse scope, not the committed composer connection.
    pub fn picker_provider_filter(&self) -> Option<&str> {
        matches!(self.panel, TuiPanel::Model | TuiPanel::Variant)
            .then(|| self.model_selection.picker_binding.as_ref())
            .flatten()
            .filter(|picker| {
                self.model_selection
                    .binding
                    .as_ref()
                    .is_none_or(|current| picker.provider != current.provider)
            })
            .map(|binding| binding.provider.as_str())
    }

    pub(super) fn bind_picker(&mut self, snapshot: &CatalogSnapshot) {
        self.model_selection.picker_readiness = snapshot.chrome.provider.clone();
        self.model_selection.picker_custom = snapshot.chrome.custom_connection;
        self.model_selection.picker_binding = Some(SelectionBinding {
            location: snapshot.chrome.location.clone(),
            generation: snapshot.chrome.selection_generation,
            provider: snapshot.provider.clone(),
            agent_id: self
                .model_selection
                .binding
                .as_ref()
                .and_then(|binding| binding.agent_id.clone()),
        });
    }

    pub fn custom_picker_binding(&self) -> Option<&SelectionBinding> {
        (self.panel == TuiPanel::Model && self.model_selection.picker_custom)
            .then_some(self.model_selection.picker_binding.as_ref())
            .flatten()
            .filter(|binding| {
                binding.location == self.chrome.location
                    && binding.generation == self.chrome.selection_generation
            })
    }

    /// The all-provider list is shown unless a connection opened a scoped view.
    pub(crate) fn model_directory_active(&self) -> Option<&crate::picker::ModelDirectory> {
        self.model_scope
            .is_none()
            .then_some(self.model_directory.as_ref())
            .flatten()
    }

    /// The open model dialog currently shows the all-provider directory.
    pub fn model_directory_visible(&self) -> bool {
        self.panel == TuiPanel::Model && self.model_directory_active().is_some()
    }

    /// Pinned DialogModel title: the provider name when scoped.
    pub(crate) fn model_dialog_title(&self) -> &str {
        self.model_scope.as_deref().unwrap_or("Select model")
    }

    /// Pinned `useConnected`; a scoped view follows a successful connection.
    pub(crate) fn model_dialog_connected(&self) -> bool {
        self.model_scope.is_some()
            || self
                .model_directory
                .as_ref()
                .is_some_and(|directory| directory.connected())
    }

    /// The `●` row: the model this composer's picker currently resolves.
    pub(crate) fn dialog_current_model(&self) -> Option<ModelPreferenceModel> {
        let picker = self.picker.as_ref()?;
        Some(ModelPreferenceModel {
            provider: picker.provider().to_owned(),
            model: picker.selection()?.id.clone(),
        })
    }

    /// Native CONN04 difference: a just-saved custom connection's scoped view
    /// keeps its catalog status and retry/accounts actions. No model metadata.
    pub(crate) fn model_dialog_status(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.model_scope.is_none() || self.custom_picker_binding().is_none() {
            return lines;
        }
        if let Some(readiness) = &self.model_selection.picker_readiness {
            let status = if readiness.catalog_status == oc_core::queries::ProviderStatus::Ready
                && self
                    .picker
                    .as_ref()
                    .is_none_or(|picker| picker.options().is_empty())
            {
                "empty"
            } else {
                readiness.catalog_status.as_str()
            };
            lines.push(format!("Catalog: {status}"));
            if let Some(diagnostic) = &readiness.diagnostic {
                lines.push(format!("Catalog/request: {:?}", diagnostic.code));
            }
        }
        lines.push("← Retry GET · → Accounts / change key".into());
        lines
    }

    /// Footer actions with their effective first binding; unbound ones hide.
    pub(crate) fn model_dialog_actions(&self) -> Vec<(&'static str, String)> {
        let keys = &self.chrome.dialog_shortcuts;
        let label = |binding: &str| {
            binding
                .split(',')
                .find(|key| !key.is_empty() && *key != "none")
                .map(str::to_owned)
        };
        let connected = self.model_dialog_connected();
        [
            (
                if connected {
                    "Connect an integration"
                } else {
                    "View all integrations"
                },
                label(&keys.model_provider),
            ),
            (
                "Favorite",
                label(&keys.model_favorite).filter(|_| connected),
            ),
        ]
        .into_iter()
        .filter_map(|(title, key)| Some((title, key?)))
        .collect()
    }

    fn focused_dialog_model(&self) -> Option<ModelPreferenceModel> {
        let options = self.modal_options();
        let value = &options.get(self.select.cursor)?.value;
        crate::picker::ModelDirectory::decode(value).or_else(|| {
            Some(ModelPreferenceModel {
                provider: self.picker.as_ref()?.provider().to_owned(),
                model: value.clone(),
            })
        })
    }

    /// Ctrl+A (`model.dialog.provider`) replaces the dialog with the integration list.
    pub(super) fn model_dialog_provider(&mut self) -> KeyOutcome {
        self.open_accounts(true)
    }

    /// Ctrl+F (`model.dialog.favorite`), hidden while nothing is connected.
    pub(super) fn model_dialog_favorite(&mut self) -> KeyOutcome {
        let Some(model) = self
            .model_dialog_connected()
            .then(|| self.focused_dialog_model())
            .flatten()
        else {
            return KeyOutcome::default();
        };
        let enabled = !self
            .model_directory
            .as_ref()
            .is_some_and(|directory| directory.is_favorite(&model));
        KeyOutcome {
            intent: Some(PanelIntent::FavoriteModel { model, enabled }),
            ..Default::default()
        }
    }

    /// Enter on an all-provider row. Another provider's row first binds that
    /// provider's browse view, then follows the existing exact-id draft path.
    pub(super) fn model_directory_enter(&mut self) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        let Some(model) = self.focused_dialog_model() else {
            outcome.note = Some("no model selected".to_string());
            return outcome;
        };
        if self
            .picker
            .as_ref()
            .is_none_or(|picker| picker.provider() != model.provider)
        {
            let Some(snapshot) = self
                .model_directory
                .as_ref()
                .and_then(|directory| directory.snapshot(&model.provider))
                .cloned()
            else {
                outcome.note = Some("provider model view unavailable".to_string());
                return outcome;
            };
            self.apply_picker_catalog(snapshot);
        }
        outcome.intent = Some(PanelIntent::SelectModel {
            id: model.model.clone(),
            recent: Some(model),
        });
        outcome
    }

    /// A composer catalog snapshot has been applied at least once.
    pub fn catalog_loaded(&self) -> bool {
        self.catalog_loaded
    }

    /// Owner answer for Ctrl+X M; stale Location/generation views are dropped.
    pub fn apply_model_directory(
        &mut self,
        catalogs: oc_core::queries::ModelCatalogs,
        preference: oc_core::queries::ModelPreference,
    ) {
        if catalogs.catalogs.first().is_some_and(|snapshot| {
            snapshot.chrome.location != self.chrome.location
                || snapshot.chrome.selection_generation != self.chrome.selection_generation
        }) {
            return;
        }
        self.model_directory = Some(crate::picker::ModelDirectory::new(catalogs, preference));
        if self.panel == TuiPanel::Model {
            self.sync_modal_cursor();
        }
    }

    /// Favorite/recent ACK; DialogSelect keeps the focused index (clamped).
    pub fn apply_model_preference(&mut self, preference: oc_core::queries::ModelPreference) {
        if let Some(directory) = &mut self.model_directory {
            directory.set_preference(preference);
        }
        if self.panel == TuiPanel::Model {
            self.sync_modal_cursor();
        }
    }

    /// Connection flows open the provider-scoped dialog (no sections/extras).
    pub(super) fn open_scoped_model_dialog(&mut self, provider_name: String) {
        self.panel = TuiPanel::Model;
        self.model_scope = Some(provider_name);
        self.select.reset();
    }

    /// A new same-Location composer may remember an actually committed choice;
    /// another session's uncommitted picker draft never crosses this boundary.
    pub fn inherit_committed_model_choice(&mut self, old: &Self) {
        let (Some(current), Some(previous)) =
            (&self.model_selection.binding, &old.model_selection.binding)
        else {
            return;
        };
        if current.location != previous.location || current.provider != previous.provider {
            return;
        }
        for (agent, model) in &old.model_selection.committed_recent {
            self.model_selection.recent.retain(|(owner, choice)| {
                owner != agent || choice.id != model.id || choice.provider != model.provider
            });
            self.model_selection
                .recent
                .push_back((agent.clone(), model.clone()));
            self.remember_committed_model(agent.clone(), model.clone());
        }
        while self.model_selection.recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
            self.model_selection.recent.pop_front();
        }
    }

    fn remember_committed_model(&mut self, agent: Option<String>, model: ModelRef) {
        self.model_selection
            .committed_recent
            .retain(|(owner, choice)| {
                owner != &agent || choice.id != model.id || choice.provider != model.provider
            });
        self.model_selection
            .committed_recent
            .push_back((agent, model));
        while self.model_selection.committed_recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
            self.model_selection.committed_recent.pop_front();
        }
    }
    pub(super) fn composer_catalog(
        &mut self,
        snapshot: &CatalogSnapshot,
    ) -> Option<ComposerModelDraft> {
        let binding = SelectionBinding {
            location: snapshot.chrome.location.clone(),
            generation: snapshot.chrome.selection_generation,
            provider: snapshot.provider.clone(),
            agent_id: snapshot.agent_id.clone(),
        };
        let compatible = self
            .model_selection
            .binding
            .as_ref()
            .is_some_and(|old| old.location == binding.location);
        if !compatible {
            // Per-agent drafts can belong to another provider, but never to
            // another Location. Older captured commits keep their own scope.
            let pending = std::mem::take(&mut self.model_selection.pending);
            self.model_selection = ComposerSelection::default();
            self.model_selection.pending = pending;
        }
        self.model_selection.committed = snapshot.selected_model();
        if !snapshot.model_id.is_empty() {
            self.remember_committed_model(
                binding.agent_id.clone(),
                self.model_selection
                    .committed
                    .clone()
                    .expect("snapshot model"),
            );
        }
        if !snapshot.model_id.is_empty()
            && self
                .model_selection
                .drafts
                .get(&binding.agent_id)
                .is_none_or(|draft| {
                    draft.model.id != snapshot.model_id || draft.model.provider != snapshot.provider
                })
        {
            let model = self
                .model_selection
                .committed
                .clone()
                .expect("snapshot model");
            self.model_selection.recent.retain(|(agent, old)| {
                agent != &binding.agent_id || old.id != model.id || old.provider != model.provider
            });
            self.model_selection
                .recent
                .push_back((binding.agent_id.clone(), model));
            while self.model_selection.recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
                self.model_selection.recent.pop_front();
            }
        }
        let draft = self.model_selection.drafts.get(&binding.agent_id).cloned();
        let mut binding = binding;
        if let Some(draft) = &draft {
            binding.provider = draft.model.provider.clone();
        }
        self.model_selection.binding = Some(binding.clone());
        self.model_selection.picker_binding = Some(binding);
        draft
    }

    /// Capture the visible selection, not the model dialog's browse cursor.
    pub fn captured_model_draft(&self) -> Option<ComposerModelDraft> {
        let picker = self.picker.as_ref()?;
        let model = serde_json::from_str(&picker.persisted_record()?).ok()?;
        Some(ComposerModelDraft {
            model,
            catalog: picker.catalog().clone(),
        })
    }

    /// An unchosen target inherits only a local draft. Its owner snapshot and
    /// any existing target choice remain untouched until explicit admission.
    pub fn inherit_agent_model(&mut self, snapshot: &CatalogSnapshot, draft: ComposerModelDraft) {
        if !snapshot.model_id.is_empty()
            || snapshot.chrome.location != self.chrome.location
            || snapshot.chrome.selection_generation != self.chrome.selection_generation
            || self.model_selection.drafts.contains_key(&snapshot.agent_id)
        {
            return;
        }
        self.model_selection
            .drafts
            .insert(snapshot.agent_id.clone(), draft);
        self.model_selection.revision = self.model_selection.revision.wrapping_add(1);
    }

    pub fn captured_model_commit(&self) -> Option<ModelCommit> {
        let choice: ModelRef =
            serde_json::from_str(&self.picker.as_ref()?.persisted_record()?).ok()?;
        Some(ModelCommit {
            attachment: self.app.attachment(),
            caller: self.model_selection.caller,
            binding: self
                .model_selection
                .picker_binding
                .clone()
                .or_else(|| self.model_selection.binding.clone())?,
            model_id: choice.id,
            variant: choice.variant.filter(|v| v != "default"),
            draft_revision: self.model_selection.revision,
        })
    }

    fn remember_model_draft(&mut self) {
        let Some(commit) = self.captured_model_commit() else {
            return;
        };
        let Some(draft) = self.captured_model_draft() else {
            return;
        };
        let choice = draft.model.clone();
        self.model_selection.revision = self.model_selection.revision.wrapping_add(1);
        self.model_selection
            .drafts
            .insert(commit.binding.agent_id.clone(), draft);
        self.model_selection.recent.retain(|(agent, old)| {
            agent != &commit.binding.agent_id
                || old.id != choice.id
                || old.provider != choice.provider
        });
        self.model_selection
            .recent
            .push_back((commit.binding.agent_id, choice));
        while self.model_selection.recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
            self.model_selection.recent.pop_front();
        }
    }

    pub fn draft_model(&mut self, id: &str) -> Result<(), String> {
        let selecting_model = self.panel == TuiPanel::Model;
        let Some(picker) = self.picker.as_mut() else {
            return Err("model catalog unavailable".into());
        };
        let agent = self
            .model_selection
            .binding
            .as_ref()
            .and_then(|binding| binding.agent_id.clone());
        let variant = self
            .model_selection
            .recent
            .iter()
            .rev()
            .find(|(owner, model)| {
                owner == &agent && model.id == id && model.provider == picker.provider()
            })
            // This process's own choice (even an explicit Default) wins.
            .map(|(_, model)| model.variant.clone())
            // Pinned DialogModel: otherwise a remembered, still available effort
            // is reused instead of asking again (survives restarts).
            .unwrap_or_else(|| {
                self.model_directory
                    .as_ref()
                    .and_then(|directory| directory.remembered_variant(picker.provider(), id))
                    .filter(|variant| picker.has_variant(id, variant))
                    .map(str::to_owned)
            });
        picker
            .choose_id(id, variant.as_deref())
            .map_err(|error| error.to_string())?;
        self.remember_model_draft();
        if selecting_model
            && self.picker.as_ref().is_some_and(|picker| {
                picker.has_variants()
                    && picker
                        .selection()
                        .is_some_and(|selection| selection.variant.is_none())
            })
        {
            self.open_variants();
        } else {
            self.panel = TuiPanel::None;
        }
        self.sync_modal_cursor();
        Ok(())
    }

    /// The drafted model and its variant, for remembering the effort at once.
    pub fn drafted_variant(&self) -> Option<oc_core::queries::ModelPreferenceAction> {
        let picker = self.picker.as_ref()?;
        let selection = picker.selection()?;
        Some(oc_core::queries::ModelPreferenceAction::Variant {
            model: ModelPreferenceModel {
                provider: picker.provider().to_owned(),
                model: selection.id.clone(),
            },
            variant: selection.variant.as_ref().map(|v| v.name.clone()),
        })
    }

    pub fn draft_variant(&mut self, variant: Option<&str>) -> Result<(), String> {
        let picker = self.picker.as_mut().ok_or("model catalog unavailable")?;
        let id = picker
            .selection()
            .ok_or("selected model unavailable")?
            .id
            .clone();
        picker
            .choose_id(&id, variant)
            .map_err(|error| error.to_string())?;
        self.remember_model_draft();
        self.panel = TuiPanel::None;
        self.sync_modal_cursor();
        Ok(())
    }

    /// Returns whether a variant was drafted (no-op without named variants).
    pub fn cycle_variant_draft(&mut self) -> Result<bool, String> {
        let picker = self.picker.as_ref().ok_or("model catalog unavailable")?;
        let Some(selection) = picker.selection() else {
            return Ok(false);
        };
        let named = oc_adapters::models::available_variants(&selection.entry)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        if named.is_empty() {
            return Ok(false);
        }
        let current = picker
            .retired_variant()
            .or_else(|| {
                selection
                    .variant
                    .as_ref()
                    .map(|variant| variant.name.as_str())
            })
            .filter(|name| *name != "default");
        let next = match current {
            None => named.first().cloned(),
            Some(name) => named
                .iter()
                .position(|value| value == name)
                .and_then(|i| named.get(i + 1))
                .cloned(),
        };
        self.draft_variant(next.as_deref()).map(|()| true)
    }

    pub(super) fn commit_composer_model(&mut self) -> Result<(), String> {
        if self.home || self.session.is_none() {
            return Ok(());
        }
        if self.parent_id.is_some() {
            return Err("child session is read-only".into());
        }
        self.commit_captured_model(
            self.session.clone().expect("existing session"),
            self.captured_model_commit(),
        )
    }

    pub(super) fn commit_captured_model(
        &mut self,
        session: SessionId,
        commit: Option<ModelCommit>,
    ) -> Result<(), String> {
        if self.model_selection.pending.len() >= oc_core::session::MAX_QUEUE_ITEMS {
            return Err("model commit receipts full".into());
        }
        let Some(commit) = commit else {
            return Ok(());
        };
        let receipt = self
            .app
            .request_model_commit(session.clone(), commit.clone())
            .map_err(|error| error.to_string())?;
        self.model_selection
            .pending
            .push_back((session, commit, receipt));
        Ok(())
    }

    fn acknowledge_model(&mut self, session: &SessionId, commit: &ModelCommit) {
        if self.session.as_ref() != Some(session)
            || self.app.attachment() != commit.attachment
            || self.model_selection.caller != commit.caller
            || !self
                .model_selection
                .binding
                .as_ref()
                .is_some_and(|binding| {
                    binding.location == commit.binding.location
                        && binding.generation == commit.binding.generation
                        && binding.agent_id == commit.binding.agent_id
                })
            || commit.draft_revision < self.model_selection.acknowledged
        {
            return;
        }
        self.model_selection.acknowledged = commit.draft_revision;
        self.model_selection.binding = Some(commit.binding.clone());
        let model = ModelRef {
            provider: commit.binding.provider.clone(),
            id: commit.model_id.clone(),
            variant: commit.variant.clone(),
        };
        self.model_selection.committed = Some(model.clone());
        self.remember_committed_model(commit.binding.agent_id.clone(), model.clone());
        if self.model_selection.revision == commit.draft_revision
            && self
                .model_selection
                .drafts
                .get(&commit.binding.agent_id)
                .is_some_and(|draft| draft.model == model)
        {
            self.model_selection.drafts.remove(&commit.binding.agent_id);
        }
    }

    pub fn apply_session_model_selected(&mut self, session: &SessionId, commit: &ModelCommit) {
        // An event cannot erase an unmatched local draft. A receipt or admitted
        // submission provides the exact caller/revision association.
        if self
            .model_selection
            .pending
            .iter()
            .any(|(owner, expected, _)| owner == session && expected == commit)
            || self
                .pending
                .as_ref()
                .and_then(|pending| pending.selection.as_ref())
                == Some(commit)
        {
            self.acknowledge_model(session, commit);
        }
    }

    pub(super) fn poll_model_commits(&mut self) {
        loop {
            let Some((session, commit, receipt)) = self.model_selection.pending.front_mut() else {
                break;
            };
            let Some(result) = receipt.try_result() else {
                break;
            };
            let (session, commit) = (session.clone(), commit.clone());
            self.model_selection.pending.pop_front();
            match result {
                Ok(snapshot)
                    if snapshot.chrome.location == commit.binding.location
                        && snapshot.chrome.selection_generation == commit.binding.generation
                        && snapshot.provider == commit.binding.provider
                        && snapshot.agent_id == commit.binding.agent_id
                        && snapshot.model_id == commit.model_id
                        && snapshot.variant.as_deref().filter(|v| *v != "default")
                            == commit.variant.as_deref() =>
                {
                    self.acknowledge_model(&session, &commit)
                }
                Ok(_) => self.push_note("model commit: stale acknowledgement"),
                Err(error) => self.push_note(&format!("model commit: {error}")),
            }
        }
    }

    pub(super) fn model_submission_accepted(
        &mut self,
        session: &SessionId,
        commit: Option<&ModelCommit>,
    ) {
        if let Some(commit) = commit {
            self.acknowledge_model(session, commit);
        }
    }
}
