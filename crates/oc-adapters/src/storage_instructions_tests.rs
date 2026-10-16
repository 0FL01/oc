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
    db.checkpoint_instructions("t", &mut log, 0, &[source(Some("old"))], 0, None)
        .unwrap();
    let original = db.instruction_view("s").unwrap().1[0].clone();
    let (revision, _) = db.instruction_view("s").unwrap();
    db.checkpoint_instructions("t", &mut log, revision, &[source(Some("old"))], 1, None)
        .unwrap();
    assert_eq!(log.instruction_references.len(), 1);
    let (revision, _) = db.instruction_view("s").unwrap();
    db.record_tool_intent("read", "s", Some("t"), "read", "{}")
        .unwrap();
    let before = log.clone();
    let saved = db.turn_result("t").unwrap();
    assert!(
        db.checkpoint_instructions(
            "t",
            &mut log,
            revision - 1,
            &[source(Some("new"))],
            1,
            Some(RecordedToolOutcome {
                operation: "read",
                state: "completed",
                output: "result",
                presentation: None,
            })
        )
        .is_err()
    );
    assert_eq!(log, before);
    assert_eq!(db.turn_result("t").unwrap(), saved);
    assert_eq!(db.list_tool_ops("s").unwrap()[0].state, "started");
    assert_eq!(db.instruction_view("s").unwrap().1[0], original);
    db.checkpoint_instructions(
        "t",
        &mut log,
        revision,
        &[source(Some("new"))],
        1,
        Some(RecordedToolOutcome {
            operation: "read",
            state: "completed",
            output: "result",
            presentation: None,
        }),
    )
    .unwrap();
    let current = db.instruction_view("s").unwrap().1;
    let projected = crate::instructions::project(&log.input, &log.instruction_references, &current);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0], log.input[0]);
    assert_eq!(projected[1], current[0].input());
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
    db.checkpoint_instructions("t", &mut log, revision, &[source(None)], 1, None)
        .unwrap();
    let current = db.instruction_view("s").unwrap().1;
    assert_eq!(current[0].change, "removed");
    assert_eq!(
        crate::instructions::project(&log.input, &log.instruction_references, &current)[1],
        current[0].input()
    );
    drop(db);
    let db = Db::open(temp.path()).unwrap();
    assert!(
        db.instruction_view("s").unwrap().1[0]
            .source
            .content
            .is_none()
    );
}
