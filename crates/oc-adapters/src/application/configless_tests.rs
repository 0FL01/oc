use super::*;
use oc_core::{domain::SessionId, queries::SessionProbe};

#[tokio::test]
async fn go05_configless_home_history_and_restart_never_invent_a_model_or_accept_a_turn() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session(
            "existing",
            &project.canonicalize().unwrap().to_string_lossy(),
        )
        .unwrap();
        db.append_message("existing", "user", "LOCAL_HISTORY")
            .unwrap();
        // Fresh public metadata cache isolates this offline test from network.
        db.set_pref(
            crate::models_dev::CACHE_KEY,
            &serde_json::json!({
                "source":crate::models_dev::SOURCE,
                "fetched_at_ms":composition::go_catalog::now_ms(),
                "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{}}
            })
            .to_string(),
        )
        .unwrap();
    }
    for _ in 0..2 {
        let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
            .await
            .unwrap();
        let home = app.catalog().await.unwrap();
        assert!(home.selected_model().is_none());
        assert!(home.model_id.is_empty());
        assert!(home.variant.is_none());
        assert_eq!(
            home.chrome.selection.unwrap().diagnostic.code,
            oc_core::queries::ServiceCode::ModelUnavailable
        );
        assert_eq!(
            app.history_page(SessionId("existing".into()), None, None, 20)
                .await
                .unwrap()
                .rows[0]
                .text,
            "LOCAL_HISTORY"
        );
        assert!(
            app.submit(SessionId("fresh".into()), "must refuse".into())
                .await
                .is_err()
        );
        assert_eq!(
            app.probe_session(SessionId("fresh".into())).await.unwrap(),
            SessionProbe::Absent
        );
        assert!(
            app.submit(SessionId("existing".into()), "must refuse".into())
                .await
                .is_err()
        );
        assert_eq!(
            app.history_page(SessionId("existing".into()), None, None, 20)
                .await
                .unwrap()
                .rows
                .len(),
            1
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
    let db = Db::open(&data).unwrap();
    assert!(db.list_tool_ops("existing").unwrap().is_empty());
    drop(db);
    let conn = rusqlite::Connection::open_with_flags(
        data.join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turns", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn go05_missing_optional_provider_or_model_does_not_block_local_views() {
    let root = tempfile::tempdir().unwrap();
    for document in [
        serde_json::json!({"model":"unconfigured/exact/id"}),
        serde_json::json!({"model":"fixture/retired", "provider":{"fixture":{
            "options":{"baseURL":"https://example.com/v1","apiKey":"fixture"},"models":{}}}}),
        serde_json::json!({"model":"opencode-go/disabled-choice", "disabled_providers":["opencode-go"]}),
        serde_json::json!({"model":"fixture/m", "provider":{"fixture":{
            "options":{"apiKey":"fixture-no-endpoint"},"models":{"m":{}}}}}),
    ] {
        std::fs::write(root.path().join("opencode.json"), document.to_string()).unwrap();
        let (app, guard, _) =
            spawn_with_env(root.path(), &root.path().join("data"), BTreeMap::new())
                .await
                .unwrap();
        let snapshot = app.catalog().await.unwrap();
        assert!(snapshot.selected_model().is_some());
        assert!(
            snapshot.chrome.selection.is_some()
                || snapshot.chrome.provider.as_ref().unwrap().status
                    != oc_core::queries::ProviderStatus::Ready
        );
        assert!(
            app.submit(SessionId("unready".into()), "refuse".into())
                .await
                .is_err()
        );
        assert_eq!(
            app.probe_session(SessionId("unready".into()))
                .await
                .unwrap(),
            SessionProbe::Absent
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
    std::fs::write(
        root.path().join("opencode.json"),
        r#"{"permission":{"read":123}}"#,
    )
    .unwrap();
    assert!(
        spawn_with_env(root.path(), &root.path().join("bad-data"), BTreeMap::new())
            .await
            .is_err()
    );
    assert!(
        !root.path().join("bad-data").exists(),
        "mandatory policy fails before storage startup"
    );
}
