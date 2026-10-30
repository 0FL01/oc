use super::*;

fn db() -> (tempfile::TempDir, Db) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    db.create_session("s").unwrap();
    (tmp, db)
}

fn model() -> oc_core::queries::ModelRef {
    oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    }
}

fn turn(db: &Db) -> crate::tools::TurnLog {
    let mut log = crate::tools::TurnLog::new("t", "m", "fixture");
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        "first",
    ));
    db.accept_turn_with_selection(
        "t",
        "s",
        "first",
        "first",
        &model(),
        None,
        None,
        None,
        Some(&mut log),
    )
    .unwrap();
    log
}

fn deliver(db: &Db, log: &mut crate::tools::TurnLog) -> Vec<InboxDelivered> {
    let mut delivered = Vec::new();
    while let Some(pending) = db.inbox_next("s", false).unwrap() {
        let steer = InboxSteer {
            input_index: log.original_input_index(log.input.len()),
            pending,
        };
        log.input.push(
            crate::attachments::prepare_prompt(
                &steer.pending.payload.prompt,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap(),
        );
        delivered.extend(
            db.checkpoint_turn_with_inbox("s", log, &[], Some(&steer), None, None)
                .unwrap(),
        );
    }
    delivered
}

#[test]
fn pending_items_keep_order_change_delivery_and_withdraw() {
    let (_tmp, db) = db();
    let first = db.inbox_add("s", InboxDelivery::Queue, "first").unwrap();
    let second = db.inbox_add("s", InboxDelivery::Steer, "second").unwrap();
    assert_eq!(
        db.inbox_pending("s").unwrap(),
        [first.clone(), second.clone()]
    );
    assert!(db.inbox_has_steer("s").unwrap());
    assert!(
        db.inbox_set_delivery("s", &first.id, InboxDelivery::Steer)
            .unwrap()
    );
    assert_eq!(db.inbox_remove("s", &second.id).unwrap(), Some(second));
    assert_eq!(db.inbox_remove("s", "missing").unwrap(), None);
    assert_eq!(
        db.inbox_pending("s").unwrap(),
        [InboxItem {
            delivery: InboxDelivery::Steer,
            ..first
        }]
    );
}

#[test]
fn bounded_per_session() {
    let (_tmp, db) = db();
    for index in 0..MAX_INBOX_ITEMS {
        db.inbox_add("s", InboxDelivery::Queue, &index.to_string())
            .unwrap();
    }
    assert!(matches!(
        db.inbox_add("s", InboxDelivery::Steer, "overflow"),
        Err(StorageError::InboxFull)
    ));
    assert!(matches!(
        db.inbox_add("missing", InboxDelivery::Steer, "x"),
        Err(StorageError::SessionNotFound)
    ));
}

#[test]
fn delivered_steers_are_turn_notices_and_queue_waits() {
    let (_tmp, db) = db();
    let queued = db.inbox_add("s", InboxDelivery::Queue, "later").unwrap();
    let steer = db.inbox_add("s", InboxDelivery::Steer, "now one").unwrap();
    db.inbox_add("s", InboxDelivery::Steer, "now two").unwrap();
    let mut log = turn(&db);
    let delivered = deliver(&db, &mut log);
    assert_eq!(delivered.len(), 2);
    assert!(!db.inbox_has_steer("s").unwrap());
    assert_eq!(
        db.inbox_pending("s").unwrap(),
        std::slice::from_ref(&queued)
    );
    // The running turn reads them through the ordinary notice cursor.
    let notices = db.shell_notices_after("s", 0, 1 << 20).unwrap();
    let texts = notices
        .rows
        .iter()
        .map(|(_, role, text)| (role.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(texts, [("user", "now one"), ("user", "now two")]);
    // A delivered steer is no longer withdrawable.
    assert_eq!(db.inbox_remove("s", &steer.id).unwrap(), None);
    assert!(
        !db.inbox_set_delivery("s", &steer.id, InboxDelivery::Queue)
            .unwrap()
    );
    assert_eq!(db.inbox_next("s", true).unwrap().unwrap().item, queued);
    // Selecting is non-destructive; only a successful acceptance consumes it.
    assert_eq!(
        db.inbox_pending("s").unwrap(),
        std::slice::from_ref(&queued)
    );
    db.commit_turn(
        "t",
        "completed",
        Some(&log.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    db.accept_inbox_turn(
        "next",
        "s",
        "later",
        "later",
        &model(),
        None,
        None,
        None,
        None,
        Some(&queued),
    )
    .unwrap();
    assert!(db.inbox_next("s", true).unwrap().is_none());
}

#[test]
fn idle_boundary_prefers_a_leftover_steer_over_the_oldest_queue() {
    let (_tmp, db) = db();
    db.inbox_add("s", InboxDelivery::Queue, "queued").unwrap();
    let steer = db
        .inbox_add("s", InboxDelivery::Steer, "late steer")
        .unwrap();
    assert_eq!(db.inbox_next("s", false).unwrap().unwrap().item, steer);
    assert_eq!(db.inbox_pending("s").unwrap().len(), 2);
    db.inbox_remove("s", &steer.id).unwrap();
    // After an interrupt queued input stays parked; a settled turn takes it.
    assert!(db.inbox_next("s", false).unwrap().is_none());
    assert_eq!(
        db.inbox_next("s", true).unwrap().unwrap().item.text,
        "queued"
    );
}

#[test]
fn requeued_initial_steer_cannot_be_consumed_or_failed_by_its_old_capture() {
    let (_tmp, db) = db();
    let item = db
        .inbox_add("s", InboxDelivery::Steer, "now queued")
        .unwrap();
    db.inbox_set_delivery("s", &item.id, InboxDelivery::Queue)
        .unwrap();
    assert!(matches!(
        db.accept_inbox_turn(
            "blocked",
            "s",
            "now queued",
            "now queued",
            &model(),
            None,
            None,
            None,
            None,
            Some(&item)
        ),
        Err(StorageError::OperationNotFound)
    ));
    db.inbox_fail(
        "s",
        &item,
        &CoreError::Application("no longer eligible".into()),
    )
    .unwrap();
    assert!(db.read_history("s").unwrap().is_empty());
    let pending = db.inbox_read("s", &item.id).unwrap().unwrap().item;
    assert_eq!(pending.delivery, InboxDelivery::Queue);
    assert!(pending.error.is_none());
}

#[test]
fn family_delete_removes_pending_and_delivered_inbox_rows() {
    let (_tmp, db) = db();
    db.set_pref("tui.session_location.s", "/fixture").unwrap();
    db.inbox_add("s", InboxDelivery::Steer, "delivered")
        .unwrap();
    let mut log = turn(&db);
    deliver(&db, &mut log);
    db.commit_turn(
        "t",
        "completed",
        Some(&log.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    db.inbox_add("s", InboxDelivery::Queue, "pending").unwrap();
    db.delete_root_family("s", "/fixture").unwrap();
    assert!(matches!(
        db.inbox_pending("s"),
        Ok(items) if items.is_empty()
    ));
}

#[test]
fn failed_acceptance_and_checkpoint_leave_the_admitted_input_pending() {
    let (_tmp, db) = db();
    let item = db
        .inbox_add("s", InboxDelivery::Queue, "retained queue")
        .unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER reject_acceptance BEFORE INSERT ON turn_acceptances BEGIN SELECT RAISE(ABORT,'injected acceptance failure'); END;").unwrap();
    assert!(
        db.accept_inbox_turn(
            "bad",
            "s",
            "retained queue",
            "retained queue",
            &model(),
            None,
            None,
            None,
            None,
            Some(&item)
        )
        .is_err()
    );
    assert_eq!(db.inbox_pending("s").unwrap(), std::slice::from_ref(&item));
    assert!(db.read_history("s").unwrap().is_empty());
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_acceptance;")
        .unwrap();
    let mut log = turn(&db);
    db.inbox_set_delivery("s", &item.id, InboxDelivery::Steer)
        .unwrap();
    let steer = InboxSteer {
        input_index: log.input.len(),
        pending: db.inbox_read("s", &item.id).unwrap().unwrap(),
    };
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        "retained queue",
    ));
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER reject_checkpoint BEFORE UPDATE ON turns BEGIN SELECT RAISE(ABORT,'injected checkpoint failure'); END;").unwrap();
    assert!(
        db.checkpoint_turn_with_inbox("s", &mut log, &[], Some(&steer), None, None)
            .is_err()
    );
    assert!(db.inbox_read("s", &item.id).unwrap().is_some());
    assert_eq!(
        db.read_history("s").unwrap(),
        [("user".into(), "first".into())]
    );
    db.conn.lock().unwrap().execute_batch("DROP TRIGGER reject_checkpoint; CREATE TRIGGER reject_nudge BEFORE INSERT ON prefs WHEN NEW.key='nudge-test' BEGIN SELECT RAISE(ABORT,'injected nudge failure'); END;").unwrap();
    db.set_pref("selection-test", "captured").unwrap();
    let before = db.turn_result("t").unwrap();
    let before_log = log.to_json();
    assert!(
        db.checkpoint_turn_with_inbox(
            "s",
            &mut log,
            &[],
            Some(&steer),
            Some(("selection-test", "command", Some("captured"))),
            Some(("nudge-test", "prepared")),
        )
        .is_err()
    );
    assert_eq!(db.turn_result("t").unwrap(), before);
    assert_eq!(log.to_json(), before_log);
    assert!(db.inbox_read("s", &item.id).unwrap().is_some());
    assert_eq!(
        db.get_pref("selection-test").unwrap().as_deref(),
        Some("captured")
    );
    assert!(db.get_pref("nudge-test").unwrap().is_none());
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_nudge;")
        .unwrap();
    db.checkpoint_turn_with_inbox(
        "s",
        &mut log,
        &[],
        Some(&steer),
        Some(("selection-test", "command", Some("captured"))),
        Some(("nudge-test", "prepared")),
    )
    .unwrap();
    assert_eq!(
        db.get_pref("nudge-test").unwrap().as_deref(),
        Some("prepared")
    );
}

#[test]
fn rich_pending_reopens_and_steered_message_restores_its_own_images_and_fork_links() {
    use oc_core::prompt::{ImageMark, PromptImage};
    let (tmp, db) = db();
    db.set_pref("tui.session_location.s", "/fixture").unwrap();
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let prompt = PromptInput {
        text: "Ω [Image 1]".into(),
        images: vec![PromptImage {
            mark: ImageMark {
                start: 3,
                end: 12,
                ordinal: 1,
            },
            png: png.into_inner().into(),
        }],
    };
    let (item, _) = db
        .inbox_add_rich(
            "s",
            InboxDelivery::Steer,
            &InboxPayload {
                prompt: prompt.clone(),
                command: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(item.images, 1);
    let mut another = prompt.clone();
    another.text = "Ж [Image 1]".into();
    db.inbox_add_rich(
        "s",
        InboxDelivery::Steer,
        &InboxPayload {
            prompt: another.clone(),
            command: None,
        },
        None,
    )
    .unwrap();
    assert!(
        !serde_json::to_string(&db.inbox_pending("s").unwrap())
            .unwrap()
            .contains("png")
    );
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(
        db.inbox_read("s", &item.id)
            .unwrap()
            .unwrap()
            .payload
            .original(),
        prompt
    );
    let mut log = turn(&db);
    let delivered = deliver(&db, &mut log);
    let message = &delivered[0].message.0;
    assert_eq!(
        db.user_images("s", message, &prompt.text).unwrap(),
        vec![prompt.images[0].mark.clone()]
    );
    assert_eq!(
        Db::user_prompt_in(&db.conn.lock().unwrap(), "s", message, prompt.text.clone()).unwrap(),
        prompt
    );
    assert_eq!(delivered.len(), 2);
    assert_eq!(
        Db::user_prompt_in(
            &db.conn.lock().unwrap(),
            "s",
            &delivered[1].message.0,
            another.text.clone()
        )
        .unwrap(),
        another
    );
    // Retire the closed storage-owned response. Steered images must remain
    // restorable by their own original RAW coordinates, not a HOT summary.
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::Assistant,
        "done",
    ));
    log.spans.push(oc_core::queries::AssistantSpan {
        request: None,
        id: "closed-storage-span".into(),
        step: 1,
        status: "completed".into(),
        started: 1,
        completed: Some(2),
        retry: None,
        error: None,
        finish: Some("stop".into()),
    });
    db.checkpoint_turn("t", &log.to_json().to_string()).unwrap();
    let counts = log.closed_counts();
    let working = log
        .renewed_task_checkpoint("reduced task", counts, counts[0], |_| false)
        .unwrap();
    let (segment, hot) = log.prepare_closed_segment(counts, working, 0).unwrap();
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    assert!(hot.input.is_empty());
    db.commit_turn(
        "t",
        "completed",
        Some(&hot.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    for (message, expected) in [
        (&delivered[0].message.0, &prompt),
        (&delivered[1].message.0, &another),
    ] {
        assert_eq!(
            Db::user_prompt_in(
                &db.conn.lock().unwrap(),
                "s",
                message,
                expected.text.clone()
            )
            .unwrap(),
            *expected
        );
    }
    let next = db
        .accept_turn("next", "s", "next", "next", &model())
        .unwrap();
    let fork = db
        .fork_session("s", &next.user_message, "/fixture", "fixture", "{}")
        .unwrap();
    let copied = db.read_history(&fork.session.0).unwrap();
    assert_eq!(
        copied
            .iter()
            .filter(|(_, text)| text == &prompt.text)
            .count(),
        1
    );
    let message: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT message_id FROM session_inbox WHERE session_id=?1",
            [&fork.session.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        Db::user_prompt_in(
            &db.conn.lock().unwrap(),
            &fork.session.0,
            &message,
            prompt.text.clone()
        )
        .unwrap(),
        prompt
    );
    assert!(db.inbox_pending(&fork.session.0).unwrap().is_empty());
    assert!(
        db.fork_session("s", &delivered[0].message.0, "/fixture", "fixture", "{}")
            .is_err(),
        "steer is not an independent turn boundary"
    );
}

#[test]
fn delivered_command_keeps_capture_and_expansion_after_reopen_and_fork() {
    let (tmp, db) = db();
    db.set_pref("tui.session_location.s", "/fixture").unwrap();
    let original = "/review ORIGINAL";
    let payload = InboxPayload {
        prompt: "EXPANDED ORIGINAL".into(),
        command: Some(CommandCapture {
            id: "review".into(),
            generation: 7,
            digest: Some("captured-catalog".into()),
            original: original.into(),
        }),
    };
    db.inbox_add_rich("s", InboxDelivery::Steer, &payload, None)
        .unwrap();
    let mut log = turn(&db);
    deliver(&db, &mut log);
    db.commit_turn(
        "t",
        "completed",
        Some(&log.to_json().to_string()),
        Some("done"),
    )
    .unwrap();
    let next = db
        .accept_turn("next", "s", "next", "next", &model())
        .unwrap();
    let fork = db
        .fork_session("s", &next.user_message, "/fixture", "fixture", "{}")
        .unwrap();
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    for session in ["s", fork.session.0.as_str()] {
        let (raw, text, pending): (String, String, Option<String>) = db.conn.lock().unwrap().query_row(
            "SELECT command,text,payload FROM session_inbox WHERE session_id=?1 AND message_id IS NOT NULL",
            [session], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        let capture: CommandCapture = serde_json::from_str(&raw).unwrap();
        assert_eq!(capture.id, "review");
        assert_eq!(capture.generation, 7);
        assert_eq!(capture.digest.as_deref(), Some("captured-catalog"));
        assert_eq!(capture.original, original);
        assert_eq!(text, original);
        assert!(pending.is_none());
    }
    assert!(
        db.turn_result("t")
            .unwrap()
            .1
            .unwrap()
            .contains("EXPANDED ORIGINAL")
    );
}

#[test]
fn legacy_text_schema_upgrades_without_losing_pending_or_delivered_links() {
    let (tmp, db) = db();
    let message = Db::insert_message(&db.conn.lock().unwrap(), "s", "user", "delivered").unwrap();
    db.conn.lock().unwrap().execute_batch("DROP TABLE session_inbox; CREATE TABLE session_inbox(id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),delivery TEXT NOT NULL CHECK(delivery IN ('steer','queue')),text TEXT NOT NULL,message_id TEXT UNIQUE REFERENCES messages(id)); INSERT INTO session_inbox VALUES('legacy','s','queue','old text',NULL); PRAGMA user_version=0;").unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO session_inbox VALUES('done','s','steer','delivered',?1)",
            [&message],
        )
        .unwrap();
    drop(db);
    let db = Db::open(tmp.path()).unwrap();
    assert_eq!(
        db.inbox_read("s", "legacy")
            .unwrap()
            .unwrap()
            .payload
            .original(),
        PromptInput::from("old text")
    );
    assert!(Db::inbox_message_in(&db.conn.lock().unwrap(), "s", &message).unwrap());
    Db::inbox_schema(&db.conn.lock().unwrap()).unwrap();
    assert_eq!(db.inbox_pending("s").unwrap().len(), 1);
}

#[test]
fn withdrawn_steer_and_later_selection_cannot_be_overwritten_by_prepared_delivery() {
    let (_tmp, db) = db();
    let mut log = turn(&db);
    let item = db.inbox_add("s", InboxDelivery::Steer, "draft").unwrap();
    let steer = InboxSteer {
        input_index: log.input.len(),
        pending: db.inbox_read("s", &item.id).unwrap().unwrap(),
    };
    db.inbox_set_delivery("s", &item.id, InboxDelivery::Queue)
        .unwrap();
    assert!(matches!(
        db.checkpoint_turn_with_inbox(
            "s",
            &mut log,
            &[],
            Some(&steer),
            None,
            Some(("nudge-test", "preview"))
        ),
        Err(StorageError::CompressionConflict)
    ));
    assert!(db.get_pref("nudge-test").unwrap().is_none());
    db.inbox_set_delivery("s", &item.id, InboxDelivery::Steer)
        .unwrap();
    db.set_pref("selection-test", "newer explicit commit")
        .unwrap();
    db.checkpoint_turn_with_inbox(
        "s",
        &mut log,
        &[],
        Some(&steer),
        Some(("selection-test", "command", "captured".into())),
        None,
    )
    .unwrap();
    assert_eq!(
        db.get_pref("selection-test").unwrap().as_deref(),
        Some("newer explicit commit")
    );
}
