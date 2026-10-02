//! Model-compatible projection of the retained journal; never mutates raw input.
use super::*;

impl TurnLog {
    pub(crate) fn input_for(&self, model: &str, provider: &str) -> Vec<crate::provider::InputItem> {
        let mut input = self
            .working
            .as_ref()
            .and_then(|w| Self::from_json(w).ok())
            .map_or_else(Vec::new, |w| w.input_for(model, provider));
        input.extend(
            self.input
                .iter()
                .enumerate()
                .filter_map(|(index, item)| self.compatible_item(index, item, model, provider)),
        );
        input
    }

    pub(crate) fn instruction_input_for(
        &self,
        model: &str,
        provider: &str,
        facts: &[crate::instructions::Fact],
    ) -> Vec<crate::provider::InputItem> {
        let mut input = self
            .working
            .as_ref()
            .and_then(|w| Self::from_json(w).ok())
            .map_or_else(Vec::new, |w| {
                w.instruction_input_for(model, provider, facts)
            });
        input.extend(crate::instructions::project_items(
            &self.input,
            &self.instruction_references,
            facts,
            |index, item| self.compatible_item(index, item, model, provider),
        ));
        input
    }

    fn compatible_item(
        &self,
        index: usize,
        item: &crate::provider::InputItem,
        model: &str,
        provider: &str,
    ) -> Option<crate::provider::InputItem> {
        let origin = self
            .requests
            .iter()
            .rev()
            .find(|request| request.input_start <= self.original_input_index(index));
        let compatible = origin.map_or(
            self.model == model && self.provider == provider,
            |request| request.model.id == model && request.model.provider == provider,
        );
        if compatible {
            return Some(item.clone());
        }
        neutral(item)
    }
}

fn neutral(item: &crate::provider::InputItem) -> Option<crate::provider::InputItem> {
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
            let text = value["summary"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>();
            (!text.is_empty()).then(|| InputItem::message(InputRole::Assistant, text))
        }
        _ => None,
    }
}
