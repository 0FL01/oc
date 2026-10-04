use super::*;

fn seeded() -> LoadedDefs {
    let mut defs = LoadedDefs::default();
    for agent in builtin_agents() {
        defs.agents.insert(agent.id.clone(), agent);
    }
    defs
}

#[test]
fn builtin_registration_supplied_field_merge_and_explicit_eligibility() {
    let mut defs = seeded();
    assert_eq!(defs.agents.len(), 4);
    for id in ["build", "plan"] {
        assert!(defs.agents[id].primary_capable());
        assert!(!defs.agents[id].subagent_capable());
    }
    for id in ["general", "explore"] {
        assert!(!defs.agents[id].primary_capable());
        assert!(defs.agents[id].subagent_capable());
    }
    assert!(defs.agents["general"].body.is_empty());
    assert!(defs.agents["explore"].body.contains("very thorough"));
    let original = agent_digest(&defs.agents["explore"]);
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{
            "explore":{"model":{"providerID":"fixture","modelID":"own","variant":"careful"},"hidden":true,"permissions":{"custom_*":"ask"}},
            "new":{"description":"new profile"}
        }}),
        "global",
    );
    let body = defs.agents["explore"].body.clone();
    let changed = agent_digest(&defs.agents["explore"]);
    assert_ne!(original, changed);
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agent":{
            "explore":{"description":"local description","permission":{"custom__blocked":"deny"}},
            "build":{"mode":"all","system":"own Build system"}
        }}),
        "project",
    );
    assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
    let explore = &defs.agents["explore"];
    assert_eq!(explore.body, body);
    assert_eq!(explore.model.as_deref(), Some("fixture/own"));
    assert_eq!(explore.variant.as_deref(), Some("careful"));
    assert_eq!(explore.mode.as_deref(), Some("subagent"));
    assert!(explore.hidden);
    assert!(defs.agents["new"].primary_capable());
    assert!(!defs.agents["new"].subagent_capable());
    assert!(defs.agents["build"].subagent_capable());
    assert_eq!(defs.agents["build"].body, "own Build system");
    assert_ne!(changed, agent_digest(explore));
    let authority = BTreeMap::from([("*".into(), Permission::Allow)]);
    assert_eq!(
        explore
            .permission_rules
            .evaluate(&authority, "custom__blocked", "*"),
        Permission::Deny
    );
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agent":{"explore":{"disabled":true}}}),
        "project",
    );
    assert!(!defs.agents.contains_key("explore"));
}

#[test]
fn markdown_partial_override_retains_builtin_mode_system_and_prior_model() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("agents")).unwrap();
    std::fs::write(
        temp.path().join("agents/explore.md"),
        "---\ndescription: local\n---\nMarkdown search system",
    )
    .unwrap();
    let mut defs = seeded();
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agent":{"explore":{"model":"fixture/own","variant":"careful"}}}),
        "global",
    );
    merge_definition_root(
        &mut defs,
        &DefRoot {
            dir: temp.path().into(),
            origin: "markdown".into(),
        },
    );
    let explore = &defs.agents["explore"];
    assert_eq!(explore.mode.as_deref(), Some("subagent"));
    assert_eq!(explore.model.as_deref(), Some("fixture/own"));
    assert_eq!(explore.variant.as_deref(), Some("careful"));
    assert_eq!(explore.body, "Markdown search system");
}
