use super::*;
use crate::provider::{InputItem, InputRole, ResponsesConfig, WireBinding, protocol::Protocol};
use oc_core::queries::NativeProtocol;
use serde_json::json;

fn config(protocol: Protocol) -> ResponsesConfig {
    ResponsesConfig {
        base_url: "https://example.com/prefix/v1".into(),
        api_key: "SECRET_BINDING_KEY".into(),
        timeout: None,
        chunk_timeout_ms: 1_000,
        connect_timeout: std::time::Duration::from_secs(1),
        allow_private: false,
        headers: Default::default(),
        set_cache_key: true,
        wire: WireBinding {
            protocol,
            ..Default::default()
        },
    }
}

#[test]
fn go04_full_binding_authority_is_required_for_opaque_replay() {
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Messages] {
        let config = config(protocol);
        let binding = config.provenance("p", "api-model").unwrap();
        let mut log = TurnLog::new("t", "catalog-model", "p");
        log.protocol = binding.protocol;
        log.binding = Some(binding.clone());
        log.input = vec![
            InputItem::message(InputRole::User, "task"),
            InputItem::ProviderOutput(
                json!({"type":"reasoning","id":"r","summary":[{"text":"public plan"}],"encrypted_content":"OPAQUE_CANARY","messages_thinking":{"type":"thinking","thinking":"plan","signature":"SIGNATURE_CANARY"}}),
            ),
            InputItem::ProviderOutput(
                json!({"type":"function_call","id":"opaque-call","call_id":"call","name":"read","arguments":"{}","status":"completed"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "call".into(),
                output: "SETTLED_RESULT".into(),
            },
            InputItem::ProviderOutput(
                json!({"type":"message","id":"opaque-message","role":"assistant","content":[{"type":"output_text","text":"done"}]}),
            ),
        ];
        let raw = log.to_json();
        assert!(!raw.to_string().contains("SECRET_BINDING_KEY"));
        let reopened = TurnLog::from_json(&raw).unwrap();
        assert_eq!(
            reopened.input_for_bound("catalog-model", "p", Some(&binding)),
            log.input
        );
        let mut mismatches = vec![None];
        for field in [
            "provider",
            "api_model",
            "deployment",
            "auth_scope",
            "protocol",
        ] {
            let mut other = binding.clone();
            match field {
                "provider" => other.provider = "other".into(),
                "api_model" => other.api_model = "other-api".into(),
                "deployment" => other.deployment = "a".repeat(64),
                "auth_scope" => other.auth_scope = "b".repeat(64),
                _ => {
                    other.protocol = if binding.protocol == NativeProtocol::Chat {
                        NativeProtocol::Messages
                    } else {
                        NativeProtocol::Chat
                    }
                }
            }
            mismatches.push(Some(other));
        }
        for other in mismatches {
            let input = reopened.input_for_bound("catalog-model", "p", other.as_ref());
            let projected = serde_json::to_string(&input).unwrap();
            assert!(
                !projected.contains("OPAQUE_CANARY")
                    && !projected.contains("SIGNATURE_CANARY")
                    && !projected.contains("opaque-")
            );
            assert!(
                projected.contains("public plan")
                    && projected.contains("SETTLED_RESULT")
                    && projected.contains("done")
            );
            assert_eq!(input.iter().filter(|i| matches!(i, InputItem::ProviderOutput(v) if v["type"] == "function_call" && v["call_id"] == "call")).count(), 1);
            assert_eq!(
                input.iter().filter(|i| i.call_output().is_some()).count(),
                1
            );
        }
        assert_eq!(log.to_json(), raw, "projection must never rewrite RAW");
        let mut legacy = raw.clone();
        legacy.as_object_mut().unwrap().remove("protocol");
        legacy.as_object_mut().unwrap().remove("binding");
        let legacy = TurnLog::from_json(&legacy).unwrap();
        assert_eq!(legacy.protocol, NativeProtocol::Responses);
        assert!(
            !serde_json::to_string(&legacy.input_for_bound("catalog-model", "p", Some(&binding)))
                .unwrap()
                .contains("OPAQUE_CANARY")
        );
        let mut invalid = raw;
        invalid["protocol"] = "unknown-wire".into();
        assert!(TurnLog::from_json(&invalid).is_err());
    }
}

#[test]
fn go04_receipt_origins_and_hot_selection_preserve_binding_not_current_choice() {
    let a = config(Protocol::Messages).provenance("p", "api-a").unwrap();
    let b = config(Protocol::Chat).provenance("p", "api-b").unwrap();
    let mut log = TurnLog::new("t", "catalog", "p");
    log.binding = Some(a.clone());
    log.protocol = a.protocol;
    log.input = vec![InputItem::message(InputRole::User, "task")];
    for (index, binding) in [(1, &a), (2, &b)] {
        log.requests.push(oc_core::queries::RequestIdentity {
            binding: Some(binding.clone()),
            model: oc_core::queries::ModelRef {
                provider: "p".into(),
                id: "catalog".into(),
                variant: None,
            },
            model_label: "catalog".into(),
            span: format!("span-{index}"),
            input_start: index,
            context_limit: 100,
            input_limit: 100,
            output_limit: 10,
            estimated_input: 1,
            dcp_min_context: 1,
            dcp_max_context: 100,
            tool_fingerprint: "tools".into(),
            context_fingerprint: "context".into(),
        });
        log.input.push(InputItem::ProviderOutput(json!({"type":"reasoning","summary":[],"messages_thinking":{"type":"redacted_thinking","data":format!("opaque-{index}")}})));
    }
    let raw = log.to_json();
    assert!(
        serde_json::to_string(&log.input_for_bound("catalog", "p", Some(&a)))
            .unwrap()
            .contains("opaque-1")
    );
    assert!(
        !serde_json::to_string(&log.input_for_bound("catalog", "p", Some(&a)))
            .unwrap()
            .contains("opaque-2")
    );
    let selected = log.selected_closed_groups(|_| true).unwrap();
    assert_eq!(selected.binding, log.binding);
    let working = log
        .current_working_checkpoint("summary", log.closed_counts(), |_| true)
        .unwrap();
    let hot = TurnLog::from_json(&working).unwrap();
    assert_eq!(hot.binding, log.binding);
    assert_eq!(hot.protocol, NativeProtocol::Messages);
    assert_eq!(hot.requests, log.requests);
    assert_eq!(log.to_json(), raw);
}
