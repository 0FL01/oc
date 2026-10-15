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
            "explore":{"model":{"providerID":"fixture","modelID":"own","variant":"careful"},"hidden":true,"permissions":[{"action": "custom_*", "resource": "*", "effect": "ask"}]},
            "new":{"description":"new profile"}
        }}),
        "global",
    );
    let body = defs.agents["explore"].body.clone();
    let changed = agent_digest(&defs.agents["explore"]);
    assert_ne!(original, changed);
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{
            "explore":{"description":"local description", "permissions": [{"action": "custom__blocked", "resource": "*", "effect": "deny"}]},
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
        &serde_json::json!({"agents":{"explore":{"disabled":true}}}),
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
        &serde_json::json!({"agents":{"explore":{"model":"fixture/own","variant":"careful"}}}),
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

const CONFIG_SKILL: &str = "customize-opencode";

fn registered() -> LoadedDefs {
    let mut defs = LoadedDefs::default();
    register_builtins(&mut defs);
    defs
}

fn skill_snapshot(defs: &LoadedDefs) -> crate::tools::SkillSnapshot {
    let files = defs
        .skills
        .values()
        .map(|s| (s.id.clone(), s.body.clone()))
        .collect::<Vec<_>>();
    let (snapshot, warnings) = crate::tools::SkillSnapshot::build(&files);
    assert!(warnings.is_empty(), "{warnings:?}");
    snapshot
}

#[test]
fn skill01_builtin_registration_pins_a_complete_metadata_only_manual() {
    let defs = registered();
    assert!(defs.skills.contains_key(CONFIG_SKILL));
    let skill = &defs.skills[CONFIG_SKILL];
    assert_eq!(skill.name, CONFIG_SKILL);
    assert_eq!(skill.origin, "builtin");
    assert!(skill.description.contains("native Rust oc"));
    assert!(skill.body.len() < 51_200);
    assert!(skill.body.lines().count() < 2_000);
    assert_eq!(
        defs.order
            .iter()
            .filter(|s| s.starts_with("skill.customize-opencode@"))
            .count(),
        1
    );
    let snapshot = skill_snapshot(&defs);
    assert_eq!(snapshot.entries[CONFIG_SKILL].body, skill.body);
    let projection = snapshot.projection();
    assert_eq!(
        serde_json::to_value(projection).unwrap(),
        serde_json::json!([{
            "id": CONFIG_SKILL, "name": skill.name, "description": skill.description
        }])
    );
    assert!(crate::tools::preflight_skill(&snapshot, CONFIG_SKILL).is_ok());
}

#[test]
fn skill01_override_provenance_invalid_collision_and_hidden_preview_keep_callable_body() {
    let temp = tempfile::tempdir().unwrap();
    let mut defs = registered();
    assert!(defs.skills.contains_key(CONFIG_SKILL));
    for (origin, body, hidden) in [
        ("global", "Global configuration guide", false),
        ("project", "Project configuration guide", true),
    ] {
        let root = temp.path().join(origin);
        std::fs::create_dir_all(root.join("skills")).unwrap();
        let text = format!(
            "---\nname: {CONFIG_SKILL}\ndescription: Native configuration override\nmetadata:\n  opencode/autoinvoke: {}\n---\n{body}\n",
            !hidden
        );
        std::fs::write(root.join(format!("skills/{CONFIG_SKILL}.md")), &text).unwrap();
        merge_definition_root(
            &mut defs,
            &DefRoot {
                dir: root,
                origin: origin.into(),
            },
        );
        assert_eq!(defs.skills[CONFIG_SKILL].body, text);
        assert_eq!(defs.skills[CONFIG_SKILL].origin, origin);
    }
    assert_eq!(
        defs.shadowed,
        vec![
            "skill.customize-opencode: builtin -> global",
            "skill.customize-opencode: global -> project"
        ]
    );
    assert_eq!(
        defs.order
            .iter()
            .filter(|s| s.starts_with("skill.customize-opencode@"))
            .count(),
        1
    );
    let valid = defs.skills[CONFIG_SKILL].clone();
    let snapshot = skill_snapshot(&defs);
    assert!(snapshot.projection().is_empty());
    assert_eq!(snapshot.entries[CONFIG_SKILL].body, valid.body);
    assert!(crate::tools::preflight_skill(&snapshot, CONFIG_SKILL).is_ok());
    std::fs::write(
        temp.path()
            .join(format!("project/skills/{CONFIG_SKILL}.md")),
        "---\ndescription: false\n---\nInvalid replacement",
    )
    .unwrap();
    merge_definition_root(
        &mut defs,
        &DefRoot {
            dir: temp.path().join("project"),
            origin: "project".into(),
        },
    );
    assert_eq!(defs.skills[CONFIG_SKILL], valid);
    assert_eq!(defs.diagnostics.len(), 1);
    assert_eq!(
        defs.diagnostics[0].failure.code,
        oc_core::queries::ServiceCode::InvalidDefinition
    );
}

#[test]
fn skill01_capacity_allows_replacement_but_not_new_ids_or_byte_budget_bypass() {
    let defs = registered();
    let total_bytes = defs.skills.values().map(|s| s.body.len()).sum::<usize>()
        + defs.agents.values().map(|a| a.body.len()).sum::<usize>()
        + defs.commands.values().map(|c| c.body.len()).sum::<usize>();
    let mut out = Collector {
        defs,
        total_bytes,
        observe: None,
    };
    let root = DefRoot {
        dir: PathBuf::from("native-skill-fixture"),
        origin: "project".into(),
    };
    for index in 0..MAX_DEFS_PER_KIND {
        if out.defs.skills.len() == MAX_DEFS_PER_KIND {
            break;
        }
        let id = format!("slot-{index}");
        insert_skill_text(
            &mut out,
            &root,
            id,
            String::new(),
            "---\ndescription: Valid slot\n---\nSmall body".into(),
            Path::new("native-skill-fixture/slot.md"),
        );
    }
    assert_eq!(out.defs.skills.len(), 256);
    assert!(out.defs.diagnostics.is_empty());
    let text = "---\ndescription: Replacement at capacity\n---\nLatest valid body";
    insert_skill_text(
        &mut out,
        &root,
        CONFIG_SKILL.into(),
        String::new(),
        text.into(),
        Path::new("native-skill-fixture/customize-opencode.md"),
    );
    assert!(
        out.defs.diagnostics.is_empty(),
        "{:?}",
        out.defs.diagnostics
    );
    assert_eq!(out.defs.skills[CONFIG_SKILL].body, text);
    let before = out.total_bytes;
    let valid = out.defs.skills[CONFIG_SKILL].clone();
    insert_skill_text(
        &mut out,
        &root,
        "new-id".into(),
        String::new(),
        "New ID".into(),
        Path::new("native-skill-fixture/new-id.md"),
    );
    assert!(!out.defs.skills.contains_key("new-id"));
    insert_skill_text(
        &mut out,
        &root,
        CONFIG_SKILL.into(),
        String::new(),
        "x".repeat(MAX_TOTAL_BYTES + 1),
        Path::new("native-skill-fixture/customize-opencode.md"),
    );
    assert_eq!(out.defs.skills[CONFIG_SKILL], valid);
    assert_eq!(out.total_bytes, before);
    assert_eq!(out.defs.diagnostics.len(), 2);
    assert!(
        out.defs
            .diagnostics
            .iter()
            .all(|d| d.failure.code == oc_core::queries::ServiceCode::CapacityExceeded)
    );
}
