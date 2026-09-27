use super::*;
use crate::tools::TurnLog;
use oc_core::queries::{ConversationAction, ModelRef, TranscriptPart};

fn accepted(db: &Db, session: &str, turn: &str, text: &str) -> TurnLog {
    let model = ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let user = db
        .accept_turn(turn, session, text, text, &model)
        .unwrap()
        .user_message;
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(user);
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        text,
    ));
    log
}

fn complete(db: &Db, session: &str, turn: &str, text: &str) -> String {
    let log = accepted(db, session, turn, text);
    let user = log.user_message.clone().unwrap();
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some("answer"),
    )
    .unwrap();
    user
}

fn compress(
    db: &Db,
    session: &str,
    turn: &str,
    start: &str,
    end: &str,
    summary: &str,
) -> DcpRunSnapshot {
    let mut log = accepted(db, session, turn, "compress now");
    let op = format!("{turn}:compress");
    log.display_parts.push(serde_json::json!({"tool":op}));
    log.input.push(crate::provider::InputItem::ProviderOutput(
        serde_json::json!({"type":"function_call","call_id":op,"name":"compress","arguments":"{}"}),
    ));
    db.record_turn_tool_intent(
        &op,
        session,
        turn,
        "compress",
        "{}",
        &log.to_json().to_string(),
    )
    .unwrap();
    log.input
        .push(crate::provider::InputItem::FunctionCallOutput {
            call_id: op.clone(),
            output: "compressed".into(),
        });
    let raw = db.conversation_history_full(session).unwrap();
    let history = raw
        .iter()
        .map(|(id, role, text)| oc_core::session::Message {
            id: oc_core::session::MessageId(id.clone()),
            role: if role == "user" {
                oc_core::session::Role::User
            } else {
                oc_core::session::Role::Assistant
            },
            text: text.clone(),
        })
        .collect::<Vec<_>>();
    let report = crate::dcp::compress_ranges_atomic(
        db,
        session,
        &history,
        &[crate::dcp::ValidatedRange {
            topic: "durable run".into(),
            start_id: start.into(),
            end_id: end.into(),
            summary: summary.into(),
        }],
        &oc_core::context_plan::ProtectedSpec::default(),
        Some(&crate::dcp::CompressionCommitMetadata {
            operation_id: &op,
            operation_state: "completed",
            operation_output: "compressed",
            turn_id: Some(turn),
            turn_log: Some(&log.to_json().to_string()),
            preference_updates: &[],
        }),
    )
    .unwrap_or_else(|error| panic!("compress {turn}: {error:?}"));
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    report.snapshot
}

