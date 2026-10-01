use super::{
    Db, SESSION_LOCATION_PREFIX, SessionMeta, StorageError, StoredDeck, TAB_ADOPTION_VALUE,
    tab_adoption_key, tab_deck_key,
};
use rusqlite::{OptionalExtension as _, params};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

fn test_model() -> oc_core::queries::ModelRef {
    oc_core::queries::ModelRef {
        provider: "test".into(),
        id: "m".into(),
        variant: None,
    }
}

/// `(name, type)` of a table's columns in declaration order.
fn session_columns(db: &Db) -> Vec<(String, String)> {
    let conn = db.conn.lock().expect("db mutex");
    let mut stmt = conn
        .prepare("SELECT name, type FROM pragma_table_info('sessions') ORDER BY cid ASC")
        .expect("prepare");
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("rows");
    rows.collect::<Result<Vec<_>, _>>().expect("columns")
}

/// `(seq, role, text)` rows as stored, never via a projection.
fn raw_history(db: &Db, session: &str) -> Vec<(i64, String, String)> {
    let conn = db.conn.lock().expect("db mutex");
    let mut stmt = conn
        .prepare("SELECT seq, role, text FROM messages WHERE session_id = ?1 ORDER BY seq ASC")
        .expect("prepare");
    let rows = stmt
        .query_map([session], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("rows");
    rows.collect::<Result<Vec<_>, _>>().expect("rows")
}

/// Applied migration versions in order.
fn migrations(db: &Db) -> Vec<i64> {
    let conn = db.conn.lock().expect("db mutex");
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
        .expect("prepare");
    let rows = stmt.query_map([], |row| row.get(0)).expect("rows");
    rows.collect::<Result<Vec<_>, _>>().expect("rows")
}

fn tmp_root(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    // Keep the name in the test log without moving the dir.
    let _ = name;
    dir
}

#[test]
fn legacy_acceptance_migration_is_linear_atomic_and_one_time() {
    let tmp = tempfile::tempdir().unwrap();
    const TURNS: i64 = 1024;
    {
        let db = Db::open(tmp.path()).unwrap();
        db.create_session("a").unwrap();
        db.create_session("b").unwrap();
        let mut conn = db.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        // All associations are missing, not merely one row in a modern DB.
        // Sessions interleave globally, and every third turn has no log.
        for n in 0..TURNS {
            let session = if n % 2 == 0 { "a" } else { "b" };
            let turn = format!("legacy-{n}");
            let user = format!("legacy-user-{n}");
            let status = if n % 3 == 0 { "unknown" } else { "completed" };
            let result = (status == "completed")
                .then(|| serde_json::json!({"turn_id":turn,"user_message":user}).to_string());
            tx.execute("INSERT INTO turns(id,session_id,status,prompt,result) VALUES (?1,?2,?3,'prompt',?4)",params![turn,session,status,result]).unwrap();
            tx.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES (?1,?2,?3,'user','prompt')",params![user,session,n*2+1]).unwrap();
            tx.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES (?1,?2,?3,'model_switch','notice')",params![format!("notice-{n}"),session,n*2]).unwrap();
            // Latest acceptance before the first user wins. An assistant
            // notice between acceptance and user must not reset admission.
            for (kind, payload) in [
                ("turn_started", turn.clone()),
                (
                    "accepted_model",
                    serde_json::to_string(&test_model()).unwrap(),
                ),
                (
                    "accepted_model",
                    serde_json::to_string(&oc_core::queries::ModelRef {
                        id: format!("model-{n}"),
                        ..test_model()
                    })
                    .unwrap(),
                ),
                ("message", format!("notice-{n}")),
                ("message", user),
            ] {
                tx.execute(
                    "INSERT INTO events(session_id,kind,payload) VALUES (?1,?2,?3)",
                    params![session, kind, payload],
                )
                .unwrap();
            }
            if status == "completed" {
                tx.execute(
                    "INSERT INTO events(session_id,kind,payload) VALUES (?1,'turn_finished',?2)",
                    params![session, turn],
                )
                .unwrap();
            }
        }
        tx.execute_batch("DELETE FROM schema_migrations WHERE version=4;
                DROP INDEX events_session_seq;
                CREATE TRIGGER fail_acceptance BEFORE INSERT ON turn_acceptances WHEN NEW.turn_id='legacy-512' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        tx.commit().unwrap();
        assert!(super::apply_turn_acceptance_schema(&conn).is_err());
        assert_eq!(
            conn.query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            !conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=4)",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
        assert!(!conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='events_session_seq')",[],|r|r.get::<_,bool>(0)).unwrap());
        conn.execute_batch("DROP TRIGGER fail_acceptance").unwrap();
    }
    let start = std::time::Instant::now();
    {
        let db = Db::open(tmp.path()).unwrap();
        // Generous guard: the former join already exceeded 20s at 400
        // turns. This fixture should take milliseconds, not cubic work.
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "migration took {:?}",
            start.elapsed()
        );
        let conn = db.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            TURNS
        );
        let wrong: i64 = conn.query_row("SELECT count(*) FROM turn_acceptances a JOIN turns t ON t.id=a.turn_id WHERE a.session_id!=t.session_id OR a.user_message!='legacy-user-' || substr(t.id,8) OR json_extract(a.model_ref,'$.id')!='model-' || substr(t.id,8)",[],|r|r.get(0)).unwrap();
        assert_eq!(wrong, 0);
        let unknown: i64 = conn.query_row("SELECT count(*) FROM turn_acceptances a JOIN turns t ON t.id=a.turn_id WHERE t.status='unknown' AND t.result IS NULL",[],|r|r.get(0)).unwrap();
        assert_eq!(unknown, (TURNS + 2) / 3);
        // A marked migration must never repeat backfill work on reopen.
        conn.execute_batch("DELETE FROM turn_acceptances WHERE turn_id='legacy-0'; CREATE TRIGGER no_backfill BEFORE INSERT ON turn_acceptances BEGIN SELECT RAISE(ABORT,'repeated migration'); END;").unwrap();
    }
    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        TURNS - 1
    );
}

#[test]
fn legacy_acceptance_migration_bounds_payload_and_rolls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("source").unwrap();
    for (turn, model) in [
        ("first", test_model()),
        (
            "oversized",
            oc_core::queries::ModelRef {
                id: "m".repeat(super::MAX_ACCEPTANCE_MIGRATION_FIELD_BYTES as usize + 1),
                ..test_model()
            },
        ),
    ] {
        db.accept_turn(turn, "source", "prompt", "prompt", &model)
            .unwrap();
    }
    let conn = db.conn.lock().unwrap();
    conn.execute_batch(
        "DELETE FROM turn_acceptances; DELETE FROM schema_migrations WHERE version=4;",
    )
    .unwrap();
    assert!(
        matches!(super::apply_turn_acceptance_schema(&conn), Err(StorageError::Io(error)) if error.to_string()=="turn acceptance migration budget exceeded")
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        !conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=4)",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap()
    );
}

