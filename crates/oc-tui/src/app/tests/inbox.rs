use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{InboxAction, InboxDelivery, InboxItem, InboxSnapshot};

/// A session view whose first turn the owner accepted and is still running.
async fn running(name: &str) -> (TuiState, tokio::sync::mpsc::Receiver<InboxMsg>) {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, sid(name));
    state.apply_catalog(snapshot());
    type_text(&mut state, "first").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submit")
    };
    ack.send(Ok(WorkerTurnId("running".into()))).unwrap();
    state.poll_submission();
    assert!(state.active_turn().is_some());
    (state, inbox)
}

/// Answer the owner side of one steer/queue admission.
async fn admit(
    inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>,
    reply: Result<(), oc_core::session::CoreError>,
) -> (String, InboxDelivery) {
    loop {
        match inbox.recv().await.expect("owner message") {
            // The composer's model choice is committed with the input (OC2).
            InboxMsg::SessionSelection { ack, .. } => drop(ack),
            InboxMsg::Enqueue {
                text,
                delivery,
                ack,
                ..
            } => {
                let item = InboxItem {
                    id: format!("in-{}", text.text),
                    delivery,
                    text: text.text.clone(),
                    images: text.images.len(),
                    error: None,
                };
                ack.send(reply.map(|()| item)).unwrap();
                return (text.text, delivery);
            }
            _ => panic!("unexpected owner message"),
        }
    }
}

#[tokio::test]
async fn enter_while_busy_steers_into_the_running_turn() {
    let (mut state, mut inbox) = running("steer").await;
    type_text(&mut state, "look at tests too").await;
    let (outcome, admitted) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert_eq!(outcome.note, None, "no 'turn busy' refusal");
    assert_eq!(
        admitted,
        ("look at tests too".to_string(), InboxDelivery::Steer)
    );
    assert!(state.input().is_empty());
    // A pending steer is a transcript user row until the owner delivers it.
    let rows = state.transcript_rows();
    let last = rows.last().unwrap();
    assert_eq!(
        (last.role.as_str(), last.text.as_str()),
        ("user", "look at tests too")
    );
    state.apply_inbox(&sid("steer"), Vec::new());
    assert!(
        !state
            .transcript_rows()
            .iter()
            .any(|row| row.text == "look at tests too" && row.message_id.is_none())
    );
}

#[tokio::test]
async fn interrupted_steer_stays_a_neutral_user_row_not_a_resumable_queue() {
    let (mut state, mut inbox) = running("parked").await;
    type_text(&mut state, "edited prompt").await;
    let (outcome, _) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert!(outcome.note.is_none());
    let pending = state.inbox_items().to_vec();
    let turn = state.active_turn().unwrap().clone();
    state.apply_interrupted(&turn, "", 7_098);

    assert!(!state.is_busy());
    let rows = state.transcript_rows();
    assert!(
        rows.iter()
            .any(|row| row.role == "user" && row.text == "edited prompt"),
        "pending steers remain neutral user rows across the idle boundary"
    );
    assert!(!rows.iter().any(|row| row.role == "queued"));
    assert_eq!(state.inbox_items(), pending);
    assert!(
        inbox.try_recv().is_err(),
        "the owner, not the view, continues steering input"
    );

    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(
        matches!(inbox.try_recv(), Ok(InboxMsg::SessionSelection { .. })),
        "blank Enter still commits the composer's model choice"
    );
    assert!(inbox.try_recv().is_err(), "no submit or inbox promotion");
}

#[tokio::test]
async fn a_turn_that_just_ended_takes_the_prompt_as_a_new_submission() {
    let (mut state, mut inbox) = running("ended").await;
    type_text(&mut state, "next").await;
    state.handle_key(KeyAction::Enter).await;
    assert!(state.input().is_empty());
    state.handle_paste("independent next draft");
    admit(&mut inbox, Err(oc_core::session::CoreError::TurnNotActive)).await;
    state.poll_submission();
    let InboxMsg::Submit { text, ack, .. } = inbox.try_recv().unwrap() else {
        panic!("fallback submit")
    };
    assert_eq!(text.text, "next");
    ack.send(Ok(WorkerTurnId("fallback".into()))).unwrap();
    state.poll_submission();
    state.poll_submission();
    assert_eq!(state.input(), "independent next draft");
    assert!(inbox.try_recv().is_err(), "exactly one fallback");
}

