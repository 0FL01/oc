//! R4A: saved selections come from the existing application and data owner.
use super::*;
use oc_core::queries::SessionSelectionAction as Action;
use oc_core::queries::{ServiceAction, ServiceCode, ServiceStage};

#[tokio::test]
async fn r4a_missing_saved_primary_keeps_home_and_both_tabs_without_rewriting_preferences() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let config = |include: bool| {
        let mut config = serde_json::json!({
            "model":"fixture/main", "default_agent":"build",
            "permissions":{"read":"deny"},
            "provider":{"fixture":{"npm":"@ai-sdk/openai",
                "options":{"baseURL":"http://127.0.0.1:9/v1", "apiKey":"fixture-key"},
                "models":{"main":{},"alternate":{}}}},
            "agent":{"build":{"mode":"primary", "prompt":"BUILD_ONLY"},
                "vanished":{"mode":"primary", "prompt":"VANISHED_ONLY"}}
        });
        if !include {
            config["agent"].as_object_mut().unwrap().remove("vanished");
        }
        config.to_string()
    };
    std::fs::write(&path, config(true)).unwrap();
    let env = BTreeMap::new();
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let active = SessionId::new("r4a-active").unwrap();
    let parked = SessionId::new("r4a-parked").unwrap();
    app.create_session(active.clone()).await.unwrap();
    app.create_session(parked.clone()).await.unwrap();
    app.select_agent("vanished".into()).await.unwrap();
    app.session_selection(active.clone(), false, Action::Agent("vanished".into()))
        .await
        .unwrap();
    app.session_selection(parked.clone(), false, Action::Agent("vanished".into()))
        .await
        .unwrap();
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![active.clone(), parked.clone()],
        active: Some(active.clone()),
        ..app.tab_deck().await.unwrap()
    })
    .await
    .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let prefs = || {
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        let mut stmt = conn
            .prepare("SELECT key,value FROM prefs ORDER BY key")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
    };
    let saved = prefs();
    std::fs::write(&path, config(false)).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let home = app.home_selection(Action::Current).await.unwrap();
    let requested = selection_identity("agent", "vanished");
    assert_eq!(home.agent_id.as_deref(), Some(requested.as_str()));
    assert_eq!(home.chrome.selection.as_ref().unwrap().requested, requested);
    assert_eq!(
        home.chrome.selection.as_ref().unwrap().diagnostic.code,
        oc_core::queries::ServiceCode::AgentUnavailable
    );
    assert_eq!(home.model_id, "main");
    assert_eq!(
        app.tab_deck().await.unwrap().sessions,
        vec![active.clone(), parked.clone()]
    );
    for id in [active, parked] {
        let selection = app
            .session_selection(id.clone(), false, Action::Current)
            .await
            .unwrap();
        assert_eq!(selection.agent_id.as_deref(), Some(requested.as_str()));
        assert_eq!(
            selection.chrome.selection.as_ref().unwrap().requested,
            requested
        );
        assert_eq!(app.history_page(id, None, None, 10).await.unwrap().total, 0);
    }
    assert_eq!(
        prefs(),
        saved,
        "startup/queries must not rewrite saved selections"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn r4a_retired_pinned_model_and_variant_refuse_before_effects_then_repair_per_scope() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let config = |retired: bool| {
        let mut value = serde_json::json!({
            "model":"fixture/main", "default_agent":"build", "permissions":{"read":"deny"},
            "provider":{"fixture":{"npm":"@ai-sdk/openai",
                "options":{"baseURL":"http://127.0.0.1:9/v1", "apiKey":"fixture-key"},
                "models":{"main":{"variants":{"fast":{}}},"alternate":{"variants":{"fast":{}}}}}},
            "agent":{"build":{"mode":"primary", "prompt":"BUILD_ONLY"},
                "pinned":{"mode":"primary", "model":"fixture/alternate#fast", "prompt":"PINNED_ONLY"},
                "think":{"mode":"primary", "model":"fixture/main#fast", "prompt":"THINK_ONLY"}}
        });
        if retired {
            value["provider"]["fixture"]["models"]
                .as_object_mut()
                .unwrap()
                .remove("alternate");
            value["provider"]["fixture"]["models"]["main"]["variants"]["fast"]["disabled"] =
                true.into();
        }
        value.to_string()
    };
    std::fs::write(&path, config(false)).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let active = SessionId::new("r4a-pinned").unwrap();
    let parked = SessionId::new("r4a-variant").unwrap();
    for session in [&active, &parked] {
        app.create_session(session.clone()).await.unwrap();
    }
    app.select_agent("pinned".into()).await.unwrap();
    app.session_selection(active.clone(), false, Action::Agent("pinned".into()))
        .await
        .unwrap();
    app.session_selection(parked.clone(), false, Action::Agent("think".into()))
        .await
        .unwrap();
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![active.clone(), parked.clone()],
        active: Some(active.clone()),
        ..app.tab_deck().await.unwrap()
    })
    .await
    .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let prefs = || {
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        let mut stmt = conn
            .prepare("SELECT key,value FROM prefs ORDER BY key")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
    };
    let before = prefs();
    std::fs::write(&path, config(true)).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let scenarios = [
        (
            &active,
            ServiceCode::ModelUnavailable,
            "model",
            ServiceAction::SelectModel,
        ),
        (
            &parked,
            ServiceCode::VariantUnavailable,
            "variant",
            ServiceAction::SelectVariant,
        ),
    ];
    for (session, code, field, action) in scenarios {
        let snapshot = app
            .session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap();
        if session == &parked {
            assert_eq!(
                snapshot.model_id, "main",
                "parked saved agent must retain its own model"
            );
        }
        let issue = snapshot
            .chrome
            .selection
            .as_ref()
            .expect("saved choice unavailable");
        assert_eq!(
            (
                issue.diagnostic.code,
                issue.diagnostic.stage,
                issue.diagnostic.action
            ),
            (code, ServiceStage::Admission, action)
        );
        assert_eq!(issue.diagnostic.field, ["selection", field]);
        assert!(issue.requested.starts_with(&format!("{field}-")));
        let CoreError::Diagnostic(error) = app
            .submit(session.clone(), "never accepted".into())
            .await
            .unwrap_err()
        else {
            panic!("must refuse before accepting a turn")
        };
        assert_eq!(error, issue.diagnostic);
        assert_eq!(
            app.history_page(session.clone(), None, None, 10)
                .await
                .unwrap()
                .total,
            0
        );
    }
    assert_eq!(
        prefs(),
        before,
        "read/refusal cannot rewrite selection or deck"
    );
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    for table in ["turns", "turn_acceptances", "tool_operations"] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "no {table} effects");
    }
    let repaired = app
        .session_selection(active.clone(), false, Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(repaired.agent_id.as_deref(), Some("build"));
    assert!(repaired.chrome.selection.is_none());
    let repaired = app
        .session_selection(parked.clone(), false, Action::Variant(None))
        .await
        .unwrap();
    assert_eq!(repaired.variant, None);
    assert!(repaired.chrome.selection.is_none());
    let home = app
        .home_selection(Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(home.agent_id.as_deref(), Some("build"));
    assert!(home.chrome.selection.is_none());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("build")
    );
    for (session, agent) in [(active, "build"), (parked, "think")] {
        let selection = app
            .session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap();
        assert_eq!(selection.agent_id.as_deref(), Some(agent));
        assert!(selection.chrome.selection.is_none());
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn r4a_existing_primary_with_invalid_pin_is_model_unavailable_in_home_and_both_tabs() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let config = |pin: &str| {
        serde_json::json!({
            "model":"fixture/main", "default_agent":"build", "permissions":{"read":"deny"},
            "provider":{"fixture":{"npm":"@ai-sdk/openai",
                "options":{"baseURL":"http://127.0.0.1:9/v1", "apiKey":"fixture-key"},
                "models":{"main":{}, "alternate":{}}}},
            "agent":{"build":{"mode":"primary"},
                "pinned":{"mode":"primary", "model":pin, "prompt":"PIN_ONLY"}}
        })
        .to_string()
    };
    std::fs::write(&path, config("fixture/alternate")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let active = SessionId::new("r4a-invalid-pin-active").unwrap();
    let parked = SessionId::new("r4a-invalid-pin-parked").unwrap();
    for session in [&active, &parked] {
        app.create_session(session.clone()).await.unwrap();
    }
    app.select_agent("pinned".into()).await.unwrap();
    for session in [&active, &parked] {
        app.session_selection(session.clone(), false, Action::Agent("pinned".into()))
            .await
            .unwrap();
    }
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![active.clone(), parked.clone()],
        active: Some(active.clone()),
        ..app.tab_deck().await.unwrap()
    })
    .await
    .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let prefs = || {
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        let mut stmt = conn
            .prepare("SELECT key,value FROM prefs ORDER BY key")
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>()
    };
    let saved = prefs();
    // The existing primary is still a primary, but its new pin names a
    // different provider. It must never become a wire request or an agent error.
    let mut changed: serde_json::Value =
        serde_json::from_str(&config("foreign/secret-model")).unwrap();
    changed["provider"]["fixture"]["models"]
        .as_object_mut()
        .unwrap()
        .remove("alternate");
    std::fs::write(&path, changed.to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.tab_deck().await.unwrap().sessions,
        vec![active.clone(), parked.clone()]
    );
    let home = app.home_selection(Action::Current).await.unwrap();
    assert!(
        !format!("{home:?}").contains("foreign/secret-model"),
        "unavailable profile pins must remain private in the catalog projection"
    );
    for (selection, unavailable_model) in [
        (home, "foreign/secret-model"),
        (
            app.session_selection(active.clone(), false, Action::Current)
                .await
                .unwrap(),
            "alternate",
        ),
        (
            app.session_selection(parked.clone(), false, Action::Current)
                .await
                .unwrap(),
            "alternate",
        ),
    ] {
        assert_eq!(selection.agent_id.as_deref(), Some("pinned"));
        let issue = selection.chrome.selection.expect("pin unavailable");
        assert_eq!(issue.diagnostic.field, ["selection", "model"]);
        assert_eq!(issue.diagnostic.code, ServiceCode::ModelUnavailable);
        assert_eq!(issue.diagnostic.action, ServiceAction::SelectModel);
        assert_eq!(issue.diagnostic.stage, ServiceStage::Admission);
        assert_eq!(
            issue.requested,
            selection_identity("model", unavailable_model)
        );
        assert_eq!(selection.model_id, issue.requested);
        assert!(!format!("{issue:?}").contains("secret-model"));
    }
    for session in [&active, &parked] {
        let CoreError::Diagnostic(diagnostic) = app
            .submit(session.clone(), "refused".into())
            .await
            .unwrap_err()
        else {
            panic!("must refuse before accepting the turn")
        };
        assert_eq!(diagnostic.code, ServiceCode::ModelUnavailable);
        assert_eq!(diagnostic.field, ["selection", "model"]);
        assert_eq!(
            app.history_page(session.clone(), None, None, 10)
                .await
                .unwrap()
                .total,
            0
        );
    }
    assert_eq!(
        prefs(),
        saved,
        "reads and refusal must not rewrite stored choices"
    );
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    for table in ["turns", "turn_acceptances", "tool_operations"] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "no {table} effects");
    }
    let repaired = app
        .session_selection(active.clone(), false, Action::Model("main".into()))
        .await
        .unwrap();
    assert_eq!(repaired.agent_id.as_deref(), Some("pinned"));
    assert_eq!(repaired.model_id, "main");
    assert!(repaired.chrome.selection.is_none());
    assert_eq!(
        app.session_selection(parked.clone(), false, Action::Current)
            .await
            .unwrap()
            .chrome
            .selection
            .unwrap()
            .diagnostic
            .code,
        ServiceCode::ModelUnavailable
    );
    let repaired = app
        .home_selection(Action::Model("main".into()))
        .await
        .unwrap();
    assert_eq!(repaired.agent_id.as_deref(), Some("pinned"));
    assert!(repaired.chrome.selection.is_none());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert!(
        app.home_selection(Action::Current)
            .await
            .unwrap()
            .chrome
            .selection
            .is_none()
    );
    assert!(
        app.session_selection(active, false, Action::Current)
            .await
            .unwrap()
            .chrome
            .selection
            .is_none()
    );
    assert_eq!(
        app.session_selection(parked, false, Action::Current)
            .await
            .unwrap()
            .chrome
            .selection
            .unwrap()
            .diagnostic
            .code,
        ServiceCode::ModelUnavailable
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn r4a_changed_nonprimary_is_agent_unavailable_but_malformed_default_stays_fatal() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let config = |mode: &str, default: &str, pin: &str| {
        serde_json::json!({
            "model":"fixture/main", "default_agent":default, "permissions":{"read":"deny"},
            "provider":{"fixture":{"npm":"@ai-sdk/openai",
                "options":{"baseURL":"http://127.0.0.1:9/v1", "apiKey":"fixture-key"},
                "models":{"main":{}}}},
            "agent":{"build":{"mode":"primary", "model":pin},
                "former":{"mode":mode}}
        })
        .to_string()
    };
    std::fs::write(&path, config("primary", "build", "fixture/main")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let active = SessionId::new("r4a-nonprimary-active").unwrap();
    let parked = SessionId::new("r4a-nonprimary-parked").unwrap();
    for session in [&active, &parked] {
        app.create_session(session.clone()).await.unwrap();
    }
    app.select_agent("former".into()).await.unwrap();
    for session in [&active, &parked] {
        app.session_selection(session.clone(), false, Action::Agent("former".into()))
            .await
            .unwrap();
    }
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![active.clone(), parked.clone()],
        active: Some(active.clone()),
        ..app.tab_deck().await.unwrap()
    })
    .await
    .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    std::fs::write(&path, config("subagent", "build", "fixture/main")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.tab_deck().await.unwrap().sessions,
        vec![active.clone(), parked.clone()]
    );
    for choice in [
        app.home_selection(Action::Current).await.unwrap(),
        app.session_selection(active, false, Action::Current)
            .await
            .unwrap(),
        app.session_selection(parked, false, Action::Current)
            .await
            .unwrap(),
    ] {
        let issue = choice.chrome.selection.unwrap();
        assert_eq!(issue.diagnostic.code, ServiceCode::AgentUnavailable);
        assert_eq!(issue.diagnostic.field, ["selection", "agent"]);
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    std::fs::write(&path, config("subagent", "build", "invalid-pin")).unwrap();
    let failure = spawn_inner(&project, &data, BTreeMap::new(), false)
        .await
        .err()
        .expect("mandatory default model is malformed");
    assert_eq!(failure.category, SpawnFailure::Configuration);
    assert_eq!(failure.diagnostic.field, ["model"]);
    assert_ne!(failure.diagnostic.code, ServiceCode::AgentUnavailable);
    assert!(!format!("{failure:?}").contains("invalid-pin"));
    let mut malformed_policy: serde_json::Value =
        serde_json::from_str(&config("subagent", "build", "fixture/main")).unwrap();
    malformed_policy["agent"]["build"]["permission"] = serde_json::json!({"read": 42});
    std::fs::write(&path, malformed_policy.to_string()).unwrap();
    let failure = spawn_inner(&project, &data, BTreeMap::new(), false)
        .await
        .err()
        .expect("mandatory primary permission must remain fatal");
    assert_eq!(failure.category, SpawnFailure::Configuration);
    assert_ne!(failure.diagnostic.code, ServiceCode::AgentUnavailable);
}

