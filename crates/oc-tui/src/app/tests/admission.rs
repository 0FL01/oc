use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{InboxAction, InboxDelivery, InboxItem, SessionSelectionAction};

fn stage(state: &mut TuiState, agent: &str) -> CatalogSnapshot {
    let mut preview = snapshot();
    preview.agent_id = Some(agent.into());
    state.apply_agent_preview(preview.clone()).unwrap();
    preview
}

fn select_recovery(state: &mut TuiState) {
    state.panel = TuiPanel::Settings;
    state.select.reset();
    state.select.cursor = state
        .modal_options()
        .iter()
        .position(|item| item.value == "submission-refusal")
        .expect("recovery row");
}

#[tokio::test]
async fn retained_refusal_allows_recovery_routes_without_replaying_or_releasing_capture() {
    for unknown in [false, true] {
        for command in ["/settings", "/reload", "/connect", "/accounts", "/exit"] {
            let (app, mut inbox, _) = CoreApp::channel(4);
            let mut state = TuiState::new(app, sid("repair"));
            state.apply_catalog(snapshot());
            state.handle_paste("captured request Ω");
            state.handle_key(KeyAction::Enter).await;
            let InboxMsg::Submit { text, ack, .. } = inbox.try_recv().unwrap() else {
                panic!("captured submit")
            };
            assert_eq!(text.text, "captured request Ω");
            state.handle_paste(command);
            state.handle_key(KeyAction::Cancel).await;
            let outstanding = state.handle_key(KeyAction::Enter).await;
            assert!(outstanding.intent.is_none());
            assert!(outstanding.note.unwrap().contains("pending"));
            assert_eq!(state.input(), command);
            assert!(
                inbox.try_recv().is_err(),
                "outstanding command cannot replay"
            );
            state.restore_prompt(String::new());
            if unknown {
                drop(ack);
            } else {
                ack.send(Err(CoreError::Application("refused".into())))
                    .unwrap();
            }
            state.poll_submission();
            assert!(!state.has_pending_submission());
            assert!(state.input().is_empty());
            state.handle_paste(command);
            state.handle_key(KeyAction::Cancel).await;
            let outcome = state.handle_key(KeyAction::Enter).await;
            assert!(outcome.note.is_none(), "{command}: {:?}", outcome.note);
            if command == "/settings" {
                assert_eq!(state.panel(), &TuiPanel::Settings);
                assert!(
                    state
                        .modal_options()
                        .iter()
                        .any(|row| row.value == "submission-refusal")
                );
            } else if command == "/reload" {
                assert_eq!(outcome.intent, Some(PanelIntent::ReloadConfiguration));
            } else if command == "/exit" {
                assert_eq!(state.status(), &TuiStatus::Quit);
            } else {
                assert_eq!(state.panel(), &TuiPanel::Accounts);
            }
            assert!(state.input().is_empty());
            let retained = state
                .pending
                .as_ref()
                .expect("repair does not release capture");
            assert_eq!(retained.prompt.text, text.text);
            assert_eq!(retained.draft.text, text.text);
            while let Ok(request) = inbox.try_recv() {
                assert!(
                    matches!(request, InboxMsg::PromptHistory { .. }),
                    "no replay or effect"
                );
            }
        }
    }
}

