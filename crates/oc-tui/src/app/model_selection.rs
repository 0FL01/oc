//! Composer drafts and bounded owner receipts. This state never dispatches a
//! generation or infers a committed choice from a menu/stream/footer.
use super::*;
use oc_core::queries::{ModelCommit, ModelRef, SelectionBinding};
use std::collections::{BTreeMap, VecDeque};

#[cfg(test)]
#[path = "model_selection_tests.rs"]
mod tests;

pub(super) struct ComposerSelection {
    caller: u64,
    binding: Option<SelectionBinding>,
    drafts: BTreeMap<Option<String>, ModelRef>,
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
    pub(super) fn account_provider(&self) -> String {
        self.model_selection
            .binding
            .as_ref()
            .map(|b| b.provider.clone())
            .unwrap_or_else(|| "opencode-go".into())
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
            self.model_selection
                .recent
                .retain(|(owner, choice)| owner != agent || choice.id != model.id);
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
            .retain(|(owner, choice)| owner != &agent || choice.id != model.id);
        self.model_selection
            .committed_recent
            .push_back((agent, model));
        while self.model_selection.committed_recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
            self.model_selection.committed_recent.pop_front();
        }
    }
    pub(super) fn composer_catalog(&mut self, snapshot: &CatalogSnapshot) -> Option<ModelRef> {
        let binding = SelectionBinding {
            location: snapshot.chrome.location.clone(),
            generation: snapshot.chrome.selection_generation,
            provider: snapshot.provider.clone(),
            agent_id: snapshot.agent_id.clone(),
        };
        let compatible = self.model_selection.binding.as_ref().is_some_and(|old| {
            old.location == binding.location && old.provider == binding.provider
        });
        if !compatible {
            self.model_selection = ComposerSelection::default();
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
                .is_none_or(|draft| draft.id != snapshot.model_id)
        {
            let model = self
                .model_selection
                .committed
                .clone()
                .expect("snapshot model");
            self.model_selection
                .recent
                .retain(|(agent, old)| agent != &binding.agent_id || old.id != model.id);
            self.model_selection
                .recent
                .push_back((binding.agent_id.clone(), model));
            while self.model_selection.recent.len() > oc_core::session::MAX_QUEUE_ITEMS {
                self.model_selection.recent.pop_front();
            }
        }
        self.model_selection.binding = Some(binding.clone());
        self.model_selection.drafts.get(&binding.agent_id).cloned()
    }

    pub fn captured_model_commit(&self) -> Option<ModelCommit> {
        let choice: ModelRef =
            serde_json::from_str(&self.picker.as_ref()?.persisted_record()?).ok()?;
        Some(ModelCommit {
            caller: self.model_selection.caller,
            binding: self.model_selection.binding.clone()?,
            model_id: choice.id,
            variant: choice.variant.filter(|v| v != "default"),
            draft_revision: self.model_selection.revision,
        })
    }

    fn remember_model_draft(&mut self) {
        let Some(commit) = self.captured_model_commit() else {
            return;
        };
        let choice = ModelRef {
            provider: commit.binding.provider.clone(),
            id: commit.model_id,
            variant: commit.variant,
        };
        self.model_selection.revision = self.model_selection.revision.wrapping_add(1);
        self.model_selection
            .drafts
            .insert(commit.binding.agent_id.clone(), choice.clone());
        self.model_selection
            .recent
            .retain(|(agent, old)| agent != &commit.binding.agent_id || old.id != choice.id);
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
            .find(|(owner, model)| owner == &agent && model.id == id)
            .and_then(|(_, model)| model.variant.clone());
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

    pub fn cycle_variant_draft(&mut self) -> Result<(), String> {
        let picker = self.picker.as_ref().ok_or("model catalog unavailable")?;
        let Some(selection) = picker.selection() else {
            return Ok(());
        };
        let named = oc_adapters::models::available_variants(&selection.entry)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        if named.is_empty() {
            return Ok(());
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
        self.draft_variant(next.as_deref())
    }

    pub(super) fn commit_composer_model(&mut self) -> Result<(), String> {
        if self.home || self.session.is_none() {
            return Ok(());
        }
        if self.parent_id.is_some() {
            return Err("child session is read-only".into());
        }
        if self.model_selection.pending.len() >= oc_core::session::MAX_QUEUE_ITEMS {
            return Err("model commit receipts full".into());
        }
        let Some(commit) = self.captured_model_commit() else {
            return Ok(());
        };
        let session = self.session.clone().expect("existing session");
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
            || self.model_selection.caller != commit.caller
            || self.model_selection.binding.as_ref() != Some(&commit.binding)
            || commit.draft_revision < self.model_selection.acknowledged
        {
            return;
        }
        self.model_selection.acknowledged = commit.draft_revision;
        let model = ModelRef {
            provider: commit.binding.provider.clone(),
            id: commit.model_id.clone(),
            variant: commit.variant.clone(),
        };
        self.model_selection.committed = Some(model.clone());
        self.remember_committed_model(commit.binding.agent_id.clone(), model.clone());
        if self.model_selection.revision == commit.draft_revision
            && self.model_selection.drafts.get(&commit.binding.agent_id) == Some(&model)
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
