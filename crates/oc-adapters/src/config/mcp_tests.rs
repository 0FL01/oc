//! MCP09 fixtures derived from the pinned normalizer/schema, not Serde defaults.
use super::*;

fn fixture_sources() -> Vec<Source> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/mcp09-normalization.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_commit"],
        "2670273ff17da96f85c5826ced57aa1b368754fa"
    );
    fixture["documents"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, value)| Source {
            path: format!("layer-{index}/opencode.json"),
            text: value.to_string(),
            trusted: true,
        })
        .collect()
}

#[test]
fn mcp09_donor_forms_disabled_environment_and_whole_entry_precedence() {
    let sources = fixture_sources();
    let before: Vec<_> = sources.iter().map(|s| s.text.clone()).collect();
    let generation =
        assemble(&sources, &BTreeMap::new(), None).expect("donor-shaped MCP must load");
    assert_eq!(generation.mcp.len(), 4);
    assert!(!generation.mcp["chrome-devtools"].enabled);
    assert_eq!(generation.mcp["same"].command, ["later"]);
    assert!(generation.mcp["same"].enabled);
    assert_eq!(generation.provenance["mcp.same"], "layer-1/opencode.json");
    assert_eq!(
        sources.iter().map(|s| s.text.clone()).collect::<Vec<_>>(),
        before
    );
    let same = &generation.mcp["same"];
    assert!(
        same.environment.is_empty(),
        "whole replacement removes old env"
    );
    assert_eq!(
        same.timeouts,
        McpTimeouts {
            startup: Some(101),
            catalog: Some(202),
            execution: Some(303)
        }
    );
    let chrome = &generation.mcp["chrome-devtools"];
    assert_eq!(chrome.environment["npm_config_offline"], "true");
    assert_eq!(
        chrome.timeouts,
        McpTimeouts {
            startup: Some(101),
            catalog: Some(5000),
            execution: Some(5000)
        }
    );
    assert_eq!(
        generation.provenance["mcp.timeout.startup"],
        "layer-0/opencode.json"
    );
    assert_eq!(
        generation.provenance["mcp.timeout.execution"],
        "layer-1/opencode.json"
    );
    let first = assemble(&sources[..1], &BTreeMap::new(), None).unwrap();
    assert_eq!(first.mcp["same"].command, ["canonical"]);
    assert!(!first.mcp["same"].enabled);
    assert_eq!(first.mcp["same"].timeouts.execution, Some(444));
}

fn source(value: serde_json::Value) -> Source {
    Source {
        path: "/private/source-CANARY/opencode.jsonc".into(),
        text: value.to_string(),
        trusted: true,
    }
}

#[test]
fn mcp09_reserved_legacy_names_activation_defaults_and_donor_timeout_defaults() {
    let generation = assemble(
        &[source(serde_json::json!({"mcp": {
            "servers":{"type":"local","command":["servers"]},
            "timeout":{"type":"remote","url":"https://example.invalid/exact/mcp","oauth":false},
            "enabled_only":{"enabled":false}
        }}))],
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(generation.mcp.len(), 3);
    assert!(generation.mcp["servers"].enabled);
    assert!(generation.mcp["timeout"].enabled);
    assert!(
        generation.mcp["enabled_only"].failure.is_some(),
        "donor unsupported enabled-only is visible failed inventory"
    );
    let timeout = generation.mcp["timeout"].timeouts;
    assert_eq!(
        (
            timeout.startup_ms(),
            timeout.catalog_ms(None),
            timeout.execution_ms(None)
        ),
        (30_000, 30_000, 43_200_000)
    );
}

#[test]
fn mcp09_positive_integer_json_spellings_follow_pinned_number_semantics() {
    use oc_core::queries::ServiceCode;
    // The pinned JS normalizer sees the same Number after JSON.parse for each
    // spelling; PositiveInt cannot distinguish integer-valued decimal/exponent.
    for numeric in ["1000", "1000.0", "1e3"] {
        let src = Source {
            path: "numeric/opencode.json".into(),
            trusted: true,
            text: r#"{"mcp":{
                "legacy":{"type":"local","command":["exact"],"timeout":NUMERIC},
                "timeout":{"startup":NUMERIC},
                "servers":{
                    "canonical":{"type":"local","command":["exact"],"timeout":{"catalog":NUMERIC,"execution":NUMERIC}},
                    "oauth":{"type":"remote","url":"https://example.invalid/mcp","oauth":{"callback_port":NUMERIC}}
                }
            }}"#
            .replace("NUMERIC", numeric),
        };
        let generation = assemble(&[src], &BTreeMap::new(), None).unwrap();
        for id in ["legacy", "canonical"] {
            let entry = &generation.mcp[id];
            assert!(entry.failure.is_none(), "{numeric}: {id} rejected");
            assert_eq!(entry.timeouts.startup_ms(), 1000);
            assert_eq!(entry.timeouts.catalog_ms(entry.timeout), 1000);
            assert_eq!(entry.timeouts.execution_ms(entry.timeout), 1000);
        }
        assert_eq!(generation.mcp["legacy"].timeout, Some(1000));
        assert_eq!(
            generation.mcp["oauth"].failure.as_ref().unwrap().code,
            ServiceCode::UnsupportedCapability,
            "integer-valued OAuth port is recognized, but OAuth unsupported"
        );
    }
}

