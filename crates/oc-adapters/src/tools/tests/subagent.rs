use super::super::{ToolCall, ToolError, subagent_request, validate_call};
use serde_json::{Value, json};

#[test]
fn optional_strings_normalize_only_exact_empty_and_preserve_raw_arguments() {
    for (key, nonempty) in [
        ("model", "fixture/model#variant"),
        ("sessionID", "native-child"),
    ] {
        for (value, expected) in [
            (None, None),
            (Some(""), None),
            (Some(nonempty), Some(nonempty)),
            (Some(" \t"), Some(" \t")),
            (Some(" padded/value "), Some(" padded/value ")),
        ] {
            let mut arguments = json!({"agent":"explore","description":"Read","prompt":"go"});
            if let Some(value) = value {
                arguments[key] = value.into();
            }
            let call = ToolCall {
                id: "child".into(),
                name: "subagent".into(),
                arguments: arguments.clone(),
            };
            validate_call(&call).unwrap();
            let request = subagent_request(&call).unwrap();
            let (actual, other) = if key == "model" {
                (request.model.as_deref(), request.session_id.as_deref())
            } else {
                (request.session_id.as_deref(), request.model.as_deref())
            };
            assert_eq!(actual, expected, "{key}: {value:?}");
            assert_eq!(other, None);
            assert_eq!(call.arguments, arguments);
        }
    }
    let call = ToolCall {
        id: "both-empty".into(),
        name: "subagent".into(),
        arguments: json!({"agent":"explore","description":"Read","prompt":"go","model":"","sessionID":""}),
    };
    let original = call.arguments.clone();
    let request = subagent_request(&call).unwrap();
    assert!(request.model.is_none() && request.session_id.is_none());
    assert_eq!(call.arguments, original);
}

#[test]
fn optional_nonstring_values_are_invalid_at_validation_and_request_boundaries() {
    for key in ["model", "sessionID"] {
        for value in [Value::Null, json!(true), json!(7), json!([]), json!({})] {
            let mut arguments = json!({"agent":"explore","description":"Read","prompt":"go"});
            arguments[key] = value;
            let call = ToolCall {
                id: "invalid-child".into(),
                name: "subagent".into(),
                arguments: arguments.clone(),
            };
            assert!(validate_call(&call).is_err(), "{arguments}");
            assert!(
                matches!(subagent_request(&call), Err(ToolError::InvalidArgs { .. })),
                "{arguments}"
            );
            assert_eq!(call.arguments, arguments);
        }
    }
}
