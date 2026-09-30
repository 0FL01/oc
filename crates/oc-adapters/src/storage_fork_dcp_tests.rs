use super::*;
use crate::storage::compaction::dcp_lifecycle_fixture as fixture;
use oc_core::queries::ConversationAction::{Redo, Undo};

#[test]
fn prov09_fork_and_reopen_preserve_canonical_done_reasoning_and_binding() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_bound_session("source", "/project").unwrap();
    fixture::complete(&db, "source", "first", true);
    // Canonical post-parser journal, including one fulfilled call. The parser
    // and real runtime tests separately prove how these done bytes are selected.
    let reasoning = serde_json::json!({"type":"reasoning","id":"reasoning","status":"completed","summary":[],"encrypted_content":"DONE-CANARY"});
    let mut log: serde_json::Value =
        serde_json::from_str(&db.turn_result("first").unwrap().1.unwrap()).unwrap();
    log["input"]
        .as_array_mut()
        .unwrap()
        .insert(1, reasoning.clone());
    for part in log["display_parts"].as_array_mut().unwrap() {
        if let Some(index) = part["message"].as_u64() {
            part["message"] = (index + 1).into();
        }
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE turns SET result=?1 WHERE id='first'",
            [log.to_string()],
        )
        .unwrap();
    let boundary = fixture::complete(&db, "source", "boundary", false);
    let fork = db
        .fork_session("source", &boundary, "/project", "fixture", "{}")
        .unwrap()
        .session
        .0;
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    let logs = db.wire_logs_for_window(&fork, 0, 8).unwrap();
    assert_eq!(logs.len(), 1);
    let copied: serde_json::Value = serde_json::from_str(&logs[0].0).unwrap();
    assert_eq!(copied["provider"], "fixture");
    assert_eq!(copied["model"], "m");
    let input = copied["input"].as_array().unwrap();
    assert_eq!(
        input
            .iter()
            .filter(|item| item["id"] == "reasoning")
            .collect::<Vec<_>>(),
        vec![&reasoning]
    );
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call" && item["call_id"] == "reused")
            .count(),
        1
    );
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call_output" && item["call_id"] == "reused")
            .count(),
        1
    );
    assert_eq!(db.list_tool_ops(&fork).unwrap().len(), 1);
    assert_eq!(db.list_tool_ops("source").unwrap().len(), 1);
    assert!(!copied["display_parts"].to_string().contains("CANARY"));
}

#[test]
fn tool13_notice_references_follow_visible_prefix_nested_fork_and_dcp_projection() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_bound_session("source", "/project").unwrap();
    db.apply_dcp_schema().unwrap();
    fixture::complete(&db, "source", "first", false);
    let visible = db
        .append_message("source", "user", "retained native notice")
        .unwrap();
    let hidden = db
        .append_message("source", "user", "excluded native notice")
        .unwrap();
    let boundary = fixture::complete(&db, "source", "boundary", false);
    let outside = db
        .append_message("source", "user", "outside prefix notice")
        .unwrap();
    let original = db.turn_result("first").unwrap().1.unwrap();
    let mut log: serde_json::Value = serde_json::from_str(&original).unwrap();
    log["shell_notice_messages"] = serde_json::json!([visible, hidden, outside]);
    {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "UPDATE turns SET result=?1 WHERE id='first'",
            [log.to_string()],
        )
        .unwrap();
        conn.execute("INSERT INTO conversation_exclusions SELECT 'source',seq-1,seq FROM messages WHERE id=?1",[&hidden]).unwrap();
    }
    let fork = db
        .fork_session("source", &boundary, "/project", "fixture", "{}")
        .unwrap()
        .session
        .0;
    let copied = db.read_history_full(&fork).unwrap();
    let copied_notice = &copied
        .iter()
        .find(|r| r.2 == "retained native notice")
        .unwrap()
        .0;
    let copied_first = &copied
        .iter()
        .find(|r| r.0 != *copied_notice && r.1 == "user")
        .unwrap()
        .0;
    let refs: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT json_extract(result,'$.shell_notice_messages') FROM turns WHERE session_id=?1",
            [&fork],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&refs).unwrap(),
        std::slice::from_ref(copied_notice)
    );
    assert_ne!(copied_notice, &visible);
    let next = fixture::complete(&db, &fork, "fork-boundary", false);
    let nested = db
        .fork_session(&fork, &next, "/project", "fixture", "{}")
        .unwrap()
        .session
        .0;
    let nested_rows = db.read_history_full(&nested).unwrap();
    let nested_notice = &nested_rows
        .iter()
        .find(|r| r.2 == "retained native notice")
        .unwrap()
        .0;
    let refs: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT json_extract(result,'$.shell_notice_messages') FROM turns WHERE session_id=?1",
            [&nested],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&refs).unwrap(),
        std::slice::from_ref(nested_notice)
    );
    assert_ne!(nested_notice, copied_notice);
    // Cover the copied anchor/notice in the existing DCP projection. That
    // owner must remove ordinary captured-message identities with their input.
    let block = db
        .save_compression_block(
            &fork,
            "compressed",
            "summary",
            copied_first,
            copied_notice,
            &[copied_first.clone(), copied_notice.clone()],
        )
        .unwrap();
    let blocks = db.active_compression_graph(&fork, 0).unwrap();
    assert_eq!(blocks[0].id, block);
    let projected = db.active_history(&fork, 0, 1_048_576).unwrap();
    let wire = db
        .presentation_wire_logs(&fork, 0, &projected.rows, &blocks)
        .unwrap()
        .unwrap();
    assert!(
        wire.iter().all(|raw| !raw.contains(copied_notice)),
        "compressed input must not retain represented raw notice identities"
    );
    assert_eq!(db.turn_result("first").unwrap().1.unwrap(), log.to_string());
    assert_eq!(db.list_sessions().unwrap().len(), 3);
}

