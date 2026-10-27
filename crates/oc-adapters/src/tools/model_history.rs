//! Model-compatible projection of the retained journal; never mutates raw input.
use super::*;

impl TurnLog {
    pub(crate) fn input_for(&self, model: &str, provider: &str) -> Vec<crate::provider::InputItem> {
        self.input_for_bound(model, provider, None, None)
    }

    pub(crate) fn input_for_bound(
        &self,
        model: &str,
        provider: &str,
        binding: Option<&oc_core::queries::WireProvenance>,
        profile: Option<&oc_core::queries::RequestProfile>,
    ) -> Vec<crate::provider::InputItem> {
        let mut input = self
            .working
            .as_ref()
            .and_then(|w| Self::from_json(w).ok())
            .map_or_else(Vec::new, |w| {
                w.input_for_bound(model, provider, binding, profile)
            });
        input.extend(self.input.iter().enumerate().filter_map(|(index, item)| {
            self.compatible_item(index, item, model, provider, binding, profile)
        }));
        input
    }

    pub(crate) fn instruction_input_for_bound(
        &self,
        model: &str,
        provider: &str,
        facts: &[crate::instructions::Fact],
        binding: Option<&oc_core::queries::WireProvenance>,
        profile: Option<&oc_core::queries::RequestProfile>,
    ) -> Vec<crate::provider::InputItem> {
        let mut input = self
            .working
            .as_ref()
            .and_then(|w| Self::from_json(w).ok())
            .map_or_else(Vec::new, |w| {
                w.instruction_input_for_bound(model, provider, facts, binding, profile)
            });
        input.extend(crate::instructions::project_items(
            &self.input,
            &self.instruction_references,
            facts,
            |index, item| self.compatible_item(index, item, model, provider, binding, profile),
        ));
        input
    }

    /// The latest issued receipt owns the current batch, even when its profile
    /// differs from the turn's original display attribution.
    pub(crate) fn active_agent_digest(&self) -> Option<&str> {
        self.requests
            .last()
            .and_then(|r| r.profile.as_ref())
            .map_or(self.agent_digest.as_deref(), |profile| {
                profile.agent_digest.as_deref()
            })
    }

    fn compatible_item(
        &self,
        index: usize,
        item: &crate::provider::InputItem,
        model: &str,
        provider: &str,
        binding: Option<&oc_core::queries::WireProvenance>,
        profile: Option<&oc_core::queries::RequestProfile>,
    ) -> Option<crate::provider::InputItem> {
        let origin = self
            .requests
            .iter()
            .rev()
            .find(|request| request.input_start <= self.original_input_index(index));
        let compatible = binding.is_some_and(|binding| {
            origin.map_or(
                self.model == model
                    && self.provider == provider
                    && self.binding.as_ref() == Some(binding),
                |request| {
                    request.model.id == model
                        && request.model.provider == provider
                        && request.binding.as_ref() == Some(binding)
                },
            )
        });
        let compatible_profile = profile.is_none_or(|profile| {
            origin.and_then(|r| r.profile.as_ref()).map_or_else(
                || {
                    self.agent_digest == profile.agent_digest
                        // Pre-Build sessions used the unchanged empty builtin
                        // lane without a digest; do not generalize to custom profiles.
                        || (self.agent_digest.is_none()
                            && profile.agent_digest.as_deref()
                                == Some(crate::defs::agent_digest(&crate::defs::builtin_build()).as_str()))
                },
                |origin| origin == profile,
            )
        });
        if compatible && compatible_profile {
            return Some(item.clone());
        }
        neutral(item)
    }
}

pub(crate) fn neutral(item: &crate::provider::InputItem) -> Option<crate::provider::InputItem> {
    use crate::provider::{InputItem, InputRole};
    let InputItem::ProviderOutput(value) = item else {
        return Some(item.clone());
    };
    match value["type"].as_str()? {
        "function_call" => Some(InputItem::ProviderOutput(serde_json::json!({
            "type":"function_call", "call_id":value["call_id"], "name":value["name"], "arguments":value["arguments"],
        }))),
        "message" if value["role"] == "assistant" => {
            let text = value["content"]
                .as_array()?
                .iter()
                .filter(|part| matches!(part["type"].as_str(), Some("output_text" | "text")))
                .filter_map(|part| part["text"].as_str())
                .collect::<String>();
            (!text.is_empty()).then(|| InputItem::message(InputRole::Assistant, text))
        }
        "reasoning" => {
            let mut text = value["summary"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>();
            if text.is_empty() {
                text = value["text"]
                    .as_str()
                    .or_else(|| {
                        value
                            .pointer("/messages_thinking/thinking")
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or_default()
                    .into();
            }
            (!text.is_empty()).then(|| InputItem::message(InputRole::Assistant, text))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_history_tests.rs"]
mod tests;
