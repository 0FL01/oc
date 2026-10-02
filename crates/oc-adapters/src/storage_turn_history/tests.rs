use super::*;
use crate::provider::{InputItem, InputRole};
use oc_core::queries::{AssistantSpan, ModelRef, RequestIdentity};
use serde_json::json;

fn closed(db: &Db) -> TurnLog {
    db.apply_dcp_schema().unwrap();
    db.create_bound_session("s", "/project").unwrap();
    let accepted = db
        .accept_turn(
            "t",
            "s",
            "task",
            "task",
            &ModelRef {
                provider: "p".into(),
                id: "A".into(),
                variant: None,
            },
        )
        .unwrap();
    let receipt = RequestIdentity {
        model: ModelRef {
            provider: "p".into(),
            id: "A".into(),
            variant: None,
        },
        model_label: "A label".into(),
        span: "span".into(),
        input_start: 1,
        context_limit: 100_000,
        input_limit: 90_000,
        output_limit: 1000,
        estimated_input: 12,
        dcp_min_context: 100,
        dcp_max_context: 1000,
        tool_fingerprint: "schema".into(),
        context_fingerprint: "request".into(),
    };
    let mut log = TurnLog::new("t", "A", "p");
    log.user_message = Some(accepted.user_message);
    log.requests.push(receipt.clone());
    log.spans.push(AssistantSpan {
        request: Some(receipt),
        id: "span".into(),
        step: 1,
        status: "completed".into(),
        started: 10,
        completed: Some(11),
        retry: None,
        error: None,
        finish: Some("tool_calls".into()),
    });
    log.input = vec![
        InputItem::message(InputRole::User, "task"),
        InputItem::ProviderOutput(
            json!({"type":"reasoning","id":"reason","encrypted_content":"opaque","summary":[{"text":"public fact"}]}),
        ),
        InputItem::ProviderOutput(
            json!({"type":"function_call","call_id":"reused","id":"provider-call","name":"bash","arguments":"{}"}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "reused".into(),
            output: "known committed effect".into(),
        },
    ];
    log.opaque.push(json!({"encrypted_content":"opaque"}));
    log.instruction_references
        .push(crate::instructions::Reference { event: 7, index: 1 });
    log.shell_notice_messages.push("notice-5".into());
    log.call_occurrences.insert(2, 0);
    log.display_parts
        .push(json!({"tool":"op","span":"span","call_input_index":2}));
    db.record_turn_tool_intent("op", "s", "t", "bash", "{}", &log.to_json().to_string())
        .unwrap();
    db.record_tool_outcome("op", "completed", Some("known committed effect"))
        .unwrap();
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    log
}

fn working(log: &TurnLog) -> serde_json::Value {
    log.current_working_checkpoint(
        "## Work State\nknown committed effect",
        log.closed_counts(),
        |_| false,
    )
    .unwrap()
}

#[test]
fn raw_page_integrity_rejects_corrupted_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let mut log = closed(&db);
    log.shell_notice_messages.clear();
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    let (segment, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 5)
        .unwrap();
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER immutable_turn_raw_update")
        .unwrap();
    for corrupt in ["{}", "not JSON"] {
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE turn_raw_segments SET payload=?1 WHERE turn_id='t'",
                [corrupt],
            )
            .unwrap();
        assert!(
            db.raw_turn_segment("t", 1, 100_000).is_err(),
            "unchecked raw page: {corrupt}"
        );
    }
    for (pointer, corrupt) in [
        ("/base/ends/0", json!(1)),
        ("/end/ends/0", json!(5)),
        ("/journal/turn_id", json!("foreign")),
        ("/journal/input", json!([])),
        ("/journal/requests/0/input_start", json!(99)),
        ("/journal/display_parts/0/call_input_index", json!(99)),
        ("/end/version", json!(9)),
    ] {
        let mut altered = segment.clone();
        *altered.pointer_mut(pointer).unwrap() = corrupt;
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE turn_raw_segments SET payload=?1 WHERE turn_id='t'",
                [altered.to_string()],
            )
            .unwrap();
        assert!(
            db.raw_turn_segment("t", 1, 100_000).is_err(),
            "accepted {pointer}"
        );
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET payload=?1 WHERE turn_id='t'",
            [segment.to_string()],
        )
        .unwrap();
    assert_eq!(
        db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap(),
        segment.to_string()
    );
    let reads = db.history_read_counters();
    assert!(db.raw_turn_segment("t", 1, 1).is_err());
    assert_eq!(
        db.history_read_counters(),
        reads,
        "over-budget page transferred content"
    );
    // A descriptor can be well-formed while its indexed endpoint is not.
    db.conn
        .lock()
        .unwrap()
        .execute("UPDATE turn_raw_segments SET input_end=input_end+1", [])
        .unwrap();
    assert!(db.raw_turn_segment("t", 1, 100_000).is_err());
    db.conn
        .lock()
        .unwrap()
        .execute("UPDATE turn_raw_segments SET input_end=input_end-1", [])
        .unwrap();
    let mut tail = hot;
    let mut span = log.spans[0].clone();
    span.id = "next".into();
    span.request.as_mut().unwrap().span = "next".into();
    span.request.as_mut().unwrap().input_start = 4;
    tail.requests.push(span.request.clone().unwrap());
    tail.spans.push(span);
    tail.input.push(InputItem::ProviderOutput(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"next"}]})));
    db.checkpoint_turn("t", &tail.to_json().to_string())
        .unwrap();
    let (second, replacement) = tail
        .prepare_closed_segment(tail.closed_counts(), working(&tail), 5)
        .unwrap();
    db.commit_closed_turn_segment(&tail, &second, &replacement, None)
        .unwrap();
    let mut bad_descriptor = second["end"].clone();
    bad_descriptor["ends"][0] = json!(0);
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET descriptor=?1,input_end=0 WHERE ordinal=2",
            [bad_descriptor.to_string()],
        )
        .unwrap();
    assert!(
        db.raw_turn_segment("t", 1, 100_000).is_err(),
        "bad successor accepted"
    );
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET descriptor=?1,input_end=5 WHERE ordinal=2",
            [second["end"].to_string()],
        )
        .unwrap();
    let mut bad_base = second.clone();
    bad_base["base"]["ends"][1] = json!(0);
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET payload=?1 WHERE ordinal=2",
            [bad_base.to_string()],
        )
        .unwrap();
    assert!(
        db.raw_turn_segment("t", 2, 100_000).is_err(),
        "bad predecessor link accepted"
    );
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET base_descriptor=?1 WHERE ordinal=2",
            [bad_base["base"].to_string()],
        )
        .unwrap();
    assert!(
        db.raw_turn_segment("t", 1, 100_000).is_err(),
        "successor's corrupt base accepted"
    );
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET payload=?1,base_descriptor=?2 WHERE ordinal=2",
            params![second.to_string(), second["base"].to_string()],
        )
        .unwrap();
    assert_eq!(
        db.raw_turn_segment("t", 2, 100_000).unwrap().unwrap(),
        second.to_string()
    );
    db.commit_turn(
        "t",
        "completed",
        Some(&replacement.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    let boundary = db
        .accept_turn("later", "s", "later", "later", &log.requests[0].model)
        .unwrap()
        .user_message;
    assert!(
        db.fork_session("s", &boundary, "/project", "p", "{}")
            .is_ok()
    );
    // Ordinary recovery still reads bounded HOT + LAST scalar only.
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turn_raw_segments SET payload='{}' WHERE ordinal=1",
            [],
        )
        .unwrap();
    assert!(db.turn_result("t").is_ok());
    assert!(
        db.fork_session("s", &boundary, "/project", "p", "{}")
            .is_err(),
        "fork copied corrupt raw page"
    );
    db.conn.lock().unwrap().execute_batch("DROP TRIGGER immutable_turn_raw_delete; DELETE FROM turn_raw_segments WHERE ordinal=2 AND turn_id='t';").unwrap();
    assert!(
        db.raw_turn_segment("t", 2, 100_000).is_err(),
        "missing committed page reported absent"
    );
}

