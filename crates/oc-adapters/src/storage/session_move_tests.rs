use super::*;
use std::collections::BTreeMap;

fn admitted(db: &Db) -> MoveRecord {
    db.create_bound_session("root", "/source").unwrap();
    db.create_child_session("root", "child", None, None, None)
        .unwrap();
    db.set_pref("tui.session_location.child", "/source")
        .unwrap();
    db.append_message("root", "user", "immutable source user")
        .unwrap();
    db.begin_turn("source-turn", "root", "source prompt")
        .unwrap();
    db.record_tool_intent(
        "move-op",
        "root",
        Some("source-turn"),
        "opencode_session_move",
        "{\"directory\":\"/destination\"}",
    )
    .unwrap();
    let record = MoveRecord {
        operation: "move-op".into(),
        session: "root".into(),
        source_turn: "source-turn".into(),
        source: "/source".into(),
        directory: "/destination".into(),
        fingerprint: "a".repeat(64),
    };
    db.admit_session_move(&record).unwrap();
    record
}

#[test]
fn tool19_terminal_proof_atomic_rollback_and_same_id_reopen_dedup() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let record = admitted(&db);
    assert!(db.ready_session_moves("/source").unwrap().is_empty());
    assert!(db.apply_session_move(&record).is_err());
    let history = db.read_history("root").unwrap();
    db.finish_turn("source-turn", "completed", Some("immutable source result"))
        .unwrap();
    assert_eq!(
        db.ready_session_moves("/source").unwrap(),
        std::slice::from_ref(&record)
    );
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER move_rollback BEFORE INSERT ON events WHEN NEW.kind='session_moved' BEGIN SELECT RAISE(ABORT,'private failure'); END;").unwrap();
    assert!(db.apply_session_move(&record).is_err());
    assert_eq!(
        db.get_pref("tui.session_location.root").unwrap().as_deref(),
        Some("/source")
    );
    assert_eq!(
        db.ready_session_moves("/source").unwrap(),
        std::slice::from_ref(&record)
    );
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER move_rollback")
        .unwrap();
    assert!(db.apply_session_move(&record).unwrap());
    assert_eq!(
        db.get_pref("tui.session_location.child")
            .unwrap()
            .as_deref(),
        Some("/source")
    );
    assert_eq!(db.read_history("root").unwrap(), history);
    assert_eq!(
        db.turn_result("source-turn").unwrap(),
        ("completed".into(), Some("immutable source result".into()))
    );
    drop(db);
    let reopened = Db::open(root.path()).unwrap();
    assert!(!reopened.apply_session_move(&record).unwrap());
    assert!(reopened.ready_session_moves("/source").unwrap().is_empty());
    assert_eq!(
        reopened
            .get_pref("tui.session_location.root")
            .unwrap()
            .as_deref(),
        Some("/destination")
    );
    let count: i64 = reopened
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM events WHERE kind='session_moved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn owned_child_terminal_boundary_defers_pending_parent_move_without_relaxing_family_guard() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let record = admitted(&db);
    db.begin_turn("child-turn", "child", "owned child").unwrap();
    db.finish_turn("source-turn", "completed", Some("source terminal"))
        .unwrap();
    assert!(db.ready_session_moves("/source").unwrap().is_empty());
    assert!(db.apply_session_move(&record).is_err());
    assert_eq!(
        db.get_pref("tui.session_location.root").unwrap().as_deref(),
        Some("/source")
    );
    db.finish_turn("child-turn", "completed", Some("captured source terminal"))
        .unwrap();
    assert_eq!(
        db.ready_session_moves("/source").unwrap(),
        std::slice::from_ref(&record)
    );
    assert!(db.apply_session_move(&record).unwrap());
    assert_eq!(
        db.get_pref("tui.session_location.child")
            .unwrap()
            .as_deref(),
        Some("/source")
    );
    assert_eq!(
        db.turn_result("child-turn").unwrap(),
        ("completed".into(), Some("captured source terminal".into()))
    );
}

