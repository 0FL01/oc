use super::*;

fn model(id: &str) -> oc_core::queries::ModelRef {
    oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: id.into(),
        variant: None,
    }
}
fn turn(db: &Db, id: &str, text: &str, selected: &str) -> String {
    let user = db
        .accept_turn(id, "s", text, text, &model(selected))
        .unwrap()
        .user_message;
    db.commit_turn(id, "completed", None, Some(&format!("answer {text}")))
        .unwrap();
    user
}

#[test]
fn conversation_points_restore_dcp_branch_restart_and_admission_rollback() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = turn(&db, "one", "one", "m");
    let old = db
        .save_compression_block(
            "s",
            "old",
            "old summary",
            &first,
            &first,
            std::slice::from_ref(&first),
        )
        .unwrap();
    db.save_prune_mark("s", &first).unwrap();
    let nudge_key = "dcp.nudge.s\0fixture\0m";
    db.set_pref(nudge_key, "{\"turns_since_compress\":1}")
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO dcp_tool_projection_v2 VALUES ('s','call',0,'hidden')",
            [],
        )
        .unwrap();
    let second = db
        .accept_turn("two", "s", "expanded /review", "/review", &model("other"))
        .unwrap()
        .user_message;
    db.delete_compression_block(&old).unwrap();
    db.save_compression_block(
        "s",
        "new",
        "new summary",
        &second,
        &second,
        std::slice::from_ref(&second),
    )
    .unwrap();
    db.save_prune_mark("s", &second).unwrap();
    db.set_pref(nudge_key, "{\"turns_since_compress\":2}")
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE dcp_tool_projection_v2 SET action='purged' WHERE session_id='s'",
            [],
        )
        .unwrap();
    db.commit_turn("two", "completed", None, Some("tail"))
        .unwrap();
    let raw = db.read_history_full("s").unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE compression_blocks SET summary='late unsaved summary' WHERE session_id='s'",
            [],
        )
        .unwrap();
    // Repeated completion cannot rewrite an already-saved point's DCP version.
    db.finish_turn("two", "completed", None).unwrap();
    db.set_pref(nudge_key, "{\"turns_since_compress\":3}")
        .unwrap();
    let undone = db
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(undone.draft.as_deref(), Some("/review"));
    assert!(undone.can_undo && undone.can_redo);
    assert_eq!(
        db.load_compression_blocks("s").unwrap()[0].summary,
        "old summary"
    );
    assert_eq!(
        db.load_prune_mark("s").unwrap().as_deref(),
        Some(first.as_str())
    );
    assert!(
        db.load_dcp_tool_projection("s")
            .unwrap()
            .hidden
            .contains(&("call".into(), 0))
    );
    assert_eq!(
        db.get_pref(nudge_key).unwrap().as_deref(),
        Some("{\"turns_since_compress\":1}")
    );
    assert_eq!(db.history_len("s").unwrap(), 2);
    assert_eq!(db.read_history_full("s").unwrap(), raw);
    // Another session cannot reuse an archived block identity while this one is undone.
    db.create_session("other").unwrap();
    let other = db.append_message("other", "user", "other").unwrap();
    let other_block = db
        .save_compression_block(
            "other",
            "other",
            "summary",
            &other,
            &other,
            std::slice::from_ref(&other),
        )
        .unwrap();
    assert_eq!(other_block, "b0003");
    // Failed durable admission rolls back invalidation and interval writes.
    assert!(
        db.accept_turn("one", "s", "bad", "bad", &model("m"))
            .is_err()
    );
    assert_eq!(db.history_len("s").unwrap(), 2);
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let redone = db
        .change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert!(!redone.can_redo);
    assert!(redone.reverted.is_none());
    assert_eq!(
        db.get_pref(nudge_key).unwrap().as_deref(),
        Some("{\"turns_since_compress\":3}")
    );
    assert_eq!(
        db.load_compression_blocks("s").unwrap()[0].summary,
        "new summary"
    );
    assert!(
        db.load_dcp_tool_projection("s")
            .unwrap()
            .purged
            .contains(&("call".into(), 0))
    );
    assert_eq!(db.read_history_full("s").unwrap(), raw);
    db.change_conversation(
        "s",
        ConversationAction::Revert {
            message: oc_core::session::MessageId(second),
        },
    )
    .unwrap();
    let accepted = db
        .accept_turn("branch", "s", "branch", "branch", &model("m"))
        .unwrap();
    assert!(
        accepted.model_switch.is_none(),
        "comparison follows active branch, not archived other model"
    );
    db.commit_turn("branch", "completed", None, Some("branch answer"))
        .unwrap();
    assert!(
        db.change_conversation("s", ConversationAction::Redo)
            .is_err()
    );
    assert_eq!(
        db.conversation_history_full("s")
            .unwrap()
            .iter()
            .map(|r| r.2.as_str())
            .collect::<Vec<_>>(),
        ["one", "answer one", "branch", "branch answer"]
    );
    for _ in 0..3 {
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        assert_eq!(db.history_len("s").unwrap(), 0);
        db.change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert_eq!(db.history_len("s").unwrap(), 4);
    }
    for index in 0..3 {
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        let name = format!("replacement-{index}");
        turn(&db, &name, &name, "m");
        assert!(
            db.change_conversation("s", ConversationAction::Redo)
                .is_err()
        );
        assert_eq!(
            db.conversation_history_full("s")
                .unwrap()
                .iter()
                .map(|r| r.2.as_str())
                .collect::<Vec<_>>(),
            ["one", "answer one", &name, &format!("answer {name}")]
        );
    }
    let conn = db.conn.lock().unwrap();
    let old_objects: i64 = conn
        .query_row("SELECT count(*) FROM conversation_objects WHERE kind=0 AND json_extract(payload,'$[3]')='old summary'", [], |r|r.get(0))
        .unwrap();
    assert_eq!(
        old_objects, 1,
        "row objects shared through repeated restore"
    );
}

