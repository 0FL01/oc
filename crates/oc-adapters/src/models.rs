//! Effective model selection for T15.
//!
//! Exact-id selection over a merged catalog (discovery + static), variant
//! validation against the entry allowlist, context+output admission, unknown
//! metadata diagnostics, and a redacted explain view. There is deliberately
//! no production automatic model fallback: an unknown id or variant is a
//! visible error even when the catalog is non-empty.

use std::collections::BTreeMap;

use thiserror::Error;

/// Standard variant order used for defaults and diagnostics.
pub const STANDARD_VARIANTS: [&str; 7] =
    ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Known model-entry metadata keys; anything else is diagnosed, not dropped.
const KNOWN_KEYS: [&str; 8] = [
    "name",
    "description",
    "limit",
    "modalities",
    "attachment",
    "reasoning",
    "tool_call",
    "variants",
];

/// Typed selection errors (ids only, no secrets).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// Requested model id is not in the catalog.
    #[error("unknown model {id}; available: {available}")]
    UnknownModel {
        /// Requested id.
        id: String,
        /// Sorted available ids (bounded).
        available: String,
    },
    /// Requested variant is missing or disabled.
    #[error("unavailable variant {variant} for {id}; enabled: {enabled}")]
    UnavailableVariant {
        /// Model id.
        id: String,
        /// Requested variant.
        variant: String,
        /// Sorted enabled variants.
        enabled: String,
    },
    /// Entry lacks a usable context+output limit.
    #[error("model {id} has no usable limit")]
    MissingLimit {
        /// Model id.
        id: String,
    },
    /// Estimated input exceeds the context limit.
    #[error("model {id} input {input} exceeds context {context}")]
    OverContext {
        /// Model id.
        id: String,
        /// Estimated input tokens.
        input: u64,
        /// Context limit.
        context: u64,
    },
    /// Requested output exceeds the output limit.
    #[error("model {id} output {output} exceeds limit {limit}")]
    OverOutput {
        /// Model id.
        id: String,
        /// Requested max output tokens.
        output: u64,
        /// Output limit.
        limit: u64,
    },
}

/// Effective model catalog: merged discovery + static entries.
#[derive(Debug, Clone, Default)]
pub struct ModelCatalog {
    /// Provider id this catalog belongs to.
    pub provider: String,
    /// Entries by model id.
    pub models: BTreeMap<String, serde_json::Value>,
}

/// Validated variant selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedVariant {
    /// Variant name.
    pub name: String,
    /// Reasoning effort when the variant carries one.
    pub reasoning_effort: Option<String>,
}

/// Effective selection: entry + optional validated variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Model id.
    pub id: String,
    /// Entry snapshot from the catalog.
    pub entry: serde_json::Value,
    /// Validated variant, if any.
    pub variant: Option<SelectedVariant>,
}