#[test]
fn segmented_family_explicit_delete_preserves_fork_and_rolls_back_faults() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let mut log = closed(&db);
    log.shell_notice_messages.clear();
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    let (segment, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 0)
        .unwrap();
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    db.commit_turn(
        "t",
        "completed",
        Some(&hot.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    let model = log.requests[0].model.clone();
    let boundary = db
        .accept_turn("later", "s", "later", "later", &model)
        .unwrap()
        .user_message;
    let fork = db
        .fork_session("s", &boundary, "/project", "p", "{}")
        .unwrap();
    db.commit_turn("later", "completed", Some("{}"), None)
        .unwrap();
    let fork_turn: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT id FROM turns WHERE session_id=?1",
            [&fork.session.0],
            |r| r.get(0),
        )
        .unwrap();
    let fork_raw = db
        .raw_turn_segment(&fork_turn, 1, 100_000)
        .unwrap()
        .unwrap();
    db.create_bound_session("sibling", "/project").unwrap();
    db.create_child_session("s", "child", None, None, None)
        .unwrap();
    let accepted = db
        .accept_turn("child-turn", "child", "task", "task", &model)
        .unwrap();
    let mut child = log.clone();
    child.turn_id = "child-turn".into();
    child.user_message = Some(accepted.user_message);
    child.display_parts[0]["tool"] = json!("child-op");
    db.record_turn_tool_intent(
        "child-op",
        "child",
        "child-turn",
        "bash",
        "{}",
        &child.to_json().to_string(),
    )
    .unwrap();
    db.record_tool_outcome("child-op", "completed", Some("known committed effect"))
        .unwrap();
    db.checkpoint_turn("child-turn", &child.to_json().to_string())
        .unwrap();
    let (child_segment, child_hot) = child
        .prepare_closed_segment(child.closed_counts(), working(&child), 0)
        .unwrap();
    db.commit_closed_turn_segment(&child, &child_segment, &child_hot, None)
        .unwrap();
    assert!(
        db.delete_root_family("s", "/project").is_err(),
        "active child deleted"
    );
    db.commit_turn(
        "child-turn",
        "completed",
        Some(&child_hot.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    assert!(db.delete_root_family("child", "/project").is_err());
    assert!(db.delete_root_family("s", "/wrong").is_err());
    for sql in [
        "DELETE FROM turn_raw_segments WHERE turn_id='t'",
        "UPDATE turn_raw_segments SET payload='{}' WHERE turn_id='t'",
    ] {
        assert!(db.conn.lock().unwrap().execute(sql, []).is_err());
    }
    db.conn.lock().unwrap().execute_batch("CREATE TABLE deletion_fault(id TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER deletion_fault AFTER DELETE ON sessions WHEN OLD.id='s' BEGIN INSERT INTO deletion_fault VALUES('missing'); END;").unwrap();
    assert!(
        db.delete_root_family("s", "/project").is_err(),
        "commit fault ignored"
    );
    assert_eq!(
        db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap(),
        segment.to_string()
    );
    assert_eq!(
        db.raw_turn_segment("child-turn", 1, 100_000)
            .unwrap()
            .unwrap(),
        child_segment.to_string()
    );
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER deletion_fault")
        .unwrap();
    db.delete_root_family("s", "/project").unwrap();
    assert!(db.raw_turn_segment("t", 1, 100_000).unwrap().is_none());
    assert!(
        db.raw_turn_segment("child-turn", 1, 100_000)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.raw_turn_segment(&fork_turn, 1, 100_000)
            .unwrap()
            .unwrap(),
        fork_raw
    );
    assert!(db.session_meta(&fork.session.0).is_ok());
    assert!(db.session_meta("sibling").is_ok());
    assert!(db.session_meta("s").is_err());
    assert!(db.session_meta("child").is_err());
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT output FROM tool_operations WHERE turn_id=?1",
                [&fork_turn],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "known committed effect"
    );
}

#[test]
fn atomic_raw_hot_faults_roundtrip_and_reopen_preserve_original_references() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let log = closed(&db);
    let original = log.to_json();
    let (segment, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 5)
        .unwrap();
    assert_eq!(TurnLog::from_json(&segment["journal"]).unwrap(), log);
    // Faults before append and after append/before HOT update both roll back.
    db.conn.lock().unwrap().execute_batch("CREATE TABLE fault_commit(id TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED);").unwrap();
    for trigger in [
        "CREATE TRIGGER fault BEFORE INSERT ON turn_raw_segments BEGIN SELECT RAISE(ABORT,'before append'); END;",
        "CREATE TRIGGER fault BEFORE UPDATE OF result ON turns BEGIN SELECT RAISE(ABORT,'after append'); END;",
        "CREATE TRIGGER fault AFTER UPDATE OF result ON turns BEGIN INSERT INTO fault_commit VALUES('missing-session'); END;",
    ] {
        db.conn.lock().unwrap().execute_batch(trigger).unwrap();
        assert!(
            db.commit_closed_turn_segment(&log, &segment, &hot, None)
                .is_err()
        );
        assert_eq!(
            db.turn_result("t").unwrap().1.unwrap(),
            original.to_string()
        );
        assert!(db.raw_turn_segment("t", 1, 100_000).unwrap().is_none());
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fault")
            .unwrap();
    }
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    let raw = db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw).unwrap()["journal"],
        original
    );
    assert!(db.raw_turn_segment("t", 1, 1).is_err());
    assert!(
        db.commit_closed_turn_segment(&log, &segment, &hot, None)
            .is_err()
    );
    assert!(
        db.conn
            .lock()
            .unwrap()
            .execute("UPDATE turn_raw_segments SET payload='{}'", [])
            .is_err()
    );
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    let restored = TurnLog::from_json(
        &serde_json::from_str(&db.turn_result("t").unwrap().1.unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored, hot);
    assert_eq!(restored.original_input_index(0), 4);
    assert_eq!(db.next_call_occurrence("t", "reused").unwrap(), 1);
    assert_eq!(db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap(), raw);
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT state FROM tool_operations WHERE id='op'", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "completed"
    );
}