#[test]
fn conversation_revisions_share_large_metadata_and_growing_memberships() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = turn(&db, "first", "first", "m");
    // An aggregate above the removed ceiling remains a valid session. Each
    // summary stays within the runtime's existing per-summary byte budget.
    let summary = "x".repeat(60_000);
    let mut block = String::new();
    for i in 0..150 {
        block = db
            .save_compression_block(
                "s",
                &format!("topic {i}"),
                &summary,
                &first,
                &first,
                std::slice::from_ref(&first),
            )
            .unwrap();
    }
    let bytes = || -> i64 {
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT SUM(length(CAST(payload AS BLOB))) FROM conversation_objects",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    let baseline = bytes();
    assert!(baseline > 8 * 1024 * 1024);
    let (base_versions,base_disk): (i64,i64) = db.conn.lock().unwrap().query_row("SELECT (SELECT COUNT(*) FROM conversation_versions),(SELECT page_count FROM pragma_page_count)*(SELECT page_size FROM pragma_page_size)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    for i in 0..40 {
        let accepted = db
            .accept_turn(&format!("t{i}"), "s", "prompt", "prompt", &model("m"))
            .unwrap();
        db.set_pref(
            "dcp.nudge.s\0fixture\0m",
            &format!("{{\"turns_since_compress\":{i}}}"),
        )
        .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO compression_members VALUES(?1,?2)",
                params![block, accepted.user_message],
            )
            .unwrap();
        db.commit_turn(&format!("t{i}"), "completed", None, Some("assistant"))
            .unwrap();
    }
    assert!(
        bytes() - baseline < 20_000,
        "only new memberships and nudge rows grow, not stable summary/archive references"
    );
    let conn = db.conn.lock().unwrap();
    let (versions,disk): (i64,i64) = conn.query_row("SELECT (SELECT COUNT(*) FROM conversation_versions),(SELECT page_count FROM pragma_page_count)*(SELECT page_size FROM pragma_page_size)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(
        versions - base_versions,
        80,
        "only 40 changed nudge rows and 40 new member rows, no per-point reference set"
    );
    assert!(
        disk - base_disk < 512 * 1024,
        "actual database growth stays incremental too"
    );
    let objects: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_objects WHERE kind=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(objects, 150);
    let context_bytes: i64 = conn
        .query_row(
            "SELECT SUM(length(metadata)) FROM conversation_contexts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(context_bytes < 1000, "points carry scalar revisions");
    drop(conn);
    // Recovery settles a started turn in the same over-8MiB session.
    db.accept_turn("crash", "s", "crash", "crash", &model("m"))
        .unwrap();
    db.recover_interrupted_tools().unwrap();
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let version_count = || -> i64 {
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM conversation_versions", [], |r| {
                r.get(0)
            })
            .unwrap()
    };
    let before_restore = version_count();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(
        version_count(),
        before_restore,
        "unchanged restoration creates no reference set"
    );
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert!(
        version_count() - before_restore <= 4,
        "restore journals only changed nudge/member rows"
    );
    assert_eq!(db.load_compression_blocks("s").unwrap().len(), 150);
    assert!(
        bytes_for_objects(&db) - baseline < 20_000,
        "restore shares immutable objects too"
    );
}

