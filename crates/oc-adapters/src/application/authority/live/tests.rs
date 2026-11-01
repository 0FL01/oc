use super::*;
use oc_core::queries::{HistoryPartWindow, HistoryTurn, TranscriptPart};

#[test]
fn covering_checkpoints_recover_public_hints_but_not_unlinked_drafts() {
    use oc_core::tool_stream::{ToolStreamEvent, ToolStreamIdentity};
    let live = Live::default();
    let session = SessionId("root".into());
    let turn = WorkerTurnId("turn".into());
    let emit = |event| live.emit("/fixture", None, event);
    let limited = || live.state.lock().unwrap().turns["root"].view.truncated;
    let checkpoint = |projection| CoreEvent::TurnPresentation {
        session: session.clone(),
        turn: turn.clone(),
        projection,
    };
    emit(CoreEvent::TurnStarted {
        session: session.clone(),
        turn: turn.clone(),
        model_switch: None,
    });
    let text = "x".repeat(TEXT_BYTES + 1);
    emit(CoreEvent::TextDelta {
        session: session.clone(),
        turn: turn.clone(),
        delta: text.clone(),
    });
    assert!(limited());
    let mut projection = HistoryTurn {
        id: turn.0.clone(),
        revision: 1,
        parts: vec![TranscriptPart::Text(text)],
        part_states: vec![oc_core::queries::PartState::default()],
        window: Some(HistoryPartWindow {
            start: 0,
            end: 1,
            total: 1,
        }),
        ..Default::default()
    };
    emit(checkpoint(projection.clone()));
    assert!(
        !limited(),
        "the durable checkpoint covers the clipped public hint"
    );
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "item".into(),
        call_id: "call".into(),
    };
    emit(CoreEvent::ToolArgumentStream {
        session: session.clone(),
        turn: turn.clone(),
        event: ToolStreamEvent::Pending {
            identity: identity.clone(),
            name: "read".into(),
            preview: "{".into(),
            truncated: false,
            output_presentation: None,
        },
    });
    for _ in 0..PARTS {
        emit(CoreEvent::TextDelta {
            session: session.clone(),
            turn: turn.clone(),
            delta: "a".into(),
        });
        emit(CoreEvent::ReasoningDelta {
            session: session.clone(),
            turn: turn.clone(),
            delta: "b".into(),
        });
    }
    assert!(limited());
    projection.revision = 2;
    projection.parts = (1 + 2 * PARTS - 240..1 + 2 * PARTS)
        .map(|sequence| {
            if sequence % 2 == 1 {
                TranscriptPart::Text("a".into())
            } else {
                TranscriptPart::Reasoning {
                    text: "b".into(),
                    duration_ms: None,
                }
            }
        })
        .collect();
    projection.window = Some(HistoryPartWindow {
        start: 1 + 2 * PARTS - 240,
        end: 1 + 2 * PARTS,
        total: 1 + 2 * PARTS,
    });
    projection.part_states = (1 + 2 * PARTS - 240..1 + 2 * PARTS)
        .map(|sequence| oc_core::queries::PartState {
            sequence,
            ..Default::default()
        })
        .collect();
    emit(checkpoint(projection.clone()));
    assert!(
        limited(),
        "a public checkpoint does not prove admission of a missing draft"
    );
    projection.revision = 1;
    emit(checkpoint(projection));
    assert!(
        live.state.lock().unwrap().turns["root"]
            .view
            .events
            .iter()
            .any(|event| matches!(event,
        CoreEvent::TurnPresentation { projection, .. } if projection.revision == 2))
    );
    emit(CoreEvent::ToolArgumentStream {
        session,
        turn,
        event: ToolStreamEvent::Linked {
            identity,
            op: "durable-op".into(),
        },
    });
    assert!(
        !limited(),
        "Linked proves the missing draft now has a durable owner"
    );
    let state = live.state.lock().unwrap();
    assert!(state.turns["root"].bytes <= TURN_BYTES);
    assert!(state.turns["root"].view.events.len() <= PARTS);
}

#[test]
fn canonical_root_and_selected_child_windows_survive_snapshot_and_replacement() {
    let live = Live::default();
    let root = SessionId("root".into());
    let child = SessionId("child".into());
    let text = "λ\\\n".repeat(crate::storage::TRANSCRIPT_PAGE_BYTES / 4);
    let projection = HistoryTurn {
        parts: vec![TranscriptPart::Text(text)],
        window: Some(HistoryPartWindow {
            start: 0,
            end: 1,
            total: 1,
        }),
        ..Default::default()
    };
    let event = |session: &SessionId| CoreEvent::TurnPresentation {
        session: session.clone(),
        turn: WorkerTurnId(session.0.clone()),
        projection: projection.clone(),
    };
    assert!(bytes(&event(&root)) > 768 * 1024);
    for session in [&root, &child] {
        live.emit(
            "/fixture",
            None,
            CoreEvent::TurnStarted {
                session: session.clone(),
                turn: WorkerTurnId(session.0.clone()),
                model_switch: None,
            },
        );
        live.emit("/fixture", None, event(session));
        live.emit("/fixture", None, event(session));
    }
    let snapshot = live.snapshot(
        TabDeckSnapshot {
            location: "/fixture".into(),
            sessions: vec![root.clone()],
            active: Some(root),
            ..Default::default()
        },
        Some(&child),
    );
    assert_eq!(snapshot.turns.len(), 2);
    assert!(
        snapshot
            .turns
            .iter()
            .all(|turn| !turn.truncated && turn.events.len() == 2)
    );
    for turn in &snapshot.turns {
        assert!(
            matches!(&turn.events[1],CoreEvent::TurnPresentation {projection:actual,..} if actual==&projection)
        );
    }
    let wire = serde_json::to_vec(&snapshot).unwrap();
    assert!(wire.len() < 2 * TURN_BYTES);
    assert_eq!(
        serde_json::from_slice::<WorkspaceSnapshot>(&wire).unwrap(),
        snapshot
    );
    assert!(
        live.state
            .lock()
            .unwrap()
            .turns
            .values()
            .all(|turn| turn.bytes <= TURN_BYTES)
    );
}
