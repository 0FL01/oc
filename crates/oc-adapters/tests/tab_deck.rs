//! Application-owner tab preference: isolation, strict saves, and hostile restore.

use std::collections::BTreeMap;

use oc_adapters::application;
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;
use oc_core::queries::TabDeckSnapshot;
use oc_core::session::CoreError;
use rusqlite::{Connection, params};

fn id(raw: &str) -> SessionId {
    SessionId(raw.into())
}

fn deck(base: &TabDeckSnapshot, ids: &[&str], active: Option<&str>) -> TabDeckSnapshot {
    TabDeckSnapshot {
        location: base.location.clone(),
        revision: base.revision.clone(),
        sessions: ids.iter().map(|id| SessionId((*id).into())).collect(),
        active: active.map(id),
    }
}

fn assert_contents(snapshot: &TabDeckSnapshot, ids: &[&str], active: Option<&str>) {
    assert_eq!(
        snapshot.sessions,
        ids.iter().map(|s| id(s)).collect::<Vec<_>>()
    );
    assert_eq!(snapshot.active, active.map(id));
}

fn setup() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    BTreeMap<String, String>,
) {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = serde_json::json!({
        "model": "fixture/m",
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": "http://127.0.0.1:9/v1", "apiKey": "dummy"},
            "models": {"m": {}}
        }}
    })
    .to_string();
    std::fs::write(a.path().join("opencode.json"), &config).unwrap();
    std::fs::write(b.path().join("opencode.json"), config).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    (a, b, data, env)
}

fn key(location: &str) -> String {
    format!(
        "tui.selection.tab_deck:{}",
        serde_json::to_string(&[location]).unwrap()
    )
}

fn marker(location: &str, root: &str) -> String {
    format!(
        "tui.selection.tab_adoption:{}",
        serde_json::to_string(&[location, root]).unwrap()
    )
}

async fn accept_fresh(app: &oc_core::core_app::CoreApp, root: &str) {
    let mut events = app.subscribe();
    app.submit_fresh(id(root), "first input".into(), None)
        .await
        .unwrap();
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            CoreEvent::TurnFinished { session, .. } | CoreEvent::TurnFailed { session, .. }
                if session == id(root) =>
            {
                break;
            }
            _ => {}
        }
    }
}

fn count(conn: &Connection, table: &str, id: &str) -> i64 {
    let sql = if table == "sessions" {
        "SELECT count(*) FROM sessions WHERE id = ?1"
    } else {
        match table {
            "turns" => "SELECT count(*) FROM turns WHERE session_id = ?1",
            "messages" => "SELECT count(*) FROM messages WHERE session_id = ?1",
            "events" => "SELECT count(*) FROM events WHERE session_id = ?1",
            _ => panic!("unknown table"),
        }
    };
    conn.query_row(sql, [id], |row| row.get(0)).unwrap()
}

