//! Canonical settings and ignored excess keys preserve usable configuration.
use super::*;

fn active_probe() -> Value {
    json!({
        "permissions":[{"action":"*","resource":"*","effect":"allow"}],
        "mcp":{"servers":{"probe":{"type":"local","command":[
            "/bin/sh","-c","printf started > r1-mcp-started"
        ]}}}
    })
}

#[test]
fn cfg11_canonical_settings_and_unknown_warnings_keep_mcp_and_provider() {
    let fixture = Fixture::new();
    let mut additions = fixture.base_config();
    merge_object(&mut additions, active_probe());
    additions["providers"]["fixture"]["settings"]["setCacheKey"] = json!({"vendor":"opaque"});
    fixture.write_base_config(additions);
    let mut process = fixture.spawn(
        &fixture.project_a,
        "s-canonical-control",
        "control",
        "canonical",
    );
    let (mut socket, _) = fixture.accept(&mut process);
    respond_text(&mut socket, "CANONICAL_V2_OK");
    fixture.respond_title(&mut process);
    assert!(process.wait().success(), "{}", process.diagnostics());
    assert_eq!(
        fs::read_to_string(fixture.project_a.join("r1-mcp-started")).unwrap(),
        "started",
        "positive control really starts the explicitly configured local peer"
    );
    fixture.assert_no_loader_execution();

    let model_path = format!("/providers/fixture/models/{MODEL}");
    for (id, target, unknown) in [
        (
            "provider",
            "",
            json!({"old":{"options":{"apiKey":"SCHEMA_VALUE_CANARY"}}}),
        ),
        ("permission", "", json!({"read":"allow"})),
        ("npm", "/providers/fixture", json!("SCHEMA_VALUE_CANARY")),
        (
            "options",
            "/providers/fixture",
            json!({"apiKey":"SCHEMA_VALUE_CANARY"}),
        ),
        ("modalities", model_path.as_str(), json!({"input":["text"]})),
    ] {
        let fixture = Fixture::new();
        let mut additions = fixture.base_config();
        merge_object(&mut additions, active_probe());
        additions
            .pointer_mut(target)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(id.into(), unknown);
        fixture.write_base_config(additions);
        let mut process = fixture.spawn(&fixture.project_a, "s-excess", "continue", id);
        let (mut socket, request) = fixture.accept(&mut process);
        assert_eq!(request["model"], MODEL);
        assert!(!request.to_string().contains("SCHEMA_VALUE_CANARY"));
        respond_text(&mut socket, "KNOWN_SIBLINGS_KEPT");
        fixture.respond_title(&mut process);
        assert!(process.wait().success(), "{}", process.diagnostics());
        let diagnostic = process.diagnostics();
        assert_eq!(
            diagnostic.contains("unknown_config_key"),
            id != "permission" && id != "provider"
        );
        assert!(!diagnostic.contains("SCHEMA_VALUE_CANARY"));
        assert_eq!(process.output().trim(), "KNOWN_SIBLINGS_KEPT");
        assert_eq!(
            fs::read_to_string(fixture.project_a.join("r1-mcp-started")).unwrap(),
            "started"
        );
        fixture.assert_no_request();
        fixture.assert_no_loader_execution();
    }
}
