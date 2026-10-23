//! Auxiliary input-history receipts never block the sole input/draw task.
use super::*;
use oc_core::core_app::PromptHistoryReceipt;

struct Append {
    text: String,
    receipt: PromptHistoryReceipt,
}
struct Recall {
    generation: u64,
    revision: u64,
    count: usize,
    receipt: PromptHistoryReceipt,
}
#[derive(Default)]
pub(super) struct Receipts {
    append: Vec<Append>,
    failed: Vec<String>,
    recall: Option<Recall>,
}
const MAX_RECEIPTS: usize = 16;

impl TuiState {
    pub fn has_pending_prompt_history(&self) -> bool {
        !self.history_receipts.append.is_empty() || self.history_receipts.recall.is_some()
    }

    pub(super) fn remember_prompt_history(
        &mut self,
        text: String,
    ) -> Result<(), oc_core::session::CoreError> {
        if self.history_receipts.append.len() + self.history_receipts.failed.len() >= MAX_RECEIPTS {
            return Err(oc_core::session::CoreError::QueueFull);
        }
        let receipt = match self.app.request_prompt_history(Some(text.clone())) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.history_receipts.failed.push(text);
                return Err(error);
            }
        };
        self.history_receipts.append.push(Append { text, receipt });
        Ok(())
    }

    pub fn poll_prompt_history(&mut self) -> bool {
        let mut pending = std::mem::take(&mut self.history_receipts.append);
        let mut changed = false;
        for mut append in pending.drain(..) {
            match append.receipt.try_result() {
                None => self.history_receipts.append.push(append),
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    self.history_receipts.failed.push(append.text);
                    self.push_note(&format!(
                        "prompt history: {error}; Up recalls retained input"
                    ));
                    changed = true;
                }
            }
        }
        if let Some(mut recall) = self.history_receipts.recall.take() {
            match recall.receipt.try_result() {
                None => self.history_receipts.recall = Some(recall),
                Some(Ok(entries))
                    if recall.generation == self.generation
                        && recall.revision == self.input_revision
                        && self.prompt_history_key_owner() =>
                {
                    for index in 0..recall.count {
                        changed |= self.apply_prompt_recall(
                            true,
                            if index == 0 {
                                entries.clone()
                            } else {
                                Vec::new()
                            },
                        );
                    }
                }
                Some(Err(error)) => {
                    self.push_note(&format!("prompt history: {error}"));
                    changed = true;
                }
                Some(Ok(_)) => {}
            }
        }
        changed
    }

    pub(super) fn start_prompt_recall(
        &mut self,
        previous: bool,
    ) -> Result<bool, oc_core::session::CoreError> {
        if !self.prompt_history_key_owner() {
            return Ok(false);
        }
        if self.editor.browsing_history() {
            return Ok(self.apply_prompt_recall(previous, Vec::new()));
        }
        if !previous {
            self.history_receipts.recall = None;
            return Ok(false);
        }
        if !self.history_receipts.failed.is_empty() {
            let entries = std::mem::take(&mut self.history_receipts.failed);
            return Ok(self.apply_prompt_recall(true, entries));
        }
        if let Some(recall) = &mut self.history_receipts.recall
            && recall.generation == self.generation
            && recall.revision == self.input_revision
        {
            recall.count = (recall.count + 1).min(oc_core::queries::MAX_PROMPT_HISTORY_ENTRIES);
            return Ok(true);
        }
        self.history_receipts.recall = Some(Recall {
            generation: self.generation,
            revision: self.input_revision,
            count: 1,
            receipt: self.app.request_prompt_history(None)?,
        });
        Ok(true)
    }

    fn apply_prompt_recall(&mut self, previous: bool, entries: Vec<String>) -> bool {
        let changed = self.editor.recall(&mut self.input, previous, entries);
        if changed {
            self.editor
                .move_to(if previous { 0 } else { self.input.len() }, false);
            self.input_revision += 1;
            self.slash_selected = 0;
            self.clear_mentions();
        }
        changed
    }
}
