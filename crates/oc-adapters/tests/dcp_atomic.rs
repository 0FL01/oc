use oc_adapters::dcp::{
    self, CompressionBlock, CompressionCommitMetadata, DcpError, ValidatedRange,
    compress_ranges_atomic,
};
use oc_adapters::storage::Db;
use oc_core::context_plan::ProtectedSpec;
use oc_core::session::{Message, MessageId, Role};

fn range(start: &str, end: &str, summary: &str) -> ValidatedRange {
    ValidatedRange {
        topic: "atomic ranges".to_string(),
        start_id: start.to_string(),
        end_id: end.to_string(),
        summary: summary.to_string(),
    }
}

fn seed_history(db: &Db, session: &str, count: usize) -> Vec<Message> {
    (0..count)
        .map(|index| {
            let (role, role_name) = if index % 2 == 0 {
                (Role::User, "user")
            } else {
                (Role::Assistant, "assistant")
            };
            let text = if index + 1 == count {
                "live tail".to_string()
            } else {
                format!("payload-{index}-{}", "x".repeat(2_048))
            };
            let id = db
                .append_message(session, role_name, &text)
                .expect("append message");
            Message {
                id: MessageId(id),
                role,
                text,
            }
        })
        .collect()
}

#[test]
fn second_range_failure_rolls_back_blocks_members_and_tool_journal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&temp.path().join("data")).expect("open db");
    dcp::apply_dcp_schema(&db).expect("DCP schema");
    db.create_session("s").expect("create session");
    let history = seed_history(&db, "s", 7);
    let raw_before = db.read_history_full("s").expect("raw history");

    db.begin_turn("turn-1", "s", "compress").expect("turn");
    db.record_tool_intent("op-1", "s", Some("turn-1"), "compress", "{\"ranges\":2}")
        .expect("tool intent");
    let metadata = CompressionCommitMetadata {
        operation_id: "op-1",
        operation_state: "completed",
        operation_output: "{\"blocks\":2}",
        turn_id: Some("turn-1"),
        turn_log: Some("{\"tool_output\":\"committed\"}"),
        preference_updates: &[],
    };
    // Deliberately reverse input order. The planner must keep each summary
    // attached to its own anchors while producing transcript-ordered blocks.
    let ranges = vec![
        range(&history[2].id.0, &history[3].id.0, "late-summary"),
        range(&history[0].id.0, &history[1].id.0, "early-summary"),
    ];

    let sql = rusqlite::Connection::open(db.root().join("oc.sqlite")).expect("external sqlite");
    sql.execute_batch(
        "CREATE TRIGGER fail_second_range
         BEFORE INSERT ON compression_blocks
         WHEN NEW.id = 'b0002'
         BEGIN SELECT RAISE(ABORT, 'injected second range failure'); END;",
    )
    .expect("install trigger");

    assert!(matches!(
        compress_ranges_atomic(
            &db,
            "s",
            &history,
            &ranges,
            &ProtectedSpec::default(),
            Some(&metadata),
        ),
        Err(DcpError::Storage)
    ));
    let block_count: i64 = sql
        .query_row("SELECT COUNT(*) FROM compression_blocks", [], |row| {
            row.get(0)
        })
        .expect("block count");
    let member_count: i64 = sql
        .query_row("SELECT COUNT(*) FROM compression_members", [], |row| {
            row.get(0)
        })
        .expect("member count");
    assert_eq!((block_count, member_count), (0, 0));
    assert_eq!(db.tool_state("op-1").expect("tool state"), "started");
    assert_eq!(
        db.turn_result("turn-1").expect("turn result"),
        ("started".to_string(), None)
    );
    assert_eq!(
        db.read_history_full("s").expect("raw after failure"),
        raw_before
    );

    sql.execute_batch("DROP TRIGGER fail_second_range")
        .expect("drop trigger");
    let report = compress_ranges_atomic(
        &db,
        "s",
        &history,
        &ranges,
        &ProtectedSpec::default(),
        Some(&metadata),
    )
    .expect("atomic compression");
    assert_eq!(report.blocks.len(), 2);
    assert_eq!(report.blocks[0].start_msg, history[0].id.0);
    assert_eq!(report.blocks[0].summary, "early-summary");
    assert_eq!(report.blocks[1].start_msg, history[2].id.0);
    assert_eq!(report.blocks[1].summary, "late-summary");
    assert_eq!(report.blocks[0].members.len(), 2);
    assert_eq!(report.blocks[1].members.len(), 2);
    assert!(report.after_bytes < report.before_bytes);
    assert!(report.saved_tokens > 0);
    assert_eq!(db.tool_state("op-1").expect("committed state"), "completed");
    assert_eq!(
        db.list_tool_ops("s").expect("committed operation")[0]
            .output
            .as_deref(),
        Some("{\"blocks\":2}")
    );
    assert_eq!(
        db.turn_result("turn-1").expect("committed turn").1,
        Some("{\"tool_output\":\"committed\"}".to_string())
    );
    assert_eq!(
        db.read_history_full("s").expect("raw after success"),
        raw_before
    );

    let overlap = compress_ranges_atomic(
        &db,
        "s",
        &history,
        &[range(&history[1].id.0, &history[1].id.0, "overlap")],
        &ProtectedSpec::default(),
        None,
    );
    assert!(matches!(overlap, Err(DcpError::ExistingOverlap { .. })));
    assert_eq!(dcp::load_blocks(&db, "s").expect("blocks").len(), 2);

    let nested = compress_ranges_atomic(
        &db,
        "s",
        &history,
        &[range("b0001", "b0002", "combined (b1) (b2)")],
        &ProtectedSpec::default(),
        None,
    )
    .expect("block anchors may subsume older active blocks");
    assert_eq!(nested.blocks[0].id, "b0003");
    assert_eq!(nested.blocks[0].members.len(), 4);
    let all = dcp::load_blocks(&db, "s").expect("nested blocks");
    assert_eq!(
        all.len(),
        3,
        "older blocks remain available for nested expansion"
    );
    assert!(all[0].members.is_empty() && all[1].members.is_empty());
    assert_eq!(all[2].members.len(), 4);
    let projected = dcp::project_history(&raw_before, &all, None);
    assert_eq!(
        projected
            .iter()
            .filter(|(_, text)| text.starts_with("[compressed "))
            .count(),
        1
    );
    assert!(projected[0].1.contains("early-summary"));
    assert!(projected[0].1.contains("late-summary"));

    let second_level = compress_ranges_atomic(
        &db,
        "s",
        &history,
        &[range("b0003", "m0005", "second level (b3) plus fifth")],
        &ProtectedSpec::default(),
        None,
    )
    .expect("a block created from block anchors remains a usable anchor");
    assert_eq!(second_level.blocks[0].id, "b0004");
    assert_eq!(second_level.blocks[0].start_msg, "m0001");
    assert_eq!(second_level.blocks[0].end_msg, "m0005");
    assert_eq!(second_level.blocks[0].members.len(), 5);
    let all = dcp::load_blocks(&db, "s").expect("second-level blocks");
    let projected = dcp::project_history(&raw_before, &all, None);
    assert!(projected[0].1.contains("early-summary"));
    assert!(projected[0].1.contains("late-summary"));
}

