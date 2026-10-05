//! Pinned v2.0.12 config semantics (schema/config/compaction.ts and normalize.ts).
use oc_core::queries::{ConfigDiagnostic, ConfigDiagnosticAction, ConfigDiagnosticKind};

/// Explicit adapter capability. No model names are interpreted by core.
pub trait NativeCompaction: Send + Sync {
    fn compact<'a>(
        &'a self,
        route: &'a str,
        input: &'a [crate::provider::InputItem],
        cancel: &'a std::sync::atomic::AtomicBool,
        // Invoke immediately before each actual physical send, never for None.
        dispatch: &'a mut (dyn FnMut() -> Result<(), crate::provider::ProviderError> + Send),
    ) -> std::pin::Pin<
        Box<
            dyn Future<Output = Result<Option<NativeCheckpoint>, crate::provider::ProviderError>>
                + Send
                + 'a,
        >,
    >;
}
pub struct NativeCheckpoint {
    /// Exact prepared route identity returned by the registered mechanism.
    pub route: String,
    /// Real opaque replacement item supplied by the provider mechanism.
    pub replacement: serde_json::Value,
    pub usage: Option<oc_core::compaction::CompactionUsage>,
}
/// Measured primary input plus output, bound to an unchanged causal prefix.
/// Tool results and new user input after that prefix are estimated separately.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct UsageAnchor {
    pub scope: String,
    pub prefix: String,
    pub items: usize,
    pub tokens: u64,
}

pub(crate) fn fingerprint(value: &impl serde::Serialize) -> String {
    use sha2::{Digest as _, Sha256};
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("context serialization"))
    )
}

impl UsageAnchor {
    pub fn estimate(&self, scope: &str, input: &[crate::provider::InputItem]) -> Option<u64> {
        let prefix = input.get(..self.items)?;
        if self.scope != scope || self.prefix != fingerprint(&prefix) {
            return None;
        }
        let added = estimate_tail(&input[self.items..]);
        Some(self.tokens.saturating_add(added))
    }
}

/// Pinned util/token.ts uses JS string length / 4 rounded to nearest.
/// Count content rather than JSON wire envelopes, ids, or opaque ciphertext.
fn estimate_tail(input: &[crate::provider::InputItem]) -> u64 {
    use crate::provider::{InputContent, InputItem};
    let text = |s: &str| (s.encode_utf16().count() as u64).saturating_add(2) / 4;
    input.iter().fold(0u64, |total, item| {
        let tokens = match item {
            InputItem::EffortUpdate { .. } => 0,
            InputItem::Message { content, .. } => content
                .iter()
                .map(|part| match part {
                    InputContent::InputText { text: s } | InputContent::OutputText { text: s } => {
                        text(s)
                    }
                    InputContent::InputImage { .. } => 1_500,
                })
                .sum(),
            InputItem::FunctionCallOutput { output, .. } => text(output),
            InputItem::McpFunctionCallOutput { output, .. } => output.estimated_tokens(),
            InputItem::ReadFunctionCallOutput { output, .. } => output.estimated_tokens(),
            InputItem::ProviderOutput(value) => match value["type"].as_str() {
                Some("function_call") => text(&format!(
                    "{}{}",
                    value["name"].as_str().unwrap_or_default(),
                    value["arguments"].as_str().unwrap_or_default()
                )),
                Some("message" | "reasoning") => value
                    .get("content")
                    .or_else(|| value.get("summary"))
                    .and_then(|parts| parts.as_array())
                    .map(|parts| {
                        parts
                            .iter()
                            .map(|p| text(p["text"].as_str().unwrap_or_default()))
                            .sum()
                    })
                    .unwrap_or(0),
                // Native opaque replacements have no locally measurable size.
                _ => 0,
            },
        };
        total.saturating_add(tokens)
    })
}