#[test]
fn standalone_new_turn_fork_undo_redo_rebases_scoped_metadata_without_source_collisions() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_bound_session("source", "/project").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = fixture::complete(&db, "source", "source-tip", true);
    let prefix = db.read_history_full("source").unwrap();
    let coverage = serde_json::json!(["source-tip", "reused", 0]).to_string();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO dcp_coverage VALUES('source','call',?1)",
            [&coverage],
        )
        .unwrap();
    let run = fixture::standalone(
        &db,
        "source",
        &first,
        &first,
        "standalone fork summary",
        Some("source-manual"),
    );
    let boundary = fixture::complete(&db, "source", "next", false);
    let original = db.read_history_full("source").unwrap();
    let accounting = db.dcp_accounting("source").unwrap();
    let fork = db
        .fork_session("source", &boundary, "/project", "fixture", "{}")
        .unwrap();
    let root = fork.session.0;
    let copied = db.read_history_full(&root).unwrap();
    assert_eq!(copied.len(), prefix.len());
    let imported = db
        .list_tool_ops(&root)
        .unwrap()
        .into_iter()
        .filter_map(|o| o.dcp)
        .collect::<Vec<_>>();
    assert_eq!(imported.len(), 1);
    let imported = imported.into_iter().next().unwrap();
    assert_ne!(imported.operation_id, run.operation_id);
    assert_ne!(imported.block_ids, run.block_ids);
    assert_eq!(imported.session, root);
    assert_eq!(imported.bar, run.bar);
    assert_eq!(imported.cumulative, run.cumulative);
    assert_eq!(db.dcp_accounting(&root).unwrap(), accounting);
    let page = db
        .dcp_summary_page(&root, &imported.operation_id, 0, 0, 8192)
        .unwrap()
        .unwrap();
    assert_eq!(page.text, "standalone fork summary");
    assert!(
        db.dcp_summary_page(&root, &run.operation_id, 0, 0, 8192)
            .unwrap()
            .is_none()
    );
    let conn = db.conn.lock().unwrap();
    let copied_turn: String = conn
        .query_row(
            "SELECT turn_id FROM turn_acceptances WHERE session_id=?1",
            [&root],
            |r| r.get(0),
        )
        .unwrap();
    let copied_coverage: String = conn
        .query_row(
            "SELECT identity FROM dcp_coverage WHERE session_id=?1 AND kind='call'",
            [&root],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        copied_coverage,
        serde_json::json!([copied_turn, "reused", 0]).to_string()
    );
    let state: (String, Option<String>) = conn
        .query_row(
            "SELECT state,turn_id FROM tool_operations WHERE id=?1 AND session_id=?2",
            params![imported.operation_id, root],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, ("completed".into(), None));
    drop(conn);
    db.change_conversation(&root, Undo).unwrap();
    assert!(db.dcp_accounting(&root).unwrap().is_none());
    assert!(db.dcp_run(&root, &imported.operation_id).unwrap().is_none());
    assert!(db.conversation_history_full(&root).unwrap().is_empty());
    assert_eq!(
        db.dcp_run("source", &run.operation_id).unwrap(),
        Some(run.clone())
    );
    assert_eq!(db.read_history_full("source").unwrap(), original);
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    db.change_conversation(&root, Redo).unwrap();
    assert_eq!(
        db.dcp_run(&root, &imported.operation_id).unwrap(),
        Some(imported.clone())
    );
    assert_eq!(db.dcp_accounting(&root).unwrap(), accounting);
    assert_eq!(db.conversation_history_full(&root).unwrap(), copied);
    assert_eq!(db.read_history_full("source").unwrap(), original);
    // Recompress inherited summary with no new raw coverage; copied immutable
    // tool identity must not collide with the source or create another item.
    let recompressed = fixture::standalone(
        &db,
        &root,
        &imported.block_ids[0],
        &imported.block_ids[0],
        "short",
        None,
    );
    assert_eq!((recompressed.new_messages, recompressed.new_tools), (0, 0));
    assert!(recompressed.ordinal > imported.ordinal);
    let new_boundary = fixture::complete(&db, &root, "fork-next", false);
    let recursive = db
        .fork_session(&root, &new_boundary, "/project", "fixture", "{}")
        .unwrap()
        .session
        .0;
    assert_eq!(
        db.dcp_accounting(&recursive).unwrap(),
        Some(recompressed.cumulative.clone())
    );
    db.change_conversation(&recursive, Undo).unwrap();
    db.change_conversation(&recursive, Redo).unwrap();
    let recursive_runs = db
        .list_tool_ops(&recursive)
        .unwrap()
        .into_iter()
        .filter_map(|o| o.dcp)
        .collect::<Vec<_>>();
    assert_eq!(recursive_runs.len(), 2);
    assert!(recursive_runs.iter().all(|r| r.session == recursive
        && r.operation_id != run.operation_id
        && r.operation_id != imported.operation_id
        && r.operation_id != recompressed.operation_id));
    assert_eq!(db.dcp_run("source", &run.operation_id).unwrap(), Some(run));
}

