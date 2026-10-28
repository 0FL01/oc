//! Source-derived visible-message traces, not the legacy counter-only hook.
use super::*;

fn request(
    config: &DcpConfig,
    state: &mut NudgeState,
    tokens: u64,
    messages: &[(String, bool)],
) -> Option<Nudge> {
    state.on_turn();
    evaluate_request(config, state, "fixture/m", 100000, tokens, 20, messages)
}

fn config() -> DcpConfig {
    DcpConfig {
        min_context: 100,
        max_context: 200,
        min_context_percent: None,
        max_context_percent: None,
        ..Default::default()
    }
}

#[test]
fn dcp12_source_context_visible_distance_buffer_and_removed_anchor() {
    let mut state = NudgeState::default();
    let mut view = vec![("user-0".into(), true)];
    let cfg = config();
    let first = request(&cfg, &mut state, 201, &view).unwrap();
    assert_eq!(first.force, NudgeForce::Hard);
    assert!(first.text.contains("DCP SOFT CONTEXT THRESHOLD PASSED"));
    assert!(first.text.contains("not the model's context limit"));
    assert!(
        first.text.contains(
            "Context estimate: about 201 tokens of a 100000-token model window; DCP soft threshold 200."
        ),
        "{}",
        first.text
    );
    assert!(
        !first.text.contains("user-0"),
        "cadence anchors are not compress IDs"
    );
    // Long Horizon: no fresh alarm at the tail of every request while the
    // anchor is still within the cadence distance.
    for index in 1..5 {
        view.push((format!("assistant-{index}"), false));
        assert!(request(&cfg, &mut state, 201 + index, &view).is_none());
        assert_eq!(state.cadence.context.as_deref(), Some("user-0"));
        assert_eq!(state.emitted, 1);
        assert_eq!(state.last_reminder, Some(1));
    }
    view.push(("assistant-5".into(), false));
    assert_eq!(
        request(&cfg, &mut state, 201, &view).unwrap().force,
        NudgeForce::Hard
    );
    let saved = serde_json::to_string(&state).unwrap();
    let mut reopened: NudgeState = serde_json::from_str(&saved).unwrap();
    assert!(
        request(&cfg, &mut reopened, 201, &view).is_none(),
        "a restored anchor is not re-announced"
    );
    assert_eq!(reopened.cadence.context.as_deref(), Some("assistant-5"));
    assert_eq!(reopened.emitted, 2);
    assert!(request(&cfg, &mut reopened, 200, &view).is_none());
    assert!(
        request(
            &DcpConfig {
                enabled: false,
                ..cfg.clone()
            },
            &mut reopened,
            201,
            &view
        )
        .is_none()
    );
    // Dropped original messages never survive as a hidden anchor inventory.
    let replacement = vec![("fresh-user".into(), true)];
    assert!(request(&cfg, &mut reopened, 201, &replacement).is_some());
    assert!(
        !serde_json::to_string(&reopened)
            .unwrap()
            .contains("assistant-5")
    );
    let buffered = DcpConfig {
        summary_buffer: true,
        ..cfg
    };
    assert!(request(&buffered, &mut NudgeState::default(), 201, &replacement).is_none());
}

#[test]
fn dcp12_source_last_user_pair_iteration_threshold_and_new_user_reset() {
    let cfg = config();
    let mut state = NudgeState::default();
    let mut view = vec![("first".into(), true)];
    assert!(
        request(&cfg, &mut state, 100, &view).is_none(),
        "first user has no preceding assistant pair"
    );
    view.extend([
        ("previous-assistant".into(), false),
        ("next-user".into(), true),
    ]);
    let nudge = request(&cfg, &mut state, 200, &view).unwrap();
    assert_eq!(nudge.force, NudgeForce::Soft, "at max is not over max");
    assert!(
        nudge
            .text
            .contains("Evaluate the conversation for compressible ranges")
    );
    assert_eq!(state.cadence.turn.as_deref(), Some("next-user"));
    for index in 1..15 {
        view.push((format!("step-{index}"), false));
        assert!(request(&cfg, &mut state, 200, &view).is_none());
    }
    view.push(("step-15".into(), false));
    assert_eq!(
        request(&cfg, &mut state, 200, &view).unwrap().force,
        NudgeForce::Hard
    );
    for index in 16..20 {
        view.push((format!("step-{index}"), false));
        assert!(request(&cfg, &mut state, 200, &view).is_none());
    }
    view.push(("step-20".into(), false));
    assert!(request(&cfg, &mut state, 200, &view).is_some());
    view.push(("new-user".into(), true));
    assert_eq!(
        request(&cfg, &mut state, 200, &view).unwrap().force,
        NudgeForce::Soft,
        "lifetime iterations cannot escalate a new user's turn"
    );
    view.push(("new-assistant".into(), false));
    assert!(request(&cfg, &mut state, 200, &view).is_none());
    assert!(request(&cfg, &mut state, 99, &view).is_none());
    assert!(state.cadence.turn.is_none() && state.cadence.iteration.is_none());
}