/// Pinned estimateTokens without a reported primary usage anchor: content of
/// messages plus initial instructions and actual offered tool schemas.
pub(crate) fn estimate_context(
    fixed: &[crate::provider::InputItem],
    history: &[crate::provider::InputItem],
    pending: &[crate::provider::InputItem],
    tools: &[crate::provider::ToolDef],
) -> u64 {
    tools.iter().fold(
        estimate_tail(fixed)
            .saturating_add(estimate_tail(history))
            .saturating_add(estimate_tail(pending)),
        |sum, tool| {
            let text = format!("{}{}{}", tool.name, tool.description, tool.parameters);
            sum.saturating_add((text.encode_utf16().count() as u64).saturating_add(2) / 4)
        },
    )
}

pub fn route_identity(
    provider: &str,
    model: &str,
    config: &crate::provider::ResponsesConfig,
) -> Result<String, crate::provider::ProviderError> {
    use sha2::{Digest as _, Sha256};
    // Use exactly the adapter's effective case-insensitive header semantics,
    // including credential/tenant scope. Only the digest is ever persisted.
    let headers = crate::provider::request_headers(config)?;
    let headers = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_bytes()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let value = serde_json::to_vec(&(
        "responses-route-v2",
        provider,
        model,
        config.base_url.trim_end_matches('/'),
        headers,
        config.set_cache_key,
    ))
    .expect("route serialization");
    Ok(format!("{:x}", Sha256::digest(value)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionConfig {
    pub auto: bool,
    pub keep_tokens: u64,
    pub buffer: u64,
}
impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto: true,
            keep_tokens: 15_000,
            buffer: 20_000,
        }
    }
}
impl CompactionConfig {
    pub fn merge(&mut self, value: &serde_json::Value) -> Vec<ConfigDiagnostic> {
        self.merge_from("", value).1
    }
    pub(crate) fn merge_from(
        &mut self,
        source: &str,
        value: &serde_json::Value,
    ) -> (Option<serde_json::Value>, Vec<ConfigDiagnostic>) {
        let (normalized, notes) = normalize_compaction(source, value);
        if let Some(value) = &normalized {
            if let Some(auto) = value["auto"].as_bool() {
                self.auto = auto;
            }
            if let Some(tokens) = value.pointer("/keep/tokens").and_then(|v| v.as_u64()) {
                self.keep_tokens = tokens;
            }
            if let Some(buffer) = value["buffer"].as_u64() {
                self.buffer = buffer;
            }
        }
        (normalized, notes)
    }
    pub fn required(
        &self,
        input: u64,
        context: u64,
        output: u64,
        input_limit: Option<u64>,
    ) -> bool {
        let ceiling = context
            .saturating_sub(output.min(32_000).max(self.buffer))
            .min(
                input_limit
                    .map(|limit| limit.saturating_sub(self.buffer))
                    .unwrap_or(u64::MAX),
            );
        self.auto && context > 0 && input >= ceiling
    }
}