#[test]
fn unrelated_or_future_standalone_records_do_not_bypass_prefix_refusal() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_bound_session("source", "/project").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = fixture::complete(&db, "source", "seed", false);
    fixture::standalone(
        &db,
        "source",
        &first,
        &first,
        "genuine standalone summary",
        None,
    );
    let boundary = fixture::complete(&db, "source", "boundary", false);
    db.record_tool_intent("unrelated", "source", None, "read", "{}")
        .unwrap();
    db.record_tool_outcome("unrelated", "completed", Some("done"))
        .unwrap();
    let count = db.list_sessions().unwrap().len();
    let error = db
        .fork_session("source", &boundary, "/project", "fixture", "{}")
        .unwrap_err()
        .to_string();
    assert!(error.contains("unanchored tool record"), "{error}");
    assert_eq!(db.list_sessions().unwrap().len(), count);
    assert!(db.tab_adoptions("/project").unwrap().is_empty());
    // Even compress/completed alone is not evidence of a genuine prefix cut.
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE tool_operations SET name='compress' WHERE id='unrelated'",
            [],
        )
        .unwrap();
    assert!(
        db.fork_session("source", &boundary, "/project", "fixture", "{}")
            .unwrap_err()
            .to_string()
            .contains("unanchored tool record")
    );
    db.conn
        .lock()
        .unwrap()
        .execute("DELETE FROM tool_operations WHERE id='unrelated'", [])
        .unwrap();
    // A real future run is not referenced by the earlier boundary's cut.
    fixture::standalone(
        &db,
        "source",
        &boundary,
        &boundary,
        "future compression",
        Some("future-op"),
    );
    assert!(
        db.fork_session("source", &boundary, "/project", "fixture", "{}")
            .unwrap_err()
            .to_string()
            .contains("unanchored tool record")
    );
    assert_eq!(db.list_sessions().unwrap().len(), count);
    assert!(db.tab_adoptions("/project").unwrap().is_empty());
}