#[test]
fn protected_user_tag_and_file_messages_are_preserved_verbatim() {
    let protected_user = "user fact with literal (b999)";
    let protected_tag = "assistant <protect>TAG-FACT</protect> surrounding text";
    let protected_file = "tool result for src/critical.rs remains exact";
    let mut history = vec![
        Message {
            id: MessageId("m0001".to_string()),
            role: Role::User,
            text: protected_user.to_string(),
        },
        Message {
            id: MessageId("m0002".to_string()),
            role: Role::Assistant,
            text: protected_tag.to_string(),
        },
        Message {
            id: MessageId("m0003".to_string()),
            role: Role::Assistant,
            text: protected_file.to_string(),
        },
    ];
    for index in 4..=6 {
        history.push(Message {
            id: MessageId(format!("m{index:04}")),
            role: Role::Assistant,
            text: "unprotected filler ".repeat(512),
        });
    }
    history.push(Message {
        id: MessageId("m0007".to_string()),
        role: Role::User,
        text: "live tail".to_string(),
    });
    let spec = ProtectedSpec {
        protect_user_messages: true,
        protect_tags: true,
        file_globs: vec!["src/*.rs".to_string()],
        ..ProtectedSpec::default()
    };
    let plan = dcp::plan_compression(
        "s",
        &history,
        &[range("m0001", "m0006", "compact")],
        &spec,
        &[],
        None,
        1,
    )
    .expect("protected plan");
    let summary = &plan.blocks[0].summary;
    assert!(summary.contains(protected_user));
    assert!(summary.contains(protected_tag));
    assert!(summary.contains(protected_file));
    assert!(plan.after_bytes < plan.before_bytes);
}

