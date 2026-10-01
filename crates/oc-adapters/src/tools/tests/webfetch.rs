use super::*;

#[test]
fn tool17_strict_format_seconds_and_known_options_before_io() {
    for arguments in [
        serde_json::json!({"url":"http://no-io.invalid", "format":"xml"}),
        serde_json::json!({"url":"http://no-io.invalid", "timeout":0}),
        serde_json::json!({"url":"http://no-io.invalid", "timeout":121}),
        serde_json::json!({"url":"http://no-io.invalid", "timeout":"30"}),
        serde_json::json!({"url":"http://no-io.invalid", "timeoutSeconds":30}),
        serde_json::json!({"url":"http://no-io.invalid", "headers":{}}),
        serde_json::json!({"url":"http://no-io.invalid", "format":null}),
    ] {
        let call = ToolCall {
            id: "invalid".into(),
            name: "webfetch".into(),
            arguments,
        };
        assert!(
            super::super::validate_call(&call).is_err(),
            "accepted {:?}",
            call.arguments
        );
    }
    for arguments in [
        serde_json::json!({"url":"https://example.com"}),
        serde_json::json!({"url":"https://example.com", "format":"html", "timeout":0.5}),
        serde_json::json!({"url":"https://example.com", "format":"text", "timeout":120}),
    ] {
        let call = ToolCall {
            id: "valid".into(),
            name: "webfetch".into(),
            arguments,
        };
        assert!(super::super::validate_call(&call).is_ok());
    }
}

#[test]
fn tool17_model_independent_schema_defaults_and_units() {
    let tool = crate::runtime::builtin_tool_defs()
        .into_iter()
        .find(|t| t.name == "webfetch")
        .unwrap();
    assert_eq!(
        tool.parameters["properties"]["format"]["default"],
        "markdown"
    );
    assert_eq!(
        tool.parameters["properties"]["format"]["enum"],
        serde_json::json!(["text", "markdown", "html"])
    );
    assert_eq!(tool.parameters["properties"]["timeout"]["maximum"], 120);
    assert_eq!(tool.parameters["additionalProperties"], false);
    assert!(tool.description.contains("seconds"));
}