#[test]
fn legacy_acceptance_migration_accepts_large_aggregate_and_reopens_once() {
    let tmp = tempfile::tempdir().unwrap();
    const TURNS: i64 = 1040;
    let model = serde_json::to_string(&oc_core::queries::ModelRef {
        id: "m".repeat(65_000),
        ..test_model()
    })
    .unwrap();
    assert!(model.len() < super::MAX_ACCEPTANCE_MIGRATION_FIELD_BYTES as usize);
    // Few rows with individually valid bounded references keep fixture
    // cost low while crossing the former global processed-byte cutoff.
    assert!(TURNS as usize * model.len() > 64 * 1024 * 1024);
    {
        let db = Db::open(tmp.path()).unwrap();
        let mut conn = db.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute(
            "INSERT INTO sessions(id,created_at) VALUES ('source','fixture')",
            [],
        )
        .unwrap();
        for n in 0..TURNS {
            let turn = format!("turn-{n}");
            let user = format!("user-{n}");
            tx.execute(
                "INSERT INTO turns(id,session_id,status,prompt) VALUES (?1,'source','unknown','p')",
                [&turn],
            )
            .unwrap();
            tx.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES (?1,'source',?2,'user','p')",params![user,n]).unwrap();
            for (kind, payload) in [
                ("turn_started", &turn),
                ("accepted_model", &model),
                ("message", &user),
            ] {
                tx.execute(
                    "INSERT INTO events(session_id,kind,payload) VALUES ('source',?1,?2)",
                    params![kind, payload],
                )
                .unwrap();
            }
        }
        tx.execute("DELETE FROM schema_migrations WHERE version=4", [])
            .unwrap();
        tx.commit().unwrap();
    }
    {
        // Opening the legacy database must migrate its entire valid journal.
        let db = Db::open(tmp.path()).unwrap();
        let conn = db.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM turn_acceptances", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            TURNS
        );
        let preserved: i64 = conn.query_row("SELECT count(*) FROM turn_acceptances a JOIN turns t ON t.id=a.turn_id AND t.session_id=a.session_id JOIN messages m ON m.id=a.user_message AND m.session_id=a.session_id WHERE a.model_ref=?1 AND a.user_message='user-' || substr(a.turn_id,6)",[&model],|r|r.get(0)).unwrap();
        assert_eq!(preserved, TURNS);
        assert!(
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=4)",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap()
        );
        // A rerun would require this explicitly named journal index. Removing
        // it only in the fixture proves reopen skips the marked migration,
        // without deleting any recovered admission associations.
        conn.execute_batch("DROP INDEX events_session_seq").unwrap();
    }
    let db = Db::open(tmp.path()).unwrap();
    let conn = db.conn.lock().unwrap();
    let preserved: i64 = conn.query_row("SELECT count(*) FROM turn_acceptances WHERE model_ref=?1 AND session_id='source' AND user_message='user-' || substr(turn_id,6)",[&model],|r|r.get(0)).unwrap();
    assert_eq!(preserved, TURNS);
}

#[test]
fn manual_root_title_overrides_generated_and_rolls_back_with_event_failure() {
    let tmp = tmp_root("manual-title");
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("root").unwrap();
    db.create_child_session("root", "child", None, None, Some("child title"))
        .unwrap();
    db.set_generated_title("root", "generated").unwrap();
    db.rename_root_session("root", "manual").unwrap();
    db.set_generated_title("root", "late generation").unwrap();
    assert_eq!(
        db.session_meta("root").unwrap().title.as_deref(),
        Some("manual")
    );
    assert!(matches!(
        db.rename_root_session("child", "wrong"),
        Err(StorageError::SessionNotFound)
    ));
    assert!(matches!(
        db.rename_root_session("missing", "wrong"),
        Err(StorageError::SessionNotFound)
    ));
    assert_eq!(
        db.session_meta("child").unwrap().title.as_deref(),
        Some("child title")
    );
    let conn = db.conn.lock().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE session_id='root' AND kind='session_updated'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    conn.execute_batch("CREATE TRIGGER fail_title_event BEFORE INSERT ON events WHEN NEW.kind='session_updated' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    drop(conn);
    assert!(db.rename_root_session("root", "should roll back").is_err());
    assert_eq!(
        db.session_meta("root").unwrap().title.as_deref(),
        Some("manual")
    );
}

#[test]
fn regenerated_title_cas_preserves_manual_edit_and_event_atomicity() {
    let tmp = tmp_root("regenerated-title");
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("root").unwrap();
    db.create_child_session("root", "child", None, None, Some("child"))
        .unwrap();
    db.rename_root_session("root", "original").unwrap();
    let original_event = db.root_title_stamp("root").unwrap().unwrap().1;
    assert!(
        !db.compare_and_set_root_title("child", Some("child"), original_event, "bad")
            .unwrap()
    );
    assert!(
        !db.compare_and_set_root_title("root", None, original_event, "bad")
            .unwrap()
    );
    db.rename_root_session("root", "manual during request")
        .unwrap();
    assert!(
        !db.compare_and_set_root_title("root", Some("original"), original_event, "late")
            .unwrap()
    );
    // The title text can return to its original value without restoring
    // the generation's authority to overwrite an intervening manual edit.
    db.rename_root_session("root", "original").unwrap();
    assert!(
        !db.compare_and_set_root_title("root", Some("original"), original_event, "late")
            .unwrap()
    );
    db.rename_root_session("root", "manual during request")
        .unwrap();
    let fresh_event = db.root_title_stamp("root").unwrap().unwrap().1;
    assert!(
        db.compare_and_set_root_title("root", Some("manual during request"), fresh_event, "new")
            .unwrap()
    );
    let new_event = db.root_title_stamp("root").unwrap().unwrap().1;
    assert!(
        db.compare_and_set_root_title("root", Some("new"), new_event, "new")
            .unwrap()
    );
    assert!(
        !db.compare_and_set_root_title(
            "root",
            Some("manual during request"),
            fresh_event,
            "duplicate"
        )
        .unwrap()
    );
    let conn = db.conn.lock().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE session_id='root' AND kind='session_updated'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 5);
    conn.execute_batch("CREATE TRIGGER refuse_title_event BEFORE INSERT ON events WHEN NEW.kind='session_updated' BEGIN SELECT RAISE(ABORT, 'test refusal'); END;").unwrap();
    drop(conn);
    assert!(
        db.compare_and_set_root_title("root", Some("new"), new_event, "rolled back")
            .is_err()
    );
    assert_eq!(
        db.session_meta("root").unwrap().title.as_deref(),
        Some("new")
    );
}

