use super::*;
use serde_json::json;

fn paths(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| format!("{} → {}", finding.path, finding.replacement))
        .collect()
}

#[test]
fn cfg11_document_reports_every_v1_key_once_without_values() {
    let document = json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {
            "a": {"npm": "@ai-sdk/openai", "options": {"apiKey": "SECRET-VALUE"},
                  "models": {"m": {"id": "x", "tool_call": true, "variants": {"high": {}}}}},
            "b": {"npm": "@ai-sdk/openai"}
        },
        "mcp": {
            "timeout": 5000,
            "web": {"type": "remote", "url": "https://example", "enabled": true,
                    "timeout": 1000, "oauth": {"clientId": "c"}}
        },
        "permission": {"bash": "ask"},
        "tools": {"write": false},
        "agent": {"x": {"prompt": "p", "permission": {"edit": "deny"}, "color": "primary"}},
        "command": {"c": {"template": "t", "variant": "high"}},
        "plugin": ["dcp"],
        "snapshot": true,
        "autoupdate": false,
        "small_model": "a/m",
        "enabled_providers": ["a"],
        "experimental": {"mcp_timeout": 1, "subagent_depth": 2},
        "compaction": {"reserved": 1, "auto": true},
        "skills": {"paths": ["x"]},
        "permissions": [{"action": "bash", "resource": "*", "effect": "allow"},
                        {"action": "read", "resource": "*", "effect": "allow"}]
    });
    let findings = paths(&scan_document(&document));
    for expected in [
        "provider → providers",
        "provider.<id>.npm → package",
        "provider.<id>.options → settings / headers / body",
        "provider.<id>.models.<id>.id → modelID",
        "provider.<id>.models.<id>.tool_call → capabilities.tools",
        "provider.<id>.models.<id>.variants → variants: [{id, settings}]",
        "mcp.timeout → timeout: {startup, catalog, execution}",
        "mcp.<id> → mcp.servers.<id>",
        "mcp.<id>.enabled → disabled (inverted)",
        "mcp.<id>.oauth.clientId → client_id",
        "permission → permissions: [{action, resource, effect}]",
        "tools → permissions: [{action, resource, effect}]",
        "agent → agents",
        "agent.<id>.prompt → system",
        "agent.<id>.permission → permissions: [{action, resource, effect}]",
        "agent.<id>.color → color: \"#RRGGBB\"",
        "command.<id>.variant → model: \"provider/model#variant\"",
        "plugin → plugins",
        "snapshot → snapshots",
        "small_model → agents.title.model",
        "experimental.mcp_timeout → mcp.timeout: {catalog, execution}",
        "compaction.reserved → compaction.buffer",
        "skills → skills: [path or URL, ...]",
        "permissions[].action \"bash\" → shell",
    ] {
        assert!(
            findings.iter().any(|line| line == expected),
            "missing {expected}: {findings:#?}"
        );
    }
    // Two providers with `npm` produce one finding per document.
    assert_eq!(
        findings
            .iter()
            .filter(|line| line.starts_with("provider.<id>.npm"))
            .count(),
        1
    );
    let all = findings.join("\n");
    assert!(!all.contains("SECRET-VALUE") && !all.contains("@ai-sdk") && !all.contains("\"a\""));
}

#[test]
fn cfg12_canonical_document_has_no_findings() {
    let document = json!({
        "$schema": "https://opencode.ai/config.json",
        "model": "a/m",
        "providers": {"a": {"package": "@ai-sdk/openai", "settings": {"baseURL": "u"},
                            "models": {"m": {"modelID": "x", "variants": [{"id": "high"}]}}}},
        "mcp": {"timeout": {"catalog": 1}, "servers": {"web": {"type": "remote", "url": "u",
                "disabled": true, "oauth": false, "timeout": {"execution": 1}}}},
        "permissions": [{"action": "edit", "resource": "*", "effect": "allow"}],
        "agents": {"x": {"system": "s", "color": "#a0b0c0", "mode": "primary",
                         "permissions": [{"action": "shell", "resource": "*", "effect": "ask"}]}},
        "commands": {"c": {"template": "t", "subtask": true}},
        "plugins": ["dcp", {"package": "p", "options": {}}],
        "snapshots": true,
        "update": "notify",
        "experimental": {"subagent_depth": 2, "policies": []},
        "compaction": {"auto": true, "keep": {"tokens": 1}, "buffer": 2},
        "skills": ["x"],
        "dcp": {"enabled": true}
    });
    assert_eq!(scan_document(&document), Vec::new());
}

#[test]
fn cfg11_markdown_and_cli_findings() {
    let agent = json!({"description": "d", "mode": "subagent", "variant": "high",
                       "permission": {"edit": "deny"}, "maxSteps": 3, "anything": 1});
    let lines = paths(&scan_agent_frontmatter(agent.as_object().unwrap()));
    assert_eq!(
        lines,
        [
            "permission → permissions: [{action, resource, effect}]",
            "maxSteps → steps",
            "<key> → remove (not in the OpenCode 2 schema)",
        ]
    );
    let command = json!({"description": "d", "agent": "a", "subtask": true});
    assert!(scan_command_frontmatter(command.as_object().unwrap()).is_empty());
    let cli = json!({"theme": "opencode", "leader_timeout": 1,
                     "keybinds": {"session_undo": "u", "session.redo": "r"}});
    assert_eq!(
        paths(&scan_cli(&cli)),
        [
            "leader_timeout → leader.timeout",
            "theme → theme: {name}",
            "keybinds.session_undo → keybinds.session.undo",
        ]
    );
}

#[test]
fn cfg11_report_counts_repeats_and_bounds_lines() {
    let mut report = Report::default();
    let finding = Finding {
        path: "permission".into(),
        replacement: RULES,
    };
    report.add("global agent definitions", std::slice::from_ref(&finding));
    report.add("global agent definitions", std::slice::from_ref(&finding));
    for index in 0..70 {
        report.add(&format!("scope{index}"), std::slice::from_ref(&finding));
    }
    let diagnostic = report.diagnostic();
    assert_eq!(diagnostic.code, ServiceCode::LegacyConfig);
    assert_eq!(
        diagnostic.details[0],
        "global agent definitions: permission → permissions: [{action, resource, effect}] (×2)"
    );
    assert_eq!(diagnostic.details.len(), MAX_LINES + 1);
    assert_eq!(diagnostic.details[MAX_LINES], "… 7 more");
    assert!(
        diagnostic
            .to_string()
            .contains("\n  global agent definitions")
    );
}
