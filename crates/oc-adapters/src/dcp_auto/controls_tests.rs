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
