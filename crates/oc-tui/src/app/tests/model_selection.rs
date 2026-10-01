use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::SessionSelectionAction;

fn model_snapshot() -> CatalogSnapshot {
    let mut owner = snapshot();
    owner
        .models
        .iter_mut()
        .find(|model| model.id == "a")
        .unwrap()
        .variants
        .push(oc_core::queries::VariantEntry {
            name: "low".into(),
            disabled: false,
            reasoning_effort: Some("low".into()),
        });
    owner
}

#[tokio::test]
async fn tool12_draft_only_busy_blank_commit_late_failure_keeps_newer_draft() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, sid("draft"));
    let mut owner = model_snapshot();
    owner.chrome.location = Some("/fixture".into());
    owner.chrome.selection_generation = 7;
    state.apply_catalog(owner.clone());
    state.begin_compress_turn(WorkerTurnId("active".into()));
    state.draft_model("b").unwrap();
    assert!(inbox.try_recv().is_err());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection {
        action: SessionSelectionAction::Commit(first),
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("blank commit");
    };
    assert_eq!(first.model_id, "b");
    assert_eq!(first.binding.generation, 7);
    assert_eq!(first.binding.location.as_deref(), Some("/fixture"));
    let mut other_caller = first.clone();
    other_caller.caller += 1;
    state.apply_session_model_selected(&sid("draft"), &other_caller);
    state.apply_catalog(owner.clone());
    assert_eq!(state.captured_model_commit().unwrap().model_id, "b");
    state.draft_model("a").unwrap();
    state.draft_variant(Some("low")).unwrap();
    state.apply_session_model_selected(&sid("draft"), &first);
    owner.model_id = "b".into();
    owner.variant = None;
    ack.send(Ok(owner.clone())).unwrap();
    state.poll_submission();
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    assert_eq!(
        state.captured_model_commit().unwrap().variant.as_deref(),
        Some("low")
    );
    state.apply_catalog(owner.clone());
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("second commit");
    };
    ack.send(Err(CoreError::Application("stale".into())))
        .unwrap();
    state.poll_submission();
    assert_eq!(
        state.captured_model_commit().unwrap().variant.as_deref(),
        Some("low")
    );
    assert!(
        state
            .note
            .as_ref()
            .is_some_and(|(note, _)| note.contains("model commit") && note.contains("stale"))
    );
    assert_eq!(state.active_turn, Some(WorkerTurnId("active".into())));
    state.parent_id = Some("parent".into());
    state.handle_key(KeyAction::Enter).await;
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn tool12_message_captures_draft_and_pending_acceptance_does_not_erase_newer_choice() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("ordered"));
    state.apply_catalog(model_snapshot());
    state.draft_model("b").unwrap();
    type_text(&mut state, "/workspace captured-command").await;
    state.commands.push("workspace".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit {
        text,
        selection: Some(captured),
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured submit");
    };
    assert_eq!(text, "/workspace captured-command");
    assert_eq!(captured.model_id, "b");
    state.draft_model("a").unwrap();
    state.draft_variant(Some("low")).unwrap();
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    assert_eq!(
        state.captured_model_commit().unwrap().variant.as_deref(),
        Some("low")
    );
    assert!(state.input().is_empty());
    assert!(inbox.try_recv().is_err());
}
