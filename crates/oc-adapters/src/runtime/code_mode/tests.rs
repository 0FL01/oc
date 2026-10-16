use super::*;

#[test]
fn execute_previews_are_structured_redacted_bounded_and_refusals_are_terminal() {
    let secrets = vec!["secret-canary".to_owned()];
    let arguments = serde_json::json!({"code": "return 'secret-canary';"});
    let intent = intent_presentation("op", &arguments, &secrets).unwrap();
    assert!(intent.label_only && intent.is_valid());
    assert!(
        !serde_json::to_string(&intent)
            .unwrap()
            .contains("secret-canary")
    );
    let refused =
        refusal_presentation("op", Some(intent), "error: secret-canary", &secrets).unwrap();
    assert!(refused.is_valid() && !refused.label_only);
    assert!(refused.execute.as_ref().unwrap().error);
    assert_eq!(refused.execute.as_ref().unwrap().elapsed_ms, Some(0));
    assert_eq!(refused.body, "error: [redacted]");
    let input = serde_json::json!({"Authorization":"credential-canary", "nested":{"token":"other-canary"},
        "query":"secret-canary", "oversized":"界".repeat(4096), "small":7});
    let (preview, limited) = input_preview(&input, &secrets);
    let wire = preview.to_string();
    assert!(limited && wire.len() <= EXECUTE_INPUT_BYTES);
    for secret in ["secret-canary", "credential-canary", "other-canary"] {
        assert!(!wire.contains(secret));
    }
    assert_eq!(preview["small"], 7);
    assert!(!preview.as_object().unwrap().contains_key("oversized"));
    let (changed, _) = tokio::sync::watch::channel(());
    let progress = ExecuteProgress {
        facts: Mutex::new(Execute::default()),
        changed,
        secrets: &secrets,
    };
    for _ in 0..EXECUTE_CALLS_CAP + 100 {
        let index = progress.start("tools.fixture.call", &input);
        progress.finish(index, false);
    }
    let facts = progress.facts.lock().unwrap();
    assert!(facts.is_valid() && facts.calls_limited);
    assert_eq!(facts.calls.len(), EXECUTE_CALLS_CAP);
    assert!(
        facts
            .calls
            .iter()
            .all(|call| call.status == ExecuteCallStatus::Completed)
    );
}
