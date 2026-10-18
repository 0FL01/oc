use super::*;
use oc_core::queries::{HistoryPartWindow, HistoryTurn, TranscriptPart};

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
