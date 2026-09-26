use std::collections::BTreeMap;

use oc_adapters::application;
use oc_core::domain::SessionId;
use oc_core::session::CoreError;
use rusqlite::Connection;

fn id(raw: &str) -> SessionId {
    SessionId(raw.into())
}

fn title(conn: &Connection, session: &str) -> Option<String> {
    conn.query_row("SELECT title FROM sessions WHERE id=?1", [session], |row| {
        row.get(0)
    })
    .unwrap()
}

#[tokio::test]
async fn rename_is_durable_visible_and_rejects_invalid_foreign_or_child() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = serde_json::json!({
        "model": "fixture/m",
        "provider": {"fixture": {"npm":"@ai-sdk/openai", "options": {
            "baseURL":"http://127.0.0.1:9/v1", "apiKey":"dummy"
        }, "models":{"m":{}}}}
    })
    .to_string();
    std::fs::write(a.path().join("opencode.json"), &config).unwrap();
    std::fs::write(b.path().join("opencode.json"), config).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    app.create_session(id("root")).await.unwrap();
    let conn = Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO sessions(id,created_at,parent_id,title) VALUES ('child','test','root','child title')", []).unwrap();
    assert_eq!(
        app.rename_session(id("missing"), "No".into()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.rename_session(id("child"), "No".into()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.regenerate_title(id("child")).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.regenerate_title(id("missing")).await,
        Err(CoreError::SessionNotFound)
    );
    for bad in [
        " ",
        "a\nb",
        "\na",
        "a\u{202e}b",
        "\u{200b}",
        "a\u{200b}b",
        "a\u{200d}b",
        "\u{200d}👩",
        "👩\u{200d}",
        "👩 \u{200d}💻",
        "👩\u{200d} 💻",
        "👩\u{200d}\u{200d}💻",
        "👩\u{200d}a",
        "👩\u{200d}💻\u{202e}",
        "\u{fe0f}",
        "\u{2800}",
        &"é".repeat(129),
    ] {
        assert_eq!(
            app.rename_session(id("root"), bad.into()).await,
            Err(CoreError::Application("invalid session title".into()))
        );
        assert_eq!(title(&conn, "root"), None);
    }
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test')",
        ["tui.session_location.ghost", a.path().to_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        app.rename_session(id("ghost"), "No".into()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.regenerate_title(id("ghost")).await,
        Err(CoreError::SessionNotFound)
    );
    app.rename_session(id("root"), format!("  {}  ", "é".repeat(128)))
        .await
        .unwrap();
    assert_eq!(title(&conn, "root"), Some("é".repeat(128)));
    assert_eq!(
        app.session_list("É".into(), true).await.unwrap()[0].id,
        id("root")
    );
    app.rename_session(id("root"), "  Coding 👩🏽\u{200d}💻  ".into())
        .await
        .unwrap();
    assert_eq!(title(&conn, "root").as_deref(), Some("Coding 👩🏽\u{200d}💻"));
    assert_eq!(
        app.history_page(id("root"), None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Coding 👩🏽\u{200d}💻")
    );
    app.rename_session(id("root"), "  Hand-picked title 🦀  ".into())
        .await
        .unwrap();
    assert_eq!(
        title(&conn, "root").as_deref(),
        Some("Hand-picked title 🦀")
    );
    let roots = app.session_list(String::new(), false).await.unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, id("root"));
    assert_eq!(roots[0].title, "Hand-picked title 🦀");
    assert!(roots[0].updated_at.is_some());
    assert_eq!(roots[0].date_group, "Today");
    assert_eq!(roots[0].directory.as_deref(), a.path().to_str());
    assert!(!app.session_picker_context(None).await.unwrap().all_projects);
    app.session_picker_context(Some(true)).await.unwrap();
    assert_eq!(
        app.history_page(id("root"), None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Hand-picked title 🦀")
    );
    conn.execute_batch("CREATE TRIGGER fail_rename BEFORE UPDATE ON sessions WHEN NEW.title='refused' BEGIN SELECT RAISE(ABORT, 'raw sqlite diagnostic'); END;").unwrap();
    assert_eq!(
        app.rename_session(id("root"), "refused".into()).await,
        Err(CoreError::Application("session storage unavailable".into()))
    );
    assert_eq!(
        title(&conn, "root").as_deref(),
        Some("Hand-picked title 🦀")
    );
    conn.execute_batch("DROP TRIGGER fail_rename").unwrap();
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    app.create_session(id("foreign")).await.unwrap();
    let foreign_deck = app.tab_deck().await.unwrap();
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![id("foreign")],
        active: Some(id("foreign")),
        ..foreign_deck
    })
    .await
    .unwrap();
    assert_eq!(
        app.session_list(String::new(), false)
            .await
            .unwrap()
            .iter()
            .map(|row| &row.id.0)
            .collect::<Vec<_>>(),
        ["foreign"]
    );
    assert_eq!(
        app.session_list("picked".into(), true).await.unwrap()[0].id,
        id("root")
    );
    assert!(
        app.session_list("picked".into(), false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.rename_session(id("root"), "Wrong place".into()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.regenerate_title(id("root")).await,
        Err(CoreError::SessionNotFound)
    );
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(
        app.rename_session(id("foreign"), "Wrong place".into())
            .await,
        Err(CoreError::SessionNotFound)
    );
    use oc_core::queries::{SessionPickerAction, SessionPickerResult};
    assert_eq!(
        app.picker_session_action(
            id("foreign"),
            "unmatched query".into(),
            true,
            SessionPickerAction::Rename("Denied".into())
        )
        .await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.picker_session_action(
            id("child"),
            String::new(),
            true,
            SessionPickerAction::Delete
        )
        .await,
        Err(CoreError::SessionNotFound)
    );
    app.session_picker_context(Some(false)).await.unwrap();
    assert_eq!(
        app.picker_session_action(
            id("foreign"),
            String::new(),
            false,
            SessionPickerAction::Delete
        )
        .await,
        Err(CoreError::SessionNotFound)
    );
    app.session_picker_context(Some(true)).await.unwrap();
    assert_eq!(
        app.picker_session_action(
            id("foreign"),
            String::new(),
            true,
            SessionPickerAction::Rename("Cross-directory selected title".into())
        )
        .await
        .unwrap(),
        SessionPickerResult::Renamed
    );
    assert_eq!(
        title(&conn, "foreign").as_deref(),
        Some("Cross-directory selected title")
    );
    assert_eq!(
        app.catalog().await.unwrap().chrome.location.as_deref(),
        a.path().to_str()
    );
    let local_deck = app.tab_deck().await.unwrap();
    conn.execute(
        "INSERT INTO sessions(id,created_at,parent_id) VALUES ('foreign-child','1','foreign')",
        [],
    )
    .unwrap();
    let SessionPickerResult::Deleted(deleted) = app
        .picker_session_action(
            id("foreign"),
            "Cross-directory".into(),
            true,
            SessionPickerAction::Delete,
        )
        .await
        .unwrap()
    else {
        panic!("deleted")
    };
    assert_eq!(deleted.location, b.path().to_str().unwrap());
    assert!(deleted.sessions.is_empty());
    assert_eq!(app.tab_deck().await.unwrap(), local_deck);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE id IN ('foreign','foreign-child')",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    assert!(app.tab_deck().await.unwrap().sessions.is_empty());
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(
        title(&conn, "root").as_deref(),
        Some("Hand-picked title 🦀")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert!(app.session_picker_context(None).await.unwrap().all_projects);
    assert_eq!(
        app.history_page(id("root"), None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Hand-picked title 🦀")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn listed_picker_open_validates_before_publication_and_atomically_adopts_existing_root() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = |model: &str| {
        serde_json::json!({"model":format!("fixture/{model}"),"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},"models":{model:{}}}}}).to_string()
    };
    std::fs::write(a.path().join("opencode.json"), config("model-a")).unwrap();
    std::fs::write(b.path().join("opencode.json"), config("model-b")).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.path().display().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    for root in ["a-first", "a-second"] {
        app.create_session(id(root)).await.unwrap();
    }
    let deck = app.tab_deck().await.unwrap();
    let old = app
        .save_tab_deck(oc_core::queries::TabDeckSnapshot {
            sessions: vec![id("a-second"), id("a-first")],
            active: Some(id("a-first")),
            ..deck
        })
        .await
        .unwrap();
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    app.create_session(id("foreign-requested")).await.unwrap();
    app.rename_session(id("foreign-requested"), "Named foreign root".into())
        .await
        .unwrap();
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    app.session_picker_context(Some(true)).await.unwrap();
    assert!(
        app.history_page(id("foreign-requested"), None, None, 10)
            .await
            .is_err()
    );
    assert!(
        app.session_selection(
            id("foreign-requested"),
            false,
            oc_core::queries::SessionSelectionAction::Current
        )
        .await
        .is_err()
    );
    assert!(
        app.open_picker_session(
            id("foreign-requested"),
            "unmatched".into(),
            true,
            old.clone()
        )
        .await
        .is_err()
    );
    std::fs::write(b.path().join("opencode.json"), "{").unwrap();
    assert!(matches!(
        app.open_picker_session(
            id("foreign-requested"),
            "Named foreign".into(),
            true,
            old.clone()
        )
        .await,
        Err(CoreError::LocationSwitch { .. })
    ));
    assert_eq!(
        app.catalog().await.unwrap().chrome.location.as_deref(),
        a.path().to_str()
    );
    assert_eq!(app.tab_deck().await.unwrap(), old);
    std::fs::write(b.path().join("opencode.json"), config("model-b")).unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_picker_target BEFORE INSERT ON prefs WHEN NEW.key LIKE 'tui.selection.tab_deck:%' AND NEW.value LIKE '%foreign-requested%' BEGIN SELECT RAISE(ABORT,'injected target save failure'); END;").unwrap();
    let mut candidate = old.clone();
    candidate.active = Some(id("a-second"));
    assert!(matches!(
        app.open_picker_session(id("foreign-requested"), String::new(), true, candidate)
            .await,
        Err(CoreError::TabDeckStorage)
    ));
    assert_eq!(
        app.tab_deck().await.unwrap(),
        old,
        "failed second deck write rolls back the first"
    );
    assert_eq!(
        app.catalog().await.unwrap().chrome.location.as_deref(),
        a.path().to_str()
    );
    conn.execute_batch("DROP TRIGGER refuse_picker_target")
        .unwrap();
    drop(conn);
    let receipt = app
        .open_picker_session(id("foreign-requested"), "Named foreign".into(), true, old)
        .await
        .unwrap();
    assert_eq!(receipt.session, id("foreign-requested"));
    assert_eq!(receipt.location, b.path().to_str().unwrap());
    assert_eq!(receipt.catalog.model_id, "model-b");
    assert_eq!(receipt.deck.sessions, vec![id("foreign-requested")]);
    assert_eq!(receipt.page.title.as_deref(), Some("Named foreign root"));
    assert!(
        app.history_page(id("a-first"), None, None, 10)
            .await
            .is_err()
    );
    assert!(
        app.session_selection(
            id("a-first"),
            false,
            oc_core::queries::SessionSelectionAction::Current
        )
        .await
        .is_err()
    );
    assert_eq!(
        receipt.previous_deck.sessions,
        vec![id("a-second"), id("a-first")]
    );
    let again = app
        .open_picker_session(
            id("foreign-requested"),
            String::new(),
            true,
            receipt.deck.clone(),
        )
        .await
        .unwrap();
    assert_eq!(again.deck.sessions, vec![id("foreign-requested")]);
    let back = app
        .open_picker_session(id("a-first"), String::new(), true, again.deck)
        .await
        .unwrap();
    assert_eq!(back.deck.sessions, vec![id("a-second"), id("a-first")]);
    assert_eq!(back.catalog.model_id, "model-a");
    assert_eq!(app.list_sessions().await.unwrap().len(), 3);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap().sessions, back.deck.sessions);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn explicit_family_delete_is_atomic_busy_location_guarded_and_durable() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},"models":{"m":{}}}}}).to_string();
    std::fs::write(a.path().join("opencode.json"), &config).unwrap();
    std::fs::write(b.path().join("opencode.json"), config).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.path().display().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    app.create_session(id("root")).await.unwrap();
    app.create_session(id("fork-root")).await.unwrap();
    let deck = app.tab_deck().await.unwrap();
    app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
        sessions: vec![id("fork-root"), id("root")],
        active: Some(id("root")),
        ..deck
    })
    .await
    .unwrap();
    let conn = Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute_batch("INSERT INTO sessions(id,created_at,parent_id,title) VALUES ('child','1','root','Child'),('grandchild','1','child','Grandchild');
        INSERT INTO messages(id,session_id,seq,role,text) VALUES ('root-msg','root',1,'user','immutable archive'),('child-msg','child',1,'assistant','child archive'),('fork-msg','fork-root',1,'user','copied archive');
        INSERT INTO turns(id,session_id,status,prompt) VALUES ('child-turn','child','started','busy child');
        INSERT INTO tool_operations(id,session_id,turn_id,name,state) VALUES ('tool','child','child-turn','bash','completed');
        INSERT INTO events(session_id,kind,payload) VALUES ('fork-root','fork_provenance','root');
        INSERT INTO turns(id,session_id,status,prompt) VALUES ('root-turn','root','completed','settled');
        INSERT INTO turn_acceptances(turn_id,session_id,user_message,model_ref) VALUES ('root-turn','root','root-msg','fixture/m');
        INSERT INTO compression_blocks VALUES ('block','root','topic','summary','root-msg','root-msg','1'); INSERT INTO compression_members VALUES ('block','root-msg');
        INSERT INTO prefs(key,value,updated_at) VALUES ('tui.selection.session:[\"project\",\"fixture\",\"child\"]','choice','1');").unwrap();
    assert!(
        app.session_list(String::new(), true)
            .await
            .unwrap()
            .iter()
            .find(|row| row.id == id("root"))
            .unwrap()
            .running
    );
    let saved = app.tab_deck().await.unwrap();
    assert_eq!(
        app.delete_session(id("root")).await,
        Err(CoreError::TurnBusy)
    );
    assert_eq!(
        app.rename_session(id("root"), "busy".into()).await,
        Err(CoreError::TurnBusy)
    );
    assert_eq!(app.tab_deck().await.unwrap(), saved);
    conn.execute(
        "UPDATE turns SET status='completed' WHERE id='child-turn'",
        [],
    )
    .unwrap();
    assert_eq!(
        app.delete_session(id("child")).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        app.delete_session(id("missing")).await,
        Err(CoreError::SessionNotFound)
    );
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(
        app.delete_session(id("root")).await,
        Err(CoreError::SessionNotFound)
    );
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_family_delete BEFORE DELETE ON sessions WHEN OLD.id='child' BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    assert!(app.delete_session(id("root")).await.is_err());
    assert_eq!(app.tab_deck().await.unwrap(), saved);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tool_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    conn.execute_batch("DROP TRIGGER refuse_family_delete;")
        .unwrap();
    let accepted = app.delete_session(id("root")).await.unwrap();
    assert_eq!(accepted.sessions, vec![id("fork-root")]);
    assert_eq!(accepted.active, Some(id("fork-root")));
    assert_eq!(app.list_sessions().await.unwrap(), vec![id("fork-root")]);
    assert_eq!(
        conn.query_row("SELECT text FROM messages", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "copied archive"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tool_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM compression_members", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.list_sessions().await.unwrap(), vec![id("fork-root")]);
    assert_eq!(
        app.tab_deck().await.unwrap().sessions,
        vec![id("fork-root")]
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