#[test]
fn rich_second_segment_keeps_original_receipt_and_bounded_selected_model_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let log = closed(&db);
    let (first, mut hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 5)
        .unwrap();
    db.commit_closed_turn_segment(&log, &first, &hot, None)
        .unwrap();
    let facts = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../../fixtures/mcp12-media.json"
    ))
    .unwrap()["result"]
        .clone();
    let output =
        crate::mcp_result::project_rich(serde_json::from_value(facts).unwrap(), &[], &[]).unwrap();
    let mut span = log.spans[0].clone();
    span.id = "second-span".into();
    span.request.as_mut().unwrap().input_start = 4;
    span.request.as_mut().unwrap().span = span.id.clone();
    hot.requests.push(span.request.clone().unwrap());
    hot.spans.push(span);
    hot.input.push(InputItem::ProviderOutput(json!({"type":"function_call","id":"second-call","call_id":"reused","name":"media__mixed","arguments":"{}"})));
    hot.call_occurrences
        .insert(0, db.next_call_occurrence("t", "reused").unwrap());
    hot.display_parts
        .push(json!({"tool":"media-op","span":"second-span","call_input_index":0}));
    db.record_turn_tool_intent(
        "media-op",
        "s",
        "t",
        "media__mixed",
        "{}",
        &hot.to_json().to_string(),
    )
    .unwrap();
    hot.input.push(InputItem::McpFunctionCallOutput {
        call_id: "reused".into(),
        output,
    });
    db.record_tool_outcome("media-op", "completed", Some("native output settled"))
        .unwrap();
    db.checkpoint_turn("t", &hot.to_json().to_string()).unwrap();
    let selected = hot
        .current_working_checkpoint("## Work State\nmedia fact", hot.closed_counts(), |_| true)
        .unwrap();
    let (second, selected_hot) = hot
        .prepare_closed_segment(hot.closed_counts(), selected, 5)
        .unwrap();
    db.commit_closed_turn_segment(&hot, &second, &selected_hot, None)
        .unwrap();
    assert_eq!(second["base"]["ends"][0], 4);
    assert_eq!(second["journal"]["requests"][0]["input_start"], 4);
    assert_eq!(second["journal"]["native_mcp_results"][0]["input_index"], 1);
    assert_eq!(TurnLog::from_json(&second["journal"]).unwrap(), {
        let mut original = hot.clone();
        original.raw_prefix = None;
        original.working = None;
        original
    });
    assert_eq!(db.next_call_occurrence("t", "reused").unwrap(), 2);
    assert!(matches!(
        selected_hot.input_for("B", "p").last(),
        Some(InputItem::McpFunctionCallOutput { .. })
    ));
    assert_eq!(selected_hot.call_occurrences.len(), 0);
    let selected = TurnLog::from_json(selected_hot.working.as_ref().unwrap()).unwrap();
    assert_eq!(selected.call_occurrences.get(&2), Some(&1));
    assert_eq!(selected.input_origins[2..], [Some(4), Some(5)]);
    assert_eq!(selected.requests[0].input_start, 4);
    assert_eq!(
        db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap(),
        first.to_string()
    );
}