#[tokio::test]
async fn admission_prepares_agent_then_sends_exact_capture_without_clobbering_next_drafts() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("prepare"));
    state.apply_catalog(snapshot());
    let prepared = stage(&mut state, "y");
    state.draft_model("b").unwrap();
    state.draft_variant(Some("low")).unwrap();
    type_text(&mut state, "captured Ω").await;
    state.handle_key(KeyAction::Enter).await;
    assert!(state.input().is_empty());
    assert!(
        state.history().rows().is_empty(),
        "no optimistic canonical row"
    );
    let first_render = state.transcript_rows();
    let pending = first_render.last().unwrap();
    assert_eq!(pending.role, "pending");
    assert_eq!(pending.text, "captured Ω");
    assert_eq!(
        pending.text.as_ptr(),
        state.transcript_rows().last().unwrap().text.as_ptr(),
        "paints share immutable pending source"
    );
    let InboxMsg::SessionSelection {
        action: SessionSelectionAction::Agent(agent),
        ack,
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("prepare")
    };
    assert_eq!(agent, "y");
    assert!(
        inbox.try_recv().is_err(),
        "input waits for preparation ACK, not the UI"
    );
    stage(&mut state, "x");
    state.draft_model("a").unwrap();
    state.handle_paste("next editor");
    assert!(
        state
            .handle_key(KeyAction::Enter)
            .await
            .note
            .unwrap()
            .contains("pending")
    );
    ack.send(Ok(prepared)).unwrap();
    state.poll_submission();
    let InboxMsg::Submit {
        text,
        selection: Some(captured),
        ack,
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("prepared submit")
    };
    assert_eq!(text.text, "captured Ω");
    assert_eq!(captured.binding.agent_id.as_deref(), Some("y"));
    assert_eq!(captured.binding.provider, "ludka2");
    assert_eq!(captured.model_id, "b");
    assert_eq!(captured.variant.as_deref(), Some("low"));
    assert_eq!(state.composer_agent(), Some("x"));
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.input(), "next editor");
    assert_eq!(state.composer_agent(), Some("x"));
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    assert_eq!(state.history().rows()[0].text, "captured Ω");
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn admission_preparation_cancel_retains_receipt_and_never_sends_input() {
    let (app, mut inbox, _) = CoreApp::channel(1);
    let mut state = TuiState::new(app, sid("cancel-prepare"));
    state.apply_catalog(snapshot());
    let prepared = stage(&mut state, "y");
    type_text(&mut state, "not sent").await;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection { ack, .. } = inbox.try_recv().unwrap() else {
        panic!("prepare")
    };
    let outcome = state.handle_key(KeyAction::Cancel).await;
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::CancelSession {
            session: sid("cancel-prepare"),
            turn: None,
            cancel_compaction: false
        })
    );
    assert!(state.has_pending_submission());
    ack.send(Ok(prepared)).unwrap();
    state.poll_submission();
    assert!(!state.has_pending_submission());
    assert!(state.has_occupied_submission());
    assert!(
        state
            .failed_submission_note()
            .unwrap()
            .contains("cancelled before input")
    );
    assert!(inbox.try_recv().is_err());
    state.recover_submission(true);
    assert_eq!(state.input(), "not sent");
}

#[tokio::test]
async fn admission_canonical_refresh_carries_preparation_and_its_exact_revision() {
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new(app.clone(), sid("refresh-prepare"));
    state.apply_catalog(snapshot());
    let prepared = stage(&mut state, "y");
    type_text(&mut state, "captured").await;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection { ack, .. } = inbox.try_recv().unwrap() else {
        panic!("prepare")
    };
    state.handle_paste("next");
    let identity = state.view_identity();
    let mut refreshed = TuiState::new(app, sid("refresh-prepare"));
    refreshed.apply_catalog(snapshot());
    refreshed.preserve_shared_composer(state);
    assert_eq!(refreshed.view_identity(), identity);
    assert!(refreshed.has_pending_submission());
    ack.send(Ok(prepared)).unwrap();
    refreshed.poll_submission();
    assert_eq!(refreshed.staged_agent(), None);
    let InboxMsg::Submit { text, ack, .. } = inbox.try_recv().unwrap() else {
        panic!("captured submit")
    };
    assert_eq!(text.text, "captured");
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    refreshed.poll_submission();
    assert_eq!(refreshed.input(), "next");
}

#[tokio::test]
async fn admission_preparation_unknown_or_followup_full_retains_capture_without_retry() {
    for unknown in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(1);
        let mut state = TuiState::new(app, sid("prepare-failure"));
        state.apply_catalog(snapshot());
        let prepared = stage(&mut state, "y");
        type_text(&mut state, "retained").await;
        state.handle_key(KeyAction::Enter).await;
        let InboxMsg::SessionSelection { ack, .. } = inbox.try_recv().unwrap() else {
            panic!("prepare")
        };
        let occupied = if unknown {
            drop(ack);
            None
        } else {
            ack.send(Ok(prepared)).unwrap();
            Some(
                state
                    .app
                    .request_submit(sid("occupy"), "other request".into())
                    .unwrap(),
            )
        };
        state.poll_submission();
        state.poll_submission();
        assert!(!state.has_pending_submission());
        assert!(state.has_occupied_submission());
        assert_eq!(state.pending.as_ref().unwrap().prompt.text, "retained");
        if unknown {
            assert!(state.failed_submission_note().unwrap().contains("unknown"));
        } else {
            let InboxMsg::Submit { session, .. } = inbox.try_recv().unwrap() else {
                panic!("only the unrelated occupying request")
            };
            assert_eq!(session, sid("occupy"));
        }
        assert!(
            inbox.try_recv().is_err(),
            "no retry or input after failed preparation"
        );
        drop(occupied);
        state.recover_submission(true);
        assert_eq!(state.input(), "retained");
    }
}