#[test]
fn mcp09_failed_recognized_matrix_keeps_healthy_and_disabled_entries() {
    use oc_core::queries::ServiceCode;
    let bad = [
        (
            serde_json::json!({"type":"local","command":"CANARY"}),
            "command",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":[]}),
            "command",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"environment":{"X":9}}),
            "environment",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"environment":{"BAD=KEY":"CANARY"},"enabled":false}),
            "environment",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"timeout":0}),
            "timeout",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"timeout":-1}),
            "timeout",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"timeout":1.5}),
            "timeout",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"codemode":true}),
            "codemode",
            ServiceCode::UnsupportedCapability,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"protocol":"auto"}),
            "protocol",
            ServiceCode::UnsupportedProtocol,
        ),
        (
            serde_json::json!({"type":"local","command":["x"],"protocol":"2026-07-28"}),
            "protocol",
            ServiceCode::UnsupportedProtocol,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","environment":{"X":"CANARY"}}),
            "environment",
            ServiceCode::UnsupportedCapability,
        ),
        (
            serde_json::json!({"type":"remote","url":"CANARY","enabled":false}),
            "url",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","headers":{"Authorization":"Bearer CANARY-a","authorization":"Bearer CANARY-b"}}),
            "headers",
            ServiceCode::AuthorizationHeaderConflict,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","headers":{"X-Flag":1}}),
            "headers",
            ServiceCode::InvalidConfig,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","oauth":true}),
            "oauth",
            ServiceCode::UnsupportedCapability,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","oauth":{"clientId":"CANARY","clientSecret":"CANARY","scope":"CANARY","callbackPort":3456,"redirectUri":"CANARY"}}),
            "oauth",
            ServiceCode::UnsupportedCapability,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","oauth":{"client_id":"CANARY","client_secret":"CANARY","scope":"CANARY","callback_port":3456,"redirect_uri":"CANARY","auth_server_metadata_url":"CANARY"}}),
            "oauth",
            ServiceCode::UnsupportedCapability,
        ),
        (
            serde_json::json!({"type":"remote","url":"https://example.invalid/mcp","oauth":{"callback_port":0}}),
            "oauth.callback_port",
            ServiceCode::InvalidConfig,
        ),
    ];
    for (raw, field, code) in bad {
        let generation = assemble(&[source(serde_json::json!({"mcp":{
            "broken":raw,"healthy":{"type":"local","command":["x"],"codemode":false,"metadata":"CANARY"},
            "disabled":{"type":"local","command":["npx"],"enabled":false,"environment":{"TOKEN":"{file:missing}"}}
        }}))], &BTreeMap::new(), None).unwrap();
        assert_eq!(generation.mcp.len(), 3);
        let issue = generation.mcp["broken"].failure.as_ref().unwrap();
        assert!(issue.service.starts_with("server-") && issue.service.len() <= 64);
        assert!(!issue.service.contains("broken"));
        assert_eq!(issue.field[..2], ["mcp", "entry"]);
        assert_eq!(issue.field[2..].join("."), field);
        assert_eq!(issue.code, code);
        assert!(!generation.mcp["disabled"].enabled);
        assert!(generation.mcp["healthy"].failure.is_none());
        let safe = format!("{issue} {issue:?}");
        assert!(!safe.contains("CANARY") && !safe.contains("/private") && !safe.contains("https:"));
        assert!(!format!("{:?}", generation.mcp).contains("CANARY"));
    }
}

#[test]
fn mcp09_substitution_and_no_follow_trust_are_not_optional_failure_catches() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("overlay"), "OVERLAY-CANARY").unwrap();
    let mut src = source(serde_json::json!({"mcp":{"local":{
        "type":"local","command":["{env:PROGRAM}","two words", ""],"cwd":"{env:CWD}",
        "environment":{"OVERLAY":"{file:overlay}","INHERITED":"{env:CANARY}","PATH":"{env:PATH}"}
    }}}));
    src.path = temp
        .path()
        .join("opencode.json")
        .to_string_lossy()
        .into_owned();
    let env = BTreeMap::from([
        ("PROGRAM".into(), "exact-program".into()),
        ("CWD".into(), "nested".into()),
        ("CANARY".into(), "ENV-CANARY".into()),
        ("PATH".into(), "/fake/bin".into()),
    ]);
    let generation = assemble(&[src.clone()], &env, None).unwrap();
    assert_eq!(
        generation.mcp["local"].command,
        ["exact-program", "two words", ""]
    );
    assert_eq!(generation.mcp["local"].cwd.as_deref(), Some("nested"));
    assert_eq!(
        generation.mcp["local"].environment["OVERLAY"],
        "OVERLAY-CANARY"
    );
    let explain = explain_redacted(&generation).to_string();
    for value in ["OVERLAY-CANARY", "ENV-CANARY", "/fake/bin"] {
        assert!(!explain.contains(value));
    }
    src.trusted = false;
    assert!(matches!(
        assemble(&[src.clone()], &env, None),
        Err(ConfigError::Untrusted { .. })
    ));
    src.trusted = true;
    std::fs::remove_file(temp.path().join("overlay")).unwrap();
    std::os::unix::fs::symlink(temp.path().join("outside"), temp.path().join("overlay")).unwrap();
    assert!(matches!(
        assemble(&[src], &env, None),
        Err(ConfigError::Untrusted { .. })
    ));
}

#[test]
fn mcp09_invalid_global_timeout_and_policy_remain_fatal() {
    for raw in [
        serde_json::json!({"mcp":{"timeout":{"startup":0}}}),
        serde_json::json!({"permissions":{"read":"CANARY"},"mcp":{"broken":false}}),
    ] {
        assert!(matches!(
            assemble(&[source(raw)], &BTreeMap::new(), None),
            Err(ConfigError::Invalid { .. })
        ));
    }
}
