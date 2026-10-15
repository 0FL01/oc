//! Canonical package JSON versus legacy rejection before actual effects.
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
fn cfg11_canonical_opaque_settings_work_but_v1_stops_before_mcp_and_provider() {
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
    for (id, target, legacy) in [
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
            .insert(id.into(), legacy);
        fixture.write_base_config(additions);
        let mut process = fixture.spawn(&fixture.project_a, "s-legacy-refused", "must not run", id);
        assert!(!fixture.wait_for_preflight_failure(&mut process).success());
        let diagnostic = process.diagnostics();
        assert!(diagnostic.contains("legacy_config"), "{id}: {diagnostic}");
        assert!(!diagnostic.contains("SCHEMA_VALUE_CANARY"));
        assert!(!fixture.project_a.join("r1-mcp-started").exists(), "{id}");
        fixture.assert_no_request();
        fixture.assert_no_loader_execution();
    }
}