#[test]
fn second_owner_is_refused() {
    let tmp = tmp_root("busy");
    let root = tmp.path().join("data");
    let _first = Db::open(&root).expect("first open");
    let second = Db::open(&root);
    assert!(matches!(second, Err(StorageError::DataRootBusy)));
}

#[test]
fn v02_bounded_projection_exposes_loss_and_legacy_availability() {
    use serde_json::json;
    let tmp = tmp_root("projection");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    let parts: Vec<_> = (0..250).map(|_| json!({"reasoning":"thought"})).collect();
    db.checkpoint_turn("t", &json!({"display_parts":parts}).to_string())
        .unwrap();
    let projection = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(projection.parts.len(), 240);
    assert_eq!(projection.omitted_parts, 10);
    assert!(projection.truncated);
    assert_eq!(projection.part_states[239].sequence, 239);
    db.record_tool_intent("op", "s", Some("t"), "apply_patch", &"x".repeat(70 * 1024))
        .unwrap();
    db.checkpoint_turn("t",&json!({"display_parts":[{"tool":"op"},{"message":0}],"input":[{"content":[{"type":"output_text","text":"é".repeat(70*1024)}]}]}).to_string()).unwrap();
    let projection = db.turn_presentation("s", "t").unwrap().unwrap();
    assert!(projection.part_states[0].input_omitted);
    assert!(projection.part_states[1].truncated);
    assert!(
        matches!(&projection.parts[0],oc_core::queries::TranscriptPart::Tool(op) if op.op=="op")
    );
    db.checkpoint_turn("t", "{}").unwrap();
    let legacy = db.turn_presentation("s", "t").unwrap().unwrap();
    assert!(legacy.legacy_text_only);
    assert!(legacy.parts.is_empty());
}

#[test]
fn context_usage_projection_requires_a_reported_pair_and_keeps_billing_separate() {
    use serde_json::json;

    let tmp = tmp_root("context-usage");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    db.checkpoint_turn("t", &json!({"display_parts":[]}).to_string())
        .unwrap();
    db.update_turn_display("t", &json!({"context_usage":[6000,763]}))
        .unwrap();
    let projected = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(projected.context_usage, Some((6000, 763)));
    assert_eq!(projected.usage, None);

    drop(db);
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let reopened = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(reopened.context_usage, Some((6000, 763)));
    assert_eq!(reopened.usage, None);

    db.update_turn_display("t", &json!({"context_usage":[6000]}))
        .unwrap();
    assert_eq!(
        db.turn_presentation("s", "t")
            .unwrap()
            .unwrap()
            .context_usage,
        None
    );
    db.update_turn_display("t", &json!({"context_usage":null}))
        .unwrap();
    assert_eq!(
        db.turn_presentation("s", "t")
            .unwrap()
            .unwrap()
            .context_usage,
        None
    );
}

#[test]
fn turn_tool_reference_and_intent_are_atomic() {
    let tmp = tmp_root("display-intent");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_display BEFORE UPDATE OF result ON turns BEGIN SELECT RAISE(ABORT, 'injected display failure'); END;").unwrap();
    assert!(
        db.record_turn_tool_intent(
            "op",
            "s",
            "t",
            "read",
            "{}",
            "{\"display_parts\":[{\"tool\":\"op\"}]}"
        )
        .is_err()
    );
    assert!(
        db.tool_state("op").is_err(),
        "no orphaned intent without its display reference"
    );
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_display;")
        .unwrap();
    db.record_turn_tool_intent(
        "op",
        "s",
        "t",
        "read",
        "{}",
        "{\"display_parts\":[{\"tool\":\"op\"}]}",
    )
    .unwrap();
    assert_eq!(db.tool_state("op").unwrap(), "started");
    assert!(
        db.turn_result("t")
            .unwrap()
            .1
            .unwrap()
            .contains("display_parts")
    );
}

#[test]
fn session_picker_same_tick_orders_observed_activity_before_ids_and_limit() {
    let tmp = tmp_root("picker-tied-time");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let ids: Vec<_> = (0..61)
        .map(|i| format!("reverse-{:02}", (i * 37) % 61))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        db.create_session(id).unwrap();
        if i == 0 {
            db.rename_root_session(id, "Oldest real activity").unwrap();
        }
        // An actual millisecond may contain several mutations. Force one
        // observed tick to exercise the persisted journal tie-breaker.
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET updated_at='1700000000.125' WHERE id=?1",
                [id],
            )
            .unwrap();
    }
    let rows = db.session_list("", None).unwrap();
    assert_eq!(rows.len(), 50);
    assert_eq!(
        rows.iter()
            .map(|entry| entry.id.0.clone())
            .collect::<Vec<_>>(),
        ids.iter().rev().take(50).cloned().collect::<Vec<_>>()
    );
    assert_eq!(db.session_list("Oldest", None).unwrap()[0].id.0, ids[0]);
    assert!(rows[0].updated_at.as_deref().unwrap().contains('.'));
}

#[test]
fn session_picker_uses_checkout_common_directory_for_worktree_footer() {
    let tmp = tmp_root("picker-worktree");
    let project = tmp.path().join("canonical-project");
    let worktree = tmp.path().join("feature-checkout");
    let gitdir = project.join(".git/worktrees/feature-checkout");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir_all(worktree.join("nested")).unwrap();
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    assert_eq!(
        super::session_project_root(&worktree.join("nested")),
        Some(project.clone())
    );
    let db = Db::open(&tmp.path().join("data")).unwrap();
    db.create_bound_session("canonical", project.to_str().unwrap())
        .unwrap();
    db.create_bound_session("worktree", worktree.join("nested").to_str().unwrap())
        .unwrap();
    let entries = db.session_list("", None).unwrap();
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.id.0 == "canonical")
            .unwrap()
            .worktree,
        None
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.id.0 == "worktree")
            .unwrap()
            .worktree
            .as_deref(),
        Some("feature-checkout")
    );
    assert_eq!(
        db.session_list("", Some(project.to_str().unwrap()))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn session_picker_is_bounded_searches_before_limit_and_preserves_unknown_legacy_time() {
    let tmp = tmp_root("session-picker");
    let root = tmp.path().join("data");
    let db = Db::open(&root).unwrap();
    for i in 0..60 {
        let id = format!("root-{i:02}");
        db.create_session(&id).unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET title=?2, created_at='0', updated_at=?3 WHERE id=?1",
                params![id, format!("Named {i}"), format!("{}", 86400 * (i + 1))],
            )
            .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET updated_at=?2 WHERE id=?1",
                params![id, format!("{}", 86400 * (i + 1) + 43200)],
            )
            .unwrap();
    }
    db.create_child_session("root-00", "child", None, None, Some("Named child"))
        .unwrap();
    let rows = db.session_list("", None).unwrap();
    assert_eq!(rows.len(), 50);
    assert_eq!(rows[0].id.0, "root-59");
    assert!(rows[0].date_group.ends_with("1970"));
    assert_eq!(db.session_list("Named 0", None).unwrap()[0].id.0, "root-00");
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE sessions SET title=NULL,updated_at=NULL WHERE id='root-00'",
            [],
        )
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute("UPDATE sessions SET updated_at=NULL WHERE id='root-00'", [])
        .unwrap();
    let legacy = db.session_list("1970-01-01", None).unwrap();
    assert_eq!(legacy.len(), 1);
    assert_eq!(legacy[0].title, "New session - 1970-01-01T00:00:00.000Z");
    assert_eq!(legacy[0].updated_at, None);
    assert_eq!(legacy[0].date_group, "Update time unavailable");
    drop(db);
    let db = Db::open(&root).unwrap();
    assert_eq!(
        db.session_list("1970-01-01", None).unwrap()[0].updated_at,
        None
    );
    db.append_message("root-00", "user", "real activity")
        .unwrap();
    assert_eq!(
        db.session_list("1970-01-01", None).unwrap()[0].date_group,
        "Today"
    );
    assert_eq!(db.list_sessions().unwrap().len(), 61);
}

