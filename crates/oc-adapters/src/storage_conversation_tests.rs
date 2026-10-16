use super::*;

fn model(id: &str) -> oc_core::queries::ModelRef {
    oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: id.into(),
        variant: None,
    }
}

#[test]
fn child_notice_is_not_an_undo_prompt_and_redo_keeps_the_exact_delivery() {
    use oc_core::{
        domain::SessionId,
        queries::{ChildJob, ChildState},
        session::MessageId,
    };
    for explicit in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let first = db
            .create_bound_session_and_accept_turn_with_reminder(
                "s",
                "/project",
                "one",
                "one",
                "one",
                None,
                &model("fixture"),
                None,
                None,
            )
            .unwrap()
            .user_message;
        db.commit_turn("one", "completed", None, Some("answer one"))
            .unwrap();
        let second = turn(&db, "two", "two", "fixture");
        db.create_child_session(
            "s",
            "child",
            Some("helper"),
            Some("fixture/child"),
            Some("work"),
        )
        .unwrap();
        db.record_tool_intent(
            "launch",
            "s",
            Some("two"),
            "subagent",
            "{\"agent\":\"helper\",\"prompt\":\"work\"}",
        )
        .unwrap();
        db.admit_child_job(&ChildJob {
            parent: SessionId("s".into()),
            child: SessionId("child".into()),
            operation: "launch".into(),
            generation: 1,
            location: "/project".into(),
            agent: "helper".into(),
            model: "fixture/child".into(),
            description: "work".into(),
            delivery_id: "child-delivery:launch".into(),
            state: ChildState::Admitted,
            background: true,
            turn: None,
            result: None,
            message_id: None,
        })
        .unwrap();
        db.finish_child_job("launch", ChildState::Completed, "finished data")
            .unwrap();
        let notice = db.deliver_child_notices().unwrap().remove(0);
        let notice_id = MessageId(notice.job.message_id.unwrap());
        let raw = db.read_history_full("s").unwrap();
        let projection = db.child_history("s", &notice_id).unwrap().unwrap();
        let undone = db
            .change_conversation(
                "s",
                if explicit {
                    ConversationAction::Revert {
                        message: MessageId(first),
                    }
                } else {
                    ConversationAction::Undo
                },
            )
            .unwrap();
        assert_eq!(
            undone.draft.as_deref(),
            Some(if explicit { "one" } else { "two" })
        );
        assert_eq!(
            undone.reverted.as_ref().unwrap().user_messages,
            if explicit { 2 } else { 1 }
        );
        if !explicit {
            assert_eq!(undone.reverted.unwrap().message.0, second);
        }
        assert!(undone.can_redo);
        assert_eq!(
            db.child_history("s", &notice_id).unwrap(),
            None,
            "current-branch projection excludes reverted delivery"
        );
        assert_eq!(db.read_history_full("s").unwrap(), raw);
        drop(db);
        let db = Db::open(root.path()).unwrap();
        let restored = db
            .change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert!(restored.reverted.is_none() && !restored.can_redo);
        assert_eq!(db.conversation_history_full("s").unwrap(), raw);
        assert_eq!(db.child_history("s", &notice_id).unwrap(), Some(projection));
        assert!(db.deliver_child_notices().unwrap().is_empty());
    }
}