#[tokio::test]
async fn r4a_malformed_nondefault_saved_pin_reports_model_not_agent_and_allows_repair() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let path = project.join("opencode.json");
    let config = |pin: &str| {
        serde_json::json!({
            "model":"fixture/main", "default_agent":"build", "permissions":{"read":"deny"},
            "provider":{"fixture":{"npm":"@ai-sdk/openai",
                "options":{"baseURL":"http://127.0.0.1:9/v1", "apiKey":"fixture-key"},
                "models":{"main":{}}}},
            "agent":{"build":{"mode":"primary"},
                "pinned":{"mode":"primary", "model":pin}}
        })
        .to_string()
    };
    std::fs::write(&path, config("fixture/main")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    app.select_agent("pinned".into()).await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    std::fs::write(&path, config("malformed-pin-canary")).unwrap();
    // An admitted profile's invalid pin must also be redacted while an
    // independent optional provider catalog attempt is Pending. This exercises
    // the projection against that exact typed owner state, not error prose.
    let mut composition = crate::composition::load_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    composition.provider_state.catalog_status = oc_core::queries::ProviderStatus::Pending;
    let mut effective = Effective::from_composition(&composition);
    assert!(effective.set_agent(&composition, "pinned").is_err());
    effective.retain_invalid_saved_agent(&composition, "pinned");
    let projected = effective.snapshot(&composition);
    assert!(!format!("{projected:?}").contains("malformed-pin-canary"));
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let home = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(home.agent_id.as_deref(), Some("pinned"));
    let issue = home.chrome.selection.unwrap();
    assert_eq!(issue.diagnostic.code, ServiceCode::ModelUnavailable);
    assert_eq!(issue.diagnostic.field, ["selection", "model"]);
    assert_eq!(
        issue.requested,
        selection_identity("model", "malformed-pin-canary")
    );
    assert!(!format!("{issue:?}").contains("malformed-pin-canary"));
    assert!(
        app.home_selection(Action::Model("main".into()))
            .await
            .unwrap()
            .chrome
            .selection
            .is_none()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