#[test]
fn session_picker_migrates_old_schema_without_inventing_update_times() {
    let tmp = tmp_root("session-picker-migration");
    let root = tmp.path().join("data");
    std::fs::create_dir_all(&root).unwrap();
    let conn = rusqlite::Connection::open(root.join("oc.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions(id TEXT PRIMARY KEY,created_at TEXT NOT NULL);
            INSERT INTO sessions VALUES ('legacy','946684800');",
    )
    .unwrap();
    drop(conn);
    let db = Db::open(&root).unwrap();
    let entry = db.session_list("", None).unwrap().remove(0);
    assert_eq!(entry.title, "New session - 2000-01-01T00:00:00.000Z");
    assert_eq!(entry.created_at, "946684800");
    assert_eq!(entry.updated_at, None);
    assert_eq!(entry.date_group, "Update time unavailable");
    db.create_session("fresh").unwrap();
    assert!(db.session_list("", None).unwrap()[0].updated_at.is_some());
}

#[test]
fn durable_input_turn_event_roundtrip() {
    let tmp = tmp_root("durable");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    db.create_session("s-1").expect("session");
    let m1 = db.append_message("s-1", "user", "hello").expect("msg");
    assert_eq!(m1, "m0001");
    db.begin_turn("t-1", "s-1", "hello").expect("begin");
    db.record_tool_intent("op-1", "s-1", Some("t-1"), "read", "{}")
        .expect("intent");
    db.record_tool_outcome("op-1", "succeeded", Some("ok"))
        .expect("outcome");
    db.finish_turn("t-1", "completed", Some("done"))
        .expect("finish");
    let history = db.read_history("s-1").expect("history");
    assert_eq!(history, vec![("user".to_string(), "hello".to_string())]);
    assert_eq!(db.list_sessions().expect("list"), vec!["s-1".to_string()]);
    assert_eq!(db.tool_state("op-1").expect("state"), "succeeded");
}

#[test]
fn crash_intent_recovers_unknown_without_replay() {
    let tmp = tmp_root("crash");
    let root = tmp.path().join("data");
    {
        let db = Db::open(&root).expect("open");
        db.create_session("s-c").expect("session");
        db.record_tool_intent("op-crash", "s-c", None, "bash", "sleep 1")
            .expect("intent");
        // Drop without outcome: simulated kill after intent, before side-effect ack.
    }
    {
        let db = Db::open(&root).expect("reopen");
        let marked = db.recover_interrupted_tools().expect("recover");
        assert_eq!(marked, 1);
        assert_eq!(db.tool_state("op-crash").expect("state"), "unknown");
        // Second recovery is a no-op; never autoreplays the mutation.
        assert_eq!(db.recover_interrupted_tools().expect("again"), 0);
    }
}

#[test]
fn blob_orphan_collected_referenced_kept() {
    let tmp = tmp_root("blob");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    let digest = db.write_blob(b"referenced-bytes").expect("write");
    assert_eq!(db.read_blob(&digest).expect("read"), b"referenced-bytes");
    // Simulate crash: file durable, DB row missing.
    let orphan = db.root().join("blobs").join("orphan-file");
    fs::write(&orphan, b"orphan").expect("orphan write");
    // Grace zero collects only the orphan; the referenced blob survives.
    let removed = db.gc_orphans(Duration::ZERO).expect("gc");
    assert_eq!(removed, 1);
    assert!(!orphan.exists());
    assert!(db.root().join("blobs").join(&digest).exists());
    // Referenced deletion is never performed by GC.
    let removed = db.gc_orphans(Duration::ZERO).expect("gc2");
    assert_eq!(removed, 0);
}

#[test]
fn unsafe_roots_refused() {
    assert!(matches!(
        Db::open(Path::new("/")),
        Err(StorageError::UnsafeRoot(_))
    ));
    assert!(matches!(
        Db::open(Path::new("/tmp")),
        Err(StorageError::UnsafeRoot(_))
    ));
    assert!(matches!(
        Db::open(Path::new("relative/path")),
        Err(StorageError::UnsafeRoot(_))
    ));
    assert!(matches!(
        Db::open(Path::new("/tmp/../tmp/data-test-oc")),
        Err(StorageError::UnsafeRoot(_))
    ));
}

#[test]
fn symlink_root_and_escape_not_followed() {
    let tmp = tmp_root("link");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside");
    let link = tmp.path().join("link-root");
    std::os::unix::fs::symlink(&outside, &link).expect("symlink");
    assert!(matches!(Db::open(&link), Err(StorageError::UnsafeRoot(_))));

    let db = Db::open(&tmp.path().join("data")).expect("open");
    // Symlink inside blob dir pointing outside must not be followed/deleted.
    let victim = outside.join("victim.txt");
    fs::write(&victim, b"do-not-delete").expect("victim");
    std::os::unix::fs::symlink(&victim, db.root().join("blobs").join("evil-link"))
        .expect("evil link");
    let removed = db.gc_orphans(Duration::ZERO).expect("gc");
    assert!(
        victim.exists(),
        "outside file must survive, removed={removed}"
    );
}

#[test]
fn quota_and_permissions_enforced() {
    let tmp = tmp_root("quota");
    let db = Db::open_with_quota(&tmp.path().join("data"), 16).expect("open");
    db.write_blob(b"12345678").expect("fits");
    let err = db.write_blob(b"234567890").expect_err("over quota");
    assert!(matches!(err, StorageError::StorageFull));

    // Permissive existing root is secured to 0700 on next open.
    let root2 = tmp.path().join("loose");
    {
        let _db = Db::open(&root2).expect("open loose");
    }
    fs::set_permissions(&root2, fs::Permissions::from_mode(0o777)).expect("chmod");
    let _db = Db::open(&root2).expect("reopen secures");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&root2).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}

