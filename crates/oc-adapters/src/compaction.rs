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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noncausal_tokens: Option<u64>,
}

pub(crate) fn fingerprint(value: &impl serde::Serialize) -> String {
    use sha2::{Digest as _, Sha256};
    struct Writer(Sha256);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Sha256::new());
    serde_json::to_writer(&mut writer, value).expect("context serialization");
    format!("{:x}", writer.0.finalize())
}

/// Usage-prefix identity. DCP compress IDs are request-local affordances
/// (present only while compress is available), so they never change measured
/// identity; an anchor recorded before IDs existed still matches.
pub(crate) fn prefix_fingerprint(items: &[&crate::provider::InputItem]) -> String {
    fingerprint(
        &items
            .iter()
            .map(|item| crate::dcp::without_message_ids(item))
            .collect::<Vec<_>>(),
    )
}

impl UsageAnchor {
    pub fn suffix<'a, 'i>(
        &self,
        scope: &str,
        input: &'a [&'i crate::provider::InputItem],
    ) -> Option<&'a [&'i crate::provider::InputItem]> {
        let prefix = input.get(..self.items)?;
        if self.scope != scope || self.prefix != prefix_fingerprint(prefix) {
            return None;
        }
        // Only this prefix is measured. Runtime admission must price every new
        // item, including opaque output from a later response without usage.
        Some(&input[self.items..])
    }
}

pub fn route_identity(
    provider: &str,
    model: &str,
    config: &crate::provider::ResponsesConfig,
) -> Result<String, crate::provider::ProviderError> {
    let config = config.for_selection(model, None);
    Ok(route_for_binding(
        model,
        &config.provenance(provider, model)?,
    ))
}

/// Cache placement and per-session affinity are not replay authority. Only
/// captured wire provenance can authorize a native checkpoint; old route
/// versions without these facts intentionally cannot match.
pub(crate) fn route_for_binding(model: &str, binding: &oc_core::queries::WireProvenance) -> String {
    fingerprint(&("native-route-v4", model, binding))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionConfig {
    pub auto: bool,
    pub keep_tokens: u64,
    /// Explicit free room before automatic compaction; unset keeps the
    /// pinned share-of-window reserve.
    pub buffer: Option<u64>,
}
impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto: true,
            keep_tokens: 15_000,
            buffer: None,
        }
    }
}
/// Pinned compaction.ts RESERVE_SHARE: share of the window kept free.
const RESERVE_SHARE_PERCENT: u64 = 15;
/// Pinned compaction.ts RESERVE_MIN.
const RESERVE_MIN: u64 = 16_000;
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
                self.buffer = Some(buffer);
            }
        }
        (normalized, notes)
    }
    /// Pinned compaction.ts `calculateCeiling`: the window is the input limit,
    /// else the context. The advertised output cap is a ceiling for the reply,
    /// not room to keep free, so it does not move the threshold. An unknown
    /// window never triggers automatic compaction.
    pub fn required(&self, input: u64, context: u64, input_limit: Option<u64>) -> bool {
        let window = input_limit.filter(|limit| *limit > 0).unwrap_or(context);
        if !self.auto || window == 0 {
            return false;
        }
        let ceiling = match self.buffer {
            // A buffer that leaves no room, such as one set for a larger model, is ignored.
            Some(buffer) if buffer < window => window - buffer,
            _ => window.saturating_sub((window * RESERVE_SHARE_PERCENT / 100).max(
                if window >= 2 * RESERVE_MIN {
                    RESERVE_MIN
                } else {
                    0
                },
            )),
        };
        input >= ceiling
    }
    pub(crate) fn required_for_budget(
        &self,
        input: u64,
        context: u64,
        input_limit: Option<u64>,
        admitted_input: u64,
    ) -> bool {
        // The soft ceiling is clamped to admission, which already keeps the
        // minimum reply and safety margin free.
        self.required(input, context, input_limit)
            || (self.auto && context > 0 && input >= admitted_input)
    }
}