/// Narrow pinned normalize.ts:318–374 port. Unknown keys are silently omitted;
/// malformed recognized leaves do not erase valid neighbors or lower layers.
pub(crate) fn normalize_compaction(
    source: &str,
    value: &serde_json::Value,
) -> (Option<serde_json::Value>, Vec<ConfigDiagnostic>) {
    use serde_json::{Map, Value};
    let mut notes = Vec::new();
    let note = |notes: &mut Vec<ConfigDiagnostic>, field: &[&str], kind| {
        notes.push(ConfigDiagnostic {
            source: source.into(),
            field: field.iter().map(|s| (*s).into()).collect(),
            kind,
            action: if kind == ConfigDiagnosticKind::Conflict {
                ConfigDiagnosticAction::RetainNative
            } else {
                ConfigDiagnosticAction::Skip
            },
        })
    };
    let Some(obj) = value.as_object() else {
        note(&mut notes, &["compaction"], ConfigDiagnosticKind::Invalid);
        return (None, notes);
    };
    for key in ["tail_turns", "prune"] {
        if obj.contains_key(key) {
            note(
                &mut notes,
                &["compaction", key],
                ConfigDiagnosticKind::Unsupported,
            );
        }
    }
    let mut result = Map::new();
    if let Some(auto) = obj.get("auto") {
        if auto.is_boolean() {
            result.insert("auto".into(), auto.clone());
        } else {
            note(
                &mut notes,
                &["compaction", "auto"],
                ConfigDiagnosticKind::Invalid,
            );
        }
    }
    let integer = |value: Option<&Value>, field: &[&str], notes: &mut Vec<ConfigDiagnostic>| {
        value.and_then(|v| {
            // Effect Schema.Int is a JS safe integer, including encoded 1.0.
            let number = v
                .as_f64()
                .filter(|n| {
                    n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= 9_007_199_254_740_991.0
                })
                .map(|n| n as u64);
            if number.is_none() {
                note(notes, field, ConfigDiagnosticKind::Invalid);
            }
            number
        })
    };
    let legacy = integer(
        obj.get("preserve_recent_tokens"),
        &["compaction", "preserve_recent_tokens"],
        &mut notes,
    );
    let keep = obj.get("keep").and_then(|v| v.as_object());
    if obj.contains_key("keep") && keep.is_none() {
        note(
            &mut notes,
            &["compaction", "keep"],
            ConfigDiagnosticKind::Invalid,
        );
    }
    let native = integer(
        keep.and_then(|k| k.get("tokens")),
        &["compaction", "keep", "tokens"],
        &mut notes,
    );
    if native.is_some() && legacy.is_some() && native != legacy {
        note(
            &mut notes,
            &["compaction", "keep", "tokens"],
            ConfigDiagnosticKind::Conflict,
        );
    }
    if let Some(tokens) = native.or(legacy) {
        result.insert("keep".into(), serde_json::json!({"tokens":tokens}));
    }
    let legacy = integer(obj.get("reserved"), &["compaction", "reserved"], &mut notes);
    let native = integer(obj.get("buffer"), &["compaction", "buffer"], &mut notes);
    if native.is_some() && legacy.is_some() && native != legacy {
        note(
            &mut notes,
            &["compaction", "buffer"],
            ConfigDiagnosticKind::Conflict,
        );
    }
    if let Some(buffer) = native.or(legacy) {
        result.insert("buffer".into(), buffer.into());
    }
    (
        (!result.is_empty() || obj.is_empty()).then_some(Value::Object(result)),
        notes,
    )
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use crate::provider::{InputItem, InputRole};

    #[test]
    fn compaction_pinned_normalization_differential_fixtures_and_layers() {
        use serde_json::json;
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/compaction_normalization.json"
        ))
        .unwrap();
        for case in fixtures["cases"].as_array().unwrap() {
            let (normalized, notes) = normalize_compaction("fixture", &case["input"]);
            let encoded = normalized
                .map(|c| json!({"compaction":c}))
                .unwrap_or(json!({}));
            let diagnostics: Vec<_> = notes
                .iter()
                .map(|n| {
                    assert_eq!(n.source, "fixture");
                    assert_eq!(
                        n.action,
                        if n.kind == ConfigDiagnosticKind::Conflict {
                            ConfigDiagnosticAction::RetainNative
                        } else {
                            ConfigDiagnosticAction::Skip
                        }
                    );
                    json!({"kind":n.kind,"path":n.field,"message":n.message()})
                })
                .collect();
            assert_eq!(encoded, case["encoded"], "{}", case["name"]);
            assert_eq!(json!(diagnostics), case["diagnostics"], "{}", case["name"]);
        }
        let sources = [
            crate::config::Source { path:"global".into(),trusted:true,text:json!({"compaction":{"auto":false,"keep":{"tokens":11},"buffer":12}}).to_string() },
            crate::config::Source { path:"project".into(),trusted:true,text:json!({"compaction":{"auto":null,"preserve_recent_tokens":13,"keep":{"tokens":-1},"reserved":14,"buffer":15,"prune":true,"future":"SECRET-VALUE"}}).to_string() },
            crate::config::Source { path:"last".into(),trusted:true,text:json!({"compaction":{"auto":"SECRET-VALUE","buffer":false}}).to_string() },
        ];
        let generation = crate::config::assemble(&sources, &Default::default(), None).unwrap();
        assert_eq!(
            (
                generation.compaction.auto,
                generation.compaction.keep_tokens,
                generation.compaction.buffer
            ),
            (false, 13, 15)
        );
        assert_eq!(generation.provenance["compaction.auto"], "global");
        assert_eq!(generation.provenance["compaction.keep.tokens"], "project");
        assert_eq!(generation.provenance["compaction.buffer"], "project");
        assert_eq!(generation.provenance["compaction"], "project");
        assert_eq!(
            generation
                .config_diagnostics
                .iter()
                .map(|n| n.source.as_str())
                .collect::<Vec<_>>(),
            vec!["project", "project", "project", "project", "last", "last"]
        );
        assert!(
            !serde_json::to_string(&generation.config_diagnostics)
                .unwrap()
                .contains("SECRET-VALUE")
        );
        assert_eq!(
            CompactionConfig::default(),
            CompactionConfig {
                auto: true,
                keep_tokens: 15_000,
                buffer: 20_000
            }
        );
        let bad = [crate::config::Source {
            path: "bad".into(),
            trusted: true,
            text: json!({"compaction":{"prune":true},"snapshot":true}).to_string(),
        }];
        assert!(crate::config::assemble(&bad, &Default::default(), None).is_err());
    }

    #[test]
    fn compaction_content_fallback_and_exact_limit_boundary() {
        let fixed = [InputItem::message(InputRole::Developer, "12345678")];
        let history = [InputItem::message(InputRole::User, "1234")];
        let pending = [InputItem::FunctionCallOutput {
            call_id: "unmeasured-envelope-id".into(),
            output: "123456789012".into(),
        }];
        let tools = [crate::provider::ToolDef {
            name: "name".into(),
            description: "desc".into(),
            parameters: serde_json::json!({}),
        }];
        assert_eq!(estimate_context(&fixed, &history, &pending, &tools), 9); // 2+1+3+round(10/4)
        let c = CompactionConfig::default();
        assert!(!c.required(19_999, 40_000, 2_048, None));
        assert!(c.required(20_000, 40_000, 2_048, None));
        assert!(!c.required(9_999, 40_000, 2_048, Some(30_000)));
        assert!(c.required(10_000, 40_000, 2_048, Some(30_000)));
        assert!(!c.required(67_999, 100_000, 100_000, None));
        assert!(c.required(68_000, 100_000, 100_000, None));
        assert!(!c.required(u64::MAX, 0, 2_048, None));
    }

    #[test]
    fn compaction_usage_anchor_counts_only_new_parts_and_refuses_changed_prefix() {
        let mut input = vec![
            InputItem::message(InputRole::User, "measured request"),
            InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","name":"read","arguments":"{}","call_id":"call"}),
            ),
        ];
        let anchor = UsageAnchor {
            scope: "route/model/instructions/tools".into(),
            prefix: fingerprint(&input),
            items: input.len(),
            tokens: 24800,
        };
        input.push(InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: "new tool result".into(),
        });
        input.push(InputItem::message(InputRole::User, "next"));
        assert_eq!(anchor.estimate(&anchor.scope, &input), Some(24805));
        assert_eq!(anchor.estimate("different-model", &input), None);
        input[0] = InputItem::message(InputRole::User, "DCP changed request");
        assert_eq!(anchor.estimate(&anchor.scope, &input), None);
        assert!(!CompactionConfig::default().required(24800, 0, 2048, None));
    }
}
