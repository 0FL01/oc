use super::*;

#[test]
fn mcp12_log_attachment_roundtrip_requires_native_graph_and_does_not_infer_provider_media() {
    let facts =
        serde_json::from_str::<Value>(include_str!("../../../../../fixtures/mcp12-media.json"))
            .unwrap()["result"]
            .clone();
    let secrets = ["mcp-media-private-canary".into()];
    let output =
        crate::mcp_result::project_rich(serde_json::from_value(facts).unwrap(), &secrets, &secrets)
            .unwrap();
    let mut log = TurnLog::new("turn", "model", "provider");
    log.input.push(InputItem::ProviderOutput(json!({"type":"function_call","call_id":"actual-call","name":"media__mixed","arguments":"{}"})));
    log.input.push(InputItem::McpFunctionCallOutput {
        call_id: "actual-call".into(),
        output,
    });
    let stored = log.to_json();
    assert!(stored["input"][1]["output"].is_string());
    assert!(stored["native_mcp_results"][0]["result"]["content"][1]["data"].is_string());
    let restored = TurnLog::from_json(&stored).unwrap();
    assert_eq!(restored, log);
    assert_eq!(
        serde_json::to_value(&restored.input).unwrap(),
        serde_json::to_value(&log.input).unwrap()
    );
    let mut bad = stored.clone();
    bad["native_mcp_results"][0]["call_id"] = "other-call".into();
    assert!(TurnLog::from_json(&bad).is_err());
    bad = stored.clone();
    bad["native_mcp_results"][0]["input_index"] = 0.into();
    assert!(TurnLog::from_json(&bad).is_err());
    bad = stored.clone();
    bad["native_mcp_results"]
        .as_array_mut()
        .unwrap()
        .push(stored["native_mcp_results"][0].clone());
    assert!(TurnLog::from_json(&bad).is_err());
    let mut legacy = stored.clone();
    legacy.as_object_mut().unwrap().remove("native_mcp_results");
    legacy["input"][1]["output"] = "ordinary text that mentions input_image".into();
    assert!(
        matches!(&TurnLog::from_json(&legacy).unwrap().input[1], InputItem::FunctionCallOutput { output, .. } if output.starts_with("ordinary text"))
    );
    legacy["input"][1]["output"] =
        json!([{"type":"input_image","image_url":"data:image/png;base64,AA=="}]);
    legacy["input"][1]["native_mcp_results"] = stored["native_mcp_results"].clone();
    assert!(matches!(
        &TurnLog::from_json(&legacy).unwrap().input[1],
        InputItem::ProviderOutput(_)
    ));
    assert!(
        TurnLog::from_json(&legacy)
            .unwrap()
            .to_json()
            .get("native_mcp_results")
            .is_none()
    );
}