fn available_ids(catalog: &ModelCatalog) -> String {
    let mut ids: Vec<&String> = catalog.models.keys().collect();
    ids.sort();
    ids.into_iter()
        .take(20)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Select a model by exact id. Never falls back to another model.
pub fn select_model(catalog: &ModelCatalog, id: &str) -> Result<Selection, SelectError> {
    match catalog.models.get(id) {
        Some(entry) => Ok(Selection {
            id: id.to_string(),
            entry: entry.clone(),
            variant: None,
        }),
        None => Err(SelectError::UnknownModel {
            id: id.to_string(),
            available: available_ids(catalog),
        }),
    }
}

fn enabled_variants(entry: &serde_json::Value) -> BTreeMap<String, Option<String>> {
    let mut out = BTreeMap::new();
    if let Some(map) = entry.get("variants").and_then(|v| v.as_object()) {
        for (name, variant) in map {
            let disabled = variant.get("disabled") == Some(&serde_json::Value::Bool(true));
            if disabled {
                continue;
            }
            let effort = variant
                .get("reasoningEffort")
                .and_then(|v| v.as_str())
                .map(String::from);
            out.insert(name.clone(), effort);
        }
    }
    out
}

/// Validate an explicit variant against the entry allowlist.
pub fn select_variant(
    selection: &Selection,
    variant: Option<&str>,
) -> Result<Selection, SelectError> {
    let enabled = enabled_variants(&selection.entry);
    match variant {
        Some(name) => match enabled.get(name) {
            Some(effort) => Ok(Selection {
                id: selection.id.clone(),
                entry: selection.entry.clone(),
                variant: Some(SelectedVariant {
                    name: name.to_string(),
                    reasoning_effort: effort.clone(),
                }),
            }),
            None => {
                let mut names: Vec<&String> = enabled.keys().collect();
                names.sort();
                Err(SelectError::UnavailableVariant {
                    id: selection.id.clone(),
                    variant: name.to_string(),
                    enabled: names
                        .into_iter()
                        .take(20)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                })
            }
        },
        None => {
            // Default: `none` when enabled, else first enabled standard, else
            // first enabled custom, else no variant at all.
            let pick = ["none"]
                .into_iter()
                .chain(STANDARD_VARIANTS)
                .map(String::from)
                .chain(enabled.keys().cloned().collect::<Vec<_>>())
                .find(|name| enabled.contains_key(name));
            Ok(Selection {
                id: selection.id.clone(),
                entry: selection.entry.clone(),
                variant: pick.map(|name| SelectedVariant {
                    reasoning_effort: enabled.get(&name).cloned().flatten(),
                    name,
                }),
            })
        }
    }
}

/// Admit an estimated workload against the entry context+output limit.
///
/// Both limits must be positive integers (mirrors the discovery deletion
/// rule: incomplete limits were never published, so their absence here is
/// an error, not a silent pass).
pub fn admit(selection: &Selection, input_tokens: u64, max_output: u64) -> Result<(), SelectError> {
    let limit = selection.entry.get("limit");
    let context = limit
        .and_then(|l| l.get("context"))
        .and_then(|v| v.as_u64())
        .filter(|v| *v > 0);
    let output = limit
        .and_then(|l| l.get("output"))
        .and_then(|v| v.as_u64())
        .filter(|v| *v > 0);
    match (context, output) {
        (Some(context), Some(output)) => {
            if input_tokens > context {
                return Err(SelectError::OverContext {
                    id: selection.id.clone(),
                    input: input_tokens,
                    context,
                });
            }
            if max_output > output {
                return Err(SelectError::OverOutput {
                    id: selection.id.clone(),
                    output: max_output,
                    limit: output,
                });
            }
            Ok(())
        }
        _ => Err(SelectError::MissingLimit {
            id: selection.id.clone(),
        }),
    }
}

/// Diagnose unknown top-level metadata keys (visible, non-fatal).
pub fn diagnose(entry: &serde_json::Value) -> Vec<String> {
    match entry.as_object() {
        Some(map) => map
            .keys()
            .filter(|key| !KNOWN_KEYS.contains(&key.as_str()))
            .map(|key| format!("unknown metadata key: {key}"))
            .collect(),
        None => vec!["entry is not an object".to_string()],
    }
}

/// Redacted explain view of a selection (no secrets exist at this layer,
/// but auth-adjacent fields are withheld by construction).
pub fn explain(provider: &str, selection: &Selection, catalog_size: usize) -> serde_json::Value {
    serde_json::json!({
        "provider": provider,
        "model": selection.id,
        "name": selection.entry.get("name"),
        "limit": selection.entry.get("limit"),
        "variant": selection.variant.as_ref().map(|v| serde_json::json!({
            "name": v.name,
            "reasoningEffort": v.reasoning_effort,
        })),
        "catalog_models": catalog_size,
    })
}

#[cfg(test)]
mod tests {
    use super::{ModelCatalog, admit, diagnose, explain, select_model, select_variant};
    use std::collections::BTreeMap;

    fn catalog() -> ModelCatalog {
        let models: BTreeMap<String, serde_json::Value> = [
            (
                "org/future".to_string(),
                serde_json::json!({
                    "name": "Future",
                    "limit": {"context": 500000, "input": 500000, "output": 32000},
                    "variants": {
                        "none": {"disabled": true},
                        "low": {"reasoningEffort": "low"},
                        "custom": {"reasoningEffort": "deep"},
                    },
                }),
            ),
            (
                "org/plain".to_string(),
                serde_json::json!({"name": "Plain"}),
            ),
        ]
        .into_iter()
        .collect();
        ModelCatalog {
            provider: "ludka2".to_string(),
            models,
        }
    }

    #[test]
    fn exact_selection_no_fallback() {
        let catalog = catalog();
        let selection = select_model(&catalog, "org/future").expect("exact");
        assert_eq!(selection.id, "org/future");
        let err = select_model(&catalog, "org/other").expect_err("unknown");
        let text = err.to_string();
        assert!(text.contains("unknown model org/other"));
        assert!(text.contains("org/future"));
        // Prefixes and case variants never match implicitly.
        assert!(select_model(&catalog, "future").is_err());
        assert!(select_model(&catalog, "ORG/FUTURE").is_err());
    }

    #[test]
    fn variant_allowlist_and_default() {
        let catalog = catalog();
        let selection = select_model(&catalog, "org/future").expect("model");
        // Disabled `none` is unavailable explicitly...
        assert!(select_variant(&selection, Some("none")).is_err());
        // ...but an enabled variant validates with its effort.
        let with = select_variant(&selection, Some("low")).expect("low");
        assert_eq!(
            with.variant
                .as_ref()
                .expect("v")
                .reasoning_effort
                .as_deref(),
            Some("low")
        );
        // Unknown names name the enabled set instead of guessing.
        let err = select_variant(&selection, Some("ultra")).expect_err("ultra");
        assert!(err.to_string().contains("low"));
        // Default skips disabled `none` for the first enabled standard.
        let defaulted = select_variant(&selection, None).expect("default");
        assert_eq!(defaulted.variant.as_ref().expect("v").name, "low");
        // Entries without variants admit no variant at all.
        let plain = select_model(&catalog, "org/plain").expect("plain");
        assert!(
            select_variant(&plain, None)
                .expect("none")
                .variant
                .is_none()
        );
        assert!(select_variant(&plain, Some("low")).is_err());
    }

    #[test]
    fn admission_context_output_and_missing() {
        let catalog = catalog();
        let selection = select_model(&catalog, "org/future").expect("model");
        assert!(admit(&selection, 1000, 1000).is_ok());
        assert!(admit(&selection, 500_001, 10).is_err());
        assert!(admit(&selection, 10, 32_001).is_err());
        let plain = select_model(&catalog, "org/plain").expect("plain");
        assert!(admit(&plain, 10, 10).is_err());
    }

    #[test]
    fn diagnostics_and_explain() {
        let entry = serde_json::json!({"name": "x", "mystery": 1, "limit": {"context": 5}});
        let notes = diagnose(&entry);
        assert_eq!(notes, vec!["unknown metadata key: mystery".to_string()]);
        assert!(diagnose(&serde_json::json!({"name": "x"})).is_empty());
        let catalog = catalog();
        let selection = select_variant(
            &select_model(&catalog, "org/future").expect("m"),
            Some("low"),
        )
        .expect("v");
        let view = explain("ludka2", &selection, catalog.models.len());
        assert_eq!(view["model"], "org/future");
        assert_eq!(view["variant"]["name"], "low");
        assert_eq!(view["limit"]["context"], 500_000);
        assert_eq!(view["catalog_models"], 2);
    }
}
