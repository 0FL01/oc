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
    app.rename_session(id("root"), format!("  {}  ", "é".repeat(128)))
        .await
        .unwrap();
    assert_eq!(title(&conn, "root"), Some("é".repeat(128)));
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
    assert_eq!(
        app.rename_session(id("root"), "Wrong place".into()).await,
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
    assert_eq!(
        title(&conn, "root").as_deref(),
        Some("Hand-picked title 🦀")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
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
