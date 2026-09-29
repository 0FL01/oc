//! Native facts in the existing turn row, not inferred from model output.
use super::TurnLog;
use crate::provider::InputItem;
use serde_json::{Value, json};
use std::collections::BTreeSet;

impl TurnLog {
    pub(super) fn encode_mcp_input(&self) -> (Vec<Value>, Vec<Value>) {
        let mut input = Vec::with_capacity(self.input.len());
        let mut native = Vec::new();
        for (index, item) in self.input.iter().enumerate() {
            if let InputItem::McpFunctionCallOutput { call_id, output } = item {
                input.push(json!({"type":"function_call_output","call_id":call_id,"output":output.display()}));
                native.push(json!({"input_index":index,"call_id":call_id,"result":output.facts()}));
            } else {
                input.push(serde_json::to_value(item).expect("typed input serialization"));
            }
        }
        (input, native)
    }

    pub(super) fn decode_mcp_input(value: &Value) -> Result<Vec<InputItem>, String> {
        let mut input: Vec<InputItem> =
            serde_json::from_value(value.get("input").cloned().unwrap_or_else(|| json!([])))
                .map_err(|_| "invalid wire input")?;
        let Some(native) = value.get("native_mcp_results") else {
            return Ok(input);
        };
        let native = native
            .as_array()
            .ok_or("invalid native MCP result attachment")?;
        if native.len() > input.len() {
            return Err("invalid native MCP result attachment".into());
        }
        let mut seen = BTreeSet::new();
        for result in native {
            let index = result["input_index"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("invalid native MCP result index")?;
            if !seen.insert(index) {
                return Err("duplicate native MCP result attachment".into());
            }
            let Some(InputItem::FunctionCallOutput { call_id, output }) = input.get(index) else {
                return Err("native MCP attachment is not a tool result".into());
            };
            if result["call_id"].as_str() != Some(call_id.as_str()) || !input[..index].iter().any(|i| {
                matches!(i, InputItem::ProviderOutput(v) if v["type"] == "function_call" && v["call_id"].as_str() == Some(call_id.as_str()))
            }) { return Err("native MCP result graph mismatch".into()); }
            let facts = crate::mcp_result::McpToolOutput::from_stored(result["result"].clone())
                .map_err(|_| "invalid native MCP result facts")?;
            if facts.display() != output {
                return Err("native MCP result presentation mismatch".into());
            }
            input[index] = InputItem::McpFunctionCallOutput {
                call_id: call_id.clone(),
                output: facts,
            };
        }
        Ok(input)
    }
}

#[cfg(test)]
mod tests;
