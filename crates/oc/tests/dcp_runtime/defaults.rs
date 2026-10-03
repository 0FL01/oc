//! DCP12 real loader/current owner snapshot/panel, independent of wire estimates.
use super::*;
use oc_core::domain::SessionId;

#[tokio::test]
async fn dcp12_no_file_owner_panel_precedence_restart_location() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.project.join("dcp.jsonc")).unwrap();
    let global = fixture.home.join("config/opencode");
    let config_path = global.join("opencode.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["agent"] = json!({"title":{"disable":true}});
    config["provider"]["fixture"]["models"][MODEL]["limit"] =
        json!({"context":100003,"output":2048});
    fs::write(&config_path, config.to_string()).unwrap();
    let env = std::collections::BTreeMap::from([
        ("HOME".into(), fixture.home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".into(),
            fixture.home.join("config").to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let session = SessionId("dcp12-owner".into());
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&fixture.project, &fixture.data(), env.clone())
            .await
            .unwrap();
    app.create_session(session.clone()).await.unwrap();
    let default = app.dcp_snapshot(session.clone()).await.unwrap();
    let facts = default.reminders.as_ref().unwrap();
    assert_eq!(
        (
            facts.min_context,
            facts.max_context,
            facts.model_context,
            facts.summary_buffer
        ),
        (40001, 55001, 100003, false)
    );
    assert_eq!(facts.model_key, format!("fixture/{MODEL}"));
    assert!(!facts.context_from_fallback && facts.budget_warning.is_none());
    let mut panel = oc_tui::dcp_panel::DcpPanelState::default();
    panel.set_snapshot(default.clone());
    let rows = panel.panel_rows().join("\n");
    assert!(
        rows.contains("reminders min 40K | max 55K tokens | summary buffer false"),
        "{rows}"
    );
    assert!(
        rows.contains("context 100K tokens (model metadata)"),
        "{rows}"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(app);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&fixture.project, &fixture.data(), env.clone())
            .await
            .unwrap();
    assert_eq!(
        app.dcp_snapshot(session.clone()).await.unwrap().reminders,
        default.reminders
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(app);

    // Supplied inline < standalone global < project < admitted project .opencode.
    config["dcp"] = json!({"compress":{"minContextLimit":10,"maxContextLimit":90000}});
    fs::write(&config_path, config.to_string()).unwrap();
    fs::write(
        global.join("dcp.json"),
        json!({"compress":{"minContextLimit":20}}).to_string(),
    )
    .unwrap();
    fs::write(
        fixture.project.join("dcp.jsonc"),
        json!({"compress":{"minContextLimit":30,"summaryBuffer":true}}).to_string(),
    )
    .unwrap();
    fs::create_dir(fixture.project.join(".opencode")).unwrap();
    fs::write(fixture.project.join(".opencode/dcp.json"),json!({"compress":{"modelMinLimits":{format!("fixture/{MODEL}"):"25%"},"modelMaxLimits":{format!("fixture/{MODEL}"):"65%"}}}).to_string()).unwrap();
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&fixture.project, &fixture.data(), env.clone())
            .await
            .unwrap();
    let exact = app
        .dcp_snapshot(session.clone())
        .await
        .unwrap()
        .reminders
        .unwrap();
    assert_eq!(
        (exact.min_context, exact.max_context, exact.summary_buffer),
        (25000, 65001, true)
    );
    let a_generation = app.catalog().await.unwrap().chrome.selection_generation;
    let other = fixture._root.path().join("other");
    fs::create_dir(&other).unwrap();
    fs::write(
        other.join("dcp.jsonc"),
        json!({"compress":{"minContextLimit":"40%","maxContextLimit":"55%"}}).to_string(),
    )
    .unwrap();
    app.switch_location(other.to_string_lossy().into_owned())
        .await
        .unwrap();
    let other_session = SessionId("dcp12-other".into());
    app.create_session(other_session.clone()).await.unwrap();
    let b = app
        .dcp_snapshot(other_session)
        .await
        .unwrap()
        .reminders
        .unwrap();
    assert_eq!(
        (b.min_context, b.max_context, b.summary_buffer),
        (40001, 55001, false)
    );
    assert!(app.catalog().await.unwrap().chrome.selection_generation > a_generation);
    app.switch_location(fixture.project.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert_eq!(
        app.dcp_snapshot(session.clone())
            .await
            .unwrap()
            .reminders
            .unwrap(),
        exact
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(app);
    for limits in [
        json!({}),
        json!({"context":0,"output":0}),
        json!({"output":2048}),
        json!({"context":100003}),
    ] {
        config["provider"]["fixture"]["models"][MODEL]["limit"] = limits.clone();
        fs::write(&config_path, config.to_string()).unwrap();
        let (app, guard, _) = oc_adapters::application::spawn_with_env(
            &fixture.project,
            &fixture.data(),
            env.clone(),
        )
        .await
        .unwrap();
        let f = app
            .dcp_snapshot(session.clone())
            .await
            .unwrap()
            .reminders
            .unwrap();
        let context = limits["context"]
            .as_u64()
            .filter(|c| *c > 0)
            .unwrap_or(32768);
        assert_eq!(
            (f.min_context, f.max_context, f.model_context),
            (context / 4, context * 65 / 100, context)
        );
        assert!(f.budget_warning.is_some());
        assert_eq!(f.context_from_fallback, context == 32768);
        panel.set_snapshot(app.dcp_snapshot(session.clone()).await.unwrap());
        assert!(
            panel
                .panel_rows()
                .iter()
                .any(|r| r.starts_with("budget: model"))
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        drop(app);
    }
    assert_eq!(
        fixture.listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "all owner queries/restart/Location produce zero POSTs"
    );
}