#[test]
fn unknown_open_and_corrupt_boundaries_fail_without_eviction() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let mut log = closed(&db);
    log.spans[0].completed = None;
    assert!(
        log.prepare_closed_segment(log.closed_counts(), working(&log), 5)
            .is_err()
    );
    log.spans[0].completed = Some(11);
    let (segment, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 5)
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE tool_operations SET state='unknown' WHERE id='op'",
            [],
        )
        .unwrap();
    assert!(
        db.commit_closed_turn_segment(&log, &segment, &hot, None)
            .is_err()
    );
    assert_eq!(
        db.turn_result("t").unwrap().1.unwrap(),
        log.to_json().to_string()
    );
    let mut corrupt = hot.to_json();
    corrupt["raw_prefix"]["version"] = 9.into();
    assert!(TurnLog::from_json(&corrupt).is_err());
    db.checkpoint_turn("t", &hot.to_json().to_string()).unwrap();
    assert!(db.turn_result("t").is_err());
}

#[test]
fn explicit_prefix_fork_rebases_raw_and_hot_without_reading_oversized_future() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let mut log = closed(&db);
    log.shell_notice_messages.clear();
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    let (segment, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 0)
        .unwrap();
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    db.commit_turn(
        "t",
        "completed",
        Some(&hot.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    let model = ModelRef {
        provider: "p".into(),
        id: "A".into(),
        variant: None,
    };
    let boundary = db
        .accept_turn("later", "s", "later", "later", &model)
        .unwrap()
        .user_message;
    // Actual requested prefix is small; its future is deliberately too large.
    db.checkpoint_turn("later",&json!({"user_message":boundary,"future":"X".repeat(crate::runtime::ACTIVE_CONTEXT_BYTES_CAP+1)}).to_string()).unwrap();
    let fork = db
        .fork_session("s", &boundary, "/project", "p", "{}")
        .unwrap();
    let copied: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT id FROM turns WHERE session_id=?1",
            [&fork.session.0],
            |r| r.get(0),
        )
        .unwrap();
    let raw = db.raw_turn_segment(&copied, 1, 100_000).unwrap().unwrap();
    let raw: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(raw["journal"]["turn_id"], copied);
    assert_ne!(
        raw["journal"]["user_message"],
        segment["journal"]["user_message"]
    );
    assert_ne!(raw["journal"]["display_parts"][0]["tool"], "op");
    assert_eq!(raw["journal"]["input"], segment["journal"]["input"]);
    assert_eq!(db.next_call_occurrence(&copied, "reused").unwrap(), 1);
    assert_eq!(
        db.raw_turn_segment("t", 1, 100_000).unwrap().unwrap(),
        segment.to_string()
    );
    let copied_hot: serde_json::Value =
        serde_json::from_str(&db.turn_result(&copied).unwrap().1.unwrap()).unwrap();
    assert_eq!(copied_hot["working"]["turn_id"], copied);
    db.change_conversation(&fork.session.0, oc_core::queries::ConversationAction::Undo)
        .unwrap();
    assert_eq!(db.history_len(&fork.session.0).unwrap(), 0);
    assert_eq!(
        db.raw_turn_segment(&copied, 1, 100_000).unwrap().unwrap(),
        raw.to_string()
    );
    db.change_conversation(&fork.session.0, oc_core::queries::ConversationAction::Redo)
        .unwrap();
    assert!(db.history_len(&fork.session.0).unwrap() > 0);
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    assert!(db.turn_result(&copied).is_ok());
}

