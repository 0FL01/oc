use super::*;

fn config() -> serde_json::Value {
    serde_json::json!({"model":"fixture/org/main","providers":{"fixture":{"settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"org/main":{"variants":[{"id": "fast", "settings": {"reasoningEffort": "high"}}]}}}},"agents":{
        "hidden":{"hidden":true,"system":"hidden own"},
        "worker":{"mode":"subagent"},"removed":{"disabled":true},
        "z-first":{"system":"first"},"a-second":{"system":"second"}
    }})
}

#[tokio::test]
async fn w9_model_object_keeps_provider_model_and_variant() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let mut value = config();
    value["model"] =
        serde_json::json!({"providerID":"fixture","model":"org/main","variant":"fast"});
    std::fs::write(project.join("opencode.json"), value.to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &root.path().join("data"), BTreeMap::new())
        .await
        .unwrap();
    let catalog = app.catalog().await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(catalog.model_id, "org/main");
    assert_eq!(catalog.variant.as_deref(), Some("fast"));
}

#[tokio::test]
async fn w5_skill_capacity_omissions_warn_on_startup_and_reload() {
    for (count, body_bytes) in [(4100, 32), (90, 768 * 1024)] {
        for preload in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let project = temp.path().join("project");
            let data = temp.path().join("data");
            std::fs::create_dir_all(&project).unwrap();
            let mut source = config();
            source["commands"] = serde_json::json!({"usable":{"template":"Kept sibling"}});
            std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
            let write_skills = || {
                for index in 0..count {
                    let dir = project.join(format!(".opencode/skills/slot-{index:03}"));
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        dir.join("SKILL.md"),
                        format!(
                            "---\ndescription: Kept skill\n---\n{}",
                            "x".repeat(body_bytes)
                        ),
                    )
                    .unwrap();
                }
            };
            if preload {
                write_skills();
            }
            let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
                .await
                .unwrap();
            let catalog = if preload {
                app.catalog().await.unwrap()
            } else {
                write_skills();
                app.reload_location().await.unwrap().catalog
            };
            let skills = app.skills().await.unwrap();
            assert!(!skills.is_empty() && skills.len() < count);
            assert!(catalog.commands.iter().any(|id| id == "usable"));
            assert_eq!(catalog.agent_id.as_deref(), Some("build"));
            assert!(
                catalog
                    .chrome
                    .service_diagnostics
                    .iter()
                    .any(|d| d.code == oc_core::queries::ServiceCode::CapacityExceeded)
            );
            app.shutdown().await.unwrap();
            guard.join().await.unwrap();
        }
    }
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
async fn w8_config_agent_capacity_never_admits_a_missing_deny_profile_as_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = config();
    source["default_agent"] = serde_json::json!("zz-guard");
    for index in 0..4096 {
        source["agents"][format!("slot-{index:04}")] = serde_json::json!({"system":"Small body"});
    }
    source["agents"]["zz-guard"] = serde_json::json!({"system":"Guard body","permissions":[{"action":"read","resource":"*","effect":"deny"}]});
    std::fs::write(temp.path().join("opencode.json"), source.to_string()).unwrap();
    let error = match composition::load_with_env(temp.path(), BTreeMap::new()).await {
        Ok(partial) => panic!(
            "capacity omitted the deny profile and admitted {:?}",
            partial.default_agent
        ),
        Err(error) => error,
    };
    assert!(error.contains("capacity_exceeded"), "{error}");
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
            "---\nmodel: fixture/org/main#fast\npermissions:\n  - action: read\n    resource: \"*\"\n    effect: deny\n---\nVALID_SELECTED_PROFILE",
        )
        .unwrap();
        let complete = composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(complete.default_agent.as_deref(), Some("team/selected"));
        for index in 0..4097 {
            std::fs::write(agents.join(format!("noise-{index:04}.txt")), "").unwrap();
        }
        let complete = composition::load_with_env(temp.path(), BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(complete.default_agent.as_deref(), Some("team/selected"));
        assert_eq!(
            complete.agents["team/selected"].permission_rules.evaluate(
                &BTreeMap::new(),
                "read",
                "src/x"
            ),
            crate::config::Permission::Deny
        );
        for index in 0..fillers {
            std::fs::write(agents.join(format!("foreign-{index:04}.md")), "").unwrap();
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
    config["agents"]["pinned"] =
        serde_json::json!({"model":"fixture/org/main#fast","system":"pinned body"});
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
    config["agents"]["build"] = serde_json::json!({"color":"#12ab34"});
    std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    let snapshot = Effective::from_composition(&c).snapshot(&c, 1);
    assert_eq!(
        snapshot.chrome.agent_colors,
        BTreeMap::from([
            ("build".to_string(), "#12ab34".to_string()),
            ("explore".to_string(), "#7fd88f".to_string()),
            ("general".to_string(), "#9d7cd8".to_string()),
            ("plan".to_string(), "#fab283".to_string()),
            ("worker".to_string(), "#654321".to_string()),
            ("z-first".to_string(), "#123456".to_string()),
        ])
    );
    let catalog = subagent_catalog(&c).unwrap();
    assert_eq!(catalog.agents["worker"].color.as_deref(), Some("#654321"));
    assert_eq!(catalog.agents["build"].color.as_deref(), Some("#12ab34"));
    assert_eq!(catalog.agents["plan"].color.as_deref(), Some("#fab283"));
    assert_eq!(catalog.agents["general"].color.as_deref(), Some("#9d7cd8"));
    assert_eq!(catalog.agents["explore"].color.as_deref(), Some("#7fd88f"));
}

#[tokio::test]
async fn r6_hidden_profiles_do_not_occupy_categorical_color_slots() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config();
    config["agents"] = serde_json::json!({"review":{}});
    std::fs::write(temp.path().join("opencode.json"), config.to_string()).unwrap();
    let c = composition::load_with_env(temp.path(), BTreeMap::new())
        .await
        .unwrap();
    let mut selected = Effective::from_composition(&c);
    selected.set_agent(&c, "review").unwrap();
    let snapshot = selected.snapshot(&c, 1);
    assert!(!snapshot.agents.iter().any(|agent| agent.id == "title"));
    let review = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == "review")
        .unwrap();
    assert_eq!(review.color_index, 4);
    assert_eq!(
        workspace_publication(&c, &selected).agent_color_index,
        Some(4)
    );
}