#[test]
fn history_pages_and_tool_ops_are_bounded() {
    let tmp = tmp_root("tui22");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    assert!(db.read_history_page("ghost", 10, None).is_err());
    assert!(db.list_tool_ops("ghost").is_err());
    db.create_session("s").expect("session");
    assert_eq!(db.history_len("s").expect("len"), 0);
    for i in 0..5 {
        db.append_message("s", "user", &format!("m{i}"))
            .expect("msg");
    }
    assert_eq!(db.history_len("s").expect("len"), 5);
    let page = db.read_history_page("s", 2, None).expect("page");
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].0, 5);
    assert_eq!(page[1].2, "m3");
    let rest = db
        .read_history_page("s", 100, Some(page[1].0))
        .expect("rest");
    assert_eq!(rest.len(), 3);
    // Over-limit requests clamp instead of growing the store.
    let clamped = db.read_history_page("s", 10_000, None).expect("clamp");
    assert_eq!(clamped.len(), 5);

    db.record_tool_intent("op1", "s", None, "read", "{}")
        .expect("intent");
    db.record_tool_outcome("op1", "completed", Some("ok"))
        .expect("outcome");
    let ops = db.list_tool_ops("s").expect("ops");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name, "read");
    assert_eq!(ops[0].state, "completed");
}

#[test]
fn typed_history_ids_are_exact_bounded_and_stable_across_paging_and_reopen() {
    let tmp = tmp_root("history-identities");
    let root = tmp.path().join("data");
    let expected = {
        let db = Db::open(&root).unwrap();
        db.create_session("s").unwrap();
        for _ in 0..super::HISTORY_PAGE_MAX + 3 {
            db.append_message("s", "user", "same text").unwrap();
        }
        // Identity is opaque: it must not be reconstructed from sequence.
        db.conn
            .lock()
            .unwrap()
            .execute("UPDATE messages SET id='opaque-owner-id' WHERE seq=2", [])
            .unwrap();
        let tail = db.read_history_page_typed("s", usize::MAX, None).unwrap();
        assert_eq!(tail.len(), super::HISTORY_PAGE_MAX);
        assert!(db.read_history_page_typed("s", 0, None).unwrap().is_empty());
        assert!(db.read_history_page_typed("missing", 1, None).is_err());
        let mut all = tail.clone();
        all.extend(
            db.read_history_page_typed("s", 10, Some(tail.last().unwrap().seq))
                .unwrap(),
        );
        all.reverse();
        assert_eq!(all[1].id.0, "opaque-owner-id");
        let forward = db
            .read_history_after_typed("s", usize::MAX, all[2].seq)
            .unwrap();
        assert_eq!(forward, all[3..]);
        let tuples = db.read_history_after("s", usize::MAX, all[2].seq).unwrap();
        assert_eq!(
            tuples,
            forward
                .iter()
                .map(|r| (r.seq, r.role.clone(), r.text.clone()))
                .collect::<Vec<_>>()
        );
        all
    };
    let db = Db::open(&root).unwrap();
    let head = db.read_history_after_typed("s", 3, 0).unwrap();
    assert_eq!(head, expected[..3]);
    assert_eq!(
        db.read_history_after_typed("s", usize::MAX, head[2].seq)
            .unwrap(),
        expected[3..]
    );
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE messages SET id=?1 WHERE seq=1",
            ["x".repeat(super::HISTORY_PAGE_ID_BYTES + 1)],
        )
        .unwrap();
    assert!(db.read_history_after_typed("s", 1, 0).is_err());
}

#[test]
fn prefs_roundtrip() {
    let tmp = tmp_root("prefs");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    assert_eq!(db.get_pref("tui.x").expect("get"), None);
    db.set_pref("tui.x", "{\"a\":1}").expect("set");
    assert_eq!(
        db.get_pref("tui.x").expect("get"),
        Some("{\"a\":1}".to_string())
    );
    db.set_pref("tui.x", "v2").expect("overwrite");
    assert_eq!(db.get_pref("tui.x").expect("get"), Some("v2".to_string()));
}

#[test]
fn child_schema_migration_is_idempotent_across_reopen() {
    let tmp = tmp_root("child-migrate");
    let root = tmp.path().join("data");
    let fresh = {
        let db = Db::open(&root).expect("open");
        let columns = session_columns(&db);
        let names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "id",
                "created_at",
                "parent_id",
                "agent",
                "model",
                "title",
                "updated_at"
            ]
        );
        assert!(columns.iter().all(|(_, ty)| ty == "TEXT"));
        assert_eq!(db.list_sessions().expect("list"), Vec::<String>::new());
        // One `sessions` row per session; the index exists even on fresh DBs.
        let conn = db.conn.lock().expect("db mutex");
        let index: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'sessions_parent'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("index lookup");
        assert_eq!(index.as_deref(), Some("sessions_parent"));
        // Sentinel proves a reopen skips the migration instead of
        // re-running the guarded ALTERs.
        conn.execute(
            "UPDATE schema_migrations SET applied_at = 'sentinel' WHERE version = 3",
            [],
        )
        .expect("mark");
        columns
    };
    let db = Db::open(&root).expect("reopen");
    assert_eq!(session_columns(&db), fresh, "reopen keeps the schema");
    // T50 adds the versioned background lifecycle without changing sessions.
    assert_eq!(migrations(&db), vec![1, 3, 4, 5, 6]);
    let conn = db.conn.lock().expect("db mutex");
    let applied: String = conn
        .query_row(
            "SELECT applied_at FROM schema_migrations WHERE version = 3",
            [],
            |row| row.get(0),
        )
        .expect("v3 row");
    assert_eq!(applied, "sentinel", "reopen re-applied migration 3");
}

#[test]
fn child_schema_upgrades_legacy_database_to_same_schema() {
    let tmp = tmp_root("child-upgrade");
    let root = tmp.path().join("data");
    fs::create_dir_all(&root).expect("root");
    // A v1 database: sessions without the child columns, migration 1 only.
    {
        let conn = rusqlite::Connection::open(root.join("oc.sqlite")).expect("legacy sqlite");
        conn.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 CREATE TABLE sessions(id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
                 INSERT INTO schema_migrations(version, applied_at) VALUES (1, 't04');
                 INSERT INTO sessions(id, created_at) VALUES ('legacy', '1');",
        )
        .expect("legacy schema");
    }
    let db = Db::open(&root).expect("open legacy");
    assert_eq!(migrations(&db), vec![1, 3, 4, 5, 6]);
    let legacy = db.session_meta("legacy").expect("legacy meta");
    assert_eq!(
        legacy,
        SessionMeta {
            parent_id: None,
            agent: None,
            model: None,
            title: None,
        }
    );
    assert_eq!(
        db.children_of("legacy").expect("children"),
        Vec::<String>::new()
    );

    let fresh_root = tmp.path().join("fresh");
    let fresh = Db::open(&fresh_root).expect("fresh open");
    assert_eq!(
        session_columns(&db),
        session_columns(&fresh),
        "upgraded schema equals fresh schema"
    );
}

