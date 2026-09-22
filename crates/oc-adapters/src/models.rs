//! Effective model selection for T15.
//!
//! Exact-id selection over a merged catalog (discovery + static), variant
//! validation against the entry allowlist, context+output admission, unknown
//! metadata diagnostics, and a redacted explain view. There is deliberately
//! no production automatic model fallback: an unknown id or variant is a
//! visible error even when the catalog is non-empty.

use std::collections::BTreeMap;

use thiserror::Error;

/// Standard variant order used for diagnostics.
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
        None => Ok(Selection {
            id: selection.id.clone(),
            entry: selection.entry.clone(),
            variant: None,
        }),
    }
}

/// Native fallback caps and default output request; never catalog capacities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FallbackLimits {
    /// Estimated total context cap, in tokens.
    pub context: std::num::NonZeroU64,
    /// Output cap when unknown, and default requested output, in tokens.
    pub output: std::num::NonZeroU64,
}

impl Default for FallbackLimits {
    fn default() -> Self {
        Self {
            context: std::num::NonZeroU64::new(32_768).expect("positive constant"),
            output: std::num::NonZeroU64::new(4_096).expect("positive constant"),
        }
    }
}

/// Reserve for estimation uncertainty, in addition to output reservation.
pub const SAFETY_MARGIN: u64 = 1_024;

/// Request-local admission policy, separate from published model metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionBudget {
    /// Known context or explicitly native fallback cap.
    pub context: u64,
    /// Clamped output tokens to send on the wire.
    pub output: u64,
    /// Maximum estimated input after output and safety reservation.
    pub input: u64,
    /// Visible diagnostic when either capacity is unknown.
    pub warning: Option<String>,
}

/// Resolve a bounded workload without changing the model snapshot. Zero requested
/// output means the native default, including for callers with unknown metadata.
pub fn budget(
    selection: &Selection,
    requested_output: u64,
    fallback: FallbackLimits,
) -> AdmissionBudget {
    let positive = |key| {
        selection
            .entry
            .pointer(key)
            .and_then(serde_json::Value::as_u64)
            .filter(|n| *n > 0)
    };
    let known_context = positive("/limit/context");
    let known_output = positive("/limit/output");
    let context = known_context.unwrap_or(fallback.context.get());
    let output_cap = known_output.unwrap_or(fallback.output.get());
    let output = if requested_output == 0 {
        fallback.output.get()
    } else {
        requested_output
    }
    .min(output_cap);
    let input = positive("/limit/input")
        .unwrap_or(u64::MAX)
        .min(context.saturating_sub(output))
        .saturating_sub(SAFETY_MARGIN);
    let warning = (known_context.is_none() || known_output.is_none()).then(|| {
        let unknown = match (known_context, known_output) {
            (None, None) => "context and output",
            (None, _) => "context",
            _ => "output",
        };
        format!("model {} has unknown {unknown} limits; using native fallback caps (context={}, output={}); request budget context={context}, output={output}, estimated input<={input}; these are not discovered model capacities", selection.id, fallback.context, fallback.output)
    });
    AdmissionBudget {
        context,
        output,
        input,
        warning,
    }
}

/// Enforce a resolved request budget (including optional model input limit).
pub fn admit_budget(
    selection: &Selection,
    input_tokens: u64,
    budget: &AdmissionBudget,
) -> Result<(), SelectError> {
    if input_tokens > budget.input || budget.output.saturating_add(SAFETY_MARGIN) >= budget.context
    {
        return Err(SelectError::OverContext {
            id: selection.id.clone(),
            input: input_tokens,
            context: budget.input,
        });
    }
    Ok(())
}

/// Admit with the native defaults. Runtime callers retain the resolved budget
/// so the wire uses the same clamped output and exposes its warning.
pub fn admit(selection: &Selection, input_tokens: u64, max_output: u64) -> Result<(), SelectError> {
    admit_budget(
        selection,
        input_tokens,
        &budget(selection, max_output, FallbackLimits::default()),
    )
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
        // No selection means no overlay, even with enabled variants.
        let defaulted = select_variant(&selection, None).expect("default");
        assert!(defaulted.variant.is_none());
        assert!(
            select_variant(&with, None)
                .expect("clear")
                .variant
                .is_none()
        );
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
        assert!(admit(&selection, 10, 32_001).is_ok());
        let plain = select_model(&catalog, "org/plain").expect("plain");
        assert!(admit(&plain, 10, 10).is_ok());
    }

    #[test]
    fn fallback_budget_preserves_metadata_and_bounds_partial_zero_limits() {
        use super::{FallbackLimits, SAFETY_MARGIN, admit_budget, budget};
        let fallback: FallbackLimits =
            serde_json::from_value(serde_json::json!({"context": 16_384, "output": 2_048}))
                .unwrap();
        for (limit, context, output) in [
            (serde_json::Value::Null, 16_384, 2_048),
            (serde_json::json!({"context": 8_192}), 8_192, 2_048),
            (serde_json::json!({"output": 512}), 16_384, 512),
            (
                serde_json::json!({"context": 0, "output": 0}),
                16_384,
                2_048,
            ),
            (
                serde_json::json!({"context": 8_192, "output": 0}),
                8_192,
                2_048,
            ),
            (
                serde_json::json!({"context": 0, "output": 512}),
                16_384,
                512,
            ),
        ] {
            let selection = super::Selection {
                id: "future".into(),
                entry: serde_json::json!({"limit": limit}),
                variant: None,
            };
            let before = selection.entry.clone();
            let resolved = budget(&selection, 99_999, fallback);
            assert_eq!((resolved.context, resolved.output), (context, output));
            assert_eq!(resolved.input, context - output - SAFETY_MARGIN);
            assert!(
                resolved
                    .warning
                    .as_deref()
                    .unwrap()
                    .contains("not discovered model capacities")
            );
            assert!(admit_budget(&selection, resolved.input, &resolved).is_ok());
            assert!(admit_budget(&selection, resolved.input + 1, &resolved).is_err());
            assert_eq!(selection.entry, before);
        }
        let selection = super::Selection {
            id: "known".into(),
            entry: serde_json::json!({"limit": {"context": 8_192, "input": 3_000, "output": 1_000}}),
            variant: None,
        };
        let resolved = budget(&selection, 10_000, fallback);
        assert_eq!(resolved.input, 3_000 - SAFETY_MARGIN);
        assert_eq!(resolved.output, 1_000);
        assert!(resolved.warning.is_none());
        assert_eq!(budget(&selection, 0, fallback).output, 1_000);
        assert_eq!(budget(&selection, 100, fallback).output, 100);
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