#[test]
fn model_shell_notice_is_bounded_exact_data_and_redo_preserves_its_raw_delivery() {
    use crate::shell::jobs::{Outcome, Provenance};
    use oc_core::session::MessageId;
    for explicit in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let first = db
            .create_bound_session_and_accept_turn_with_reminder(
                "s",
                "/project",
                "one",
                "one",
                "one",
                None,
                &model("fixture"),
                None,
                None,
            )
            .unwrap()
            .user_message;
        db.commit_turn("one", "completed", None, Some("answer one"))
            .unwrap();
        let second = turn(&db, "two", "two", "fixture");
        let command = format!("printf Ω界\n{}", "Ω界".repeat(2048));
        db.record_tool_intent(
            "model-op",
            "s",
            Some("two"),
            "shell",
            &serde_json::json!({"command":command}).to_string(),
        )
        .unwrap();
        let provenance = Provenance {
            version: 1,
            session: "s".into(),
            turn: "two".into(),
            operation: "model-op".into(),
            location: "/project".into(),
            generation: 7,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "fixture".into(),
            provider: "fixture".into(),
            command,
            cwd: "/project".into(),
            selected_shell: "/bin/sh".into(),
        };
        db.admit_shell_job_mode(&provenance, false).unwrap();
        let mut outcome = Outcome::unknown("literal cancelled failed successful process prose");
        outcome.state = "completed".into();
        outcome.exit = Some(0);
        outcome.stdout = "[stderr] data, not metadata".into();
        db.finish_shell_job("model-op", &outcome).unwrap();
        let notice = db.deliver_shell_notices().unwrap().remove(0);
        assert!(!notice.user_requested);
        let id = MessageId(notice.message_id.clone());
        let projection = db.model_shell_notice("s", &id).unwrap().unwrap();
        assert_eq!(projection.operation, "model-op");
        assert_eq!(
            projection.state, "completed",
            "state never comes from process prose"
        );
        assert!(projection.command.len() <= 2048 && projection.command.ends_with('…'));
        assert!(projection.command.starts_with("printf Ω界"));
        assert!(db.user_shell_result("s", &id).unwrap().is_none());
        db.create_session("foreign").unwrap();
        assert!(db.model_shell_notice("foreign", &id).unwrap().is_none());
        let spoof = MessageId(db.append_message("foreign", "user", &notice.text).unwrap());
        assert!(db.model_shell_notice("foreign", &spoof).unwrap().is_none());
        let raw = db.read_history_full("s").unwrap();
        for version in [serde_json::json!(true), serde_json::json!(1.0)] {
            let mut malformed = serde_json::to_value(&provenance).unwrap();
            malformed["version"] = version;
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE shell_jobs SET provenance=?1 WHERE operation_id='model-op'",
                    [serde_json::to_string(&malformed).unwrap()],
                )
                .unwrap();
            assert!(
                db.model_shell_notice("s", &id).unwrap().is_none(),
                "SQLite coercion must not qualify a non-integer schema version"
            );
        }
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE shell_jobs SET provenance=?1 WHERE operation_id='model-op'",
                [serde_json::to_string(&provenance).unwrap()],
            )
            .unwrap();
        for version in [serde_json::json!(true), serde_json::json!(1.0)] {
            let mut malformed = serde_json::to_value(&outcome).unwrap();
            malformed["version"] = version;
            db.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE shell_jobs SET outcome=?1 WHERE operation_id='model-op'",
                    [serde_json::to_string(&malformed).unwrap()],
                )
                .unwrap();
            assert!(db.model_shell_notice("s", &id).unwrap().is_none());
        }
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE shell_jobs SET outcome=?1 WHERE operation_id='model-op'",
                [serde_json::to_string(&outcome).unwrap()],
            )
            .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE shell_jobs SET provenance='{' WHERE operation_id='model-op'",
                [],
            )
            .unwrap();
        assert!(db.model_shell_notice("s", &id).unwrap().is_none());
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE shell_jobs SET provenance=?1 WHERE operation_id='model-op'",
                [serde_json::to_string(&provenance).unwrap()],
            )
            .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM events WHERE session_id='s' AND kind='shell_notice'",
                [],
            )
            .unwrap();
        assert!(
            db.model_shell_notice("s", &id).unwrap().is_none(),
            "positive delivery event is required"
        );
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO events(session_id,kind,payload) VALUES('s','shell_notice',?1)",
                [&notice.delivery_id],
            )
            .unwrap();
        assert_eq!(
            db.model_shell_notice("s", &id).unwrap(),
            Some(projection.clone())
        );
        let undone = db
            .change_conversation(
                "s",
                if explicit {
                    ConversationAction::Revert {
                        message: MessageId(first),
                    }
                } else {
                    ConversationAction::Undo
                },
            )
            .unwrap();
        assert_eq!(
            undone.draft.as_deref(),
            Some(if explicit { "one" } else { "two" })
        );
        assert_eq!(
            undone.reverted.as_ref().unwrap().user_messages,
            if explicit { 2 } else { 1 }
        );
        if !explicit {
            assert_eq!(undone.reverted.unwrap().message.0, second);
        }
        assert!(undone.can_redo);
        assert!(db.model_shell_notice("s", &id).unwrap().is_none());
        assert_eq!(db.read_history_full("s").unwrap(), raw);
        drop(db);
        let db = Db::open(root.path()).unwrap();
        let redo = db
            .change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert!(!redo.can_redo && redo.reverted.is_none());
        assert_eq!(db.conversation_history_full("s").unwrap(), raw);
        assert_eq!(db.model_shell_notice("s", &id).unwrap(), Some(projection));
        assert!(db.deliver_shell_notices().unwrap().is_empty());
        let copied = MessageId(db.append_message("s", "user", &notice.text).unwrap());
        assert!(db.model_shell_notice("s", &copied).unwrap().is_none());
        assert!(
            db.change_conversation("s", ConversationAction::Undo)
                .is_err(),
            "ordinary unlinked legacy user messages still refuse unsupported Undo"
        );
    }
}