#[tokio::test]
async fn admission_preparation_keeps_explicit_provider_and_rejection_never_sends_input() {
    for reject in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(2);
        let mut state = TuiState::new(app, sid("provider-prepare"));
        state.apply_catalog(snapshot());
        let mut preview = snapshot();
        preview.agent_id = Some("y".into());
        preview.provider = "explicit-other-provider".into();
        state.apply_agent_preview(preview).unwrap();
        state.draft_model("b").unwrap();
        type_text(&mut state, "exact provider").await;
        state.handle_key(KeyAction::Enter).await;
        let InboxMsg::SessionSelection { ack, .. } = inbox.try_recv().unwrap() else {
            panic!("prepare")
        };
        if reject {
            ack.send(Err(CoreError::TurnBusy)).unwrap();
            state.poll_submission();
            assert!(inbox.try_recv().is_err());
            assert!(state.has_occupied_submission() && !state.has_pending_submission());
            state.recover_submission(true);
            assert_eq!(state.input(), "exact provider");
        } else {
            let mut prepared = snapshot();
            prepared.agent_id = Some("y".into());
            ack.send(Ok(prepared)).unwrap();
            state.poll_submission();
            let InboxMsg::Submit {
                selection: Some(captured),
                ..
            } = inbox.try_recv().unwrap()
            else {
                panic!("submit")
            };
            assert_eq!(captured.binding.provider, "explicit-other-provider");
            assert_eq!(captured.binding.agent_id.as_deref(), Some("y"));
            assert_eq!(captured.model_id, "b");
        }
    }
}

#[tokio::test]
async fn admission_unknown_fresh_retains_route_and_rich_editor_until_explicit_empty_restore() {
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new_home(app);
    type_text(&mut state, "Ω ").await;
    state.paste_clipboard_image(b"retained-png".as_slice().into());
    let original = state.editor.prompt(state.input());
    let original_caret = state.editor.cursor;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SubmitFresh { session, ack, .. } = inbox.try_recv().unwrap() else {
        panic!("fresh")
    };
    assert_eq!(state.submission_session(), Some(&session));
    drop(ack);
    state.poll_submission();
    assert_eq!(state.attached_session(), None);
    assert_eq!(state.submission_session(), Some(&session));
    assert!(!state.has_pending_submission() && !state.is_busy());
    assert!(
        state
            .failed_submission_note()
            .unwrap()
            .contains("may already be accepted")
    );
    assert!(state.history().rows().is_empty());
    state.handle_paste(" ");
    select_recovery(&mut state);
    let restore = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert_eq!(state.terminal_key(restore), Some(KeyAction::QueuedUndo));
    assert!(
        state
            .handle_panel_key(KeyAction::QueuedUndo)
            .note
            .unwrap()
            .contains("Empty")
    );
    assert_eq!(
        state.input(),
        " ",
        "even whitespace is a next draft, never overwritten"
    );
    assert_eq!(state.submission_session(), Some(&session));
    state.restore_prompt(String::new());
    state.handle_panel_key(KeyAction::QueuedUndo);
    assert_eq!(state.editor.prompt(state.input()), original);
    assert_eq!(state.editor.cursor, original_caret);
    state.handle_key(KeyAction::Undo).await;
    assert!(
        state.editor.prompt(state.input()).images.is_empty(),
        "restored rich draft retains its undo stack"
    );
    state.handle_key(KeyAction::Redo).await;
    assert_eq!(state.editor.prompt(state.input()), original);
    assert_eq!(state.submission_session(), None);
    assert_eq!(
        state.tab_route_session(),
        Some(&session),
        "recovery does not retire the provisional tab route"
    );
    assert!(inbox.try_recv().is_err(), "restore never resends");
}

#[tokio::test]
async fn admission_busy_unknown_has_no_fallback_and_discard_keeps_next_editor() {
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new(app, sid("busy-unknown"));
    state.begin_linked_turn(WorkerTurnId("running".into()));
    type_text(&mut state, "once").await;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Enqueue { ack, delivery, .. } = inbox.try_recv().unwrap() else {
        panic!("inbox")
    };
    assert_eq!(delivery, InboxDelivery::Steer);
    state.handle_paste("next");
    drop(ack);
    state.poll_submission();
    state.poll_submission();
    assert!(!state.has_pending_submission());
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert!(inbox.try_recv().is_err(), "unknown is never TurnNotActive");
    select_recovery(&mut state);
    state.handle_panel_key(KeyAction::DeleteOrQuit);
    assert_eq!(state.input(), "next");
    assert!(!state.has_occupied_submission());
}