#[test]
fn child_rows_persist_across_reopen() {
    let tmp = tmp_root("child-persist");
    let root = tmp.path().join("data");
    {
        let db = Db::open(&root).expect("open");
        db.create_session("root").expect("root");
        db.create_child_session(
            "root",
            "kid",
            Some("explore"),
            Some("openai/gpt-5#low"),
            Some("Review code"),
        )
        .expect("child");
        db.append_message("kid", "user", "hello").expect("msg");
    }
    let db = Db::open(&root).expect("reopen");
    assert_eq!(
        db.session_meta("kid").expect("meta"),
        SessionMeta {
            parent_id: Some("root".to_string()),
            agent: Some("explore".to_string()),
            model: Some("openai/gpt-5#low".to_string()),
            title: Some("Review code".to_string()),
        }
    );
    assert_eq!(
        db.children_of("root").expect("children"),
        vec!["kid".to_string()]
    );
    assert_eq!(
        db.read_history("kid").expect("history"),
        vec![("user".to_string(), "hello".to_string())]
    );
    let root_meta = db.session_meta("root").expect("root meta");
    assert_eq!(root_meta.parent_id, None);
    assert_eq!(root_meta.agent, None);
    assert_eq!(root_meta.model, None);
    assert_eq!(root_meta.title, None);
}

#[test]
fn children_of_returns_only_direct_children_in_insertion_order() {
    let tmp = tmp_root("child-order");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    db.create_session("p").expect("p");
    db.create_session("other").expect("other");
    // Ids are deliberately out of lexical order: insertion order wins.
    db.create_child_session("p", "c-b", None, None, None)
        .expect("c-b");
    db.create_child_session("p", "c-a", None, None, None)
        .expect("c-a");
    db.create_child_session("other", "c-x", None, None, None)
        .expect("c-x");
    db.create_child_session("c-b", "g", None, None, None)
        .expect("g");
    db.create_child_session("p", "c-c", None, None, None)
        .expect("c-c");
    assert_eq!(db.children_of("p").expect("p"), vec!["c-b", "c-a", "c-c"]);
    assert_eq!(db.children_of("c-b").expect("c-b"), vec!["g"]);
    assert_eq!(db.children_of("other").expect("other"), vec!["c-x"]);
    assert_eq!(db.children_of("g").expect("leaf"), Vec::<String>::new());
    assert!(matches!(
        db.children_of("ghost"),
        Err(StorageError::SessionNotFound)
    ));
}

#[test]
fn child_creation_never_touches_parent_history() {
    let tmp = tmp_root("child-immutable");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    db.create_session("p").expect("p");
    db.append_message("p", "user", "one").expect("m1");
    db.append_message("p", "assistant", "two").expect("m2");
    let before = raw_history(&db, "p");
    db.create_child_session("p", "k1", Some("general"), None, None)
        .expect("k1");
    db.append_message("k1", "user", "child text")
        .expect("child msg");
    db.create_child_session("p", "k2", None, Some("model"), Some("title"))
        .expect("k2");
    assert_eq!(raw_history(&db, "p"), before);
    assert_eq!(db.history_len("p").expect("len"), 2);
}

#[test]
fn create_child_session_requires_parent_and_unique_ids() {
    let tmp = tmp_root("child-errors");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    assert!(matches!(
        db.create_child_session("ghost", "k", None, None, None),
        Err(StorageError::SessionNotFound)
    ));
    assert!(matches!(
        db.session_meta("k"),
        Err(StorageError::SessionNotFound)
    ));
    db.create_session("p").expect("p");
    db.create_child_session("p", "k", None, None, None)
        .expect("child");
    assert!(matches!(
        db.create_child_session("p", "k", None, None, None),
        Err(StorageError::SessionAlreadyExists)
    ));
    assert!(matches!(
        db.create_session("k"),
        Err(StorageError::SessionAlreadyExists)
    ));
}