#[test]
fn direct_user_shell_is_not_an_undo_prompt_and_whole_tail_redo_preserves_raw_and_job() {
    for explicit in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let first = db
            .create_bound_session_and_accept_turn_with_reminder(
                "s",
                "/project",
                "one",
                "one",
                "one",
                None,
                &model("fixture"),
                None,
                None,
            )
            .unwrap()
            .user_message;
        db.commit_turn("one", "completed", None, Some("answer one"))
            .unwrap();
        let second = turn(&db, "two", "two", "fixture");
        let p = crate::shell::jobs::Provenance {
            version: 1,
            session: "s".into(),
            turn: String::new(),
            operation: "owned-shell".into(),
            location: "/project".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "fixture".into(),
            provider: "fixture".into(),
            command: "printf data".into(),
            cwd: "/project".into(),
            selected_shell: "/bin/sh".into(),
        };
        drop(db.admit_user_shell_job(&p, false, None).unwrap());
        let mut outcome = crate::shell::jobs::Outcome::unknown("fixture outcome");
        outcome.state = "completed".into();
        outcome.exit = Some(0);
        outcome.stdout = "process output".into();
        db.finish_shell_job(&p.operation, &outcome).unwrap();
        let notice = db.deliver_shell_notices().unwrap().remove(0);
        let raw = db.read_history_full("s").unwrap();
        let projected = db
            .user_shell_result("s", &oc_core::session::MessageId(notice.message_id.clone()))
            .unwrap()
            .unwrap();
        let action = if explicit {
            ConversationAction::Revert {
                message: oc_core::session::MessageId(first),
            }
        } else {
            ConversationAction::Undo
        };
        let undone = db.change_conversation("s", action).unwrap();
        assert_eq!(
            undone.draft.as_deref(),
            Some(if explicit { "one" } else { "two" })
        );
        assert!(undone.can_redo);
        assert_eq!(
            undone.reverted.as_ref().unwrap().user_messages,
            if explicit { 2 } else { 1 }
        );
        if !explicit {
            assert_eq!(undone.reverted.unwrap().message.0, second);
        }
        assert_eq!(db.read_history_full("s").unwrap(), raw);
        assert!(db.deliver_shell_notices().unwrap().is_empty());
        drop(db);
        let db = Db::open(root.path()).unwrap();
        let redo = db
            .change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert!(!redo.can_redo && redo.reverted.is_none());
        assert_eq!(db.conversation_history_full("s").unwrap(), raw);
        assert_eq!(
            db.user_shell_result("s", &oc_core::session::MessageId(notice.message_id))
                .unwrap()
                .unwrap(),
            projected
        );
        assert_eq!(db.shell_job_phase("s", "owned-shell").unwrap(), "terminal");
        assert!(db.deliver_shell_notices().unwrap().is_empty());
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
fn direct_user_shell_admission_after_undo_or_revert_atomically_opens_the_new_branch() {
    for revert in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let first = db
            .create_bound_session_and_accept_turn_with_reminder(
                "s",
                "/project",
                "one",
                "one",
                "one",
                None,
                &model("fixture"),
                None,
                None,
            )
            .unwrap()
            .user_message;
        db.commit_turn("one", "completed", None, Some("answer one"))
            .unwrap();
        let second = turn(&db, "two", "two", "fixture");
        let raw = db.read_history_full("s").unwrap();
        let staged = db
            .change_conversation(
                "s",
                if revert {
                    ConversationAction::Revert {
                        message: oc_core::session::MessageId(first),
                    }
                } else {
                    ConversationAction::Undo
                },
            )
            .unwrap();
        assert!(staged.can_redo);
        let visible = db.conversation_history_full("s").unwrap();
        let p = crate::shell::jobs::Provenance {
            version: 1,
            session: "s".into(),
            turn: String::new(),
            operation: "new-branch-shell".into(),
            location: "/project".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "fixture".into(),
            provider: "fixture".into(),
            command: "printf new-branch".into(),
            cwd: "/project".into(),
            selected_shell: "/bin/sh".into(),
        };
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_shell_branch BEFORE INSERT ON prefs WHEN NEW.key='tui.prompt_history.v1' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(db.admit_user_shell_job(&p, false, None).is_err());
        assert_eq!(db.conversation_history_full("s").unwrap(), visible);
        assert_eq!(db.read_history_full("s").unwrap(), raw);
        assert_eq!(db.reverted_conversation("s").unwrap(), staged.reverted);
        let redo: bool = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_redo WHERE session_id='s')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            redo,
            "failed admission rolls back the branch transition too"
        );
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_shell_branch")
            .unwrap();
        drop(db.admit_user_shell_job(&p, false, None).unwrap());
        assert!(db.reverted_conversation("s").unwrap().is_none());
        assert!(
            db.change_conversation("s", ConversationAction::Redo)
                .is_err()
        );
        let input = db.read_history_page_typed("s", 1, None).unwrap().remove(0);
        assert!(db.user_shell_result("s", &input.id).unwrap().unwrap().input);
        let mut outcome = crate::shell::jobs::Outcome::unknown("fixture outcome");
        outcome.state = "completed".into();
        outcome.exit = Some(0);
        outcome.stdout = "new branch output".into();
        db.finish_shell_job(&p.operation, &outcome).unwrap();
        let notice = db.deliver_shell_notices().unwrap().remove(0);
        assert!(notice.user_requested);
        let result_id = oc_core::session::MessageId(notice.message_id);
        assert!(
            db.read_history_message_typed("s", &result_id)
                .unwrap()
                .is_some()
        );
        assert!(
            db.read_history_message_typed("s", &input.id)
                .unwrap()
                .is_some()
        );
        assert!(
            db.read_history_message_typed("s", &oc_core::session::MessageId(second))
                .unwrap()
                .is_none()
        );
        let raw_after = db.read_history_full("s").unwrap();
        assert_eq!(
            &raw_after[..raw.len()],
            raw.as_slice(),
            "old RAW is unchanged"
        );
        assert_eq!(raw_after.len(), raw.len() + 2);
        let (turns, points): (i64, i64) = db.conn.lock().unwrap().query_row(
            "SELECT (SELECT COUNT(*) FROM turns WHERE session_id='s'),(SELECT COUNT(*) FROM conversation_points WHERE session_id='s')",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(
            (turns, points),
            (2, 2),
            "Shell creates no LLM turn or context point"
        );
        drop(db);
        let db = Db::open(root.path()).unwrap();
        assert!(
            db.read_history_message_typed("s", &result_id)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            db.user_shell_result("s", &result_id)
                .unwrap()
                .unwrap()
                .output
                .shell
                .unwrap()
                .stdout,
            "new branch output"
        );
        assert_eq!(db.shell_job_phase("s", &p.operation).unwrap(), "terminal");
        assert!(db.deliver_shell_notices().unwrap().is_empty());
    }
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
            "INSERT INTO dcp_tool_projection_v2(session_id,call_id,occurrence,action) VALUES ('s','call',0,'hidden')",
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