#[test]
fn oversized_legacy_and_inconsistent_wire_descriptor_refuse_before_archive_reads() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let log = closed(&db);
    db.checkpoint_turn(
        "t",
        &"X".repeat(crate::runtime::ACTIVE_CONTEXT_BYTES_CAP + 1),
    )
    .unwrap();
    assert!(db.turn_result("t").is_err());
    assert_eq!(db.history_read_counters(), [0; 6]);
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    assert_eq!(
        db.turn_result("t").unwrap().1.unwrap(),
        log.to_json().to_string()
    );
    let (_, hot) = log
        .prepare_closed_segment(log.closed_counts(), working(&log), 5)
        .unwrap();
    db.checkpoint_turn("t", &hot.to_json().to_string()).unwrap();
    assert!(
        db.presentation_wire_logs(
            "s",
            0,
            &[(log.user_message.unwrap(), "user".into(), "task".into())],
            &[]
        )
        .is_err()
    );
    assert_eq!(db.history_read_counters()[2..4], [0; 2]);
}

#[test]
fn latest_windows_do_not_stop_execution_and_each_part_keeps_its_actual_span() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let initial = closed(&db);
    let (segment, mut hot) = initial
        .prepare_closed_segment(initial.closed_counts(), working(&initial), 5)
        .unwrap();
    db.commit_closed_turn_segment(&initial, &segment, &hot, None)
        .unwrap();
    for n in 1..=300 {
        let span_id = format!("span-{n}");
        let mut receipt = initial.requests[0].clone();
        receipt.span = span_id.clone();
        receipt.input_start = hot.original_input_index(0);
        receipt.model_label = format!("actual-{n}");
        let mut span = initial.spans[0].clone();
        span.id = span_id.clone();
        span.request = Some(receipt.clone());
        hot.requests.push(receipt);
        hot.spans.push(span);
        hot.input.push(InputItem::ProviderOutput(
            json!({"type":"function_call","call_id":"reused","name":"bash","arguments":"{}"}),
        ));
        hot.call_occurrences
            .insert(0, db.next_call_occurrence("t", "reused").unwrap());
        let op = format!("op-{n}");
        hot.display_parts
            .push(json!({"tool":op,"span":span_id,"call_input_index":0}));
        db.record_turn_tool_intent(&op, "s", "t", "bash", "{}", &hot.to_json().to_string())
            .unwrap();
        hot.input.push(InputItem::FunctionCallOutput {
            call_id: "reused".into(),
            output: "known".into(),
        });
        db.record_tool_outcome(&op, "completed", Some("known"))
            .unwrap();
        db.checkpoint_turn("t", &hot.to_json().to_string()).unwrap();
        let selected = hot
            .current_working_checkpoint("## Work State\nknown", hot.closed_counts(), |_| false)
            .unwrap();
        let (segment, replacement) = hot
            .prepare_closed_segment(hot.closed_counts(), selected, 5)
            .unwrap();
        db.commit_closed_turn_segment(&hot, &segment, &replacement, None)
            .unwrap();
        hot = replacement;
    }
    let view = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(view.spans.len(), 192);
    assert_eq!(view.spans.first().unwrap().id, "span-109");
    assert_eq!(view.parts.len(), 240);
    assert_eq!(view.part_states.first().unwrap().sequence, 61);
    assert_eq!(
        view.part_states.first().unwrap().model_label.as_deref(),
        Some("actual-61")
    );
    assert_eq!(
        view.part_states.last().unwrap().model_label.as_deref(),
        Some("actual-300")
    );
    assert_eq!(view.omitted_parts, 61);
    assert_eq!(hot.raw_prefix.as_ref().unwrap().ordinal, 301);
    assert!(hot.to_json().to_string().len() < 4096);
    assert_eq!(db.next_call_occurrence("t", "reused").unwrap(), 301);
}