#[test]
fn root_creation_rolls_back_when_event_insert_fails() {
    let tmp = tmp_root("root-atomic");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_session_created BEFORE INSERT ON events
             WHEN NEW.kind = 'session_created'
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
        )
        .unwrap();

    assert!(matches!(
        db.create_session("root"),
        Err(StorageError::Sqlite(_))
    ));
    {
        let conn = db.conn.lock().unwrap();
        let sessions: i64 = conn
            .query_row(
                "SELECT count(*) FROM sessions WHERE id = 'root'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let events: i64 = conn
            .query_row(
                "SELECT count(*) FROM events WHERE session_id = 'root'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sessions, 0, "failed creation must not leave a session row");
        assert_eq!(events, 0, "failed creation must not leave an event");
        conn.execute_batch("DROP TRIGGER fail_session_created;")
            .unwrap();
    }

    db.create_session("root")
        .expect("retry must not encounter a duplicate");
    assert!(matches!(
        db.create_session("root"),
        Err(StorageError::SessionAlreadyExists)
    ));
    let conn = db.conn.lock().unwrap();
    let sessions: i64 = conn
        .query_row(
            "SELECT count(*) FROM sessions WHERE id = 'root'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let events: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE session_id = 'root' AND kind = 'session_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sessions, 1);
    assert_eq!(events, 1);
}

/// Verify every durable component, including the binding, through the
/// backing tables rather than through the public session projection.
fn fresh_turn_rows(db: &Db, session: &str, turn: &str) -> (i64, i64, i64, i64, i64) {
    let conn = db.conn.lock().unwrap();
    let count =
        |sql: &str, value: &str| -> i64 { conn.query_row(sql, [value], |row| row.get(0)).unwrap() };
    (
        count("SELECT count(*) FROM sessions WHERE id = ?1", session),
        count(
            "SELECT count(*) FROM prefs WHERE key = ?1",
            &format!("{SESSION_LOCATION_PREFIX}{session}"),
        ),
        count("SELECT count(*) FROM events WHERE session_id = ?1", session),
        count("SELECT count(*) FROM turns WHERE id = ?1", turn),
        count(
            "SELECT count(*) FROM messages WHERE session_id = ?1",
            session,
        ),
    )
}

#[test]
fn fresh_admission_checks_union_before_root_insert_and_deduplicates_markers() {
    let tmp = tmp_root("fresh-deck-union");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let location = "/project";
    let key = tab_deck_key(location);
    let names: Vec<String> = (0..15).map(|i| format!("saved-{i}")).collect();
    let deck = StoredDeck {
        version: 1,
        sessions: names,
        active: Some("saved-0".into()),
    };
    db.set_pref(&key, &serde_json::to_string(&deck).unwrap())
        .unwrap();
    // A marker already represented by the stored preference must not
    // consume a second slot.
    db.create_bound_session("saved-0", location).unwrap();
    db.set_pref(&tab_adoption_key(location, "saved-0"), TAB_ADOPTION_VALUE)
        .unwrap();
    db.conn.lock().unwrap().execute_batch(
            "CREATE TRIGGER reject_root BEFORE INSERT ON sessions
             WHEN NEW.id = 'blocked' BEGIN SELECT RAISE(ABORT, 'root inserted before admission'); END;"
        ).unwrap();
    let accepted = db.create_bound_session_and_accept_turn(
        "sixteenth",
        location,
        "t1",
        "prompt",
        "user",
        None,
        &test_model(),
    );
    assert_eq!(accepted.unwrap().user_message, "m0001");
    let refusal = db.create_bound_session_and_accept_turn(
        "blocked",
        location,
        "t2",
        "prompt",
        "user",
        None,
        &test_model(),
    );
    assert!(
        matches!(refusal, Err(StorageError::Io(ref e)) if e.kind() == std::io::ErrorKind::InvalidData)
    );
    assert_eq!(fresh_turn_rows(&db, "blocked", "t2"), (0, 0, 0, 0, 0));
    assert_eq!(
        db.get_pref(&tab_adoption_key(location, "blocked")).unwrap(),
        None
    );
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_root")
        .unwrap();
    // The trigger is gone; the full union still refuses the root.
    assert!(matches!(
        db.create_bound_session_and_accept_turn("blocked", location, "t2", "prompt", "user", None, &test_model()),
        Err(StorageError::Io(ref e)) if e.kind() == std::io::ErrorKind::InvalidData
    ));
}

#[test]
fn fresh_turn_sql_failures_roll_back_root_binding_and_every_event() {
    let tmp = tmp_root("fresh-turn-failure");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let selection_key =
        |session: &str| format!("tui.selection.session:[\"/project\",\"provider\",\"{session}\"]");
    for (table, session, turn) in [
        ("turns", "turn-fail", "t1"),
        ("messages", "message-fail", "t2"),
    ] {
        let trigger = format!(
            "CREATE TRIGGER fail_fresh BEFORE INSERT ON {table}
                 BEGIN SELECT RAISE(ABORT, 'injected fresh turn failure'); END;"
        );
        db.conn.lock().unwrap().execute_batch(&trigger).unwrap();
        assert!(matches!(
            db.create_bound_session_and_accept_turn(
                session,
                "/project",
                turn,
                "prompt",
                "visible input",
                Some((&selection_key(session), "choice")),
                &test_model()
            ),
            Err(StorageError::Sqlite(_))
        ));
        assert_eq!(fresh_turn_rows(&db, session, turn), (0, 0, 0, 0, 0));
        assert_eq!(db.get_pref(&selection_key(session)).unwrap(), None);
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_fresh;")
            .unwrap();
        assert_eq!(
            db.create_bound_session_and_accept_turn(
                session,
                "/project",
                turn,
                "prompt",
                "visible input",
                Some((&selection_key(session), "choice")),
                &test_model()
            )
            .unwrap()
            .user_message,
            if session == "turn-fail" {
                "m0001"
            } else {
                "m0002"
            }
        );
        assert_eq!(fresh_turn_rows(&db, session, turn), (1, 1, 4, 1, 1));
        assert_eq!(
            db.get_pref(&selection_key(session)).unwrap().as_deref(),
            Some("choice")
        );
    }
}

#[test]
fn fresh_turn_selection_insert_failure_rolls_back_everything_and_retries() {
    let tmp = tmp_root("fresh-selection-failure");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let key = "tui.selection.session:[\"/project\",\"provider\",\"fresh\"]";
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_selection BEFORE INSERT ON prefs
             WHEN NEW.key LIKE 'tui.selection.session:%'
             BEGIN SELECT RAISE(ABORT, 'injected selection failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        db.create_bound_session_and_accept_turn(
            "fresh",
            "/project",
            "turn",
            "prompt",
            "user",
            Some((key, "choice")),
            &test_model()
        ),
        Err(StorageError::Sqlite(_))
    ));
    assert_eq!(fresh_turn_rows(&db, "fresh", "turn"), (0, 0, 0, 0, 0));
    assert_eq!(db.get_pref(key).unwrap(), None);
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_selection;")
        .unwrap();
    assert_eq!(
        db.create_bound_session_and_accept_turn(
            "fresh",
            "/project",
            "turn",
            "prompt",
            "user",
            Some((key, "choice")),
            &test_model()
        )
        .unwrap()
        .user_message,
        "m0001"
    );
    assert_eq!(fresh_turn_rows(&db, "fresh", "turn"), (1, 1, 4, 1, 1));
    assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("choice"));
}

#[test]
fn fresh_turn_selection_key_cannot_override_binding_or_other_prefs() {
    let tmp = tmp_root("fresh-selection-key");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let unrelated = "tui.selection.variant:[\"provider\",\"model\"]";
    db.set_pref(unrelated, "original").unwrap();
    for (index, key) in [
        "tui.session_location.fresh",
        "tui.session_location.other",
        unrelated,
        "dcp.nudge.fresh",
        "tui.selection.session:[\"/project\",\"provider\",\"other\"]",
        "tui.selection.session:not-json",
    ]
    .iter()
    .enumerate()
    {
        assert!(
            db.create_bound_session_and_accept_turn(
                "fresh",
                "/project",
                &format!("turn{index}"),
                "prompt",
                "user",
                Some((key, "bad")),
                &test_model()
            )
            .is_err()
        );
        assert_eq!(
            fresh_turn_rows(&db, "fresh", &format!("turn{index}")),
            (0, 0, 0, 0, 0)
        );
        assert_eq!(db.get_pref("tui.session_location.fresh").unwrap(), None);
        assert_eq!(db.get_pref("tui.session_location.other").unwrap(), None);
    }
    assert_eq!(db.get_pref(unrelated).unwrap().as_deref(), Some("original"));
    assert_eq!(db.get_pref("dcp.nudge.fresh").unwrap(), None);
}