fn bytes_for_objects(db: &Db) -> i64 {
    db.conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT SUM(length(CAST(payload AS BLOB))) FROM conversation_objects",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn conversation_settlement_context_and_title_stamp_are_atomic() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    db.accept_turn("one", "s", "prompt", "prompt", &model("m"))
        .unwrap();
    let (title, stamp) = db.root_title_stamp("s").unwrap().unwrap();
    let before = db.read_history_full("s").unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_point BEFORE UPDATE OF post_context ON conversation_points BEGIN SELECT RAISE(ABORT,'injected point failure'); END;").unwrap();
    assert!(
        db.commit_turn("one", "completed", Some("{}"), Some("assistant"))
            .is_err()
    );
    assert_eq!(db.read_history_full("s").unwrap(), before);
    assert_eq!(db.turn_result("one").unwrap(), ("started".into(), None));
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_point")
        .unwrap();
    db.commit_turn("one", "completed", Some("{}"), Some("assistant"))
        .unwrap();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(db.title_context("s", false).unwrap().is_none());
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert!(
        !db.compare_and_set_root_title("s", title.as_deref(), stamp, "stale title")
            .unwrap(),
        "queued title result cannot revive after undo and redo"
    );
    assert_eq!(db.session_meta("s").unwrap().title, None);
}

#[test]
fn conversation_migration_bootstraps_real_rows_and_reads_genuine_legacy_json() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let user = db.append_message("s", "user", "legacy user").unwrap();
    let block = db
        .save_compression_block(
            "s",
            "topic",
            "real legacy summary",
            &user,
            &user,
            std::slice::from_ref(&user),
        )
        .unwrap();
    db.set_pref("dcp.nudge.s\0fixture\0m", "{\"turns_since_compress\":7}")
        .unwrap();
    // Model an older database with real mutable DCP tables but no version
    // tracking. Neither bootstrap nor a new point invents past versions.
    {
        let conn = db.conn.lock().unwrap();
        for kind in 0..TABLES.len() {
            for op in ["INSERT", "UPDATE", "DELETE"] {
                conn.execute_batch(&format!("DROP TRIGGER conversation_track_{kind}_{op};"))
                    .unwrap();
            }
        }
        conn.execute_batch("DROP TABLE conversation_versions; DROP TABLE conversation_objects; DROP TABLE conversation_revision; DROP TABLE conversation_tracked;").unwrap();
    }
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert!(
        db.change_conversation(
            "s",
            ConversationAction::Revert {
                message: oc_core::session::MessageId(user.clone())
            }
        )
        .unwrap_err()
        .to_string()
        .contains("no saved historical context")
    );
    db.accept_turn("new", "s", "new", "new", &model("m"))
        .unwrap();
    // The earlier experimental format contains genuine saved rows. Retain
    // read compatibility without loading its aggregate JSON into Rust.
    let mut tables = Vec::new();
    {
        let conn = db.conn.lock().unwrap();
        for (table, columns, predicate) in TABLES {
            let raw: String = conn.query_row(&format!("SELECT json_group_array(json_array({columns})) FROM {table} WHERE {predicate}"),["s"],|r|r.get(0)).unwrap();
            tables.push(serde_json::from_str::<serde_json::Value>(&raw).unwrap());
        }
        conn.execute(
            "INSERT INTO conversation_contexts VALUES('legacy-real',?1)",
            [serde_json::to_string(&tables).unwrap()],
        )
        .unwrap();
        conn.execute(
            "UPDATE conversation_points SET pre_context='legacy-real' WHERE turn_id='new'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE compression_blocks SET summary='new summary' WHERE id=?1",
            [block],
        )
        .unwrap();
    }
    db.commit_turn("new", "completed", None, Some("answer"))
        .unwrap();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(
        db.load_compression_blocks("s").unwrap()[0].summary,
        "real legacy summary"
    );
    assert_eq!(
        db.get_pref("dcp.nudge.s\0fixture\0m").unwrap().as_deref(),
        Some("{\"turns_since_compress\":7}")
    );
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(
        db.load_compression_blocks("s").unwrap()[0].summary,
        "new summary"
    );
}

