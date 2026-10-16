use super::*;

#[test]
fn dcp12_switch_rejects_wrong_type_and_warns_unknown_nested_field() {
    for value in [
        serde_json::json!("false"),
        serde_json::json!(0),
        serde_json::Value::Null,
    ] {
        assert!(load_config(&serde_json::json!({"compress":{"enabled":value}})).is_err());
    }
    let (_, warnings) =
        load_config(&serde_json::json!({"compress":{"sensitive/unknown":"secret"}})).unwrap();
    assert_eq!(warnings, ["unknown dcp key: compress.entry"]);
    assert!(!warnings.join(" ").contains("secret"));
}

#[test]
fn dcp12_switch_off_has_no_reminder() {
    let (config, _) = load_config(&serde_json::json!({"compress":{"enabled":false}})).unwrap();
    let mut state = NudgeState {
        iteration: 20,
        ..Default::default()
    };
    assert!(evaluate(&config, &mut state, "fixture/m", 100, 90, 0).is_none());
    assert_eq!(state.emitted, 0);
}

/// DCP 3.2.0 (d637981, AGPL-3.0-or-later) `lib/protected-patterns.ts` and
/// `lib/compress/protected-content.ts`: host tool aliases task/subagent and
/// apply_patch/patch, normalized to the native catalog (bash is the legacy
/// spelling of native shell).
#[test]
fn dcp320_protected_tool_names_follow_native_aliases() {
    let cases = [
        ("bash", "shell"),
        ("shell", "bash"),
        ("task", "subagent"),
        ("subagent", "task"),
        ("patch", "apply_patch"),
        ("apply_patch", "patch"),
    ];
    for (pattern, tool) in cases {
        assert!(
            crate::dcp_auto::tool_is_protected(&[pattern.to_string()], tool),
            "{pattern} must protect {tool}"
        );
    }
    for (pattern, tool) in [
        ("bash", "subagent"),
        ("task", "shell"),
        ("read", "edit"),
        ("sh*", "subagent"),
    ] {
        assert!(
            !crate::dcp_auto::tool_is_protected(&[pattern.to_string()], tool),
            "{pattern} vs {tool}"
        );
    }
    assert!(crate::dcp_auto::tool_is_protected(
        &["sub*".to_string()],
        "subagent"
    ));
    assert!(crate::dcp_auto::tool_is_protected(
        &["ba?h".to_string()],
        "shell"
    ));
}
