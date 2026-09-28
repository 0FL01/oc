//! VAR01: effective metadata order, exact identity and selection diagnostics.

use super::*;
use serde_json::{Value, json};

fn entry(variants: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    json!({"variants": variants.into_iter().map(|(name, value)| (name.to_string(), value)).collect::<serde_json::Map<_, _>>()})
}

fn names(entry: &Value) -> Vec<&str> {
    available_variants(entry).map(|(name, _)| name).collect()
}

#[test]
fn canonical_known_permutations_and_supported_subsets() {
    fn permutations(levels: &mut [&'static str], index: usize) {
        if index == levels.len() {
            let spec = entry(
                levels
                    .iter()
                    .map(|name| (*name, json!({"reasoningEffort": name}))),
            );
            let before = spec.clone();
            assert_eq!(names(&spec), STANDARD_VARIANTS);
            assert_eq!(
                spec, before,
                "ordering is a borrowed view, not a wire rewrite"
            );
            return;
        }
        for next in index..levels.len() {
            levels.swap(index, next);
            permutations(levels, index + 1);
            levels.swap(index, next);
        }
    }
    let mut levels = STANDARD_VARIANTS;
    permutations(&mut levels, 0);
    for mask in 0..(1 << STANDARD_VARIANTS.len()) {
        let expected: Vec<_> = STANDARD_VARIANTS
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, name)| *name)
            .collect();
        let spec = entry(expected.iter().rev().map(|name| (*name, json!({}))));
        assert_eq!(names(&spec), expected, "subset {mask}");
    }
    assert!(names(&json!({})).is_empty());
    assert!(
        names(&entry([
            ("low", json!({"disabled":true})),
            ("default", json!({}))
        ]))
        .is_empty()
    );
}

#[test]
fn explicit_effort_wins_exact_names_and_custom_aliases_stay_source_ordered() {
    let spec = entry([
        ("zeta", json!({"reasoningEffort":"deep"})),
        ("max", json!({"reasoningEffort":"low"})),
        ("high", json!({"reasoningEffort":"future"})),
        ("xhigh", json!({})),
        ("low", json!({"reasoningEffort":"high"})),
        ("none", json!({"reasoningEffort":"none"})),
        ("fast", json!({"reasoningEffort":"low"})),
        ("minimal", json!({"reasoningEffort":null})),
        ("MAX", json!({})),
        (" medium", json!({})),
        ("medium ", json!({})),
        ("Medium", json!({"reasoningEffort":"High"})),
        ("spaced", json!({"reasoningEffort":" low "})),
        ("alpha", json!({"reasoningEffort":"deep"})),
        ("default", json!({"reasoningEffort":"none"})),
        (
            "disabled",
            json!({"reasoningEffort":"medium","disabled":true}),
        ),
        ("ceiling", json!({"reasoningEffort":"max"})),
    ]);
    let expected = [
        "none", "minimal", "max", "fast", "low", "xhigh", "ceiling", "zeta", "high", "MAX",
        " medium", "medium ", "Medium", "spaced", "alpha",
    ];
    assert_eq!(names(&spec), expected);
    let declared = ordered_variants(&spec);
    assert_eq!(declared.len(), 17, "disabled and reserved metadata survive");
    let selection = Selection {
        id: "unlisted/future-model".into(),
        entry: spec.clone(),
        variant: None,
    };
    let err = select_variant(&selection, Some("retired")).unwrap_err();
    assert_eq!(
        err,
        SelectError::UnavailableVariant {
            id: selection.id.clone(),
            variant: "retired".into(),
            enabled: expected.join(", "),
        }
    );
    assert!(select_variant(&selection, Some("disabled")).is_err());
    // The raw backend allowlist remains compatible; `default` is not a named
    // available choice, and the UI's actual Default selects None instead.
    assert_eq!(
        select_variant(&selection, Some("default"))
            .unwrap()
            .variant
            .unwrap()
            .reasoning_effort
            .as_deref(),
        Some("none")
    );
    assert!(select_variant(&selection, None).unwrap().variant.is_none());
    for (name, effort) in [
        ("none", Some("none")),
        ("max", Some("low")),
        ("fast", Some("low")),
        ("xhigh", None),
        ("high", Some("future")),
        ("spaced", Some(" low ")),
        ("minimal", None),
    ] {
        let selected = select_variant(&selection, Some(name)).unwrap();
        let variant = selected.variant.unwrap();
        assert_eq!(
            (variant.name.as_str(), variant.reasoning_effort.as_deref()),
            (name, effort)
        );
        assert_eq!(selected.entry, spec);
    }
}

#[test]
fn malformed_discovery_metadata_is_still_rejected_before_ordering() {
    for value in [
        Value::Null,
        json!(23),
        json!([]),
        json!({"reasoningEffort":null}),
        json!({"reasoningEffort":false}),
        json!({"reasoningEffort":""}),
        json!({"reasoningEffort":"low", "disabled":true}),
    ] {
        assert!(
            crate::discovery::model_config(&json!({
                "id":"future", "opencode":{"variants":{"low":value}}
            }))
            .is_err()
        );
    }
}