#[test]
fn snapshots_restore_restart_fork_and_new_branch_without_tail_accounting() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_bound_session("s", "/project").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = complete(&db, "s", "seed", &"raw 🌍 content ".repeat(1024));
    let original = db.read_history_full("s").unwrap();
    let run1 = compress(
        &db,
        "s",
        "one",
        &first,
        &original[1].0,
        &"summary details ".repeat(16),
    );
    let run2 = compress(
        &db,
        "s",
        "two",
        &run1.block_ids[0],
        &run1.block_ids[0],
        "short",
    );
    assert_eq!(
        (
            run2.ordinal,
            run2.new_messages,
            run2.new_tools,
            run2.removed
        ),
        (2, 0, 0, 0)
    );
    assert!(run2.net_saved > 0);
    assert_eq!(run2.cumulative.gross_removed, run1.cumulative.gross_removed);
    assert_eq!(
        run2.cumulative.active_summary,
        oc_core::dcp_view::estimate_content("short")
    );
    let historical = db.turn_presentation("s", "one").unwrap().unwrap();
    assert!(
        historical
            .parts
            .iter()
            .any(|p| matches!(p,TranscriptPart::Tool(op) if op.dcp.as_ref()==Some(&run1)))
    );
    let boundary = complete(&db, "s", "tail", "future tail");
    let fork = db
        .fork_session("s", &boundary, "/project", "fixture", "{}")
        .unwrap();
    let root = fork.session.0;
    assert_eq!(
        db.dcp_accounting(&root).unwrap(),
        Some(run2.cumulative.clone())
    );
    let fork_runs = db
        .list_tool_ops(&root)
        .unwrap()
        .into_iter()
        .filter_map(|o| o.dcp)
        .collect::<Vec<_>>();
    assert_eq!(fork_runs.len(), 2);
    assert_eq!(fork_runs[0].bar, run1.bar);
    assert_eq!(fork_runs[1].bar, run2.bar);
    assert!(fork_runs.iter().all(|r| r.session == root
        && r.operation_id != run1.operation_id
        && r.block_ids != run1.block_ids));
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(
        db.dcp_accounting("s").unwrap(),
        Some(run1.cumulative.clone())
    );
    assert_eq!(db.dcp_run("s", &run2.operation_id).unwrap(), None);
    assert_eq!(
        db.dcp_accounting(&root).unwrap(),
        Some(run2.cumulative.clone())
    );
    assert_eq!(
        db.dcp_run("s", &run1.operation_id).unwrap(),
        Some(run1.clone())
    );
    let raw = db.read_history_full("s").unwrap();
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(
        db.dcp_run("s", &run2.operation_id).unwrap(),
        Some(run2.clone())
    );
    assert_eq!(
        db.dcp_accounting("s").unwrap(),
        Some(run2.cumulative.clone())
    );
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert_eq!(
        db.dcp_accounting("s").unwrap(),
        Some(run1.cumulative.clone()),
        "branch cut must restore the first run"
    );
    let branch = compress(
        &db,
        "s",
        "branch",
        &run1.block_ids[0],
        &run1.block_ids[0],
        "branch",
    );
    assert_eq!(branch.ordinal, 3, "abandoned ordinal is not reused");
    assert_eq!(
        branch.cumulative.compressions, 2,
        "abandoned tail success is not inherited"
    );
    assert_eq!(
        branch.cumulative.gross_removed,
        run1.cumulative.gross_removed
    );
    assert_eq!(db.dcp_run("s", &run2.operation_id).unwrap(), None);
    assert_eq!(
        db.dcp_accounting(&root).unwrap(),
        Some(run2.cumulative.clone())
    );
    assert_eq!(
        &db.read_history_full("s").unwrap()[..raw.len()],
        raw.as_slice()
    );
    db.change_conversation(&root, ConversationAction::Undo)
        .unwrap();
    assert_eq!(
        db.dcp_accounting(&root).unwrap(),
        Some(run1.cumulative.clone())
    );
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(branch.cumulative));
}

