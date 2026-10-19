use super::*;
use serde_json::json;

fn paths(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.path.clone())
        .collect()
}

#[test]
fn cfg11_document_reports_every_unknown_key_once_without_values() {
    let document = json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {"a": {"npm": "@ai-sdk/openai"}},
        "providers": {
            "a": {"npm": "@ai-sdk/openai", "options": {"apiKey": "SECRET-VALUE"},
                  "models": {"m": {"id": "x", "tool_call": true, "reasoning": true,
                                   "modalities": {}, "limit": {"context": 1, "tokens": 2},
                                   "capabilities": {"tools": true, "vision": true},
                                   "variants": [{"id": "high", "reasoningEffort": "high"}]}}},
            "b": {"npm": "@ai-sdk/openai"}
        },
        "mcp": {
            "timeout": {"catalog": 1, "connect": 2},
            "web": {"type": "remote", "url": "https://example"},
            "servers": {"x": {"type": "remote", "url": "u", "enabled": true,
                              "oauth": {"clientId": "c"}, "metadata": "m"}}
        },
        "permission": {"bash": "ask"},
        "tools": {"write": false},
        "agents": {"x": {"prompt": "p", "variant": "high", "request": {"query": 1}}},
        "commands": {"c": {"template": "t", "variant": "high"}},
        "plugins": ["dcp", {"package": "p", "version": 1}],
        "snapshot": true,
        "experimental": {"mcp_timeout": 1, "subagent_depth": 2},
        "compaction": {"reserved": 1, "auto": true, "keep": {"turns": 1}},
        "bad key\n": 1
    });
    let findings = paths(&scan_document(&document));
    for expected in [
        "provider",
        "providers.<id>.npm",
        "providers.<id>.options",
        "providers.<id>.models.<id>.id",
        "providers.<id>.models.<id>.tool_call",
        "providers.<id>.models.<id>.reasoning",
        "providers.<id>.models.<id>.modalities",
        "providers.<id>.models.<id>.limit.tokens",
        "providers.<id>.models.<id>.capabilities.vision",
        "providers.<id>.models.<id>.variants[].reasoningEffort",
        "mcp.timeout.connect",
        "mcp.web",
        "mcp.servers.<id>.enabled",
        "mcp.servers.<id>.metadata",
        "mcp.servers.<id>.oauth.clientId",
        "permission",
        "tools",
        "agents.<id>.prompt",
        "agents.<id>.variant",
        "agents.<id>.request.query",
        "commands.<id>.variant",
        "plugins[].version",
        "snapshot",
        "experimental.mcp_timeout",
        "compaction.reserved",
        "compaction.keep.turns",
        "<key>",
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
            .filter(|line| *line == "providers.<id>.npm")
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
        "default_agent": "build",
        "providers": {
            "a": {"package": "@opencode/ai/providers/openai", "name": "A",
                  "settings": {"baseURL": "u", "apiKey": "k", "timeout": false,
                               "chunkTimeout": 1, "anyPackageSetting": {"nested": true}},
                  "headers": {"X-A": "1"}, "body": {"store": false},
                  "compatibility": {"supportsPromptCacheKey": true},
                  "models": {"m": {"modelID": "x", "name": "M",
                                   "capabilities": {"tools": true, "input": ["text"], "output": ["text"]},
                                   "compatibility": {"reasoningField": "reasoning_content"},
                                   "limit": {"context": 1, "input": 1, "output": 1},
                                   "settings": {"thinking": {"type": "adaptive"}},
                                   "variants": [{"id": "high", "settings": {"reasoningEffort": "high"}}]}}},
            "openai": {"models": {"gpt": {"limit": {"context": 1}}}}
        },
        "mcp": {"timeout": {"catalog": 1}, "servers": {
            "web": {"type": "remote", "url": "u", "disabled": true, "oauth": false,
                    "headers": {"Authorization": "Bearer k"}, "timeout": {"execution": 1}},
            "local": {"type": "local", "command": ["x"], "environment": {"A": "1"},
                      "disabled": true, "timeout": {"catalog": 1, "execution": 1}}}},
        "permissions": [{"action": "edit", "resource": "*", "effect": "allow"}],
        "agents": {"x": {"system": "s", "color": "#a0b0c0", "mode": "primary",
                         "request": {"headers": {}, "body": {}},
                         "permissions": [{"action": "shell", "resource": "*", "effect": "ask"}]}},
        "commands": {"c": {"template": "t", "subtask": true}},
        "plugins": ["@tarquinen/opencode-dcp@3.2.0", {"package": "p", "options": {"any": 1}}],
        "snapshots": false,
        "update": "notify",
        "experimental": {"subagent_depth": 2, "policies": []},
        "compaction": {"auto": true, "keep": {"tokens": 1}, "buffer": 2},
        "skills": ["x"],
        "dcp": {"enabled": true},
        "animations": false,
        "terminal": {"copy": "select"}
    });
    assert_eq!(scan_document(&document), Vec::new());
}

