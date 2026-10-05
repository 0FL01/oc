use super::*;

fn config() -> serde_json::Value {
    serde_json::json!({"model":"fixture/org/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"org/main":{"variants":{"fast":{"reasoningEffort":"high"}}}}}},"agents":{
        "hidden":{"hidden":true,"system":"hidden own"},
        "worker":{"mode":"subagent"},"removed":{"disabled":true},
        "z-first":{"system":"first"},"a-second":{"system":"second"}
    }})
}

#[tokio::test]
async fn r6_primary_catalog_source_order_default_fallback_and_explicit_hidden() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config();
    let path = temp.path().join("opencode.json");
    for default in ["hidden", "worker", "removed", "absent", "z-first"] {
        config["default_agent"] = serde_json::json!(default);
        std::fs::write(&path, config.to_string()).unwrap();
        let c = composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .unwrap();
        let expected = if default == "z-first" {
            "z-first"
        } else {
            "build"
        };
        assert_eq!(c.default_agent.as_deref(), Some(expected));
        let mut selected = Effective::from_composition(&c);
        let snapshot = selected.snapshot(&c, 1);
        let ids: Vec<_> = snapshot.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            if default == "z-first" {
                vec!["z-first", "build", "plan", "a-second"]
            } else {
                vec!["build", "plan", "z-first", "a-second"]
            }
        );
        selected.set_agent(&c, "hidden").unwrap();
        assert_eq!(selected.agent_id.as_deref(), Some("hidden"));
        assert_eq!(selected.agent_prompt.as_deref(), Some("hidden own"));
        assert!(selected.set_agent(&c, "worker").is_err());
        assert!(selected.set_agent(&c, "removed").is_err());
    }
    config["agents"]["build"] = serde_json::json!({"disabled":true});
    config["agents"]["plan"] = serde_json::json!({"disabled":true});
    config["default_agent"] = serde_json::json!("worker");
    std::fs::write(&path, config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(c.default_agent.as_deref(), Some("z-first"));
    config["agents"]["z-first"]["hidden"] = serde_json::json!(true);
    config["agents"]["a-second"]["hidden"] = serde_json::json!(true);
    std::fs::write(&path, config.to_string()).unwrap();
    assert!(
        composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn r6_capacity_truncated_nested_default_never_admits_partial_catalog() {
    // Exact exhaustion skips an as-yet-unvisited child; one extra root entry
    // also exercises the existing generic overflow diagnostic. Neither may
    // turn the real, valid selected profile into an absent-default fallback.
    for fillers in [4095, 4096] {
        let temp = tempfile::tempdir().unwrap();
        let mut config = config();
        config["default_agent"] = serde_json::json!("team/selected");
        std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
        let agents = temp.path().join(".opencode/agents");
        std::fs::create_dir_all(agents.join("team")).unwrap();
        std::fs::write(
            agents.join("team/selected.md"),
            "---\nmodel: fixture/org/main#fast\n---\nVALID_SELECTED_PROFILE",
        )
        .unwrap();
        let complete = composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(complete.default_agent.as_deref(), Some("team/selected"));
        for index in 0..fillers {
            std::fs::write(agents.join(format!("foreign-{index:04}.txt")), "").unwrap();
        }
        let error = match composition::load_with_env(temp.path(), BTreeMap::new()).await {
            Ok(partial) => panic!(
                "{fillers} fillers admitted incomplete catalog as {:?}",
                partial.default_agent
            ),
            Err(error) => error,
        };
        assert!(error.contains("capacity_exceeded"), "{fillers}: {error}");
    }
}

#[tokio::test]
async fn r6_malformed_selected_nested_profile_never_falls_back_to_sibling() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config();
    config["default_agent"] = serde_json::json!("team/bad");
    std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
    let root = temp.path().join(".opencode/agents/team");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("bad.md"), "---\nmode: banana\n---\nBAD_PROFILE").unwrap();
    assert!(
        composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn r6_default_profile_embedded_variant_matches_explicit_and_top_level_reference() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.json");
    let mut config = config();
    config["default_agent"] = serde_json::json!("pinned");
    config["agents"]["pinned"] = serde_json::json!({"model":"fixture/org/main#fast","variant":"absent","system":"pinned body"});
    std::fs::write(&path, config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    let mut selected = Effective::from_composition(&c);
    assert_eq!(
        (c.model_id.as_str(), c.variant.as_deref()),
        ("org/main", Some("fast"))
    );
    selected.set_agent(&c, "pinned").unwrap();
    assert_eq!(
        (selected.model_id.as_str(), selected.variant.as_deref()),
        ("org/main", Some("fast"))
    );
    config.as_object_mut().unwrap().remove("default_agent");
    config["model"] = serde_json::json!("fixture/org/main#fast");
    std::fs::write(&path, config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        (c.model_id.as_str(), c.variant.as_deref()),
        ("org/main", Some("fast"))
    );
}

#[tokio::test]
async fn r6_loaded_profile_request_overlay_reaches_child_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config();
    config["agents"]["worker"]["request"] =
        serde_json::json!({"headers":{"X-Worker":"w"},"body":{"seed":3}});
    std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    let catalog = subagent_catalog(&c).unwrap();
    let worker = &catalog.agents["worker"].request;
    assert_eq!(worker.headers["x-worker"], "w");
    assert_eq!(worker.body["seed"], 3);
    assert!(catalog.agents["build"].request.is_empty());
}

#[tokio::test]
async fn r6_explicit_profile_color_reaches_tui_chrome_and_skips_slot_pin() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config();
    config["agents"]["z-first"]["color"] = serde_json::json!("#123456");
    config["agents"]["worker"]["color"] = serde_json::json!("#654321");
    std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    let snapshot = Effective::from_composition(&c).snapshot(&c, 1);
    assert_eq!(
        snapshot.chrome.agent_colors,
        BTreeMap::from([
            ("worker".to_string(), "#654321".to_string()),
            ("z-first".to_string(), "#123456".to_string()),
        ])
    );
    let catalog = subagent_catalog(&c).unwrap();
    assert_eq!(catalog.agents["worker"].color.as_deref(), Some("#654321"));
    assert!(catalog.agents["general"].color.is_none());
}
