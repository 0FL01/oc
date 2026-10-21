use super::*;
use crate::instructions::Origin;

fn source(text: Option<&str>) -> Source {
    Source {
        path: "/project/AGENTS.md".into(),
        root: "/project".into(),
        origin: Origin::Project,
        content: text.map(str::to_owned),
        digest: text.map(|s| format!("{:x}", Sha256::digest(s))),
        generation: 1,
    }
}

#[test]
fn prm01_instruction_facts_are_atomic_immutable_and_latest_only() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "prompt").unwrap();
    let mut log = crate::tools::TurnLog::new("t", "m", "p");
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        "prompt",
    ));
    assert!(
        db.checkpoint_instructions("t", &mut log, 0, &[source(Some("old"))], 0, &[], None)
            .unwrap()
            .is_none()
    );
    let original = db.instruction_view("s").unwrap().1[0].clone();
    let (revision, _) = db.instruction_view("s").unwrap();
    db.checkpoint_instructions(
        "t",
        &mut log,
        revision,
        &[source(Some("old"))],
        1,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(log.instruction_references.len(), 1);
    let (revision, _) = db.instruction_view("s").unwrap();
    db.record_tool_intent("read", "s", Some("t"), "read", "{}")
        .unwrap();
    let before = log.clone();
    let saved = db.turn_result("t").unwrap();
    let nested = Source::baseline(
        std::path::Path::new("/project/.local/opencode/AGENTS.md"),
        std::path::Path::new("/project"),
        Origin::Nested,
        "nested body never enters display receipt",
    );
    let presentation = Presentation::new("result", 6, false);
    assert!(
        db.checkpoint_instructions(
            "t",
            &mut log,
            revision - 1,
            &[source(Some("new")), nested.clone()],
            1,
            &[],
            Some(RecordedToolOutcome {
                operation: "read",
                state: "completed",
                output: "result",
                presentation: Some(&presentation),
            })
        )
        .is_err()
    );
    assert_eq!(log, before);
    assert_eq!(db.turn_result("t").unwrap(), saved);
    assert_eq!(db.list_tool_ops("s").unwrap()[0].state, "started");
    assert_eq!(db.instruction_view("s").unwrap().1[0], original);
    assert!(
        Db::tool_presentation_in(&db.conn.lock().unwrap(), "s", "read")
            .unwrap()
            .is_none()
    );
    let returned = db
        .checkpoint_instructions(
            "t",
            &mut log,
            revision,
            &[source(Some("new")), nested.clone()],
            1,
            &[],
            Some(RecordedToolOutcome {
                operation: "read",
                state: "completed",
                output: "result",
                presentation: Some(&presentation),
            }),
        )
        .unwrap()
        .unwrap();
    assert_eq!(returned.body, "result");
    assert_eq!(
        returned.instructions,
        Some(InstructionLoads {
            paths: vec![".local/opencode/AGENTS.md".into()],
            total: 1,
        })
    );
    assert_eq!(
        Db::tool_presentation_in(&db.conn.lock().unwrap(), "s", "read")
            .unwrap()
            .as_deref(),
        Some(&returned),
    );
    let current = db.instruction_view("s").unwrap().1;
    let projected = crate::instructions::project(&log.input, &log.instruction_references, &current);
    assert_eq!(projected.len(), 3);
    assert_eq!(projected[0], log.input[0]);
    assert_eq!(projected[1], current[0].input());
    assert_eq!(projected[2], current[1].input());
    let references = log.instruction_references.clone();
    let revision = db.instruction_view("s").unwrap().0;
    db.record_tool_intent("repeat", "s", Some("t"), "read", "{}")
        .unwrap();
    let repeated = db
        .checkpoint_instructions(
            "t",
            &mut log,
            revision,
            &[source(Some("new")), nested],
            1,
            &[],
            Some(RecordedToolOutcome {
                operation: "repeat",
                state: "completed",
                output: "result",
                presentation: Some(&returned),
            }),
        )
        .unwrap()
        .unwrap();
    assert!(repeated.instructions.is_none());
    assert_eq!(log.instruction_references, references);
    assert_eq!(
        Db::tool_presentation_in(&db.conn.lock().unwrap(), "s", "repeat")
            .unwrap()
            .as_deref(),
        Some(&repeated),
    );
    let stored: String = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT payload FROM events WHERE seq=?1",
            [original.event],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Fact>(&stored)
            .unwrap()
            .source
            .content
            .as_deref(),
        Some("old")
    );
    let revision = db.instruction_view("s").unwrap().0;
    db.record_tool_intent("remove", "s", Some("t"), "read", "{}")
        .unwrap();
    let removed = db
        .checkpoint_instructions(
            "t",
            &mut log,
            revision,
            &[source(None)],
            1,
            &[],
            Some(RecordedToolOutcome {
                operation: "remove",
                state: "completed",
                output: "result",
                presentation: Some(&returned),
            }),
        )
        .unwrap()
        .unwrap();
    assert!(removed.instructions.is_none());
    let current = db.instruction_view("s").unwrap().1;
    assert_eq!(current[0].change, "removed");
    assert_eq!(
        crate::instructions::project(&log.input, &log.instruction_references, &current)[1],
        current[0].input()
    );
    drop(db);
    let db = Db::open(temp.path()).unwrap();
    assert_eq!(
        Db::tool_presentation_in(&db.conn.lock().unwrap(), "s", "read")
            .unwrap()
            .as_deref(),
        Some(&returned),
    );
    assert!(
        db.instruction_view("s").unwrap().1[0]
            .source
            .content
            .is_none()
    );
}