#[test]
fn tool19_unknown_recovery_never_applies_or_replays_and_duplicate_pending_refused() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let record = admitted(&db);
    assert!(db.admit_session_move(&record).is_err());
    db.finish_turn("source-turn", "interrupted", None).unwrap();
    db.recover_interrupted_tools().unwrap();
    assert!(db.ready_session_moves("/source").unwrap().is_empty());
    assert!(db.apply_session_move(&record).is_err());
    assert_eq!(
        db.get_pref("tui.session_location.root").unwrap().as_deref(),
        Some("/source")
    );
    assert_eq!(db.list_tool_ops("root").unwrap().len(), 1);
    db.delete_root_family("root", "/source").unwrap();
    db.create_bound_session("root", "/source").unwrap();
    assert!(!db.has_pending_move("root").unwrap());
    assert!(db.ready_session_moves("/source").unwrap().is_empty());
}

#[tokio::test]
async fn tool19_admitted_generation_recovery_rejects_changed_config_and_home_is_pinned() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let config = project.join("opencode.json");
    std::fs::write(&config,r#"{"model":"fixture/m","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}}}"#).unwrap();
    let db = Db::open(&data).unwrap();
    db.create_bound_session("root", "/source").unwrap();
    let record = MoveRecord {
        operation: "move-op".into(),
        session: "root".into(),
        source_turn: "turn".into(),
        source: "/source".into(),
        directory: project.to_string_lossy().into_owned(),
        fingerprint: String::new(),
    };
    let prepared = crate::application::session_move::prepare(&db, record, BTreeMap::new())
        .await
        .unwrap();
    let frozen = prepared.record.clone();
    assert!(
        crate::application::session_move::prepare(&db, frozen.clone(), BTreeMap::new())
            .await
            .is_ok()
    );
    std::fs::write(project.join("AGENTS.md"), "changed destination instruction").unwrap();
    assert!(
        crate::application::session_move::prepare(&db, frozen, BTreeMap::new())
            .await
            .is_err()
    );
    let env = BTreeMap::from([("HOME".into(), "/admitted-home".into())]);
    assert_eq!(
        crate::application::session_move::resolve("/target/original", "../next", &env).unwrap(),
        "/target/next"
    );
    assert_eq!(
        crate::application::session_move::resolve("/target/original", "~/next", &env).unwrap(),
        "/admitted-home/next"
    );
    assert!(
        crate::application::session_move::resolve("/target/original", "~", &BTreeMap::new())
            .is_err()
    );
}

#[test]
fn tool19_source_self_handoff_is_last_and_pending_root_slots_are_reserved() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let first = admitted(&db);
    db.create_bound_session("other", "/source").unwrap();
    db.record_tool_intent(
        "other-move",
        "root",
        Some("source-turn"),
        "opencode_session_move",
        "{}",
    )
    .unwrap();
    let second = MoveRecord {
        operation: "other-move".into(),
        session: "other".into(),
        ..first.clone()
    };
    db.admit_session_move(&second).unwrap();
    db.finish_turn("source-turn", "completed", None).unwrap();
    assert_eq!(db.ready_session_moves("/source").unwrap(), [second, first]);
    let ids = (0..MAX_TABS - 1)
        .map(|i| format!("destination-{i}"))
        .collect::<Vec<_>>();
    for id in &ids {
        db.create_bound_session(id, "/destination").unwrap();
    }
    db.set_pref(
        &tab_deck_key("/destination"),
        &serde_json::to_string(&StoredDeck {
            version: 1,
            sessions: ids.clone(),
            new_session_titles: Vec::new(),
            active: ids.first().cloned(),
        })
        .unwrap(),
    )
    .unwrap();
    assert!(db.check_move_deck("root", "/destination").is_err());
    assert_eq!(
        db.get_pref("tui.session_location.root").unwrap().as_deref(),
        Some("/source")
    );
}