#[tokio::test]
async fn accepted_root_survives_restart_with_previous_tabs_and_save_retires_marker() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    app.create_session(id("prior")).await.unwrap();
    let old = app
        .save_tab_deck(deck(&empty, &["prior"], Some("prior")))
        .await
        .unwrap();
    accept_fresh(&app, "accepted").await;
    let conn = sqlite(&data);
    let marker_key = marker(&old.location, "accepted");
    assert_eq!(
        conn.query_row(
            "SELECT value FROM prefs WHERE key = ?1",
            [&marker_key],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending"
    );
    let before = count(&conn, "turns", "accepted");
    assert_eq!(before, 1);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let restored = app.tab_deck().await.unwrap();
    assert_contents(&restored, &["prior", "accepted"], Some("accepted"));
    assert_eq!(restored.revision, old.revision);
    assert!(!restored.projected());
    assert_eq!(count(&conn, "turns", "accepted"), before);
    assert_eq!(count(&conn, "sessions", "accepted"), 1);
    assert_eq!(
        app.read_history(id("accepted")).await.unwrap()[0].text,
        "first input"
    );
    let saved = app.save_tab_deck(restored).await.unwrap();
    assert_contents(&saved, &["prior", "accepted"], Some("accepted"));
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key = ?1",
            [&marker_key],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let closed = app
        .save_tab_deck(deck(&saved, &["prior"], Some("prior")))
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), closed);
    assert_eq!(count(&conn, "turns", "accepted"), before);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn multiple_pending_roots_preserve_acceptance_order_and_latest_is_active() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    accept_fresh(&app, "first").await;
    accept_fresh(&app, "second").await;
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let recovered = app.tab_deck().await.unwrap();
    assert_contents(&recovered, &["first", "second"], Some("second"));
    assert_eq!(recovered.revision, None);
    assert!(!recovered.projected());
    let conn = sqlite(&data);
    let saved = app.save_tab_deck(recovered).await.unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), saved);
    for root in ["first", "second"] {
        assert_eq!(count(&conn, "turns", root), 1);
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM prefs WHERE key=?1",
                [marker(&saved.location, root)],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn late_adoption_cannot_be_overwritten_by_old_snapshot_or_conflicting_cas() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    app.create_session(id("prior")).await.unwrap();
    let saved = app
        .save_tab_deck(deck(&empty, &["prior"], Some("prior")))
        .await
        .unwrap();
    let conn = sqlite(&data);
    let old_pref = stored(&conn, &saved.location);
    accept_fresh(&app, "late").await;
    let marker_key = marker(&saved.location, "late");
    assert_eq!(
        app.save_tab_deck(deck(&saved, &["prior"], Some("prior")))
            .await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(stored(&conn, &saved.location), old_pref);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key=?1",
            [&marker_key],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let loaded = app.tab_deck().await.unwrap();
    assert_contents(&loaded, &["prior", "late"], Some("late"));
    // A write failure must roll back the marker retirement as well as the
    // preference update; the same snapshot remains usable afterward.
    conn.execute_batch("CREATE TRIGGER fail_pending_save BEFORE UPDATE ON prefs WHEN NEW.key LIKE 'tui.selection.tab_deck:%' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert_eq!(
        app.save_tab_deck(loaded.clone()).await,
        Err(CoreError::TabDeckStorage)
    );
    assert_eq!(stored(&conn, &saved.location), old_pref);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key=?1",
            [&marker_key],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    conn.execute_batch("DROP TRIGGER fail_pending_save")
        .unwrap();
    let updated = app.save_tab_deck(loaded).await.unwrap();
    // Stale revision loses CAS, even if it includes the new root.
    conn.execute(
        "UPDATE prefs SET value = ?2 WHERE key = ?1",
        params![key(&updated.location), old_pref.unwrap()],
    )
    .unwrap();
    accept_fresh(&app, "newer").await;
    let pending = app.tab_deck().await.unwrap();
    assert_eq!(
        app.save_tab_deck(deck(&updated, &["prior", "late", "newer"], Some("newer")))
            .await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(
        stored(&conn, &updated.location),
        Some(serde_json::json!({"version":1,"sessions":["prior"],"active":"prior"}).to_string())
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key=?1",
            [marker(&updated.location, "newer")],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_contents(&pending, &["prior", "newer"], Some("newer"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn marker_insert_failure_rolls_back_entire_fresh_acceptance() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let conn = sqlite(&data);
    conn.execute_batch("CREATE TRIGGER fail_marker BEFORE INSERT ON prefs WHEN NEW.key LIKE 'tui.selection.tab_adoption:%' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert!(
        app.submit_fresh(id("rolled-back"), "input".into(), None)
            .await
            .is_err()
    );
    for table in ["sessions", "turns", "messages", "events"] {
        assert_eq!(count(&conn, table, "rolled-back"), 0, "{table}");
    }
    let location = app.tab_deck().await.unwrap().location;
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%rolled-back%';",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key IN (?1, ?2)",
            params![
                format!("tui.session_location.rolled-back"),
                marker(&location, "rolled-back")
            ],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_contents(&app.tab_deck().await.unwrap(), &[], None);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

fn assert_no_fresh_rows(conn: &Connection, location: &str, root: &str) {
    for table in ["sessions", "turns", "messages", "events"] {
        assert_eq!(count(conn, table, root), 0, "{table}");
    }
    for pref in [
        format!("tui.session_location.{root}"),
        marker(location, root),
    ] {
        assert_eq!(
            conn.query_row("SELECT count(*) FROM prefs WHERE key=?1", [pref], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%' AND key LIKE ?1",
            [format!("%{root}%")],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn fresh_root_admission_counts_saved_tabs_and_preserves_deck_on_restart() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    let names: Vec<String> = (0..15).map(|i| format!("saved-{i}")).collect();
    for name in &names {
        app.create_session(id(name)).await.unwrap();
    }
    let ids: Vec<&str> = names.iter().map(String::as_str).collect();
    let fifteen = app
        .save_tab_deck(deck(&empty, &ids[..15], Some(ids[0])))
        .await
        .unwrap();
    accept_fresh(&app, "sixteenth").await;
    let conn = sqlite(&data);
    let restored = app.tab_deck().await.unwrap();
    assert_contents(
        &restored,
        &[ids.as_slice(), &["sixteenth"]].concat(),
        Some("sixteenth"),
    );
    assert_eq!(
        stored(&conn, &fifteen.location),
        stored(&conn, &empty.location)
    );
    assert_eq!(
        app.submit_fresh(id("seventeenth"), "draft".into(), None)
            .await,
        Err(CoreError::Application("storage error".into()))
    );
    assert_no_fresh_rows(&conn, &empty.location, "seventeenth");
    assert_eq!(count(&conn, "turns", "sixteenth"), 1);
    let full = app.save_tab_deck(restored).await.unwrap();
    assert_eq!(
        app.submit_fresh(id("still-full"), "draft".into(), None)
            .await,
        Err(CoreError::Application("storage error".into()))
    );
    assert_no_fresh_rows(&conn, &empty.location, "still-full");
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), full);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn invalid_saved_deck_refuses_fresh_without_marker_or_input() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    app.create_session(id("prior")).await.unwrap();
    let saved = app
        .save_tab_deck(deck(&empty, &["prior"], Some("prior")))
        .await
        .unwrap();
    let conn = sqlite(&data);
    let original = stored(&conn, &saved.location).unwrap();
    for (i, bad) in [
        "{".to_string(),
        r#"{"version":2,"sessions":[],"active":null}"#.into(),
        serde_json::json!({"version":1,"sessions":vec!["prior";17],"active":"prior"}).to_string(),
        "x".repeat(4097),
    ]
    .into_iter()
    .enumerate()
    {
        inject(&conn, &saved.location, &bad);
        let root = format!("blocked-{i}");
        assert_eq!(
            app.submit_fresh(id(&root), "draft".into(), None).await,
            Err(CoreError::Application("storage error".into()))
        );
        assert_no_fresh_rows(&conn, &saved.location, &root);
        assert_eq!(stored(&conn, &saved.location), Some(bad));
    }
    inject(&conn, &saved.location, &original);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_contents(&app.tab_deck().await.unwrap(), &["prior"], Some("prior"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn pending_adoption_is_location_scoped_and_bounded() {
    let (a, b, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let a_loc = app.tab_deck().await.unwrap().location;
    accept_fresh(&app, "a-root").await;
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    let b_empty = app.tab_deck().await.unwrap();
    assert_contents(&b_empty, &[], None);
    accept_fresh(&app, "b-root").await;
    assert_contents(&app.tab_deck().await.unwrap(), &["b-root"], Some("b-root"));
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_contents(&app.tab_deck().await.unwrap(), &["a-root"], Some("a-root"));
    let conn = sqlite(&data);
    let foreign = marker(&b_empty.location, "b-root");
    assert_eq!(
        conn.query_row("SELECT value FROM prefs WHERE key = ?1", [&foreign], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "pending"
    );
    for i in 1..16 {
        let id = format!("pending-{i}");
        conn.execute(
            "INSERT INTO prefs(key,value,updated_at) VALUES (?1,'pending','test')",
            [marker(&a_loc, &id)],
        )
        .unwrap();
    }
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck)); // unbound marker
    for i in 1..16 {
        conn.execute(
            "DELETE FROM prefs WHERE key=?1",
            [marker(&a_loc, &format!("pending-{i}"))],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE prefs SET value='bad' WHERE key=?1",
        [marker(&a_loc, "a-root")],
    )
    .unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    conn.execute(
        "UPDATE prefs SET value='pending' WHERE key=?1",
        [marker(&a_loc, "a-root")],
    )
    .unwrap();
    let huge = marker(&a_loc, "bad");
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test')",
        params![&huge, "x".repeat(1_000_000)],
    )
    .unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    conn.execute("DELETE FROM prefs WHERE key=?1", [&huge])
        .unwrap();
    let oversized_key = marker(&a_loc, &"x".repeat(8200));
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,'pending','test')",
        [&oversized_key],
    )
    .unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    conn.execute("DELETE FROM prefs WHERE key=?1", [&oversized_key])
        .unwrap();
    for i in 1..16 {
        conn.execute(
            "INSERT INTO prefs(key,value,updated_at) VALUES (?1,'pending','test')",
            [marker(&a_loc, &format!("pending-{i}"))],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,'pending','test')",
        [marker(&a_loc, "overflow")],
    )
    .unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    conn.execute(
        "DELETE FROM prefs WHERE key=?1",
        [marker(&a_loc, "overflow")],
    )
    .unwrap();
    assert!(
        app.submit_fresh(id("seventeenth"), "input".into(), None)
            .await
            .is_err()
    );
    assert_eq!(count(&conn, "sessions", "seventeenth"), 0);
    assert_eq!(count(&conn, "turns", "seventeenth"), 0);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM prefs WHERE key=?1",
            [marker(&a_loc, "seventeenth")],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT value FROM prefs WHERE key=?1", [&foreign], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "pending"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

fn sqlite(data: &tempfile::TempDir) -> Connection {
    Connection::open(data.path().join("oc.sqlite")).unwrap()
}

fn stored(conn: &Connection, location: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM prefs WHERE key = ?1",
        [key(location)],
        |row| row.get(0),
    )
    .ok()
}

fn inject(conn: &Connection, location: &str, value: &str) {
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test') ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key(location), value],
    ).unwrap();
}

#[tokio::test]
async fn deck_is_location_scoped_and_home_restore_does_not_mint_roots() {
    let (a, b, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let a_loc = app.catalog().await.unwrap().chrome.location.unwrap();
    let a_empty = app.tab_deck().await.unwrap();
    assert_eq!(a_empty.location, a_loc);
    assert_eq!(a_empty.revision, None);
    assert!(!a_empty.projected());
    assert_contents(&a_empty, &[], None);
    assert!(app.list_sessions().await.unwrap().is_empty());
    app.create_session(id("a-1")).await.unwrap();
    app.create_session(id("a-2")).await.unwrap();
    let a_deck = app
        .save_tab_deck(deck(&a_empty, &["a-2", "a-1"], Some("a-1")))
        .await
        .unwrap();
    assert!(a_deck.revision.is_some());
    assert!(!a_deck.projected());
    let mut unscoped = a_deck.clone();
    unscoped.location.clear();
    assert_eq!(
        app.save_tab_deck(unscoped).await,
        Err(CoreError::TabDeckConflict)
    );
    let b_loc = app
        .switch_location_home(b.path().display().to_string())
        .await
        .unwrap()
        .location;
    let b_empty = app.tab_deck().await.unwrap();
    assert_eq!(b_empty.location, b_loc);
    assert_eq!(b_empty.revision, None);
    assert_contents(&b_empty, &[], None);
    app.create_session(id("b-1")).await.unwrap();
    let b_deck = app
        .save_tab_deck(deck(&b_empty, &["b-1"], None))
        .await
        .unwrap();
    assert_eq!(
        app.save_tab_deck(deck(&a_deck, &["a-1"], Some("a-1")))
            .await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(app.tab_deck().await.unwrap(), b_deck);
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), a_deck);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), a_deck);
    assert_eq!(app.list_sessions().await.unwrap().len(), 3);
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), b_deck);
    let b_cleared = app.save_tab_deck(deck(&b_deck, &[], None)).await.unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), b_cleared);
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await.unwrap(), a_deck);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let conn = sqlite(&data);
    assert_ne!(stored(&conn, &a_loc), stored(&conn, &b_loc));
}

