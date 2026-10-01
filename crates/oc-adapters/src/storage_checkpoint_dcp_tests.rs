use super::*;
use crate::storage::compaction::dcp_lifecycle_fixture as fixture;
use oc_core::{
    compaction::{CompactionAnchor, CompactionReason},
    queries::ConversationAction,
};

fn checkpoint(db: &Db, session: &str, id: &str, boundary: &str) -> CompactionSnapshot {
    let snapshot = CompactionSnapshot {
        model: None,
        anchor: CompactionAnchor::default(),
        id: id.into(),
        session: session.into(),
        reason: CompactionReason::Manual,
        state: CompactionState::Completed,
        summary: "This is a session checkpoint, not a DCP range summary".into(),
        usage: None,
        provider_native: false,
        error: None,
    };
    db.save_compaction(&snapshot).unwrap();
    db.commit_checkpoint(&snapshot, boundary, None, &Default::default())
        .unwrap();
    snapshot
}

#[test]
fn checkpoint_panel_summary_uses_remaining_projection_and_rolls_back_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = fixture::complete(&db, "s", "first", false);
    let first_end = db.read_history_full("s").unwrap()[1].0.clone();
    let second = fixture::complete(&db, "s", "second", false);
    let second_end = db.read_history_full("s").unwrap()[3].0.clone();
    let third = fixture::complete(&db, "s", "third", false);
    let third_end = db.read_history_full("s").unwrap()[5].0.clone();
    fixture::standalone(
        &db,
        "s",
        &first,
        &first_end,
        "older compressed exchange",
        None,
    );
    fixture::standalone(
        &db,
        "s",
        &second,
        &second_end,
        "pruned compressed exchange",
        None,
    );
    let run = fixture::standalone(&db, "s", &third, &third, "remaining summary", None);
    fixture::complete(&db, "s", "tail", false);
    let archive = db.read_history_full("s").unwrap();
    let before_prune = db.dcp_accounting("s").unwrap().unwrap();
    db.save_prune_mark("s", &first_end).unwrap();
    let before = db.dcp_accounting("s").unwrap().unwrap();
    checkpoint(&db, "s", "checkpoint", &second_end);
    let panel = db.dcp_accounting("s").unwrap().unwrap();
    assert_eq!(
        panel.active_summary,
        oc_core::dcp_view::estimate_content("remaining summary")
    );
    let mut expected = before.clone();
    expected.active_summary = panel.active_summary;
    assert_eq!(panel, expected);
    assert_eq!(
        db.dcp_run("s", &run.operation_id).unwrap(),
        Some(run.clone())
    );
    assert_eq!(db.read_history_full("s").unwrap(), archive);
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_none());
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(before_prune));
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(panel.clone()));

    // Accounting and checkpoint installation cannot leave different revisions.
    let failed = CompactionSnapshot {
        id: "failed-commit".into(),
        ..checkpoint_snapshot("s")
    };
    db.save_compaction(&failed).unwrap();
    let revision: i64 = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT revision FROM conversation_revision", [], |r| {
            r.get(0)
        })
        .unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TEMP TRIGGER reject_checkpoint_accounting BEFORE UPDATE ON dcp_accounting BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(
        db.commit_checkpoint(&failed, &third_end, None, &Default::default())
            .is_err()
    );
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(panel.clone()));
    assert_eq!(
        db.session_checkpoint("s").unwrap().unwrap().0,
        db.message_seq("s", &second_end).unwrap().unwrap()
    );
    let after_revision: i64 = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT revision FROM conversation_revision", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(revision, after_revision);
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(db.dcp_accounting("s").unwrap(), Some(panel));
    assert_eq!(db.dcp_run("s", &run.operation_id).unwrap(), Some(run));
    assert_eq!(db.read_history_full("s").unwrap(), archive);
}

fn checkpoint_snapshot(session: &str) -> CompactionSnapshot {
    CompactionSnapshot {
        model: None,
        anchor: CompactionAnchor::default(),
        id: "unused".into(),
        session: session.into(),
        reason: CompactionReason::Manual,
        state: CompactionState::Completed,
        summary: "completed checkpoint".into(),
        usage: None,
        provider_native: false,
        error: None,
    }
}

#[test]
fn standalone_compress_post_revision_restores_after_undo_redo_and_reopen() {
    for explicit_operation in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_session("s").unwrap();
        db.apply_dcp_schema().unwrap();
        let user = fixture::complete(&db, "s", "tip", false);
        let archive = db.read_history_full("s").unwrap();
        let run = fixture::standalone(
            &db,
            "s",
            &user,
            &user,
            "settled standalone summary",
            explicit_operation.then_some("manual-op"),
        );
        let accounting = db.dcp_accounting("s").unwrap();
        let blocks = db.load_compression_blocks("s").unwrap();
        let nudges = db.conversation_nudges("s").unwrap();
        let (saved, actual): (String, String) = db.conn.lock().unwrap().query_row("SELECT post_context,'revision:'||(SELECT revision FROM conversation_revision) FROM conversation_points WHERE turn_id='tip'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(
            saved, actual,
            "standalone commit must publish the complete settled tip revision"
        );
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        assert!(db.dcp_accounting("s").unwrap().is_none());
        assert!(db.dcp_run("s", &run.operation_id).unwrap().is_none());
        assert!(db.load_compression_blocks("s").unwrap().is_empty());
        assert_eq!(db.read_history_full("s").unwrap(), archive);
        drop(db);
        let db = Db::open(tmp.path()).unwrap();
        db.change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert_eq!(db.dcp_accounting("s").unwrap(), accounting);
        assert_eq!(
            db.dcp_run("s", &run.operation_id).unwrap(),
            Some(run.clone())
        );
        assert_eq!(
            db.load_compression_blocks("s").unwrap()[0].summary,
            blocks[0].summary
        );
        assert_eq!(db.conversation_nudges("s").unwrap(), nudges);
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        db.change_conversation("s", ConversationAction::Redo)
            .unwrap();
        drop(db);
        let db = Db::open(tmp.path()).unwrap();
        assert_eq!(db.dcp_run("s", &run.operation_id).unwrap(), Some(run));
        assert_eq!(db.read_history_full("s").unwrap(), archive);
    }
}

#[test]
fn publish_settled_context_does_not_invent_legacy_or_staged_posts() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    fixture::complete(&db, "s", "tip", false);
    let saved: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT post_context FROM conversation_points WHERE turn_id='tip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.append_message("s", "user", "genuine legacy tip")
        .unwrap();
    let conn = db.conn.lock().unwrap();
    Db::publish_settled_context(&conn, "s").unwrap();
    let unchanged: String = conn
        .query_row(
            "SELECT post_context FROM conversation_points WHERE turn_id='tip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(saved, unchanged);
    drop(conn);
    assert!(
        db.change_conversation("s", ConversationAction::Undo)
            .is_err()
    );
    db.create_session("staged").unwrap();
    fixture::complete(&db, "staged", "staged-tip", false);
    db.change_conversation("staged", ConversationAction::Undo)
        .unwrap();
    let conn = db.conn.lock().unwrap();
    let saved: String = conn
        .query_row(
            "SELECT post_context FROM conversation_points WHERE turn_id='staged-tip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    Db::publish_settled_context(&conn, "staged").unwrap();
    let unchanged: String = conn
        .query_row(
            "SELECT post_context FROM conversation_points WHERE turn_id='staged-tip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(saved, unchanged);
}
