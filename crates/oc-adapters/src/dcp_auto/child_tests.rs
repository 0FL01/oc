use super::*;
use oc_core::dcp_view::DcpUnavailable;

#[test]
fn dcp10_default_child_is_enabled_and_false_is_child_only() {
    for value in [
        serde_json::json!({}),
        serde_json::json!({"experimental":{"allowSubAgents":true}}),
    ] {
        let (config, warnings) = load_config(&value).unwrap();
        assert!(
            warnings.is_empty(),
            "obsolete ignored warning: {warnings:?}"
        );
        assert_eq!(
            config.compression_refusal(Permission::Allow, true, false),
            None
        );
        assert_eq!(
            config.compression_refusal(Permission::Allow, false, false),
            None
        );
    }
    let (config, warnings) =
        load_config(&serde_json::json!({"experimental":{"allowSubAgents":false}})).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(
        config.compression_refusal(Permission::Allow, true, true),
        Some(DcpUnavailable::ChildOptOut)
    );
    assert_eq!(
        config.compression_refusal(Permission::Allow, false, false),
        None
    );
}

#[test]
fn dcp10_child_default_never_relaxes_switches_permission_or_manual() {
    for (fragment, refusal) in [
        (
            serde_json::json!({"enabled":false}),
            DcpUnavailable::GlobalOff,
        ),
        (
            serde_json::json!({"compress":{"enabled":false}}),
            DcpUnavailable::CompressOff,
        ),
        (
            serde_json::json!({"manualMode":{"enabled":true}}),
            DcpUnavailable::ManualOnly,
        ),
    ] {
        let (config, _) = load_config(&fragment).unwrap();
        assert_eq!(
            config.compression_refusal(Permission::Allow, true, false),
            Some(refusal)
        );
        assert!(
            config
                .compression_refusal(Permission::Deny, true, true)
                .is_some()
        );
    }
    let (manual, _) = load_config(&serde_json::json!({"manualMode":{"enabled":true}})).unwrap();
    assert_eq!(
        manual.compression_refusal(Permission::Allow, false, true),
        None
    );
    assert_eq!(
        manual.compression_refusal(Permission::Deny, false, true),
        Some(DcpUnavailable::Denied)
    );
    for value in [
        serde_json::json!("false"),
        serde_json::json!(0),
        serde_json::Value::Null,
    ] {
        assert!(
            load_config(&serde_json::json!({"experimental":{"allowSubAgents":value}})).is_err()
        );
    }
}