#[tokio::test]
async fn leader_enter_queues_and_the_dock_shows_count_and_preview() {
    let (mut state, mut inbox) = running("queue").await;
    type_text(&mut state, "then update docs").await;
    state.handle_key(KeyAction::Leader).await;
    let (outcome, admitted) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert_eq!(outcome.note, None);
    assert_eq!(admitted.1, InboxDelivery::Queue);
    let rows = state.transcript_rows();
    let dock = rows.iter().find(|row| row.role == "queued").unwrap();
    assert_eq!(dock.text, "1 queued · then update docs");
    // Blank Enter promotes the first queued prompt into the running turn.
    let promote = state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        promote.intent,
        Some(PanelIntent::Inbox {
            action: InboxAction::Deliver {
                id: "in-then update docs".into(),
                delivery: InboxDelivery::Steer,
            },
            restore: false,
        })
    );
    let turn = state.active_turn().unwrap().clone();
    state.apply_interrupted(&turn, "", 100);
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .find(|row| row.role == "queued")
            .unwrap()
            .text,
        "1 queued · then update docs"
    );
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        promote.intent
    );
}

#[tokio::test]
async fn queue_accepts_workspace_commands_but_not_local_actions_or_shell() {
    let (mut state, mut inbox) = running("refuse").await;
    let mut catalog = snapshot();
    catalog.commands.push("review".into());
    state.apply_catalog(catalog);
    type_text(&mut state, "/review").await;
    state.handle_key(KeyAction::Leader).await;
    let (outcome, admitted) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert_eq!(outcome.note, None);
    assert_eq!(admitted, ("/review".into(), InboxDelivery::Queue));
    type_text(&mut state, "/quit").await;
    state.handle_key(KeyAction::Leader).await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        outcome.note.as_deref(),
        Some("This prompt cannot be queued")
    );
    assert_eq!(state.input(), "/quit");
    while let Ok(message) = inbox.try_recv() {
        assert!(matches!(message, InboxMsg::SessionSelection { .. }));
    }
}

#[tokio::test]
async fn queued_prompts_dialog_steers_deletes_and_undoes_into_the_composer() {
    let (mut state, _inbox) = running("dialog").await;
    let session = sid("dialog");
    let items = vec![
        InboxItem {
            id: "a".into(),
            delivery: InboxDelivery::Queue,
            text: "first queued".into(),
            images: 0,
            error: None,
        },
        InboxItem {
            id: "b".into(),
            delivery: InboxDelivery::Queue,
            text: "second queued".into(),
            images: 0,
            error: None,
        },
    ];
    state.apply_inbox(&session, items.clone());
    state.handle_key(KeyAction::Leader).await;
    let next = state
        .terminal_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
    state.handle_key(next).await;
    assert_eq!(state.panel(), &TuiPanel::QueuedPrompts);
    assert_eq!(state.modal_options().len(), 2);
    let steer = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        steer.intent,
        Some(PanelIntent::Inbox {
            action: InboxAction::Deliver {
                id: "a".into(),
                delivery: InboxDelivery::Steer,
            },
            restore: false,
        })
    );
    state.handle_panel_key(KeyAction::Down);
    let delete = state.handle_panel_key(KeyAction::DeleteOrQuit);
    assert_eq!(
        delete.intent,
        Some(PanelIntent::Inbox {
            action: InboxAction::Remove { id: "b".into() },
            restore: false,
        })
    );
    let undo = state.handle_panel_key(KeyAction::QueuedUndo);
    assert_eq!(
        undo.intent,
        Some(PanelIntent::Inbox {
            action: InboxAction::Remove { id: "b".into() },
            restore: true,
        })
    );
    state.restore_prompt("kept draft".into());
    let restored = state.prepare_inbox_restore("second queued".into()).unwrap();
    state.applied_inbox(
        &session,
        InboxSnapshot {
            items: vec![items[0].clone()],
            removed: Some(items[1].clone()),
            prompt: Some(restored),
        },
        true,
    );
    assert_eq!(state.input(), "kept draft\nsecond queued");
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn leader_q_without_queued_prompts_keeps_its_exit_binding() {
    let (mut state, _inbox) = running("exit").await;
    state.handle_key(KeyAction::Leader).await;
    let next = state
        .terminal_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
    state.handle_key(next).await;
    assert_ne!(state.panel(), &TuiPanel::QueuedPrompts);
}