#[test]
fn no_gain_is_reported_without_writes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&temp.path().join("data")).expect("open db");
    dcp::apply_dcp_schema(&db).expect("DCP schema");
    db.create_session("s").expect("create session");
    let history = vec![
        Message {
            id: MessageId(db.append_message("s", "user", "hi").expect("m1")),
            role: Role::User,
            text: "hi".to_string(),
        },
        Message {
            id: MessageId(db.append_message("s", "assistant", "ok").expect("m2")),
            role: Role::Assistant,
            text: "ok".to_string(),
        },
        Message {
            id: MessageId(db.append_message("s", "user", "tail").expect("m3")),
            role: Role::User,
            text: "tail".to_string(),
        },
    ];
    let result = compress_ranges_atomic(
        &db,
        "s",
        &history,
        &[range("m0001", "m0002", &"summary".repeat(64))],
        &ProtectedSpec::default(),
        None,
    );
    assert!(matches!(result, Err(DcpError::NoGain { .. })));
    assert!(dcp::load_blocks(&db, "s").expect("blocks").is_empty());
    assert_eq!(db.read_history("s").expect("raw").len(), 3);
}

#[test]
fn nested_unknown_cycle_depth_and_size_fail_during_planning() {
    let history = vec![
        Message {
            id: MessageId("m0001".to_string()),
            role: Role::User,
            text: "first ".repeat(1_024),
        },
        Message {
            id: MessageId("m0002".to_string()),
            role: Role::Assistant,
            text: "second ".repeat(1_024),
        },
        Message {
            id: MessageId("m0003".to_string()),
            role: Role::User,
            text: "live tail".to_string(),
        },
    ];
    let plan = |ranges: &[ValidatedRange], existing: &[CompressionBlock], next| {
        dcp::plan_compression(
            "s",
            &history,
            ranges,
            &ProtectedSpec::default(),
            existing,
            None,
            next,
        )
    };

    assert!(matches!(
        plan(&[range("m0001", "m0001", "unknown (b999)")], &[], 1),
        Err(DcpError::UnknownBlock { .. })
    ));
    assert!(matches!(
        plan(
            &[
                range("m0001", "m0001", "to (b2)"),
                range("m0002", "m0002", "to (b1)"),
            ],
            &[],
            1,
        ),
        Err(DcpError::Cycle { .. })
    ));

    let block = |id: String, summary: String| CompressionBlock {
        id,
        session: "s".to_string(),
        topic: "nested".to_string(),
        summary,
        start_msg: "stale".to_string(),
        end_msg: "stale".to_string(),
        members: vec![],
    };
    let deep = (1..=10)
        .map(|number| {
            let summary = if number == 10 {
                "leaf".to_string()
            } else {
                format!("(b{})", number + 1)
            };
            block(format!("b{number:04}"), summary)
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        plan(&[range("m0001", "m0001", "compact")], &deep, 11),
        Err(DcpError::NestedTooLarge { .. })
    ));

    let oversized = vec![block(
        "b0001".to_string(),
        "x".repeat(dcp::NESTED_BYTES_CAP + 1),
    )];
    assert!(matches!(
        plan(&[range("m0001", "m0001", "compact")], &oversized, 2,),
        Err(DcpError::NestedTooLarge { .. })
    ));
}