#[test]
fn repository_example_uses_admitted_keys_and_safe_defaults() {
    let document = crate::config::parse_jsonc(
        include_str!("../../../../examples/opencode.jsonc"),
        "examples/opencode.jsonc",
    )
    .unwrap();
    assert!(scan_document(&document).is_empty());
    assert_eq!(
        document["mcp"]["servers"]["chrome-devtools"]["disabled"],
        true
    );
    assert_eq!(document["mcp"]["servers"]["codex_web"]["oauth"], false);
    let provider = &document["providers"]["ludka2"];
    assert_eq!(provider["package"], "@opencode/ai/providers/openai");
    assert!(provider["settings"].get("setCacheKey").is_none());
    if let Some(models) = provider.get("models") {
        assert!(models.as_object().unwrap().is_empty());
    }
}

#[test]
fn cfg11_v2_schema_values_and_unsupported_capabilities_are_not_unknown_keys() {
    use crate::config::{ConfigError, Source, assemble, provider_wire};
    use std::collections::BTreeMap;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v2-config-semantics.json"
    ))
    .unwrap();
    for case in fixture["schema_cases"].as_array().unwrap() {
        assert!(
            scan_document(&case["document"]).is_empty(),
            "{}",
            case["id"]
        );
        let documents = [
            fixture["base"].clone(),
            case["document"].clone(),
            json!({"providers":{"fixture":{"package":"@opencode/ai/providers/openai-compatible"}}}),
        ];
        let sources = documents
            .iter()
            .enumerate()
            .map(|(index, document)| Source {
                path: format!("/schema-{index}.json"),
                text: document.to_string(),
                trusted: false,
            })
            .collect::<Vec<_>>();
        let result = assemble(&sources, &BTreeMap::new(), None)
            .and_then(|generation| provider_wire("fixture", &generation.providers["fixture"]));
        let observed = match &result {
            Ok(_) => "supported",
            Err(ConfigError::Invalid { .. }) => "invalid",
            Err(ConfigError::UnsupportedCapability { .. }) => "unsupported",
            other => panic!("unexpected schema result: {other:?}"),
        };
        assert_eq!(observed, case["native"].as_str().unwrap(), "{}", case["id"]);
        if let Err(error) = result {
            assert!(!error.to_string().contains("SCHEMA_VALUE_CANARY"));
        }
    }
}

#[test]
fn cfg11_markdown_and_cli_findings() {
    let agent = json!({"description": "d", "mode": "subagent", "variant": "high",
                       "permission": {"edit": "deny"}, "maxSteps": 3, "anything": 1});
    assert_eq!(
        paths(&scan_agent_frontmatter(agent.as_object().unwrap())),
        ["permission", "maxSteps", "anything"]
    );
    let cli = json!({"$schema": "https://opencode.ai/v2/cli.json",
                     "theme": {"name": "opencode", "accent": 1}, "leader_timeout": 1,
                     "leader": {"timeout": 1}, "terminal": {"copy_on_select": true},
                     "experimental": {"anything": true},
                     "plugins": ["x", {"package": "y", "options": {}}],
                     "keybinds": {"session_undo": "u", "session.redo": "r", "agent.cycle": "tab"}});
    assert_eq!(
        paths(&scan_cli(&cli)),
        [
            "leader_timeout",
            "theme.accent",
            "terminal.copy_on_select",
            "keybinds.session_undo",
        ]
    );
}

#[test]
fn cfg11_report_counts_repeats_and_bounds_lines() {
    let mut report = Report::default();
    let finding = Finding {
        path: "permission".into(),
    };
    report.add("global agent definitions", std::slice::from_ref(&finding));
    report.add("global agent definitions", std::slice::from_ref(&finding));
    for index in 0..70 {
        report.add(&format!("scope{index}"), std::slice::from_ref(&finding));
    }
    let diagnostic = report.diagnostic();
    let details = diagnostic.details.as_deref().unwrap();
    assert_eq!(diagnostic.code, ServiceCode::UnknownConfigKey);
    assert_eq!(details[0], "global agent definitions: permission (×2)");
    assert_eq!(details.len(), MAX_LINES + 1);
    assert_eq!(details[MAX_LINES], "… 7 more");
    let text = diagnostic.to_string();
    assert!(text.contains("unknown_config_key"), "{text}");
    assert!(text.contains("\n  global agent definitions"), "{text}");
}