#[tokio::test]
async fn rich_steer_queue_refusal_and_restoration_preserve_pngs_and_utf8_marks() {
    for delivery in [InboxDelivery::Steer, InboxDelivery::Queue] {
        let (mut state, mut inbox) = running("rich").await;
        type_text(&mut state, "Ω ").await;
        state.paste_clipboard_image(b"opaque-editor-png".as_slice().into());
        let prompt = state.editor.prompt(state.input()).trimmed();
        if delivery == InboxDelivery::Queue {
            state.handle_key(KeyAction::Leader).await;
        }
        let peer = async {
            let InboxMsg::Enqueue {
                text,
                selection,
                ack,
                ..
            } = inbox.recv().await.unwrap()
            else {
                panic!("rich enqueue")
            };
            assert_eq!(text, prompt);
            assert!(
                selection.is_some(),
                "captured composer selection travels in the same request"
            );
            ack.send(Err(CoreError::QueueFull)).unwrap();
        };
        let (outcome, ()) = tokio::join!(state.handle_key(KeyAction::Enter), peer);
        assert!(
            outcome.note.is_none(),
            "local acceptance does not wait for the owner"
        );
        assert!(state.input().is_empty());
        state.poll_submission();
        assert!(state.failed_submission_note().unwrap().contains("retained"));
        assert!(!state.has_pending_submission());
        state.handle_paste("next");
        assert!(
            state
                .recover_submission(true)
                .note
                .unwrap()
                .contains("Empty")
        );
        assert_eq!(state.input(), "next");
        state.restore_prompt(String::new());
        state.recover_submission(true);
        assert_eq!(
            state.editor.prompt(state.input()).trimmed(),
            prompt,
            "refusal retains full draft"
        );
        let original = state.editor.prompt(state.input());
        let restored = state.prepare_inbox_restore(prompt.clone()).unwrap();
        assert!(restored.text.starts_with(&original.text));
        assert_eq!(restored.images.len(), 2);
        assert_eq!(restored.images[0], original.images[0]);
        assert_eq!(restored.images[1].png, prompt.images[0].png);
        assert_eq!(
            &restored.text[restored.images[1].mark.start..restored.images[1].mark.end],
            "[Image 2]"
        );
        state.restore_prompt("x".repeat(oc_core::session::MAX_INPUT_BYTES));
        assert!(
            state.prepare_inbox_restore(prompt).is_err(),
            "overbudget restore is refused before withdrawal"
        );
    }
}

#[tokio::test]
async fn running_shell_admission_preserves_streaming_and_receipt_cancel_is_local() {
    let (mut state, mut inbox) = running("side-shell").await;
    let active = state.active_turn().cloned();
    state.handle_key(KeyAction::Char('!')).await;
    type_text(&mut state, "printf effect").await;
    assert!(state.handle_key(KeyAction::Enter).await.note.is_none());
    let InboxMsg::UserShell { cancel, ack, .. } = inbox.recv().await.unwrap() else {
        panic!("structured shell")
    };
    assert_eq!(state.status, TuiStatus::Streaming);
    state.handle_key(KeyAction::Cancel).await;
    assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(state.active_turn().cloned(), active);
    ack.send(Err(CoreError::Application(
        "cancelled shell admission".into(),
    )))
    .unwrap();
    state.poll_submission();
    assert_eq!(state.status, TuiStatus::Streaming);
    assert_eq!(state.active_turn().cloned(), active);
    assert_eq!(state.input(), "printf effect");
    assert!(
        inbox.try_recv().is_err(),
        "no model cancellation was issued"
    );
}