/// Canonical OpenCode 2 compaction (`auto`, `keep.tokens`, `buffer`). Keys outside
/// the schema are refused by the T66 allowlist before assembly; malformed
/// recognized leaves do not erase valid neighbors or lower layers.
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
    let keep = obj.get("keep").and_then(|v| v.as_object());
    if obj.contains_key("keep") && keep.is_none() {
        note(
            &mut notes,
            &["compaction", "keep"],
            ConfigDiagnosticKind::Invalid,
        );
    }
    if let Some(tokens) = integer(
        keep.and_then(|k| k.get("tokens")),
        &["compaction", "keep", "tokens"],
        &mut notes,
    ) {
        result.insert("keep".into(), serde_json::json!({"tokens":tokens}));
    }
    if let Some(buffer) = integer(obj.get("buffer"), &["compaction", "buffer"], &mut notes) {
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
        // Donor cases with keys outside the OC2 schema are refused by admission.
        let canonical = ["auto", "keep", "buffer"];
        for case in fixtures["cases"].as_array().unwrap().iter().filter(|case| {
            !case["input"]
                .as_object()
                .is_some_and(|input| input.keys().any(|key| !canonical.contains(&key.as_str())))
        }) {
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
            crate::config::Source {
                path: "global".into(),
                trusted: true,
                text: json!({"compaction":{"auto":false,"keep":{"tokens":11},"buffer":12}})
                    .to_string(),
            },
            crate::config::Source {
                path: "project".into(),
                trusted: true,
                text: json!({"compaction":{"auto":null,"keep":{"tokens":13},"buffer":15}})
                    .to_string(),
            },
            crate::config::Source {
                path: "last".into(),
                trusted: true,
                text: json!({"compaction":{"auto":"SECRET-VALUE","buffer":false}}).to_string(),
            },
        ];
        let generation = crate::config::assemble(&sources, &Default::default(), None).unwrap();
        assert_eq!(
            (
                generation.compaction.auto,
                generation.compaction.keep_tokens,
                generation.compaction.buffer
            ),
            (false, 13, Some(15))
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
            vec!["project", "last", "last"]
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
                buffer: None
            }
        );
        let bad = [crate::config::Source {
            path: "bad".into(),
            trusted: true,
            text: json!({"compaction":{"auto":true},"snapshots":true}).to_string(),
        }];
        assert!(crate::config::assemble(&bad, &Default::default(), None).is_err());
    }

    #[test]
    fn compaction_exact_limit_boundary() {
        // Pinned compaction.ts calculateCeiling: max(15% of window, 16K) is
        // kept free; the advertised output cap does not move the threshold.
        let c = CompactionConfig::default();
        assert!(!c.required(23_999, 40_000, None));
        assert!(c.required(24_000, 40_000, None));
        assert!(
            !c.required(25_499, 40_000, Some(30_000)),
            "small window keeps only the share"
        );
        assert!(c.required(25_500, 40_000, Some(30_000)));
        assert!(!c.required(424_999, 500_000, None));
        assert!(c.required(425_000, 500_000, None));
        assert!(!c.required(849_999, 1_000_000, None));
        assert!(c.required(850_000, 1_000_000, None));
        assert!(
            !c.required(u64::MAX, 0, None),
            "an unknown window never auto-compacts"
        );
        let buffered = CompactionConfig {
            buffer: Some(20_000),
            ..c.clone()
        };
        assert!(!buffered.required(479_999, 500_000, None));
        assert!(buffered.required(480_000, 500_000, None));
        let oversized = CompactionConfig {
            buffer: Some(500_000),
            ..c.clone()
        };
        assert!(
            oversized.required(425_000, 500_000, None),
            "a buffer leaving no room is ignored"
        );
        assert!(c.required_for_budget(448_976, 500_000, Some(450_000), 448_976));
        assert!(!c.required_for_budget(70_000, 100_000, None, 97_952));
        let no_buffer = CompactionConfig {
            buffer: Some(0),
            ..c
        };
        assert!(no_buffer.required_for_budget(448_976, 500_000, Some(450_000), 448_976));
    }

    #[test]
    fn usage_prefix_identity_ignores_compress_ids() {
        // An anchor measured before compress IDs existed (or while compress
        // was unavailable) still matches the tagged next request.
        let plain = InputItem::message(InputRole::User, "measured request");
        let anchor = UsageAnchor {
            scope: "scope".into(),
            prefix: fingerprint(&[&plain]),
            items: 1,
            tokens: 10,
            noncausal_tokens: Some(0),
        };
        let mut tagged = plain.clone();
        if let InputItem::Message { content, .. } = &mut tagged {
            content.push(crate::provider::InputContent::InputText {
                text: crate::dcp::message_id_tag("m0001"),
            });
        }
        let next = InputItem::message(InputRole::User, "next");
        assert_eq!(
            anchor.suffix("scope", &[&tagged, &next]),
            Some(&[&next][..])
        );
        let changed = InputItem::message(InputRole::User, "changed");
        assert_eq!(anchor.suffix("scope", &[&changed, &next]), None);
        // Authored text shaped like a tag is content, never a runtime ID.
        let authored = InputItem::message(InputRole::User, crate::dcp::message_id_tag("m9"));
        assert!(matches!(
            crate::dcp::without_message_ids(&authored),
            std::borrow::Cow::Borrowed(_)
        ));
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
            noncausal_tokens: Some(0),
        };
        input.push(InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: "new tool result".into(),
        });
        input.push(InputItem::message(InputRole::User, "next"));
        let refs = input.iter().collect::<Vec<_>>();
        assert_eq!(anchor.suffix(&anchor.scope, &refs), Some(&refs[2..]));
        assert_eq!(anchor.suffix("different-model", &refs), None);
        input[0] = InputItem::message(InputRole::User, "DCP changed request");
        assert_eq!(
            anchor.suffix(&anchor.scope, &input.iter().collect::<Vec<_>>()),
            None
        );
        assert!(!CompactionConfig::default().required(24800, 0, None));
    }
}