#[test]
fn prm01_instruction_receipts_bound_safe_labels_and_the_real_record_envelope() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "prompt").unwrap();
    let mut log = crate::tools::TurnLog::new("t", "m", "p");
    let operation = "\"".repeat(15_400);
    db.record_tool_intent(&operation, "s", Some("t"), "read", "{}")
        .unwrap();
    let sources = (0..128)
        .map(|index| {
            Source::baseline(
                &std::path::PathBuf::from(format!(
                    "/project/{index:03}-SECRET-\n-{}/AGENTS.md",
                    "界".repeat(5)
                )),
                std::path::Path::new("/project"),
                Origin::Nested,
                "private instruction body",
            )
        })
        .collect::<Vec<_>>();
    let presentation = Presentation::new("result", 6, false);
    let returned = db
        .checkpoint_instructions(
            "t",
            &mut log,
            0,
            &sources,
            0,
            &["SECRET".into()],
            Some(RecordedToolOutcome {
                operation: &operation,
                state: "completed",
                output: "result",
                presentation: Some(&presentation),
            }),
        )
        .unwrap()
        .unwrap();
    let loads = returned.instructions.as_ref().unwrap();
    assert_eq!(loads.total, 128);
    assert!(!loads.paths.is_empty());
    assert!(loads.paths.len() < loads.total);
    assert!(loads.retained_bytes() <= PREVIEW_BYTES);
    assert!(loads.paths.iter().all(|path| path.contains("[redacted]")
        && !path.contains("SECRET")
        && !path.chars().any(char::is_control)));
    assert!(returned.is_valid());
    assert_eq!(returned.retained_bytes(), 6 + loads.retained_bytes());
    assert!(
        serde_json::json!({"operation":operation,"presentation":returned})
            .to_string()
            .len()
            <= RECORD_BYTES
    );
    assert_eq!(
        Db::tool_presentation_in(&db.conn.lock().unwrap(), "s", &operation)
            .unwrap()
            .as_deref(),
        Some(&returned),
    );
    let mut invalid = loads.clone();
    invalid.paths.push(invalid.paths[0].clone());
    assert!(!invalid.is_valid());
    let mut invalid = loads.clone();
    invalid.paths = vec!["x".repeat(PREVIEW_BYTES + 1)];
    assert!(!invalid.is_valid());
    let mut invalid = loads.clone();
    invalid.total = 0;
    assert!(!invalid.is_valid());
    let mut invalid = loads.clone();
    invalid.paths = vec!["\n".into()];
    assert!(!invalid.is_valid());
    let mut invalid = loads.clone();
    invalid.paths = (0..129).map(|index| index.to_string()).collect();
    invalid.total = 129;
    assert!(!invalid.is_valid());
    let legacy = serde_json::to_value(&presentation).unwrap();
    assert!(legacy.get("instructions").is_none());
    assert_eq!(
        serde_json::from_value::<Presentation>(legacy).unwrap(),
        presentation
    );
}