#[test]
fn r8_parent_context_selection_follows_active_branch_not_dcp_projection() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.create_session("other").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = turn(&db, "one", "one", "m");
    let ids = |session: &str| -> Vec<(String, String)> {
        let conn = db.conn.lock().unwrap();
        let mut statement = conn
            .prepare("SELECT id,role FROM messages WHERE session_id=?1 ORDER BY seq")
            .unwrap();
        statement
            .query_map([session], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let first_answer = ids("s")[1].0.clone();
    // Projection-only compression is not Revert: covered raw text stays selectable.
    db.save_compression_block(
        "s",
        "b",
        "summary",
        &first,
        &first,
        std::slice::from_ref(&first),
    )
    .unwrap();
    let reverted = turn(&db, "two", "two", "m");
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    let command = db
        .accept_turn(
            "three",
            "s",
            "expanded /review body",
            "/review",
            &model("m"),
        )
        .unwrap()
        .user_message;
    let notice = {
        let conn = db.conn.lock().unwrap();
        Db::insert_message(&conn, "s", "user", "native durable notice").unwrap()
    };
    let foreign = turn_in(&db, "other");
    let select = |wanted: &[&str]| {
        db.parent_context_messages(
            "s",
            "three",
            &wanted.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    };
    assert_eq!(
        select(&[&command, &first_answer, &first, &first]).unwrap(),
        vec![
            (first.clone(), "user".into(), "one".into()),
            (
                first_answer.clone(),
                "assistant".into(),
                "answer one".into()
            ),
            (command.clone(), "user".into(), "/review".into()),
        ],
        "chronology, dedup, DCP-covered raw text and the invocation text"
    );
    for refused in [
        reverted.as_str(),
        notice.as_str(),
        foreign.as_str(),
        "m9999",
    ] {
        assert!(select(&[refused]).is_err(), "{refused}");
    }
    assert!(
        db.parent_context_messages("s", "missing", std::slice::from_ref(&first))
            .unwrap()
            .is_err()
    );
}

fn turn_in(db: &Db, session: &str) -> String {
    db.accept_turn("foreign", session, "foreign", "foreign", &model("m"))
        .unwrap()
        .user_message
}