#[test]
fn dcp12_source_success_cooldown_migration_and_failure_no_reset() {
    let cfg = config();
    let mut legacy_success: NudgeState = serde_json::from_str(
        r#"{"iteration":2,"turns_since_compress":2,"last_reminder":0,"emitted":4}"#,
    )
    .unwrap();
    for tick in 3..=5 {
        assert_eq!(
            request(
                &cfg,
                &mut legacy_success,
                201,
                &[("legacy-user".into(), true)]
            )
            .is_some(),
            tick == 5
        );
    }
    let mut state: NudgeState = serde_json::from_str(
        r#"{"iteration":19,"turns_since_compress":19,"last_reminder":15,"emitted":4}"#,
    )
    .unwrap();
    let mut view = vec![("user".into(), true)];
    assert!(request(&cfg, &mut state, 201, &view).is_some());
    // Invalid/no-gain calls only add a visible assistant, no success callback.
    for index in 1..=5 {
        view.push((format!("failed-step-{index}"), false));
        assert_eq!(request(&cfg, &mut state, 201, &view).is_some(), index == 5);
        assert_eq!(state.emitted, if index == 5 { 6 } else { 5 });
    }
    state.on_compress_success();
    assert!(
        state.cadence.context.is_none()
            && state.cadence.turn.is_none()
            && state.cadence.iteration.is_none()
    );
    for index in 1..=5 {
        view.push((format!("after-success-{index}"), false));
        assert_eq!(request(&cfg, &mut state, 201, &view).is_some(), index == 5);
    }
}

#[test]
fn dcp_custom_prompts_replace_only_the_due_reminder_body() {
    let plain = config();
    let custom = DcpConfig {
        custom_prompts: true,
        prompt_overrides: DcpPromptOverrides {
            context_limit_nudge: Some("CUSTOM_CONTEXT".into()),
            turn_nudge: Some("CUSTOM_TURN".into()),
            iteration_nudge: Some("CUSTOM_ITERATION".into()),
            ..Default::default()
        },
        ..plain.clone()
    };
    let mut plain_state = NudgeState::default();
    let mut custom_state = plain_state.clone();
    let mut view = vec![("first-user".into(), true)];
    for step in 0..18 {
        let (tokens, expected) = match step {
            0 => (201, Some("CUSTOM_CONTEXT")),
            1 => {
                view.extend([
                    ("previous-assistant".into(), false),
                    ("next-user".into(), true),
                ]);
                (200, Some("CUSTOM_TURN"))
            }
            17 => {
                view.push(("below-min".into(), true));
                (99, None)
            }
            _ => {
                view.push((format!("assistant-{step}"), false));
                (200, (step == 16).then_some("CUSTOM_ITERATION"))
            }
        };
        let baseline = request(&plain, &mut plain_state, tokens, &view);
        let actual = request(&custom, &mut custom_state, tokens, &view);
        assert_eq!(
            baseline.as_ref().map(|n| n.force),
            actual.as_ref().map(|n| n.force)
        );
        assert_eq!(actual.is_some(), expected.is_some());
        if let Some(body) = expected {
            // Overrides replace the body; measured context facts follow it.
            assert!(actual.unwrap().text.starts_with(body));
        }
        assert_eq!(
            serde_json::to_value(&plain_state).unwrap(),
            serde_json::to_value(&custom_state).unwrap()
        );
    }
}
