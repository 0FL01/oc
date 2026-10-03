use super::*;

#[test]
fn dcp12_omitted_defaults_and_partial_numeric_override() {
    let defaults = DcpConfig::default();
    let (loaded, warnings) = load_config(&serde_json::json!({})).unwrap();
    assert_eq!(defaults, loaded);
    assert!(warnings.is_empty());
    assert!(!loaded.summary_buffer);
    assert_eq!(
        (loaded.min_context_percent, loaded.max_context_percent),
        (Some(4000), Some(5500))
    );
    let t = loaded.effective_for_context("fixture/m", 100_003);
    assert_eq!(
        (t.min_context, t.max_context, t.nudge_frequency),
        (40_001, 55_001, 5)
    );
    assert_eq!(
        (loaded.iteration_threshold, loaded.nudge_force),
        (15, NudgeForce::Soft)
    );
    let (partial, _) =
        load_config(&serde_json::json!({"compress":{"minContextLimit":100}})).unwrap();
    assert_eq!(partial.min_context_percent, None);
    assert_eq!(partial.max_context_percent, Some(5500));
    let t = partial.effective_for_context("fixture/m", 100_003);
    assert_eq!((t.min_context, t.max_context), (100, 55_001));
}

#[test]
fn dcp12_total_active_boundaries_buffer_and_success_cooldown() {
    let mut config = DcpConfig::default();
    for (tokens, expected) in [
        (39_999, None),
        (40_000, Some(NudgeForce::Soft)),
        (55_000, Some(NudgeForce::Soft)),
        (55_001, Some(NudgeForce::Hard)),
    ] {
        let mut state = NudgeState::default();
        state.on_turn();
        assert_eq!(
            evaluate(&config, &mut state, "fixture/m", 100_000, tokens, 20_000).map(|n| n.force),
            expected
        );
    }
    config.summary_buffer = true;
    let mut state = NudgeState::default();
    state.on_turn();
    assert_eq!(
        evaluate(&config, &mut state, "fixture/m", 100_000, 60_000, 20_000)
            .unwrap()
            .force,
        NudgeForce::Soft
    );
    for _ in 0..4 {
        state.on_turn();
        assert!(evaluate(&config, &mut state, "fixture/m", 100_000, 80_000, 20_000).is_none());
    }
    state.on_turn();
    assert_eq!(
        evaluate(&config, &mut state, "fixture/m", 100_000, 80_000, 20_000)
            .unwrap()
            .force,
        NudgeForce::Hard
    );
    state.on_compress_success();
    for _ in 0..4 {
        state.on_turn();
        assert!(evaluate(&config, &mut state, "fixture/m", 100_000, 60_000, 20_000).is_none());
    }
    state.on_turn();
    assert_eq!(
        evaluate(&config, &mut state, "fixture/m", 100_000, 60_000, 20_000)
            .unwrap()
            .force,
        NudgeForce::Soft
    );
    state.iteration = 15;
    assert_eq!(
        evaluate(&config, &mut state, "fixture/m", 100_000, 40_000, 20_000)
            .unwrap()
            .force,
        NudgeForce::Hard
    );
}

#[test]
fn dcp12_exact_model_budget_fallback_floor_overrides_and_validation() {
    use crate::models::{FallbackLimits, Selection, budget};
    let (config, _) = load_config(&serde_json::json!({"compress":{
        "modelMinLimits":{"fixture/nested/m":"25.25%"},
        "modelMaxLimits":{"fixture/nested/m":60000}
    }}))
    .unwrap();
    for (limits, context, fallback_context, warning) in [
        (
            serde_json::json!({"context":100003,"output":2048}),
            100003,
            false,
            false,
        ),
        (serde_json::json!({}), 32768, true, true),
        (
            serde_json::json!({"context":0,"output":0}),
            32768,
            true,
            true,
        ),
        (serde_json::json!({"context":100003}), 100003, false, true),
        (serde_json::json!({"output":2048}), 32768, true, true),
    ] {
        let selection = Selection {
            id: "nested/m".into(),
            entry: serde_json::json!({"limit":limits}),
            variant: None,
        };
        let b = budget(&selection, 0, FallbackLimits::default());
        let facts = config.reminder_facts("fixture", &selection, &b).unwrap();
        assert_eq!(facts.model_key, "fixture/nested/m");
        assert_eq!(
            (
                facts.model_context,
                facts.context_from_fallback,
                facts.budget_warning.is_some()
            ),
            (context, fallback_context, warning)
        );
        assert_eq!(facts.min_context, context * 2525 / 10000);
        assert_eq!(facts.max_context, 60000);
        assert_eq!(
            selection.entry["limit"], limits,
            "budget never rewrites discovery metadata"
        );
        let other = config.reminder_facts("other", &selection, &b).unwrap();
        assert_eq!(
            (other.min_context, other.max_context),
            (context * 40 / 100, context * 55 / 100)
        );
    }
    let selection = Selection {
        id: "m".into(),
        entry: serde_json::json!({"limit":{"context":1,"output":1}}),
        variant: None,
    };
    let b = budget(&selection, 1, FallbackLimits::default());
    let tiny = DcpConfig::default()
        .reminder_facts("fixture", &selection, &b)
        .unwrap();
    assert_eq!((tiny.min_context, tiny.max_context), (1, 1));
    for fragment in [
        serde_json::json!({"compress":{"minContextLimit":"60%"}}),
        serde_json::json!({"compress":{"modelMinLimits":{"fixture/m":100000}}}),
    ] {
        let (bad, _) = load_config(&fragment).unwrap();
        let b = crate::models::AdmissionBudget {
            context: 100003,
            output: 2048,
            input: 96931,
            warning: None,
        };
        assert!(bad.reminder_facts("fixture", &selection, &b).is_err());
    }
    for invalid in ["0%", "100.01%", "40.001%", "-1%", " 40%"] {
        assert!(load_config(&serde_json::json!({"compress":{"maxContextLimit":invalid}})).is_err());
    }
    let (explicit,_) = load_config(&serde_json::json!({"compress":{"minContextLimit":10,"maxContextLimit":20,"summaryBuffer":true}})).unwrap();
    assert_eq!(
        (explicit.min_context_percent, explicit.max_context_percent),
        (None, None)
    );
    assert!(explicit.summary_buffer);
}
