use super::*;
use oc_core::{
    context_plan::ProtectedSpec,
    session::{Message, MessageId, Role},
};

fn message(id: &str, role: Role, text: &str) -> Message {
    Message {
        id: MessageId(id.into()),
        role,
        text: text.into(),
    }
}

#[test]
fn wholly_covered_omitted_block_is_consumed_and_reference_is_standalone() {
    let history = vec![
        message("m1", Role::User, "CONTROL_OBJECTIVE"),
        message("m2", Role::Assistant, ""),
        message("m3", Role::User, "fresh closed work"),
        message("m4", Role::Assistant, "closed result"),
        message("m5", Role::User, "current task"),
    ];
    let old = CompressionBlock {
        hot: None,
        id: "b0001".into(),
        session: "s".into(),
        topic: "old".into(),
        summary: "CONTROL_OBJECTIVE OBSOLETE_SUMMARY".into(),
        start_msg: "m1".into(),
        end_msg: "m2".into(),
        members: vec!["m1".into(), "m2".into()],
    };
    let range = ValidatedRange {
        topic: "replacement".into(),
        start_id: "m1".into(),
        end_id: "m4".into(),
        summary: "CONTROL_OBJECTIVE selected next move".into(),
    };
    let plan = plan_active_compression(
        "s",
        &history,
        &[range],
        &ProtectedSpec::default(),
        std::slice::from_ref(&old),
        None,
        2,
    )
    .expect("omission is intentional forgetting");
    assert_eq!(plan.consumed_blocks, vec!["b0001"]);
    assert!(!plan.blocks[0].summary.contains("OBSOLETE_SUMMARY"));
    let referenced = ValidatedRange {
        topic: "reference".into(),
        start_id: old.id.clone(),
        end_id: old.id.clone(),
        summary: "chosen (b0001)".into(),
    };
    let plan = plan_active_compression(
        "s",
        &history,
        &[referenced],
        &ProtectedSpec::default(),
        &[old],
        None,
        2,
    )
    .unwrap();
    assert!(plan.blocks[0].summary.contains("OBSOLETE_SUMMARY"));
    assert!(!plan.blocks[0].summary.contains("(b0001)"));
    let legacy = (0..=NESTED_DEPTH_CAP)
        .map(|n| CompressionBlock {
            id: format!("b{:04}", n + 1),
            session: "s".into(),
            topic: "legacy".into(),
            summary: if n == 0 {
                "LEGACY_FACT".into()
            } else {
                format!("(b{n:04})")
            },
            start_msg: "m1".into(),
            end_msg: "m2".into(),
            members: if n == NESTED_DEPTH_CAP {
                vec!["m1".into(), "m2".into()]
            } else {
                vec![]
            },
            hot: None,
        })
        .collect::<Vec<_>>();
    let top = legacy.last().unwrap().id.clone();
    let renewal = plan_active_compression(
        "s",
        &history,
        &[ValidatedRange {
            topic: "transition".into(),
            start_id: top.clone(),
            end_id: top.clone(),
            summary: format!("chosen ({top})"),
        }],
        &ProtectedSpec::default(),
        &legacy,
        None,
        20,
    )
    .expect("one standalone renewal does not add a lifetime depth to an admitted legacy reference");
    assert!(renewal.blocks[0].summary.contains("LEGACY_FACT"));

    let mut covered = history.clone();
    covered[0].text.clear();
    let legacy_protected = CompressionBlock {
        id: "b0030".into(),
        session: "s".into(),
        topic: "legacy protection".into(),
        summary: format!("chosen{PROTECTED_USER_HEADING}LEGACY_VERBATIM"),
        start_msg: "m1".into(),
        end_msg: "m2".into(),
        members: vec!["m1".into(), "m2".into()],
        hot: None,
    };
    let protection = ProtectedSpec {
        protect_user_messages: true,
        ..Default::default()
    };
    let mut previous = legacy_protected;
    for next in 31..=32 {
        let retained = plan_active_compression(
            "s",
            &covered,
            &[ValidatedRange {
                topic: "explicit protection".into(),
                start_id: previous.id.clone(),
                end_id: previous.id.clone(),
                summary: format!("chosen ({})", previous.id),
            }],
            &protection,
            &[previous],
            None,
            next,
        )
        .unwrap();
        previous = retained.blocks[0].clone();
        assert!(
            previous.summary.contains("LEGACY_VERBATIM"),
            "continuing explicit protection must survive more than its first native renewal"
        );
        assert_eq!(previous.summary.matches("LEGACY_VERBATIM").count(), 1);
    }
    for (next, ids, retained) in [(33, vec!["m1"], true), (34, vec!["m5"], false)] {
        let policy = ProtectedSpec {
            protected_message_ids: ids.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        };
        let plan = plan_active_compression(
            "s",
            &covered,
            &[ValidatedRange {
                topic: "aging".into(),
                start_id: previous.id.clone(),
                end_id: previous.id.clone(),
                summary: "chosen".into(),
            }],
            &policy,
            &[previous],
            None,
            next,
        )
        .unwrap();
        previous = plan.blocks[0].clone();
        assert_eq!(previous.summary.contains("LEGACY_VERBATIM"), retained);
    }
}

#[test]
fn addressed_legacy_coverage_does_not_transfer_4096_historical_members() {
    let data = tempfile::tempdir().unwrap();
    let db = crate::storage::Db::open(data.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let ids = (0..4100)
        .map(|_| db.append_message("s", "user", "archived").unwrap())
        .collect::<Vec<_>>();
    let block = db
        .save_compression_block(
            "s",
            "old",
            "CONTROL_OBJECTIVE",
            &ids[0],
            ids.last().unwrap(),
            &ids,
        )
        .unwrap();
    db.append_message("s", "user", "current task").unwrap();
    let ranges = [ValidatedRange {
        topic: "replacement".into(),
        start_id: block.clone(),
        end_id: block,
        summary: "CONTROL_OBJECTIVE next".into(),
    }];
    let snapshot = db
        .compression_addressed_snapshot("s", 0, &ranges)
        .expect("historical coverage is not a per-operation member quota");
    assert!(snapshot.0.len() <= 3);
    assert!(snapshot.1.iter().all(|b| b.members.len() <= 2));
}

#[test]
fn malformed_hot_metadata_cannot_recall_covered_raw() {
    let data = tempfile::tempdir().unwrap();
    let db = crate::storage::Db::open(data.path()).unwrap();
    db.create_session("s").unwrap();
    db.apply_dcp_schema().unwrap();
    let id = db.append_message("s", "user", "forgotten RAW").unwrap();
    let block = db
        .save_compression_block("s", "old", "chosen", &id, &id, std::slice::from_ref(&id))
        .unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute(
        "UPDATE compression_blocks SET hot='{}' WHERE id=?1",
        [&block],
    )
    .unwrap();
    assert!(
        db.active_compression_graph("s", 0).is_err(),
        "unknown selection metadata must not become uncovered RAW"
    );
}