#[tokio::test]
async fn hostile_restore_prunes_without_rewriting_and_invalid_saves_are_atomic() {
    let (a, b, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let location = app.catalog().await.unwrap().chrome.location.unwrap();
    let conn = sqlite(&data);
    app.create_session(id("real")).await.unwrap();
    let empty = app.tab_deck().await.unwrap();
    let saved = app
        .save_tab_deck(deck(&empty, &["real"], Some("real")))
        .await
        .unwrap();
    // The owner must serialize two readers of the same revision. Only the
    // returned token may be used for a subsequent save.
    let second_caller = app.tab_deck().await.unwrap();
    let updated = app
        .save_tab_deck(deck(&saved, &["real"], None))
        .await
        .unwrap();
    assert_ne!(updated.revision, saved.revision);
    assert_eq!(
        app.save_tab_deck(deck(&second_caller, &[], None)).await,
        Err(CoreError::TabDeckConflict)
    );
    let original = stored(&conn, &location).unwrap();
    for bad in [
        deck(&updated, &["real", "real"], Some("real")),
        deck(&updated, &["missing"], None),
        deck(&updated, &["real"], Some("missing")),
        deck(&updated, &[""], None),
        deck(&updated, &[" real"], None),
        deck(&updated, &["x\0y"], None),
        deck(&updated, &["real"; 17], None),
    ] {
        assert_eq!(app.save_tab_deck(bad).await, Err(CoreError::InvalidTabDeck));
        assert_eq!(stored(&conn, &location), Some(original.clone()));
    }
    // Failed SQL update must roll back the preference and leave the caller's
    // revision usable after the failure is repaired.
    conn.execute_batch("CREATE TRIGGER fail_deck BEFORE UPDATE ON prefs WHEN NEW.key LIKE 'tui.selection.tab_deck:%' BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert_eq!(
        app.save_tab_deck(deck(&updated, &["real"], Some("real")))
            .await,
        Err(CoreError::TabDeckStorage)
    );
    assert_eq!(stored(&conn, &location), Some(original));
    conn.execute_batch("DROP TRIGGER fail_deck").unwrap();
    let foreign = app
        .switch_location_home(b.path().display().to_string())
        .await
        .unwrap()
        .location;
    app.create_session(id("foreign")).await.unwrap();
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    // An orphaned binding must not turn a preference into an existing root.
    conn.execute(
        "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test')",
        params!["tui.session_location.orphan", &location],
    )
    .unwrap();
    let payload = serde_json::json!({
        "version": 1,
        "sessions": ["missing", "orphan", "foreign", "real", "real", " ", "bad\u{0000}id"],
        "active": "foreign"
    })
    .to_string();
    inject(&conn, &location, &payload);
    let pruned = app.tab_deck().await.unwrap();
    assert_contents(&pruned, &["real"], Some("real"));
    assert!(pruned.projected());
    assert_ne!(pruned.revision, updated.revision);
    assert_eq!(stored(&conn, &location), Some(payload.clone()));
    assert_eq!(
        app.save_tab_deck(deck(&pruned, &["real"], Some("real")))
            .await,
        Err(CoreError::TabDeckConflict)
    );
    // Even a caller that accidentally discards the projection marker cannot
    // overwrite the stored preference with the visible-only deck.
    let mut forgotten = deck(&pruned, &["real"], Some("real"));
    forgotten.revision = Some(
        forgotten
            .revision
            .as_deref()
            .unwrap()
            .strip_prefix("projected:")
            .unwrap()
            .to_string(),
    );
    assert!(!forgotten.projected());
    assert_eq!(
        app.save_tab_deck(forgotten).await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(stored(&conn, &location), Some(payload));
    assert_eq!(stored(&conn, &foreign), None);
    assert_eq!(app.list_sessions().await.unwrap().len(), 2);
    for invalid in [
        "{",
        r#"{"version":2,"sessions":[],"active":null}"#,
        &"x".repeat(4097),
        &serde_json::json!({"version":1,"sessions":vec!["real";17],"active":null}).to_string(),
    ] {
        inject(&conn, &location, invalid);
        assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
        assert_eq!(
            app.save_tab_deck(deck(&pruned, &["real"], Some("real")))
                .await,
            Err(CoreError::TabDeckConflict)
        );
        assert_eq!(stored(&conn, &location).as_deref(), Some(invalid));
    }
    conn.execute("DROP TABLE prefs", []).unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::TabDeckStorage));
    assert_eq!(
        app.save_tab_deck(deck(&updated, &[], None)).await,
        Err(CoreError::TabDeckStorage)
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn every_omitted_id_and_stale_active_makes_the_snapshot_read_only() {
    let (a, b, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let location = app.tab_deck().await.unwrap().location;
    app.create_session(id("real")).await.unwrap();
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    app.create_session(id("foreign")).await.unwrap();
    app.switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    let conn = sqlite(&data);
    for (sessions, active, expected_active) in [
        (vec!["real", "real"], Some("real"), Some("real")),
        (vec!["real", "missing"], Some("real"), Some("real")),
        (vec!["real", "foreign"], Some("real"), Some("real")),
        (vec!["real", " "], Some("real"), Some("real")),
        (vec!["real"], Some("retired"), Some("real")),
        (vec!["real"], Some("\0"), Some("real")),
    ] {
        let raw = serde_json::json!({
            "version": 1, "sessions": sessions, "active": active
        })
        .to_string();
        inject(&conn, &location, &raw);
        let projected = app.tab_deck().await.unwrap();
        assert_contents(&projected, &["real"], expected_active);
        assert!(projected.projected(), "did not mark {raw}");
        assert_eq!(
            app.save_tab_deck(deck(&projected, &["real"], expected_active))
                .await,
            Err(CoreError::TabDeckConflict)
        );
        assert_eq!(stored(&conn, &location).as_deref(), Some(raw.as_str()));
    }
    let intact = serde_json::json!({
        "version": 1, "sessions": ["real"], "active": null
    })
    .to_string();
    inject(&conn, &location, &intact);
    let clean = app.tab_deck().await.unwrap();
    assert_contents(&clean, &["real"], None);
    assert!(!clean.projected());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_slot_caps_sixteen_real_tabs_and_stale_active_recovers_existing_route() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    let names: Vec<String> = (0..16).map(|n| format!("tab-{n}")).collect();
    for name in &names {
        app.create_session(id(name)).await.unwrap();
    }
    let ids: Vec<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(
        app.save_tab_deck(deck(&empty, &ids, None)).await,
        Err(CoreError::InvalidTabDeck)
    );
    let fifteen = app
        .save_tab_deck(deck(&empty, &ids[..15], None))
        .await
        .unwrap();
    assert_contents(&app.tab_deck().await.unwrap(), &ids[..15], None);
    assert_eq!(
        app.save_tab_deck(deck(&empty, &ids, Some(ids[15]))).await,
        Err(CoreError::TabDeckConflict)
    );
    let sixteen = app
        .save_tab_deck(deck(&fifteen, &ids, Some(ids[15])))
        .await
        .unwrap();
    assert_contents(&app.tab_deck().await.unwrap(), &ids, Some(ids[15]));
    assert!(!sixteen.projected());

    let conn = sqlite(&data);
    let raw = serde_json::json!({"version":1,"sessions":ids,"active":null}).to_string();
    inject(&conn, &empty.location, &raw);
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    assert_eq!(stored(&conn, &empty.location), Some(raw));
    let mut with_missing = ids.clone();
    with_missing[3] = "retired";
    let raw =
        serde_json::json!({"version":1,"sessions":with_missing,"active":"retired"}).to_string();
    inject(&conn, &empty.location, &raw);
    let recovered = app.tab_deck().await.unwrap();
    let existing: Vec<&str> = ids.iter().copied().filter(|name| *name != ids[3]).collect();
    assert_contents(&recovered, &existing, Some(ids[0]));
    assert!(recovered.projected());
    assert_eq!(stored(&conn, &empty.location), Some(raw));
    let raw = serde_json::json!({"version":1,"sessions":ids,"active":"retired"}).to_string();
    inject(&conn, &empty.location, &raw);
    let recovered = app.tab_deck().await.unwrap();
    assert_contents(&recovered, &ids, Some(ids[0]));
    assert!(recovered.projected());
    assert_eq!(stored(&conn, &empty.location), Some(raw.clone()));
    let invalid_active =
        serde_json::json!({"version":1,"sessions":ids,"active":"\u{0000}"}).to_string();
    inject(&conn, &empty.location, &invalid_active);
    let invalid_projection = app.tab_deck().await.unwrap();
    assert_contents(&invalid_projection, &ids, Some(ids[0]));
    assert!(invalid_projection.projected());
    assert_eq!(stored(&conn, &empty.location), Some(invalid_active));
    inject(&conn, &empty.location, &raw);
    assert_eq!(
        app.save_tab_deck(deck(&sixteen, &ids, Some(ids[15]))).await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(
        app.save_tab_deck(deck(&recovered, &ids, Some(ids[0])))
            .await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(stored(&conn, &empty.location), Some(raw));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn preference_read_observes_utf8_byte_limit_and_rejects_large_corrupt_value() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    let conn = sqlite(&data);
    let base = r#"{"version":1,"sessions":["é"],"active":null}"#;
    let exactly_4096 = format!("{base}{}", " ".repeat(4096 - base.len()));
    assert_eq!(exactly_4096.len(), 4096);
    assert!(exactly_4096.chars().count() < exactly_4096.len());
    inject(&conn, &empty.location, &exactly_4096);
    let read = app.tab_deck().await.unwrap();
    assert_contents(&read, &[], None);
    assert!(read.revision.is_some());
    assert!(read.projected());
    assert_eq!(
        app.save_tab_deck(deck(&read, &[], None)).await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(stored(&conn, &empty.location), Some(exactly_4096.clone()));

    let over = format!("{exactly_4096} ");
    inject(&conn, &empty.location, &over);
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    assert_eq!(stored(&conn, &empty.location), Some(over));

    let corrupt = "{".repeat(1024 * 1024);
    inject(&conn, &empty.location, &corrupt);
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    assert_eq!(stored(&conn, &empty.location), Some(corrupt));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn malformed_preference_on_restart_refuses_restore_and_stale_save() {
    let (a, _, data, env) = setup();
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env.clone())
        .await
        .unwrap();
    let empty = app.tab_deck().await.unwrap();
    app.create_session(id("real")).await.unwrap();
    let saved = app
        .save_tab_deck(deck(&empty, &["real"], Some("real")))
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let conn = sqlite(&data);
    inject(&conn, &saved.location, "{malformed");
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(app.tab_deck().await, Err(CoreError::StoredTabDeck));
    assert_eq!(
        app.save_tab_deck(deck(&saved, &["real"], None)).await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(
        stored(&conn, &saved.location).as_deref(),
        Some("{malformed")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
