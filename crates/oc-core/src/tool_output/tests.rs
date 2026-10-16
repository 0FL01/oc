use super::*;

#[test]
fn execute_facts_are_additive_bounded_and_accounted() {
    let legacy = Presentation::new("legacy body", 11, false);
    let wire = serde_json::to_value(&legacy).unwrap();
    assert!(wire.get("execute").is_none());
    assert_eq!(
        serde_json::from_value::<Presentation>(wire).unwrap(),
        legacy
    );

    let mut facts = Presentation::new("result", 6, false);
    facts.execute = Some(Execute {
        code: Some("return await tools.fixture.add({a: 2});".into()),
        calls: vec![ExecuteCall {
            tool: "tools.fixture.add".into(),
            tool_limited: false,
            status: ExecuteCallStatus::Completed,
            input: Some(serde_json::json!({"a":2})),
            input_limited: false,
        }],
        elapsed_ms: Some(499),
        output: vec![ExecuteOutput {
            kind: ExecuteOutputKind::Json,
            text: "{\"sum\":2}".into(),
            limited: false,
        }],
        ..Default::default()
    });
    assert!(facts.is_valid());
    assert_eq!(
        facts.retained_bytes(),
        6 + facts.execute.as_ref().unwrap().retained_bytes()
    );
    assert_eq!(
        serde_json::from_value::<Presentation>(serde_json::to_value(&facts).unwrap()).unwrap(),
        facts
    );
    let execute = facts.execute.as_mut().unwrap();
    execute
        .calls
        .resize(EXECUTE_CALLS_CAP + 1, execute.calls[0].clone());
    assert!(!facts.is_valid());
    facts.execute.as_mut().unwrap().calls.truncate(1);
    facts.execute.as_mut().unwrap().calls[0].input =
        Some(serde_json::json!({"x":"x".repeat(EXECUTE_INPUT_BYTES)}));
    assert!(!facts.is_valid());
    facts.execute.as_mut().unwrap().calls[0].input = None;
    facts.execute.as_mut().unwrap().code = Some("界".repeat(EXECUTE_CODE_BYTES));
    assert!(!facts.is_valid());
    facts.execute.as_mut().unwrap().code = Some(String::new());
    facts.execute.as_mut().unwrap().output.push(ExecuteOutput {
        kind: ExecuteOutputKind::Text,
        text: "not another value section".into(),
        limited: false,
    });
    assert!(!facts.is_valid());
}

#[test]
fn receiving_execute_does_not_reconstruct_legacy_or_result_facts() {
    let mut facts = Presentation::new("", 0, false);
    facts.execute = Some(Execute::default());
    assert!(facts.is_valid());
    facts.label_only = true;
    assert!(facts.is_valid(), "receiving progress has no result body");
    facts.execute.as_mut().unwrap().error = true;
    assert!(
        !facts.is_valid(),
        "terminal failure cannot masquerade as progress"
    );
    facts.label_only = false;
    assert!(
        facts.is_valid(),
        "an invalid code argument can fail without a code preview"
    );
    facts.execute.as_mut().unwrap().code = Some(String::new());
    assert!(
        facts.is_valid(),
        "empty admitted code is different from receiving arguments"
    );
    facts.mcp_label = Some(McpLabel {
        text: "fixture_tool".into(),
        limited: false,
    });
    assert!(
        !facts.is_valid(),
        "execute is a distinct presentation family"
    );
}