#[test]
fn fresh_turn_is_fresh_only_and_preserves_existing_accept_turn() {
    let tmp = tmp_root("fresh-turn-success");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    let key = "tui.selection.session:[\"/project\",\"provider\",\"new\"]";
    db.set_pref(key, "old choice").unwrap();
    let first = db
        .create_bound_session_and_accept_turn(
            "new",
            "/project",
            "first",
            "raw prompt",
            "rendered user text",
            Some((key, "new choice")),
            &test_model(),
        )
        .unwrap();
    assert_eq!(first.user_message, "m0001");
    assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("new choice"));
    assert_eq!(
        db.get_pref("tui.session_location.new").unwrap().as_deref(),
        Some("/project")
    );
    assert_eq!(db.session_meta("new").unwrap().parent_id, None);
    assert_eq!(
        db.read_history_full("new").unwrap(),
        vec![(
            first.user_message.clone(),
            "user".into(),
            "rendered user text".into()
        )]
    );
    {
        let conn = db.conn.lock().unwrap();
        let turn: (String, String, Option<String>) = conn
            .query_row(
                "SELECT status, prompt, result FROM turns WHERE id = 'first'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(turn, ("started".into(), "raw prompt".into(), None));
        let events: Vec<(String, String)> = conn
            .prepare("SELECT kind, payload FROM events WHERE session_id = 'new' ORDER BY seq")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            events,
            vec![
                ("session_created".into(), "{}".into()),
                ("turn_started".into(), "first".into()),
                (
                    "accepted_model".into(),
                    serde_json::to_string(&test_model()).unwrap()
                ),
                ("message".into(), first.user_message),
            ]
        );
    }
    for location in ["/project", "/other"] {
        assert!(matches!(
            db.create_bound_session_and_accept_turn(
                "new",
                location,
                "duplicate",
                "bad",
                "bad",
                Some((key, "bad choice")),
                &test_model()
            ),
            Err(StorageError::SessionAlreadyExists)
        ));
    }
    assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("new choice"));
    assert_eq!(fresh_turn_rows(&db, "new", "duplicate"), (1, 1, 4, 0, 1));
    db.create_session("unbound").unwrap();
    assert!(matches!(
        db.create_bound_session_and_accept_turn(
            "unbound",
            "/project",
            "duplicate",
            "bad",
            "bad",
            None,
            &test_model()
        ),
        Err(StorageError::SessionAlreadyExists)
    ));
    assert_eq!(
        fresh_turn_rows(&db, "unbound", "duplicate"),
        (1, 0, 1, 0, 0)
    );
    assert_eq!(
        db.accept_turn("next", "new", "another prompt", "next user", &test_model())
            .unwrap()
            .user_message,
        "m0002"
    );
    assert_eq!(fresh_turn_rows(&db, "new", "next"), (1, 1, 7, 1, 2));
    assert!(matches!(
        db.accept_turn("next", "new", "bad", "bad", &test_model()),
        Err(StorageError::Sqlite(_))
    ));
    assert_eq!(fresh_turn_rows(&db, "new", "next"), (1, 1, 7, 1, 2));
}

#[test]
fn fresh_turn_duplicate_turn_id_cannot_strand_new_session() {
    let tmp = tmp_root("fresh-duplicate-turn");
    let db = Db::open(&tmp.path().join("data")).unwrap();
    db.create_session("existing").unwrap();
    db.accept_turn("taken", "existing", "first", "first", &test_model())
        .unwrap();
    assert!(matches!(
        db.create_bound_session_and_accept_turn(
            "new",
            "/project",
            "taken",
            "second",
            "second",
            None,
            &test_model()
        ),
        Err(StorageError::Sqlite(_))
    ));
    assert_eq!(fresh_turn_rows(&db, "new", "taken"), (0, 0, 0, 1, 0));
    assert_eq!(
        db.read_history("existing").unwrap(),
        vec![("user".into(), "first".into())]
    );
    assert_eq!(
        db.create_bound_session_and_accept_turn(
            "new",
            "/project",
            "free",
            "second",
            "second",
            None,
            &test_model()
        )
        .unwrap()
        .user_message,
        "m0002"
    );
    assert_eq!(fresh_turn_rows(&db, "new", "free"), (1, 1, 4, 1, 1));
}

#[test]
fn model_switch_is_atomic_scoped_replayable_and_excluded_from_context() {
    let tmp = tmp_root("model-switch-atomic");
    let path = tmp.path().join("data");
    let old = test_model();
    let new = oc_core::queries::ModelRef {
        provider: "other".into(),
        id: "new".into(),
        variant: Some("high".into()),
    };
    {
        let db = Db::open(&path).unwrap();
        db.apply_dcp_schema().unwrap();
        db.create_bound_session_and_accept_turn("a", "/one", "first", "one", "one", None, &old)
            .unwrap();
        db.create_bound_session_and_accept_turn("b", "/one", "other", "other", "other", None, &new)
            .unwrap();
        assert!(db.latest_model_switch("a").unwrap().is_none());
        db.conn.lock().unwrap().execute_batch(
                "CREATE TRIGGER refuse_user BEFORE INSERT ON messages WHEN NEW.role='user' AND NEW.text='reject'
                 BEGIN SELECT RAISE(ABORT, 'refused'); END;"
            ).unwrap();
        assert!(
            db.accept_turn("refused", "a", "reject", "reject", &new)
                .is_err()
        );
        assert!(db.latest_model_switch("a").unwrap().is_none());
        assert_eq!(db.read_history_page("a", 10, None).unwrap().len(), 1);
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER refuse_user")
            .unwrap();
        db.accept_turn("same", "a", "two", "two", &old).unwrap();
        assert!(db.latest_model_switch("a").unwrap().is_none());
        let accepted = db
            .accept_turn("changed", "a", "three", "three", &new)
            .unwrap();
        assert_eq!(accepted.model_switch.as_ref().unwrap().display_name, None);
        assert_eq!(db.latest_model_switch("a").unwrap().unwrap().previous, old);
        assert_eq!(db.latest_model_switch("a").unwrap(), accepted.model_switch);
        let page = db.read_history_page("a", 2, None).unwrap();
        assert_eq!(page[0].1, "user");
        assert_eq!(page[1].1, "model_switch");
        assert!(page[1].2.contains("\"current\""));
        assert!(!page[1].2.contains("display_name"));
        assert_eq!(db.read_history_full("a").unwrap().len(), 3);
        assert_eq!(db.active_history("a", 0, 1000).unwrap().rows.len(), 3);
        assert_eq!(db.read_history_after("a", 10, 0).unwrap().len(), 4);
    }
    let db = Db::open(&path).unwrap();
    assert_eq!(db.latest_model_switch("a").unwrap().unwrap().current, new);
    db.accept_turn("unchanged", "a", "four", "four", &new)
        .unwrap();
    assert!(db.latest_model_switch("a").unwrap().is_none());
}

#[test]
fn list_sessions_keeps_id_order_and_includes_children() {
    let tmp = tmp_root("child-list");
    let db = Db::open(&tmp.path().join("data")).expect("open");
    db.create_session("a-root").expect("a-root");
    db.create_session("z-root").expect("z-root");
    db.create_child_session("a-root", "m-child", None, None, None)
        .expect("m-child");
    db.create_child_session("a-root", "b-child", None, None, None)
        .expect("b-child");
    // Legacy behavior: every session row, no parent filter, id ascending.
    assert_eq!(
        db.list_sessions().expect("list"),
        vec!["a-root", "b-child", "m-child", "z-root"]
    );
}