#[test]
fn real_summary_utf8_pages_caps_legacy_and_effective_pruning() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = db
        .append_message("s", "user", &"large original 🌍 ".repeat(4096))
        .unwrap();
    db.append_message("s", "user", "tail").unwrap();
    let raw = db.read_history_full("s").unwrap();
    let messages = raw
        .iter()
        .map(|(id, _, text)| oc_core::session::Message {
            id: oc_core::session::MessageId(id.clone()),
            role: oc_core::session::Role::User,
            text: text.clone(),
        })
        .collect::<Vec<_>>();
    let summary = "真实🌍 résumé ".repeat(700);
    let report = crate::dcp::compress_ranges_atomic(
        &db,
        "s",
        &messages,
        &[crate::dcp::ValidatedRange {
            topic: "real summary".into(),
            start_id: first.clone(),
            end_id: first.clone(),
            summary: summary.clone(),
        }],
        &oc_core::context_plan::ProtectedSpec::default(),
        None,
    )
    .unwrap();
    let mut collected = String::new();
    let mut offset = 0;
    loop {
        let page = db
            .dcp_summary_page("s", &report.snapshot.operation_id, 0, offset, usize::MAX)
            .unwrap()
            .unwrap();
        assert!(page.text.len() <= SUMMARY_PAGE_CAP);
        assert_eq!(page.total_bytes, summary.len() as i64);
        collected.push_str(&page.text);
        match page.next_offset {
            Some(next) => {
                assert!(next > offset);
                offset = next
            }
            None => break,
        }
    }
    assert_eq!(collected, summary);
    assert!(
        db.dcp_summary_page("s", &report.snapshot.operation_id, 1, 0, 10)
            .unwrap()
            .is_none()
    );
    db.create_session("other").unwrap();
    assert!(
        db.dcp_summary_page("other", &report.snapshot.operation_id, 0, 0, 10)
            .unwrap()
            .is_none()
    );
    let before = db.dcp_accounting("s").unwrap().unwrap();
    db.save_prune_mark("s", "missing").unwrap();
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(before.clone()));
    db.save_prune_mark("s", &first).unwrap();
    let pruned = db.dcp_accounting("s").unwrap().unwrap();
    assert_eq!(pruned.prunes, 1);
    assert_eq!(pruned.active_summary, 0);
    assert_eq!(
        pruned.gross_removed, before.gross_removed,
        "covered raw payload is not removed twice"
    );
    db.save_prune_mark("s", &first).unwrap();
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(pruned));
    db.record_tool_intent("legacy", "other", None, "compress", "{}")
        .unwrap();
    db.record_tool_outcome("legacy", "completed", Some("old free text"))
        .unwrap();
    assert!(db.list_tool_ops("other").unwrap()[0].dcp.is_none());
    assert!(db.dcp_accounting("other").unwrap().is_none());
    assert_eq!(oc_core::dcp_view::estimate_content("a"), 0);
    assert_eq!(
        oc_core::dcp_view::estimate_content("😀é界"),
        1,
        "JS UTF-16 length 4, not UTF-8 bytes 9"
    );
}

#[test]
fn prefix_pruning_counts_real_tool_contents_and_refuses_unbounded_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let mut log = accepted(&db, "s", "tools", "hi");
    let arguments = "界".repeat(9);
    let output = "🌍".repeat(9);
    log.input.push(crate::provider::InputItem::ProviderOutput(serde_json::json!({"type":"function_call","call_id":"call","name":"read","arguments":arguments})));
    log.input
        .push(crate::provider::InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: output.clone(),
        });
    log.input.push(crate::provider::InputItem::ProviderOutput(serde_json::json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"answer"}]})));
    db.commit_turn(
        "tools",
        "completed",
        Some(&log.to_json().to_string()),
        Some("answer"),
    )
    .unwrap();
    let history = db.read_history_full("s").unwrap();
    let expected = 1 + 2 + 5 + 2; // independently rounded UTF-16 lengths: 2,9,18,6.
    db.save_prune_mark("s", &history[1].0).unwrap();
    let accounted = db.dcp_accounting("s").unwrap().unwrap();
    assert_eq!(
        (
            accounted.gross_removed,
            accounted.net_saved,
            accounted.prunes
        ),
        (expected, expected, 1)
    );
    assert_eq!(db.read_history_full("s").unwrap(), history);
    db.save_prune_mark("s", &history[1].0).unwrap();
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(accounted));
    db.create_session("huge").unwrap();
    let mut huge = accepted(&db, "huge", "huge-turn", "tiny prompt");
    huge.input.push(crate::provider::InputItem::ProviderOutput(serde_json::json!({"type":"function_call","call_id":"big","name":"read","arguments":"x".repeat(crate::runtime::ACTIVE_CONTEXT_BYTES_CAP+1)})));
    huge.input
        .push(crate::provider::InputItem::FunctionCallOutput {
            call_id: "big".into(),
            output: "output".into(),
        });
    db.commit_turn(
        "huge-turn",
        "completed",
        Some(&huge.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    let rows = db.read_history_full("huge").unwrap();
    assert!(
        db.presentation_wire_logs("huge", 0, &rows, &[])
            .unwrap()
            .is_none()
    );
    assert!(db.save_prune_mark("huge", &rows[1].0).is_err());
    assert_eq!(db.load_prune_mark("huge").unwrap(), None);
    assert_eq!(db.dcp_accounting("huge").unwrap(), None);
}