#[tokio::test]
async fn admission_fallback_cancel_targets_pending_admission_not_stale_painted_turn() {
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new(app, sid("fallback-cancel"));
    state.begin_linked_turn(WorkerTurnId("old-painted".into()));
    type_text(&mut state, "captured").await;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Enqueue { ack, .. } = inbox.try_recv().unwrap() else {
        panic!("inbox")
    };
    ack.send(Err(CoreError::TurnNotActive)).unwrap();
    state.poll_submission();
    let InboxMsg::Submit { ack, .. } = inbox.try_recv().unwrap() else {
        panic!("fallback")
    };
    state.handle_paste("next editor");
    assert!(state.handle_key(KeyAction::Cancel).await.intent.is_none());
    assert_eq!(
        state.handle_key(KeyAction::Cancel).await.intent,
        Some(PanelIntent::CancelSession {
            session: sid("fallback-cancel"),
            turn: None,
            cancel_compaction: false,
        })
    );
    assert!(state.has_pending_submission(), "original receipt retained");
    ack.send(Ok(WorkerTurnId("late-new".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.input(), "next editor");
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("late-new".into())));
    assert!(inbox.try_recv().is_err(), "cancel does not replay input");
}

#[tokio::test]
async fn admission_initial_full_queue_keeps_editor_and_blank_preparation_defers_exact_promotion() {
    let (app, mut inbox, _) = CoreApp::channel(1);
    let mut state = TuiState::new(app, sid("bounded"));
    state.apply_catalog(snapshot());
    let _occupy = state
        .app
        .request_submit(sid("elsewhere"), "full".into())
        .unwrap();
    type_text(&mut state, "kept locally").await;
    assert!(state.handle_key(KeyAction::Enter).await.note.is_some());
    assert_eq!(state.input(), "kept locally");
    assert!(!state.has_occupied_submission());
    drop(inbox.try_recv().unwrap());
    state.restore_prompt(String::new());
    let prepared = stage(&mut state, "y");
    state.draft_model("b").unwrap();
    state.apply_inbox(
        &sid("bounded"),
        vec![InboxItem {
            id: "captured-item".into(),
            delivery: InboxDelivery::Queue,
            text: "queued".into(),
            images: 0,
            error: None,
        }],
    );
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    let InboxMsg::SessionSelection {
        action: SessionSelectionAction::Agent(_),
        ack,
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("prepare empty")
    };
    state.apply_inbox(&sid("bounded"), Vec::new());
    state.draft_model("a").unwrap();
    ack.send(Ok(prepared)).unwrap();
    // Internal key reconciliation may run before the controller tick. It must
    // not consume/drop the deferred intent while handling independent editing.
    state.handle_key(KeyAction::Char('n')).await;
    let intent = state.poll_submission();
    assert_eq!(
        intent,
        Some(PanelIntent::Inbox {
            action: InboxAction::Deliver {
                id: "captured-item".into(),
                delivery: InboxDelivery::Steer
            },
            restore: false
        })
    );
    assert_eq!(state.poll_submission(), None, "intent returned once");
    let InboxMsg::SessionSelection {
        action: SessionSelectionAction::Commit(captured),
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("captured blank model")
    };
    assert_eq!(captured.model_id, "b");
    assert_eq!(captured.binding.agent_id.as_deref(), Some("y"));
    assert_eq!(state.captured_model_commit().unwrap().model_id, "a");
    assert_eq!(state.input(), "n");
}

#[tokio::test]
async fn admission_cancel_suppresses_a_prepared_but_not_dispatched_blank_promotion() {
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new(app, sid("cancel-promotion"));
    state.apply_catalog(snapshot());
    let prepared = stage(&mut state, "y");
    state.apply_inbox(
        &sid("cancel-promotion"),
        vec![InboxItem {
            id: "still-queued".into(),
            delivery: InboxDelivery::Queue,
            text: "not dispatched".into(),
            images: 0,
            error: None,
        }],
    );
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection { ack, .. } = inbox.try_recv().unwrap() else {
        panic!("prepare")
    };
    ack.send(Ok(prepared)).unwrap();
    let cancel = state.handle_key(KeyAction::Cancel).await;
    assert!(matches!(
        cancel.intent,
        Some(PanelIntent::CancelSession { .. })
    ));
    assert_eq!(state.poll_submission(), None);
    assert_eq!(state.inbox_items()[0].id, "still-queued");
    let InboxMsg::SessionSelection {
        action: SessionSelectionAction::Commit(_),
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("only the captured model commit was already admitted")
    };
    assert!(inbox.try_recv().is_err());
}
