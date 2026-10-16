use super::*;

#[test]
fn captured_mcp_label_survives_argument_snapshots_without_decoding_or_leaking() {
    use crate::mcp_remote::RegistryEntry;
    use crate::runtime::mcp::McpGeneration;
    use oc_core::tool_output::{McpLabel, Presentation};
    use std::sync::Arc;
    let wire = "mcp_hash_collision_2";
    let mut captured = McpGeneration::empty(9, Arc::new(tokio::sync::Notify::new()));
    captured.entries = vec![RegistryEntry {
        namespaced: wire.into(),
        server: "actual_server".into(),
        tool: "remote__tool".into(),
        description: None,
        input_schema: serde_json::json!({}),
    }];
    let original = captured.entries.clone();
    assert!(captured.label("actual_server__remote__tool", &[]).is_none());
    let facts = captured.label(wire, &[]).unwrap();
    assert!(facts.is_valid() && facts.label_only && facts.body.is_empty());
    assert_eq!(
        facts.mcp_label.as_ref().unwrap().text,
        "actual_server_remote__tool"
    );
    assert_eq!(facts.retained_bytes(), "actual_server_remote__tool".len());
    let mut streams = PendingToolStreams::new(3);
    let announced = streams
        .announce("item", "call", wire, Some(facts.clone()))
        .unwrap();
    let check = |event: &ToolStreamEvent| {
        let ToolStreamEvent::Pending {
            name,
            identity,
            output_presentation,
            ..
        } = event
        else {
            panic!("pending")
        };
        assert_eq!(name, wire);
        assert_eq!(
            (&identity.item_id, &identity.call_id, identity.round),
            (&"item".to_string(), &"call".to_string(), 3)
        );
        assert_eq!(output_presentation.as_ref(), Some(&facts));
    };
    check(&announced);
    assert!(streams.delta("item", "{\"query\":\"界").is_none());
    for event in streams.flush() {
        check(&event);
    }
    check(&streams.delta("item", &"x".repeat(4096)).unwrap());
    assert_eq!(
        captured.entries, original,
        "presentation does not rewrite the registry"
    );
    captured.entries[0].server = "known-secret\nserver".into();
    captured.entries[0].tool = format!("{}known-secret\u{7}", "界".repeat(200));
    let redacted = captured.label(wire, &["known-secret".into()]).unwrap();
    let label = redacted.mcp_label.as_ref().unwrap();
    assert!(label.text.starts_with("[redacted] server_"));
    assert!(!label.text.contains("known-secret") && !label.text.chars().any(char::is_control));
    assert!(label.limited && label.text.ends_with('…') && label.text.len() <= 512);
    assert!(redacted.is_valid());
    let legacy: Presentation = serde_json::from_str(
        r#"{"body":"old error","body_bytes":9,"body_limited":false,"generated_guidance":false}"#,
    )
    .unwrap();
    assert!(!legacy.label_only && legacy.mcp_label.is_none() && legacy.is_valid());
    for text in ["", "bad\nname", &"x".repeat(513)] {
        assert!(
            !Presentation::label(McpLabel {
                text: text.into(),
                limited: false
            })
            .is_valid()
        );
    }
    let mut invalid = (*facts).clone();
    invalid.body = "not a label-only result".into();
    assert!(!invalid.is_valid());
    invalid.body.clear();
    invalid.producer_limited = Some(false);
    assert!(
        !invalid.is_valid(),
        "intent metadata is not producer evidence"
    );
}