#[test]
fn conversation_legacy_boundary_is_explicitly_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    let id = db.append_message("s", "user", "legacy").unwrap();
    let raw = db.read_history_full("s").unwrap();
    for action in [
        ConversationAction::Undo,
        ConversationAction::Revert {
            message: oc_core::session::MessageId(id),
        },
    ] {
        let error = db.change_conversation("s", action).unwrap_err();
        assert!(error.to_string().contains("no saved historical context"));
        assert_eq!(db.conversation_history_full("s").unwrap(), raw);
    }
}

#[test]
fn conversation_whole_tail_skips_empty_user_and_preserves_original_tip() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    let first = turn(&db, "one", "one", "m");
    let second = turn(&db, "two", "two", "m");
    turn(&db, "three", "three", "m");
    turn(&db, "special", "", "m");
    let raw = db.read_history_full("s").unwrap();
    let undo = db
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(undo.draft.as_deref(), Some("three"));
    assert_eq!(undo.reverted.unwrap().user_messages, 2);
    let undo = db
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(
        undo.reverted,
        Some(RevertedConversation {
            message: oc_core::session::MessageId(second.clone()),
            user_messages: 3
        })
    );
    drop(db);
    let db = Db::open(root.path()).unwrap();
    let redo = db
        .change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert!(!redo.can_redo);
    assert!(redo.reverted.is_none());
    assert_eq!(db.conversation_history_full("s").unwrap(), raw);
    let revert = db
        .change_conversation(
            "s",
            ConversationAction::Revert {
                message: oc_core::session::MessageId(first),
            },
        )
        .unwrap();
    assert_eq!(revert.reverted.unwrap().user_messages, 4);
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(db.conversation_history_full("s").unwrap(), raw);
    db.change_conversation(
        "s",
        ConversationAction::Revert {
            message: oc_core::session::MessageId(second),
        },
    )
    .unwrap();
    turn(&db, "branch", "branch", "m");
    assert!(db.reverted_conversation("s").unwrap().is_none());
    assert!(
        db.change_conversation("s", ConversationAction::Redo)
            .is_err()
    );
    assert_eq!(db.read_history_full("s").unwrap().len(), raw.len() + 2);
}

#[test]
fn conversation_missing_tip_cannot_be_replaced_by_an_earlier_redo_point() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    turn(&db, "one", "one", "m");
    let second = turn(&db, "two", "two", "m");
    turn(&db, "three", "three", "m");
    db.conn
        .lock()
        .unwrap()
        .execute("DELETE FROM conversation_points WHERE turn_id='three'", [])
        .unwrap();
    let reverted = db
        .change_conversation(
            "s",
            ConversationAction::Revert {
                message: oc_core::session::MessageId(second),
            },
        )
        .unwrap();
    assert!(!reverted.can_redo);
    let undone = db
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(!undone.can_redo);
    assert_eq!(undone.reverted.unwrap().user_messages, 3);
    assert!(
        db.change_conversation("s", ConversationAction::Redo)
            .is_err()
    );
    assert_eq!(db.history_len("s").unwrap(), 0);
    assert_eq!(db.read_history_full("s").unwrap().len(), 6);
}
