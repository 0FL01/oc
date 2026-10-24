use super::*;

#[tokio::test]
async fn mcp_label_tracks_pending_linked_started_and_failed_without_hiding_raw() {
    use oc_core::{
        tool_output::{McpLabel, Presentation},
        tool_stream::{ToolStreamEvent, ToolStreamIdentity},
    };
    let mut state = fresh_state("mcp-label").await;
    let turn = WorkerTurnId("captured-turn".into());
    state.begin_linked_turn(turn.clone());
    state.restore_prompt("unchanged draft Ω界".into());
    let facts = Box::new(Presentation::label(McpLabel {
        text: "vis16_output".into(),
        limited: false,
    }));
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "item".into(),
        call_id: "call".into(),
    };
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            output_presentation: Some(facts.clone()),
            identity: identity.clone(),
            name: "mcp_hash_2".into(),
            preview: "{unfinished".into(),
            truncated: false,
        },
    );
    assert!(state.viewport().join("\n").contains("vis16_output"));
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Linked {
            identity,
            op: "real-operation".into(),
        },
    );
    state.apply_tool_started_with_output_presentation(
        &turn,
        "real-operation",
        "mcp_hash_2",
        "{}",
        None,
        Some(facts.clone()),
    );
    assert_eq!(state.live_parts.len(), 1);
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "real-operation",
        "mcp_hash_2",
        "failed",
        "error: actual failed outcome",
        28,
        false,
        None,
        None,
        None,
        None,
    );
    let LivePart::Tool { card, input, .. } = &state.live_parts[0] else {
        panic!("card")
    };
    assert_eq!(card.name, "mcp_hash_2");
    assert_eq!(card.display_name(), "vis16_output");
    assert_eq!(card.output_preview, "error: actual failed outcome");
    assert_eq!(card.input_preview, "{}");
    assert!(
        input.is_empty(),
        "finished input moves to the existing bounded card projection"
    );
    assert_eq!(state.input(), "unchanged draft Ω界");
    state.apply_tool_started_with_output_presentation(
        &turn,
        "budgeted-operation",
        "mcp_hash_2",
        "{}",
        None,
        Some(facts),
    );
    let body = "bounded prepared body";
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "budgeted-operation",
        "mcp_hash_2",
        "completed",
        body,
        body.len() as i64,
        false,
        None,
        None,
        None,
        Some(Box::new(Presentation::new(body, body.len() as u64, false))),
    );
    let LivePart::Tool { card, .. } = state.live_parts.last().unwrap() else {
        panic!("card")
    };
    assert_eq!(
        card.display_name(),
        "mcp_hash_2",
        "explicit budget omission is not undone by the UI"
    );
    assert_eq!(card.output_preview, body);
    assert!(
        card.output_presentation
            .as_ref()
            .unwrap()
            .mcp_label
            .is_none()
    );
    assert_eq!(state.input(), "unchanged draft Ω界");
}

#[tokio::test]
async fn model_shell_snapshot_requires_the_current_turn_for_retained_live_parts() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = sid("orphaned-live-model");
    let mut state = TuiState::new(app, session.clone());
    let turn = WorkerTurnId("original-model-turn".into());
    state.begin_linked_turn(turn.clone());
    state.apply_tool_started(
        &turn,
        "known-operation",
        "shell",
        r#"{"command":"original command"}"#,
    );
    state.restore_prompt("unchanged draft 界".into());
    // A retired worker is not authority to update any still-retained live part.
    state.active_turn = None;
    let before = state.transcript_rows();
    state.apply_model_shell_snapshot(&oc_core::queries::ShellSnapshot {
        job: oc_core::queries::ShellJob {
            session,
            shell_id: "known-operation".into(),
            location: "/original".into(),
            generation: 7,
            turn: turn.0,
            model: "fixture/model".into(),
            provider: "fixture".into(),
            command: "original command".into(),
            pid: None,
            background: true,
            output: None,
        },
        state: "completed".into(),
        stdout_cursor: 10,
        stderr_cursor: 0,
        truncated: false,
        text: "legacy envelope".into(),
        display: "must not replace an orphaned live part".into(),
        display_omitted: false,
        exit: Some(7),
        signal: None,
    });
    assert_eq!(state.transcript_rows(), before);
    assert_eq!(state.input(), "unchanged draft 界");
    assert!(state.active_turn().is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn scripted_notice_after_submit_refusal_refreshes_idle_tail_without_raw_prose() {
    use oc_core::{
        core_app::{CoreEvent, InboxMsg},
        queries::{
            ChildHistory, ChildJob, ChildNotice, ChildState, HistoryMessage, ShellHistoryNotice,
            ShellNotice, UserShellResult,
        },
        session::{MessageId, Role},
    };
    for kind in 0..3 {
        let (app, mut inbox, events) = CoreApp::channel(8);
        let session = sid("script-notice");
        let mut state = TuiState::new(app.clone(), session.clone());
        let old = HistoryMessage {
            images: Vec::new(),
            id: MessageId("older-message".into()),
            seq: 1,
            role: Role::Assistant,
            text: "older history".into(),
            turn: None,
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
        };
        state.attach_page(&HistoryPage {
            rows: vec![old.clone()],
            total: 1,
            ..Default::default()
        });
        state.restore_prompt("draft retained after refusal".into());
        state.handle_key(KeyAction::Enter).await;
        let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("real pending submission");
        };
        let mut driver = ScriptDriver::attach(&app);
        ack.send(Err(CoreError::Application("refused".into())))
            .unwrap();
        let mut row = HistoryMessage {
            images: Vec::new(),
            id: MessageId("actual-notice".into()),
            seq: 3,
            role: Role::User,
            text: "technical RAW envelope must never become a toast".into(),
            turn: None,
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
        };
        let event = if kind == 1 {
            let job = ChildJob {
                parent: session.clone(),
                child: sid("child"),
                operation: "child-operation".into(),
                generation: 7,
                location: "/original".into(),
                agent: "helper".into(),
                model: "fixture/model".into(),
                description: "captured task".into(),
                delivery_id: "actual-delivery".into(),
                state: ChildState::Completed,
                background: true,
                turn: Some("child-turn".into()),
                result: None,
                message_id: Some(row.id.0.clone()),
            };
            row.child = Some(ChildHistory::Notice(Box::new(job.clone())));
            CoreEvent::ChildNotice(ChildNotice {
                job,
                text: row.text.clone(),
            })
        } else {
            if kind == 0 {
                row.shell_notice = Some(ShellHistoryNotice {
                    operation: "actual-operation".into(),
                    state: "completed".into(),
                    command: "printf output".into(),
                });
            } else {
                row.user_shell = Some(UserShellResult {
                    input: false,
                    superseded_input: false,
                    operation: "actual-operation".into(),
                    command: "printf output".into(),
                    command_limited: false,
                    state: "completed".into(),
                    output: Box::new(oc_core::tool_output::Presentation::new(
                        "process output",
                        14,
                        false,
                    )),
                    diagnostic: None,
                });
            }
            CoreEvent::ShellNotice(ShellNotice {
                session: session.clone(),
                shell_id: "actual-operation".into(),
                delivery_id: "actual-delivery".into(),
                message_id: row.id.0.clone(),
                user_requested: kind == 2,
                state: "completed".into(),
                text: row.text.clone(),
            })
        };
        events.send(event).unwrap();
        let owner = tokio::spawn(async move {
            let InboxMsg::History {
                session: requested,
                message,
                limit,
                ack,
                ..
            } = inbox.recv().await.unwrap()
            else {
                panic!("exact notice lookup");
            };
            assert_eq!(requested, session);
            assert_eq!(message, Some(row.id.clone()));
            assert_eq!(limit, 1);
            ack.send(Ok(HistoryPage {
                rows: vec![row.clone()],
                total: 2,
                ..Default::default()
            }))
            .unwrap();
            let InboxMsg::History {
                session: requested,
                message,
                limit,
                ack,
                ..
            } = inbox.recv().await.unwrap()
            else {
                panic!("idle completed tail, not a disjoint selective append");
            };
            assert_eq!(requested, session);
            assert!(message.is_none());
            assert_eq!(limit, 100);
            ack.send(Ok(HistoryPage {
                rows: vec![old, row],
                total: 2,
                ..Default::default()
            }))
            .unwrap();
            inbox
        });
        assert_eq!(
            driver
                .pump_until_idle(&mut state, Duration::from_secs(1))
                .await,
            PumpOutcome::Idle
        );
        let mut inbox = owner.await.unwrap();
        assert!(state.input().is_empty());
        let retained = state.pending.as_ref().expect("retained refusal");
        assert_eq!(retained.prompt.text, "draft retained after refusal");
        assert_eq!(retained.draft.text, "draft retained after refusal");
        let text = state
            .transcript_lines(120, 120)
            .iter()
            .map(crate::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("older history"));
        assert!(
            text.contains(if kind == 0 {
                "↳ Shell finished"
            } else if kind == 1 {
                "↳ Helper finished"
            } else {
                "$ printf output"
            }),
            "{text}"
        );
        assert!(!text.contains("technical RAW"));
        assert!(
            !state
                .note()
                .is_some_and(|note| note.contains("technical RAW"))
        );
        assert!(inbox.try_recv().is_err(), "no effect/provider/query replay");
    }
}

#[tokio::test]
async fn vis28_scanner_clock_rolls_over_and_resets_for_each_running_transition() {
    let mut state = fresh_state("scanner-clock").await;
    let at = Instant::now();
    assert!(!state.tick_scanner(at));
    let first = WorkerTurnId("first".into());
    state.begin_compress_turn(first.clone());
    assert!(!state.tick_scanner(at));
    assert!(!state.tick_scanner(at + Duration::from_millis(39)));
    assert_eq!(state.scanner_frame(), 0);
    assert!(state.tick_scanner(at + Duration::from_millis(41)));
    assert_eq!(state.scanner_frame(), 1);
    assert!(!state.tick_scanner(at + Duration::from_millis(79)));
    assert!(state.tick_scanner(at + Duration::from_millis(80)));
    assert_eq!(state.scanner_frame(), 2);
    assert!(state.tick_scanner(at + Duration::from_millis(53 * 40)));
    assert_eq!(state.scanner_frame(), 53);
    assert!(state.tick_scanner(at + Duration::from_millis(54 * 40)));
    assert_eq!(state.scanner_frame(), 0);
    state.apply_interrupted(&first, "partial", 1);
    assert_eq!(state.scanner_frame(), 0);
    assert!(!state.tick_scanner(at + Duration::from_secs(20)));

    let second = WorkerTurnId("second".into());
    state.begin_compress_turn(second.clone());
    assert!(!state.tick_scanner(at + Duration::from_secs(20)));
    assert!(state.tick_scanner(at + Duration::from_secs(20) + Duration::from_millis(40)));
    assert_eq!(state.scanner_frame(), 1);
    state.apply_finished(&second, "done", 0);
    assert_eq!(state.scanner_frame(), 0);
    assert!(!state.tick_scanner(at + Duration::from_secs(30)));
}

#[tokio::test]
async fn transient_feedback_defaults_to_five_seconds_and_reload_hover_pauses() {
    let mut state = fresh_state("toast-timer").await;
    let start = Instant::now();
    state.push_note("legacy warning");
    state.tick_toast(start + Duration::from_secs(60));
    assert_eq!(state.note(), Some("legacy warning"));

    state.push_transient_note_at(
        "Reloading",
        NoteVariant::Info,
        Duration::from_secs(30),
        start,
    );
    state.tick_toast(start + Duration::from_secs(29));
    assert_eq!(state.note(), Some("Reloading"));
    state.set_toast_hover(true, start + Duration::from_secs(29));
    state.tick_toast(start + Duration::from_secs(90));
    assert_eq!(state.note(), Some("Reloading"));
    state.set_toast_hover(false, start + Duration::from_secs(90));
    state.tick_toast(start + Duration::from_secs(91));
    assert_eq!(state.note(), None);

    state.push_transient_note_at("Done", NoteVariant::Success, Duration::from_secs(5), start);
    state.tick_toast(start + Duration::from_secs(4));
    assert_eq!(state.note(), Some("Done"));
    state.tick_toast(start + Duration::from_secs(5));
    assert_eq!(state.note(), None);
    state.push_transient_note_at(
        "Copied to clipboard",
        NoteVariant::Info,
        Duration::from_secs(5),
        start,
    );
    state.tick_toast(start + Duration::from_secs(5));
    assert_eq!(state.note(), None);
    state.push_transient_note_at("Failed", NoteVariant::Error, Duration::from_secs(5), start);
    state.push_note("other warning");
    state.tick_toast(start + Duration::from_secs(60));
    assert_eq!(state.note(), Some("other warning"));
}

#[tokio::test]
async fn fresh_rejection_preserves_home_draft_and_retry_uses_new_candidate() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    type_text(&mut state, "retry me").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh {
        session: first,
        text,
        selection,
        ack,
    }) = inbox.recv().await
    else {
        panic!("fresh submit")
    };
    assert_eq!(text.text, "retry me");
    assert!(
        selection.is_none(),
        "the application owns the Home selection"
    );
    assert!(state.attached_session().is_none());
    assert!(state.history().rows().is_empty());
    state.handle_key(KeyAction::Enter).await;
    assert!(inbox.try_recv().is_err(), "no duplicate while pending");
    ack.send(Err(CoreError::Application("refused".into())))
        .unwrap();
    state.poll_submission();
    assert!(state.home);
    assert!(state.attached_session().is_none());
    assert!(state.input().is_empty());
    assert!(!state.has_pending_submission());
    assert!(state.has_occupied_submission());
    assert_eq!(state.submission_session(), Some(&first));
    assert!(state.history().rows().is_empty());
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(state.handle_key(KeyAction::Enter).await.note.is_some());
    assert!(
        inbox.try_recv().is_err(),
        "a failed draft is not implicitly resent"
    );
    state.recover_submission(true);
    assert_eq!(state.input(), "retry me");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh {
        session: retry,
        ack,
        ..
    }) = inbox.recv().await
    else {
        panic!("fresh retry")
    };
    assert_ne!(first, retry);
    state.set_session(sid("switched"));
    assert!(ack.send(Ok(WorkerTurnId("stale".into()))).is_err());
    state.poll_submission();
    assert!(state.history().rows().is_empty());
}

#[tokio::test]
async fn mock_busy_rejection_creates_no_fresh_root() {
    let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["slow".into(); 20], 100));
    let existing = sid("busy-existing");
    app.create_session(existing.clone()).await.unwrap();
    app.try_submit(existing.clone(), "occupy worker".into())
        .await
        .unwrap();
    let mut state = TuiState::new_home(app.clone());
    type_text(&mut state, "retry later").await;
    state.handle_key(KeyAction::Enter).await;
    await_submission(&mut state).await;
    assert!(state.home);
    assert_eq!(state.attached_session(), None);
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(state.input().is_empty());
    assert!(state.has_occupied_submission());
    state.recover_submission(true);
    assert_eq!(state.input(), "retry later");
    assert!(state.history().rows().is_empty());
    assert_eq!(app.list_sessions().await.unwrap(), vec![existing.clone()]);
    app.cancel(existing).await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn mock_fresh_acceptance_binds_only_on_receipt_and_preserves_pending_edit() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    let mut driver = ScriptDriver::attach(&app);
    let mut state = TuiState::new_home(app.clone());
    type_text(&mut state, "first prompt").await;
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(state.status(), &TuiStatus::PendingSubmission);
    assert!(state.home);
    assert!(state.attached_session().is_none());
    // Paste does not poll the receipt, so this edit deterministically
    // precedes reconciliation even if the mock owner already accepted.
    state.handle_paste(" edited");
    await_submission(&mut state).await;
    let accepted = state.attached_session().expect("accepted ID").clone();
    assert!(!state.home);
    assert_eq!(state.input(), " edited");
    assert_eq!(state.history().rows()[0].text, "first prompt");
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert_eq!(app.list_sessions().await.unwrap(), vec![accepted.clone()]);
    assert_eq!(
        driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await,
        PumpOutcome::Finished("echo: first prompt".into())
    );
    assert_eq!(state.attached_session(), Some(&accepted));
    state.handle_key(KeyAction::Enter).await;
    await_submission(&mut state).await;
    assert_eq!(state.attached_session(), Some(&accepted));
    assert_eq!(
        state
            .history()
            .rows()
            .iter()
            .filter(|row| row.role == "user")
            .count(),
        2
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn fresh_cancel_pending_uses_candidate_and_keeps_draft_on_late_accept() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    type_text(&mut state, "cancel me").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh {
        session: candidate,
        ack: submit_ack,
        ..
    }) = inbox.recv().await
    else {
        panic!("fresh request")
    };
    assert!(state.attached_session().is_none());
    state.handle_paste("next draft");
    let cancel = state.handle_key(KeyAction::Cancel).await;
    assert_eq!(
        cancel.intent,
        Some(PanelIntent::CancelSession {
            session: candidate.clone(),
            turn: None,
            cancel_compaction: false,
        })
    );
    assert!(
        state.has_pending_submission(),
        "cancel intent must not consume original receipt"
    );
    assert!(
        inbox.try_recv().is_err(),
        "priority cancellation is delegated, never queued as work"
    );
    submit_ack
        .send(Ok(WorkerTurnId("accepted-after-cancel".into())))
        .unwrap();
    await_submission(&mut state).await;
    assert_eq!(state.attached_session(), Some(&candidate));
    assert_eq!(
        state.input(),
        "next draft",
        "late acceptance after cancel cannot clear the next editor"
    );
    assert!(!state.home);
    assert_eq!(state.history().rows()[0].text, "cancel me");
}

#[tokio::test]
async fn pending_receipt_preserves_edits_and_ignores_old_generation() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("pending"));
    type_text(&mut state, "original").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    assert_eq!(text.text, "original");
    state.handle_key(KeyAction::Enter).await;
    assert!(inbox.try_recv().is_err(), "duplicate not enqueued");
    type_text(&mut state, " edited").await;
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(
        state.input(),
        " edited",
        "acceptance cannot clear later edits"
    );
    assert_eq!(state.history().rows()[0].text, "original");
    state.apply_finished(&WorkerTurnId("accepted".into()), "done", 0);
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    // Exercise stale receipt defense even if a caller violates the normal
    // switch-refused-while-busy gate; A→B→A also changes the generation.
    state.set_session(sid("other"));
    state.reset_workspace();
    state.set_session(sid("pending"));
    type_text(&mut state, "new generation draft").await;
    assert!(
        ack.send(Ok(WorkerTurnId("old".into()))).is_err(),
        "old receipt invalidated"
    );
    state.poll_submission();
    assert!(!state.is_busy());
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("new generation submission")
    };
    type_text(&mut state, " edited after enqueue").await;
    ack.send(Ok(WorkerTurnId("new".into()))).unwrap();
    state.poll_submission();
    state.apply_delta(&WorkerTurnId("old".into()), "stale text");
    state.apply_tool_started(&WorkerTurnId("old".into()), "old-op", "read", "{}");
    state.apply_finished(&WorkerTurnId("old".into()), "stale answer", 0);
    state.apply_interrupted(&WorkerTurnId("old".into()), "stale partial", 0);
    state.apply_failed(
        &WorkerTurnId("old".into()),
        &CoreError::Application("stale failure".into()),
    );
    assert_eq!(state.input(), " edited after enqueue");
    assert_eq!(state.history().rows().len(), 1);
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("new".into())));
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert!(state.live_parts.is_empty());
}

#[tokio::test]
async fn workspace_reset_invalidates_pending_state_without_waiting_for_old_receipt() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("pending"));
    type_text(&mut state, "retained draft").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    state.handle_paste("next draft");
    state.reset_workspace();
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(!state.is_busy());
    assert_eq!(state.input(), "next draft");
    assert!(ack.send(Ok(WorkerTurnId("old".into()))).is_err());
    state.poll_submission();
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "next draft");
}

#[tokio::test]
async fn pending_failure_keeps_draft_and_quit_is_not_overwritten() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("pending"));
    type_text(&mut state, "retry me").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    ack.send(Err(CoreError::Application("safe failure".into())))
        .unwrap();
    state.poll_submission();
    assert!(state.input().is_empty());
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.recover_submission(true);
    assert_eq!(state.input(), "retry me");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("retry")
    };
    state.handle_key(KeyAction::Quit).await;
    ack.send(Ok(WorkerTurnId("accepted-at-quit".into())))
        .unwrap();
    state.poll_submission();
    assert_eq!(state.status(), &TuiStatus::Quit);
    assert!(state.input().is_empty());
}

#[tokio::test]
async fn accepted_submit_clears_input_and_streams() {
    let mut state = fresh_state("s-sub").await;
    let mut driver = ScriptDriver::attach(&state.app);

    type_text(&mut state, "hi").await;
    assert_eq!(state.input(), "hi");
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert!(!outcome.consumed_input);
    assert_eq!(outcome.note, None);
    assert_eq!(outcome.intent, None);
    assert_eq!(state.status(), &TuiStatus::PendingSubmission);
    assert!(
        state.input().is_empty(),
        "local acceptance moves the submitted draft immediately"
    );
    await_submission(&mut state).await;
    assert!(state.input().is_empty());
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert!(state.is_busy());
    // Upstream user block: `┃` border plus 2-cell inner padding.
    assert!(state.viewport().iter().any(|line| line == "┃  hi"));

    let outcome = driver
        .pump_until_idle(&mut state, Duration::from_secs(5))
        .await;
    assert_eq!(outcome, PumpOutcome::Finished("echo: hi".to_string()));
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(!state.is_busy());
    let view = state.viewport();
    assert!(view.iter().any(|line| line == "┃  hi"), "{view:?}");
    // Assistant markdown sits at paddingLeft=3.
    assert!(view.iter().any(|line| line == "   echo: hi"), "{view:?}");
}

#[tokio::test]
async fn clipped_hover_draw_uses_only_its_visible_indexed_traversal() {
    use crossterm::event::MouseEventKind;
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    use ratatui::{Terminal, backend::TestBackend};

    let mut state = fresh_state("clipped-thought-hover").await;
    let mut message = msg(9, Role::Assistant, "");
    message.turn = Some(HistoryTurn {
        parts: vec![TranscriptPart::Reasoning {
            text: "**Tracing**\n\nbody".into(),
            duration_ms: None,
        }],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let frame = (11..140)
        .map(|width| Rect::new(0, 0, width, 24))
        .find(|frame| crate::shell::transcript_area(&state, *frame).width == 11)
        .expect("a terminal width with an 11-cell transcript");
    let rect = crate::shell::transcript_area(&state, frame);
    let (rows, _, _) = state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
    let row = rows
        .iter()
        .position(|line| line.plain_text() == "   + Though")
        .expect("clipped header is painted");
    let y = rect.y + row as u16;
    let x = rect.x + 5;
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    let mut draw = |state: &TuiState| {
        let before = crate::messages::indexed_traversals();
        terminal
            .draw(|frame| crate::shell::render(frame, state))
            .unwrap();
        (
            crate::messages::indexed_traversals() - before,
            terminal.backend().buffer()[(x, y)].fg,
        )
    };
    let faded = ratatui::style::Color::Rgb(0x97, 0x68, 0x2c);
    let bright = super::Theme::dark().warning();
    let (normal_work, normal_color) = draw(&state);
    assert_eq!(normal_work, 0, "the measured viewport is already warm");
    assert_eq!(normal_color, faded);
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    assert_eq!(
        draw(&state),
        (0, bright),
        "clipped hover reuses the warm viewport"
    );
    assert_eq!(
        draw(&state),
        (0, bright),
        "stationary hover reuses the warm viewport"
    );
    state.live_text = "streaming frame".into();
    assert_eq!(
        draw(&state),
        (1, bright),
        "streaming hover keeps the clipped header and re-indexes once"
    );
    state.live_text.clear();
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, rect.x + 2, y), frame);
    assert_eq!(draw(&state), (1, faded), "padding is not a hit");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Moved, rect.x + 10, y),
        frame,
    );
    assert_eq!(
        draw(&state),
        (0, bright),
        "last visible cell reuses the warm viewport"
    );
}

#[tokio::test]
async fn vis31_deadlines_do_not_reset_scanner_and_disappear_at_rest() {
    let mut state = fresh_state("deadline").await;
    let now = std::time::Instant::now();
    assert_eq!(state.next_ui_deadline(), None);
    assert!(!state.tick_ui(now));
    state.status = super::TuiStatus::Streaming;
    assert!(!state.tick_ui(now));
    let frame = std::time::Duration::from_millis(crate::scanner::FRAME_MS);
    assert_eq!(state.next_ui_deadline(), Some(now + frame));
    // Faster unrelated paints never postpone or restart the animation.
    for millis in 1..40 {
        assert!(!state.tick_ui(now + std::time::Duration::from_millis(millis)));
        assert_eq!(state.next_ui_deadline(), Some(now + frame));
    }
    assert!(state.tick_ui(now + frame));
    assert_eq!(state.next_ui_deadline(), Some(now + frame + frame));
    state.status = super::TuiStatus::Idle;
    assert_eq!(state.next_ui_deadline(), None);
    state.push_transient_note_at("deadline", super::NoteVariant::Info, frame, now);
    assert_eq!(state.next_ui_deadline(), Some(now + frame));
    state.set_toast_hover(true, now);
    assert_eq!(state.next_ui_deadline(), None);
    state.set_toast_hover(false, now);
    assert!(state.tick_ui(now + frame));
    assert_eq!(state.next_ui_deadline(), None);
}

#[tokio::test]
async fn live_view_metrics_count_open_and_frozen_text_without_rendering() {
    let mut state = fresh_state("s-metrics").await;
    let turn = WorkerTurnId("metrics-turn".into());
    state.active_turn = Some(turn.clone());
    state.apply_reasoning_delta(&turn, "old");
    state.apply_delta(&turn, "first");
    state.apply_tool_started(&turn, "op", "read", "{}");
    state.apply_reasoning_delta(&turn, "new");
    state.apply_delta(&turn, "second");
    let sample = state.live_view_metrics();
    assert_eq!(sample.text_bytes, "firstsecond".len());
    assert_eq!(sample.reasoning_bytes, "oldnew".len());
    assert_eq!(sample.part_count, 3, "two frozen segments and a tool");
    assert_eq!(sample.markdown_cache_retained_bytes, 0);
    state.apply_finished(&turn, "firstsecond", 1);
    let done = state.live_view_metrics();
    assert_eq!(done.text_bytes, 0);
    assert_eq!(done.reasoning_bytes, 0);
    assert_eq!(done.part_count, 0);
}

#[tokio::test]
async fn stale_turn_events_are_ignored() {
    let mut state = fresh_state("s-stale").await;
    let mut driver = ScriptDriver::attach(&state.app);
    type_text(&mut state, "go").await;
    state.handle_key(KeyAction::Enter).await;
    await_submission(&mut state).await;
    assert_eq!(state.status(), &TuiStatus::Streaming);

    let stale = WorkerTurnId("t-stale".to_string());
    state.apply_delta(&stale, "junk");
    state.apply_finished(&stale, "junk", 0);
    state.apply_interrupted(&stale, "junk", 0);
    state.apply_failed(&stale, &CoreError::TurnBusy);
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert!(state.is_busy());
    assert!(!state.viewport().iter().any(|line| line.contains("junk")));

    let outcome = driver
        .pump_until_idle(&mut state, Duration::from_secs(5))
        .await;
    assert_eq!(outcome, PumpOutcome::Finished("echo: go".to_string()));
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(state.viewport().iter().any(|line| line == "   echo: go"));

    // A fresh submit is accepted right after the finish.
    type_text(&mut state, "go2").await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(outcome.note, None);
    await_submission(&mut state).await;
    assert_eq!(state.status(), &TuiStatus::Streaming);
    let _ = driver
        .pump_until_idle(&mut state, Duration::from_secs(5))
        .await;
    assert_eq!(state.status(), &TuiStatus::Idle);
}

#[tokio::test]
async fn vis38_dcp_metadata_freezes_through_commit_replay_controls_and_session_switch() {
    use crate::tools::ToolRender;
    use oc_core::{
        dcp_view::{DcpAccounting, DcpNotificationMode, DcpSummaryPage},
        queries::{DcpSnapshot, HistoryTurn, ToolOpView, TranscriptPart},
    };
    let mut state = fresh_state("dcp-lifecycle").await;
    let turn = WorkerTurnId("dcp-turn".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    state.live_agent_color_index = Some(2);
    state.apply_tool_started_with_presentation(
        &turn,
        "op",
        "compress",
        r#"{"topic":"forged input topic"}"#,
        Some("Actual owner topic 中文".into()),
    );
    let pending = state
        .transcript_lines(100, 100)
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(pending.contains("Actual owner topic 中文"));
    assert!(!pending.contains("forged input topic") && !pending.contains("removed"));
    assert!(
        state.compaction_at.is_some(),
        "real DCP operation uses existing animation clock"
    );
    let run = crate::dcp_view::fixture_run("dcp-lifecycle", "op");
    state.apply_tool_finished_with_presentation(
        &turn,
        "op",
        "compress",
        "completed",
        "forged output",
        13,
        false,
        None,
        Some(run.clone()),
        None,
    );
    assert!(state.compaction_at.is_none());
    assert!(
        state.note().is_none(),
        "chat has no duplicate success toast"
    );
    let frozen = state.transcript_lines(100, 100);
    let mut rewritten = run.clone();
    rewritten.ordinal = 99;
    rewritten.cumulative.gross_removed = 9_999_999;
    state.apply_tool_finished_with_presentation(
        &turn,
        "op",
        "compress",
        "completed",
        "replacement",
        11,
        false,
        None,
        Some(rewritten),
        None,
    );
    state.apply_tool_started(&turn, "op", "compress", "late duplicate");
    assert_eq!(
        state.transcript_lines(100, 100),
        frozen,
        "confirmed run cannot be rewritten by duplicate events"
    );
    state.apply_finished(&turn, "", 100);
    let mut answer = msg(2, Role::Assistant, "");
    answer.turn = Some(HistoryTurn {
        id: turn.0.clone(),
        agent_color_index: Some(2),
        parts: vec![TranscriptPart::Tool(ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 1,
            op: "op".into(),
            name: "compress".into(),
            state: "completed".into(),
            input: None,
            output: Some("legacy output is irrelevant".into()),
            output_bytes: 27,
            output_truncated: false,
            patch_effects: None,
            dcp: Some(run.clone()),
            dcp_topic: Some("Actual owner topic".into()),
        })],
        ..Default::default()
    });
    let owner = page(vec![answer], 1, false, false);
    state.refresh_completed_page(&owner);
    state.apply_dcp_snapshot(DcpSnapshot {
        accounting: Some(DcpAccounting {
            gross_removed: 9_999_999,
            active_summary: 0,
            ..Default::default()
        }),
        ..Default::default()
    });
    let card = state
        .transcript_rows()
        .into_iter()
        .find_map(|row| row.tool)
        .unwrap();
    let ToolRender::Dcp(view) = &card.render else {
        panic!("DCP metadata")
    };
    assert_eq!(
        view.snapshot,
        Some(run.clone()),
        "current accounting does not rewrite historical headers/maps"
    );
    assert_eq!(view.color_index, Some(2));
    assert!(
        state.dcp_summary_requests().is_empty(),
        "default hides contents without querying"
    );
    state.chrome.dcp.show_compression = true;
    assert_eq!(state.dcp_summary_requests(), vec![("op".into(), 0)]);
    state.apply_dcp_summary(
        &sid("unrelated"),
        "op",
        DcpSummaryPage {
            block_id: "b7".into(),
            topic: "topic".into(),
            text: "wrong session".into(),
            total_bytes: 13,
            next_offset: None,
        },
    );
    assert_eq!(state.dcp_summary_requests().len(), 1);
    state.apply_dcp_summary(
        &sid("dcp-lifecycle"),
        "op",
        DcpSummaryPage {
            block_id: "b7".into(),
            topic: "topic".into(),
            text: "Actual bounded summary".into(),
            total_bytes: 22,
            next_offset: None,
        },
    );
    assert!(state.dcp_summary_requests().is_empty());
    assert!(
        state
            .transcript_lines(100, 100)
            .iter()
            .any(|line| line.plain_text().contains("Actual bounded summary"))
    );
    state.chrome.dcp.notification = DcpNotificationMode::Off;
    assert!(
        !state
            .transcript_lines(100, 100)
            .iter()
            .any(|line| line.plain_text().contains("▣ DCP"))
    );
    state.chrome.dcp.notification = DcpNotificationMode::Detailed;
    state.chrome.dcp.show_compression = false;
    state.attach_page(&owner);
    let ToolRender::Dcp(view) = &state.history().rows()[0].tool.as_ref().unwrap().render else {
        panic!("replayed card")
    };
    assert_eq!(view.snapshot, Some(run));
    state.app.create_session(sid("dcp-other")).await.unwrap();
    state.set_session(sid("dcp-other"));
    assert!(!state.transcript_rows().iter().any(|row| row.tool.is_some()));
}

#[tokio::test]
async fn vis38_dcp_toast_uses_existing_expiry_once_and_off_preserves_failures() {
    use crate::dcp_panel::DcpOutcome;
    use oc_core::dcp_view::{DcpNotificationChannel, DcpNotificationMode};
    let mut state = fresh_state("dcp-toast").await;
    state.chrome.dcp.channel = DcpNotificationChannel::Toast;
    let turn = WorkerTurnId("toast-turn".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    let run = crate::dcp_view::fixture_run("dcp-toast", "op");
    state.apply_tool_finished_with_presentation(
        &turn,
        "op",
        "compress",
        "completed",
        "",
        0,
        false,
        None,
        Some(run.clone()),
        None,
    );
    assert_eq!(
        state.note(),
        Some("▣ DCP | -21.9K removed, +1.3K summary — Compression #7")
    );
    assert!(
        state.dcp.notice().is_none(),
        "no duplicate persistent notice"
    );
    assert!(
        !state
            .transcript_lines(100, 100)
            .iter()
            .any(|line| line.plain_text().contains("▣ DCP"))
    );
    assert!(state.toast_expiry.is_some());
    state.tick_toast(Instant::now() + Duration::from_secs(6));
    assert!(state.note().is_none());
    state.apply_tool_finished_with_presentation(
        &turn,
        "op",
        "compress",
        "completed",
        "",
        0,
        false,
        None,
        Some(run),
        None,
    );
    assert!(
        state.note().is_none(),
        "duplicate finish must not resurrect expired toast"
    );
    state.chrome.dcp.notification = DcpNotificationMode::Off;
    state.notify_dcp(DcpOutcome::Failed {
        reason: "actual failure".into(),
    });
    assert_eq!(state.note(), Some("dcp failed: actual failure"));
    assert_eq!(state.note_variant(), Some(NoteVariant::Error));
    assert!(state.toast_expiry.is_some());
}

/// Owner effects survive every frontend adoption boundary.
#[tokio::test]
async fn vis35_live_checkpoint_switch_fork_projection_and_reopen_keep_effects() {
    use oc_core::queries::{DiffView, DiffWrap, HistoryTurn, ToolOpView, TranscriptPart};
    let mut state = fresh_state("patch-lifecycle").await;
    let turn = WorkerTurnId("patch-turn".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    let effects = crate::patch_view::tests::effects();
    let operation = ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        rowid: 1,
        op: "patch-op".into(),
        name: "apply_patch".into(),
        state: "completed".into(),
        input: None,
        output: Some("stored result".into()),
        output_bytes: 13,
        output_truncated: false,
        patch_effects: Some(effects.clone()),
        dcp: None,
        dcp_topic: None,
    };
    // A missed intent event still gets the actual result metadata.
    state.apply_tool_finished_with_effects(
        &turn,
        "patch-op",
        "apply_patch",
        "completed",
        "stored result",
        13,
        false,
        Some(effects.clone()),
    );
    assert_eq!(
        state.transcript_rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .patch_effects,
        Some(effects.clone())
    );
    let mut projection = HistoryTurn {
        id: turn.0.clone(),
        status: "completed".into(),
        parts: vec![TranscriptPart::Tool(operation)],
        ..Default::default()
    };
    state.apply_presentation(&turn, &projection);
    assert_eq!(
        state.transcript_rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .patch_effects,
        Some(effects.clone())
    );
    let mut message = msg(1, Role::Assistant, "");
    message.turn = Some(projection.clone());
    let page = HistoryPage {
        rows: vec![message.clone()],
        total: 1,
        ..Default::default()
    };
    state.apply_finished(&turn, "", 0);
    state.attach_page(&page);
    let original = state.transcript_lines(80, 120);
    assert!(
        original
            .iter()
            .any(|l| l.plain_text().contains("← Patched a.txt"))
    );
    state.set_session(sid("other-patch-session"));
    assert!(!state.transcript_rows().iter().any(|r| r.tool.is_some()));
    state.set_session(sid("patch-lifecycle"));
    state.attach_page(&page);
    assert_eq!(state.transcript_lines(80, 120), original);
    // A fork changes durable identities, not the recorded filesystem projection.
    projection.id = "fork-turn".into();
    message.turn = Some(projection);
    let mut reopened = fresh_state("fork-patch-session").await;
    reopened.attach_page(&HistoryPage {
        rows: vec![message],
        total: 1,
        ..Default::default()
    });
    assert_eq!(reopened.transcript_lines(80, 120), original);
    reopened.chrome.diffs.view = DiffView::Split;
    reopened.chrome.diffs.wrap = DiffWrap::None;
    assert_eq!(
        reopened.transcript_rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .diff_settings,
        reopened.chrome.diffs
    );
    assert_ne!(reopened.transcript_lines(80, 120), original);
    // The overlay maps to one derived card: its cached layout (and syntax
    // highlight) survives every projection instead of re-rendering per frame.
    let first = reopened.transcript_rows()[0].tool.clone().unwrap();
    let second = reopened.transcript_rows()[0].tool.clone().unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "stable overlaid card"
    );
    assert_eq!(
        reopened.history().rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .patch_effects,
        Some(effects)
    );
}

/// Live parts stay bounded when a hostile stream floods tool events.
#[tokio::test]
async fn live_parts_stay_bounded_under_tool_flood() {
    let mut state = fresh_state("s-tools-flood").await;
    state.active_turn = Some(WorkerTurnId("t-flood".to_string()));
    state.status = TuiStatus::Streaming;
    for index in 0..(LIVE_PARTS_MAX + 20) {
        state.apply_tool_started(
            &WorkerTurnId("t-flood".to_string()),
            &format!("op-{index}"),
            "read",
            &serde_json::json!({"path": format!("f{index}")}).to_string(),
        );
        state.apply_tool_finished(
            &WorkerTurnId("t-flood".to_string()),
            &format!("op-{index}"),
            "read",
            "completed",
            "ok",
            2,
            false,
        );
    }
    assert!(state.live_parts.len() <= LIVE_PARTS_MAX);
    assert!(state.live_preview_truncated);
    assert!(
        !state
            .viewport()
            .iter()
            .any(|line| line.contains("Live preview truncated"))
    );
    assert!(state.retained_bytes() <= 2 * WINDOW_BYTES + MAX_INPUT_BYTES);
    assert!(state.viewport().len() <= VIEWPORT_LINES);
    state.reset_live_projection();
    assert!(
        !state.live_preview_truncated,
        "a replaced live projection does not retain old viewing loss"
    );
}

/// A stale turn can never grow cards into the transcript.
#[tokio::test]
async fn permission_transient_uses_owner_prepared_resource_warning_without_intent() {
    use super::{LivePart, Theme};
    use oc_core::{approval::*, tool_stream::*};
    let mut state = fresh_state("permission-transient").await;
    let turn = WorkerTurnId("prepared-turn".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "item".into(),
        call_id: "call".into(),
    };
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            output_presentation: None,
            identity: identity.clone(),
            name: "read".into(),
            preview: "{\"path\":\"invented-fragment".into(),
            truncated: false,
        },
    );
    let request = ApprovalRequest {
        id: 1,
        binding: ApprovalBinding {
            session: "permission-transient".into(),
            turn: turn.0.clone(),
            call: "call".into(),
            operation: "real-operation".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 1,
            agent: None,
            agent_digest: None,
        },
        project: "project".into(),
        action: "read".into(),
        resources: vec!["prepared.txt".into()],
        save_patterns: vec![],
        preview: ApprovalPreview::Resource {
            values: vec!["prepared.txt".into()],
        },
    };
    state.project_pending_approvals(std::slice::from_ref(&request));
    let LivePart::Tool { card, input, .. } = &state.live_parts[0] else {
        panic!("transient")
    };
    assert_eq!(card.state, "permission_pending");
    assert!(input.is_empty() && card.input_preview.is_empty() && card.patch_effects.is_none());
    let lines = crate::tools::tool_block(card, Theme::dark(), 80);
    assert!(lines.iter().any(|l| {
        l.spans().iter().any(|s| {
            s.content().contains("prepared.txt") && s.style().fg == Some(Theme::dark().warning())
        })
    }));
    assert!(!state.viewport().join("\n").contains("invented-fragment"));
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Linked {
            identity,
            op: "real-operation".into(),
        },
    );
    state.apply_tool_started(
        &turn,
        "real-operation",
        "read",
        "{\"path\":\"prepared.txt\"}",
    );
    let LivePart::Tool { card, .. } = &state.live_parts[0] else {
        panic!("intent")
    };
    assert_eq!(card.state, "started");
    state.live_parts.clear();
    state.project_pending_approvals(&[request]); // lost argument hint recovers only owner data
    assert_eq!(state.live_parts.len(), 1);
    state.apply_interrupted(&turn, "", 0);
    assert!(!state.viewport().join("\n").contains("prepared.txt"));
}

#[tokio::test]
async fn rejected_prepared_patch_reconciles_without_started_or_duplicate_card() {
    use super::LivePart;
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    use oc_core::patch::{DiffAlgorithm, FileEffect, PatchEffects, PatchOperation};
    use oc_core::tool_stream::{ToolStreamEvent, ToolStreamIdentity};
    let mut state = fresh_state("s-reject-patch").await;
    let turn = WorkerTurnId("t-reject-patch".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            output_presentation: None,
            identity: ToolStreamIdentity {
                round: 1,
                item_id: "item".into(),
                call_id: "call".into(),
            },
            name: "apply_patch".into(),
            preview: "unfinished".into(),
            truncated: false,
        },
    );
    let request = ApprovalRequest {
        id: 1,
        binding: ApprovalBinding {
            session: "s-reject-patch".into(),
            turn: turn.0.clone(),
            call: "call".into(),
            operation: "prepared-operation".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 1,
            agent: None,
            agent_digest: None,
        },
        project: "project".into(),
        action: "apply_patch".into(),
        resources: vec!["approval.txt".into()],
        save_patterns: vec!["approval.txt".into()],
        preview: ApprovalPreview::Patch {
            files: vec![FileEffect {
                algorithm: DiffAlgorithm::Minimal,
                operation: PatchOperation::Update,
                path: "approval.txt".into(),
                destination: None,
                additions: 1,
                deletions: 1,
                hunks: Vec::new(),
                truncated: false,
            }],
            total_files: 1,
            truncated: false,
        },
    };
    state.project_pending_approvals(&[request]);
    state.project_pending_approvals(&[]); // Resolved before the terminal event.
    let output = r#"{"status":"permission_rejected","feedback":null}"#;
    state.apply_tool_finished_with_effects(
        &turn,
        "prepared-operation",
        "apply_patch",
        "denied",
        output,
        output.len() as i64,
        false,
        Some(PatchEffects::default()),
    );
    assert_eq!(state.live_parts.len(), 1);
    let LivePart::Tool { card, .. } = &state.live_parts[0] else {
        panic!("tool")
    };
    assert_eq!(card.op, "prepared-operation");
    assert_eq!(card.state, "denied");
    assert_eq!(card.files, ["approval.txt"]);
    assert!(card.patch_effects.as_ref().unwrap().files.is_empty());
    let shown = state.viewport().join("\n");
    assert!(shown.contains("# Patch failed approval.txt"), "{shown}");
    assert!(
        !shown.contains("← Patched") && !shown.contains("Patching"),
        "{shown}"
    );
}

#[tokio::test]
async fn live_snapshot_replacement_keeps_canonical_window_and_private_draft() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("repaired"));
    state.attach_page(&page(
        vec![msg(1, Role::User, "older request")],
        20,
        true,
        true,
    ));
    state.input = "private draft".into();
    state.scroll = 3;
    let original = state.window.rows().to_vec();
    let old = WorkerTurnId("T1".into());
    state.begin_linked_turn(old.clone());
    state.apply_delta(&old, "superseded text");
    state.apply_reasoning_delta(&old, "superseded reasoning");
    state.apply_tool_started(&old, "old-op", "shell", r#"{"command":"old"}"#);
    state.cover_projection(7, true);
    state.reset_live_projection();
    assert!(state.active_turn().is_none());
    assert!(state.live_parts.is_empty());
    assert!(state.live_text.is_empty() && state.live_reasoning.is_empty());
    assert!(state.pending_tool_seen.is_empty());
    assert!(!state.live_preview_truncated);
    assert_eq!(state.input, "private draft");
    assert_eq!(state.scroll, 3);
    assert_eq!(state.window.rows(), original);
    assert_eq!(state.projection_floor(), 7);
    let current = WorkerTurnId("T2".into());
    state.begin_linked_turn(current.clone());
    state.apply_delta(&current, "available prefix");
    state.cover_projection(9, true);
    state.cover_projection(8, false);
    assert_eq!(state.projection_floor(), 9);
    assert!(state.live_preview_truncated);
    assert_eq!(state.live_text, "available prefix");
    state.reset_live_projection();
    state.begin_linked_turn(current.clone());
    state.apply_delta(&current, "available prefix");
    assert_eq!(
        state.live_text, "available prefix",
        "repair replaces, not appends"
    );
    assert_eq!(state.input, "private draft");
}

#[tokio::test]
async fn actual_subagent_clock_uses_the_winning_canonical_or_live_card() {
    use oc_core::queries::{HistoryPartWindow, HistoryTurn, PartState, ToolOpView, TranscriptPart};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("clock-winners"));
    let turn = WorkerTurnId("parent".into());
    let card = ToolOpView {
        child_job: None,
        question: None,
        rowid: 1,
        op: "one-child".into(),
        name: "subagent".into(),
        state: "started".into(),
        input: Some(r#"{"agent":"explore","description":"Real task"}"#.into()),
        output: None,
        output_bytes: 0,
        output_truncated: false,
        output_presentation: None,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    };
    let mut accepted = msg(1, Role::User, "request");
    let mut projection = HistoryTurn {
        id: turn.0.clone(),
        status: "started".into(),
        window: Some(HistoryPartWindow {
            start: 0,
            end: 1,
            total: 1,
        }),
        parts: vec![TranscriptPart::Tool(card.clone())],
        part_states: vec![PartState {
            sequence: 0,
            ..Default::default()
        }],
        ..Default::default()
    };
    accepted.turn = Some(projection.clone());
    state.attach_page(&page(vec![accepted.clone()], 1, false, false));
    state.begin_linked_turn(turn.clone());
    state.apply_presentation(&turn, &projection);
    let now = Instant::now();
    state.tick_tool_spinners(now);
    assert_eq!(state.tool_spinner_clocks.len(), 1);
    state.apply_tool_finished(
        &turn,
        "one-child",
        "subagent",
        "completed",
        "done",
        4,
        false,
    );
    state.tick_tool_spinners(now + Duration::from_millis(80));
    assert!(
        state.tool_spinner_clocks.is_empty(),
        "terminal live replacement defeats stale canonical running"
    );
    state.apply_tool_started(&turn, "one-child", "subagent", card.input.as_ref().unwrap());
    state.tick_tool_spinners(now + Duration::from_millis(90));
    assert_eq!(state.tool_spinner_clocks.len(), 1);
    let mut terminal = card;
    terminal.state = "completed".into();
    projection.parts = vec![
        TranscriptPart::Tool(terminal),
        TranscriptPart::Text("future canonical text".into()),
    ];
    projection.part_states.push(PartState {
        sequence: 1,
        ..Default::default()
    });
    projection.window = Some(HistoryPartWindow {
        start: 0,
        end: 2,
        total: 2,
    });
    accepted.turn = Some(projection);
    state.window.reset(&page(vec![accepted], 1, false, false));
    assert!(state.canonical_history_ahead());
    state.tick_tool_spinners(now + Duration::from_millis(170));
    assert!(
        state.tool_spinner_clocks.is_empty(),
        "terminal canonical-ahead defeats stale running live copy"
    );
    assert_eq!(state.next_tool_spinner_deadline(), None);
}

#[tokio::test]
async fn actual_subagent_clocks_survive_parent_completion_and_shared_refresh() {
    use oc_core::queries::{ChildJob, ChildState};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), sid("actual-clocks"));
    let turn = WorkerTurnId("parent".into());
    let job = |op: &str| ChildJob {
        parent: sid("actual-clocks"),
        child: sid(&format!("child-{op}")),
        operation: op.into(),
        generation: 1,
        location: "/fixture".into(),
        agent: "explore".into(),
        model: "fixture/model".into(),
        description: format!("Task {op}"),
        delivery_id: format!("delivery-{op}"),
        state: ChildState::Running,
        background: true,
        turn: Some(format!("child-turn-{op}")),
        result: None,
        message_id: None,
    };
    let a = job("A");
    let mut b = job("B");
    let launch = |state: &mut TuiState, op: &str| {
        state.apply_tool_started(&turn, op, "subagent", &format!(r#"{{"agent":"explore","description":"Task {op}","prompt":"Controlled child","background":true}}"#));
        state.apply_tool_finished(
            &turn,
            op,
            "subagent",
            "running",
            "Returned background handle",
            26,
            false,
        );
    };
    state.begin_linked_turn(turn.clone());
    launch(&mut state, "A");
    state.apply_child_jobs(vec![a.clone()]);
    let now = Instant::now();
    assert!(!state.tick_tool_spinners(now));
    assert!(state.tick_tool_spinners(now + Duration::from_millis(80)));
    launch(&mut state, "B");
    state.apply_child_jobs(vec![a.clone(), b.clone()]);
    state.tick_tool_spinners(now + Duration::from_millis(90));
    assert_eq!(state.tool_spinner_clocks["A"].frame, 1);
    assert_eq!(state.tool_spinner_clocks["B"].frame, 0);
    state.apply_finished(&turn, "Parent anchor", 10);
    assert!(state.active_turn.is_none());
    state.tick_tool_spinners(now + Duration::from_millis(160));
    assert_eq!(
        state.tool_spinner_clocks["A"].frame, 2,
        "parent completion does not retire child clocks"
    );
    assert_eq!(state.tool_spinner_clocks["B"].frame, 0);
    assert!(state.window.rows().iter().filter_map(|row| row.tool.as_ref()).all(|card|
        matches!(&card.render, crate::tools::ToolRender::Subagent(view) if view.spinner.is_none())));
    let mut refreshed = TuiState::new(app, sid("actual-clocks"));
    refreshed.window = state.window.clone();
    refreshed.preserve_shared_composer(state);
    assert_eq!(refreshed.tool_spinner_clocks["A"].frame, 2);
    assert_eq!(
        refreshed.tool_spinner_clocks["B"].at,
        Some(now + Duration::from_millis(90))
    );
    b.state = ChildState::Completed;
    refreshed.apply_child_jobs(vec![a.clone(), b.clone()]);
    refreshed.tick_tool_spinners(now + Duration::from_millis(170));
    assert!(!refreshed.tool_spinner_clocks.contains_key("B"));
    let shown = refreshed
        .transcript_lines(120, 120)
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        shown.contains("✓ Explore Subagent — Task B  Background"),
        "{shown}"
    );
    assert!(
        shown.contains("⠹ Explore Subagent — Task A  Background"),
        "{shown}"
    );
    let mut catalog = snapshot();
    catalog.chrome = refreshed.chrome.clone();
    catalog.chrome.animations = Some(true);
    refreshed.apply_catalog(catalog.clone());
    assert_eq!(refreshed.tool_spinner_clocks["A"].frame, 2);
    catalog.chrome.animations = Some(false);
    refreshed.apply_catalog(catalog.clone());
    assert_eq!(refreshed.next_tool_spinner_deadline(), None);
    catalog.chrome.animations = Some(true);
    refreshed.apply_catalog(catalog);
    assert_eq!(refreshed.tool_spinner_clocks["A"].frame, 0);
    refreshed.tick_tool_spinners(now + Duration::from_secs(1));
    assert_eq!(
        refreshed.next_tool_spinner_deadline(),
        Some(now + Duration::from_millis(1080))
    );
    refreshed.reset_workspace();
    refreshed.tick_tool_spinners(now + Duration::from_secs(2));
    assert!(
        refreshed.tool_spinner_clocks.is_empty(),
        "reset must not rearm stale retained rows"
    );
    assert_eq!(refreshed.next_tool_spinner_deadline(), None);
}

fn rendered_transcript(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, state))
        .unwrap();
    let area =
        crate::shell::transcript_area(state, ratatui::layout::Rect::new(0, 0, width, height));
    let buffer = terminal.backend().buffer();
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[tokio::test]
async fn actual_subagent_clock_ticks_keep_warm_cache_and_bounded_membership() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("actual-cached"));
    let turn = WorkerTurnId("parent".into());
    state.begin_linked_turn(turn.clone());
    for index in 0..20 {
        state.apply_tool_started(&turn, &format!("op-{index}"), "subagent", r#"{"agent":"explore","description":"Controlled description","prompt":"Read-only fixture"}"#);
    }
    let now = Instant::now();
    state.tick_tool_spinners(now);
    assert_eq!(
        state.tool_spinner_clocks.len(),
        20,
        "not limited by child inventory16"
    );
    let paint = |state: &TuiState| rendered_transcript(state, 120, 40);
    paint(&state);
    let copies = state.transcript_row_copies.get();
    let builds = state.visible_projection_builds.get();
    let discoveries = state.tool_spinner_membership_builds;
    let traversals = crate::messages::indexed_traversals();
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(79)));
    paint(&state);
    assert_eq!(state.transcript_row_copies.get(), copies);
    assert_eq!(state.visible_projection_builds.get(), builds);
    for step in 1..=10 {
        assert!(state.tick_tool_spinners(
            now + Duration::from_millis(step * 80) + Duration::from_nanos(123)
        ));
        assert!(
            state
                .tool_spinner_clocks
                .values()
                .all(|clock| clock.frame == step as usize % 10)
        );
        assert_eq!(
            state.next_tool_spinner_deadline(),
            Some(now + Duration::from_millis((step + 1) * 80))
        );
        let lines = paint(&state);
        assert!(lines.iter().any(|line| line.contains(&format!(
            "{} Explore Subagent",
            crate::compaction::FRAMES[step as usize % 10]
        ))));
        assert_eq!(state.visible_projection_builds.get(), builds);
        assert_eq!(state.transcript_row_copies.get(), copies);
        assert_eq!(state.tool_spinner_membership_builds, discoveries);
        assert_eq!(crate::messages::indexed_traversals(), traversals);
    }
    paint(&state);
    let builds = state.visible_projection_builds.get();
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(1600)));
    paint(&state);
    assert_eq!(state.visible_projection_builds.get(), builds);
    state.live_parts[0] = LivePart::Text("Non-tool replacement".into());
    state.invalidate_transcript();
    state.tick_tool_spinners(now + Duration::from_millis(1610));
    assert!(!state.tool_spinner_clocks.contains_key("op-0"));
    state.set_session(sid("other"));
    state.tick_tool_spinners(now + Duration::from_secs(2));
    assert!(state.tool_spinner_clocks.is_empty());
}

#[tokio::test]
async fn ordinary_tool_spinner_animates_without_events_and_retires_when_settled() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("tool-spinner"));
    let turn = WorkerTurnId("shell-turn".into());
    state.begin_linked_turn(turn.clone());
    state.apply_tool_started(
        &turn,
        "shell-op",
        "shell",
        r#"{"command":"sleep 300 && echo 'hello ⋯'"}"#,
    );
    let now = Instant::now();
    assert!(!state.tick_tool_spinners(now));
    assert_eq!(
        state.next_tool_spinner_deadline(),
        Some(now + Duration::from_millis(80))
    );
    let frame = |state: &TuiState| {
        rendered_transcript(state, 100, 30)
            .into_iter()
            .find(|line| line.contains("sleep 300"))
            .unwrap()
    };
    assert_eq!(frame(&state), "┃  ⠋ sleep 300 && echo 'hello ⋯'");
    let area = crate::shell::transcript_area(&state, ratatui::layout::Rect::new(0, 0, 100, 30));
    let header = rendered_transcript(&state, 100, 30)
        .iter()
        .position(|line| line.contains("sleep 300"))
        .unwrap();
    state.last_mouse = Some((
        area.x + 8,
        area.y + header as u16,
        ratatui::layout::Rect::new(0, 0, 100, 30),
    ));
    frame(&state);
    let builds = state.visible_projection_builds.get();
    let copies = state.transcript_row_copies.get();
    let discoveries = state.tool_spinner_membership_builds;
    let traversals = crate::messages::indexed_traversals();
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(79)));
    frame(&state);
    assert_eq!(state.visible_projection_builds.get(), builds);
    // Only the UI deadline advances: there are no input/provider/tool events.
    assert!(state.tick_ui(now + Duration::from_millis(80)));
    assert_eq!(frame(&state), "┃  ⠙ sleep 300 && echo 'hello ⋯'");
    assert_eq!(state.visible_projection_builds.get(), builds);
    assert_eq!(state.transcript_row_copies.get(), copies);
    assert_eq!(state.tool_spinner_membership_builds, discoveries);
    assert_eq!(
        crate::messages::indexed_traversals(),
        traversals,
        "stationary Shell hover uses visible hit metadata"
    );
    assert!(
        state
            .live_parts
            .iter()
            .all(|part| !matches!(part, LivePart::Tool { card, .. } if card.spinner.is_some()))
    );

    let mut catalog = snapshot();
    catalog.chrome = state.chrome.clone();
    catalog.chrome.animations = Some(false);
    state.apply_catalog(catalog.clone());
    assert_eq!(frame(&state), "┃  ⋯ sleep 300 && echo 'hello ⋯'");
    assert_eq!(state.next_tool_spinner_deadline(), None);
    catalog.chrome.animations = Some(true);
    state.apply_catalog(catalog);
    assert_eq!(frame(&state), "┃  ⠋ sleep 300 && echo 'hello ⋯'");
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(100)));
    assert!(state.tick_tool_spinners(now + Duration::from_millis(180)));
    assert_eq!(frame(&state), "┃  ⠙ sleep 300 && echo 'hello ⋯'");

    state.apply_tool_finished(
        &turn,
        "shell-op",
        "shell",
        "completed",
        "exit 0\nhello ⋯\n",
        19,
        false,
    );
    state.tick_tool_spinners(now + Duration::from_millis(260));
    assert_eq!(frame(&state), "┃  $ sleep 300 && echo 'hello ⋯'");
    assert!(state.tool_spinner_clocks.is_empty());
    assert_eq!(state.next_tool_spinner_deadline(), None);
}

#[tokio::test]
async fn reasoning_spinner_animates_without_events_and_retires_when_settled() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("reasoning-spinner"));
    let turn = WorkerTurnId("reasoning-turn".into());
    state.begin_linked_turn(turn.clone());
    state.apply_reasoning_delta(&turn, "**Inspecting**\n\nfirst body");
    let headers = |state: &TuiState| {
        rendered_transcript(state, 100, 30)
            .into_iter()
            .filter(|line| line.contains("Thinking") || line.contains("Thought"))
            .map(|line| line.trim_end().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(headers(&state), ["   ⠋ Thinking: Inspecting"]);
    assert!(state.next_tool_spinner_deadline().unwrap() <= Instant::now());
    let now = Instant::now();
    assert!(!state.tick_tool_spinners(now));
    assert_eq!(
        state.next_tool_spinner_deadline(),
        Some(now + Duration::from_millis(80))
    );
    let builds = state.visible_projection_builds.get();
    let copies = state.transcript_row_copies.get();
    let traversals = crate::messages::indexed_traversals();
    let discoveries = state.tool_spinner_membership_builds;
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(79)));
    headers(&state);
    assert_eq!(state.visible_projection_builds.get(), builds);
    assert_eq!(state.transcript_row_copies.get(), copies);
    // Only the UI deadline advances, with no provider or input event.
    assert!(state.tick_ui(now + Duration::from_millis(80)));
    assert_eq!(headers(&state), ["   ⠙ Thinking: Inspecting"]);
    assert_eq!(state.visible_projection_builds.get(), builds);
    assert_eq!(state.transcript_row_copies.get(), copies);
    assert_eq!(crate::messages::indexed_traversals(), traversals);
    assert_eq!(state.tool_spinner_membership_builds, discoveries);

    // A running group takes its header from the already-completed first part.
    state.apply_reasoning_item_ended(&turn);
    state.apply_reasoning_delta(&turn, "**Verifying**\n\nsecond body");
    assert_eq!(headers(&state), ["   ⠙ Thinking: Verifying"]);
    state.thinking_expanded = true;
    assert!(headers(&state).iter().any(|line| line == "   ┃ ⠙ Thinking"));
    assert!(state.tick_tool_spinners(now + Duration::from_millis(160)));
    assert!(headers(&state).iter().any(|line| line == "   ┃ ⠹ Thinking"));
    state.thinking_expanded = false;
    assert_eq!(headers(&state), ["   ⠹ Thinking: Verifying"]);

    let mut catalog = snapshot();
    catalog.chrome = state.chrome.clone();
    catalog.chrome.animations = Some(false);
    state.apply_catalog(catalog.clone());
    assert_eq!(headers(&state), ["   ⋯ Thinking: Verifying"]);
    assert_eq!(state.next_tool_spinner_deadline(), None);
    catalog.chrome.animations = Some(true);
    state.apply_catalog(catalog);
    assert_eq!(headers(&state), ["   ⠋ Thinking: Verifying"]);
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(200)));
    assert!(state.tick_tool_spinners(now + Duration::from_millis(280)));
    assert_eq!(headers(&state), ["   ⠙ Thinking: Verifying"]);

    state.apply_reasoning_item_ended(&turn);
    let completed = headers(&state);
    assert_eq!(completed.len(), 1);
    assert!(completed[0].starts_with("   + Thought: Verifying · 2 steps"));
    assert_eq!(state.next_tool_spinner_deadline(), None);
    assert!(!state.tick_tool_spinners(now + Duration::from_millis(360)));
    assert!(state.reasoning_spinner_clock.is_none());

    state.apply_reasoning_delta(&turn, "**New step**\n\nbody");
    headers(&state);
    state.tick_tool_spinners(now + Duration::from_millis(400));
    state.reset_workspace();
    assert!(
        headers(&state)
            .iter()
            .any(|line| line.contains("⋯ Thinking: New step"))
    );
    state.tick_tool_spinners(now + Duration::from_millis(480));
    assert!(state.reasoning_spinner_clock.is_none());
    assert_eq!(state.next_tool_spinner_deadline(), None);
}

#[tokio::test]
async fn pending_delegation_clock_is_local_cached_and_demand_driven() {
    use oc_core::tool_stream::{ToolStreamEvent as Event, ToolStreamIdentity};
    let mut state = fresh_state("delegation-clock").await;
    let turn = WorkerTurnId("parent".into());
    state.begin_linked_turn(turn.clone());
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "a".into(),
        call_id: "a".into(),
    };
    let pending = |identity| Event::Pending {
        output_presentation: None,
        identity,
        name: "subagent".into(),
        preview: "opaque".into(),
        truncated: false,
    };
    state.apply_tool_argument_stream(&turn, &pending(identity.clone()));
    let now = Instant::now();
    assert!(!state.tick_delegations(now));
    let frame = |state: &TuiState| {
        rendered_transcript(state, 80, 24)
            .into_iter()
            .filter(|line| line.contains("Delegating…"))
            .collect::<Vec<_>>()
    };
    assert_eq!(frame(&state), ["   ⠋ Delegating…"]);
    let area = crate::shell::transcript_area(&state, ratatui::layout::Rect::new(0, 0, 80, 24));
    let header = rendered_transcript(&state, 80, 24)
        .iter()
        .position(|line| line.contains("Delegating…"))
        .unwrap();
    state.last_mouse = Some((
        area.x + 8,
        area.y + header as u16,
        ratatui::layout::Rect::new(0, 0, 80, 24),
    ));
    frame(&state);
    let builds = state.visible_projection_builds.get();
    let copies = state.transcript_row_copies.get();
    let traversals = crate::messages::indexed_traversals();
    assert!(!state.tick_delegations(now + Duration::from_millis(79)));
    frame(&state);
    assert_eq!(state.visible_projection_builds.get(), builds);
    for i in 1..=10 {
        assert!(
            state.tick_delegations(now + Duration::from_millis(i * 80) + Duration::from_nanos(750))
        );
        assert_eq!(
            frame(&state),
            [format!(
                "   {} Delegating…",
                crate::compaction::FRAMES[i as usize % 10]
            )]
        );
        assert_eq!(
            state.next_delegation_deadline(),
            Some(now + Duration::from_millis((i + 1) * 80))
        );
        assert_eq!(state.visible_projection_builds.get(), builds);
        assert_eq!(state.transcript_row_copies.get(), copies);
        assert_eq!(
            crate::messages::indexed_traversals(),
            traversals,
            "stationary pending hover never composes or traverses history"
        );
    }
    let builds = state.visible_projection_builds.get();
    assert!(!state.tick_delegations(now + Duration::from_millis(1600)));
    frame(&state);
    assert_eq!(
        state.visible_projection_builds.get(),
        builds,
        "a full skipped cycle changes no glyph"
    );
    state.apply_tool_argument_stream(&turn, &pending(identity.clone()));
    let b = ToolStreamIdentity {
        item_id: "b".into(),
        call_id: "b".into(),
        ..identity.clone()
    };
    state.apply_tool_argument_stream(&turn, &pending(b));
    assert!(!state.tick_delegations(now + Duration::from_millis(1610)));
    assert!(state.tick_delegations(now + Duration::from_millis(1680)));
    assert_eq!(frame(&state), ["   ⠙ Delegating…", "   ⠋ Delegating…"]);
    state.apply_tool_argument_stream(&turn, &pending(identity.clone()));
    assert_eq!(
        frame(&state)[0],
        "   ⠙ Delegating…",
        "snapshots preserve phase"
    );
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity,
            op: "linked".into(),
        },
    );
    assert_eq!(
        frame(&state)[0],
        "   ⠙ Delegating…",
        "Linked has no complete input yet"
    );
    let mut catalog = snapshot();
    catalog.chrome = state.chrome.clone();
    catalog.chrome.animations = Some(true);
    state.apply_catalog(catalog.clone());
    assert_eq!(
        frame(&state)[0],
        "   ⠙ Delegating…",
        "None and true are equivalent"
    );
    catalog.chrome.animations = Some(false);
    state.apply_catalog(catalog.clone());
    assert!(frame(&state).iter().all(|line| line == "   ⋯ Delegating…"));
    assert_eq!(state.next_delegation_deadline(), None);
    catalog.chrome.animations = Some(true);
    state.apply_catalog(catalog);
    assert!(frame(&state).iter().all(|line| line == "   ⠋ Delegating…"));
    assert!(!state.tick_delegations(now + Duration::from_millis(1700)));
    assert_eq!(
        state.next_delegation_deadline(),
        Some(now + Duration::from_millis(1780))
    );
    state.apply_tool_argument_stream(&turn, &Event::Clear { round: 1 });
    assert!(
        frame(&state).is_empty(),
        "warm cache must lose linked and unlinked drafts"
    );
    assert_eq!(state.next_delegation_deadline(), None);
    state.apply_interrupted(&turn, "", 1);
    // The real paint above also arms the independent tab-completion animation.
    assert_eq!(state.next_delegation_deadline(), None);
    assert!(state.live_parts.is_empty());
}

#[tokio::test]
async fn compaction_animation_patches_only_paint_and_resize_rebuilds_geometry() {
    use oc_core::compaction::{
        CompactionAnchor, CompactionReason, CompactionSnapshot, CompactionState,
    };
    let mut state = fresh_state("compaction-paint").await;
    state.apply_compaction(CompactionSnapshot {
        id: "checkpoint".into(),
        session: state.session.as_ref().unwrap().0.clone(),
        model: None,
        profile: None,
        anchor: CompactionAnchor::default(),
        reason: CompactionReason::Manual,
        state: CompactionState::Running,
        summary: "streamed summary ⋯".into(),
        usage: None,
        provider_native: false,
        error: None,
    });
    let now = state.compaction_at.unwrap();
    let first = rendered_transcript(&state, 100, 30);
    assert!(first.iter().any(|line| line.contains("⠋ Compaction")));
    let builds = state.visible_projection_builds.get();
    let copies = state.transcript_row_copies.get();
    assert!(state.tick_compaction(now + Duration::from_millis(80)));
    let second = rendered_transcript(&state, 100, 30);
    assert!(second.iter().any(|line| line.contains("⠙ Compaction")));
    assert!(
        second
            .iter()
            .any(|line| line.contains("streamed summary ⋯"))
    );
    assert_eq!(state.visible_projection_builds.get(), builds);
    assert_eq!(state.transcript_row_copies.get(), copies);
    assert!(
        rendered_transcript(&state, 70, 30)
            .iter()
            .any(|line| line.contains("⠙ Compaction"))
    );
    assert_eq!(state.visible_projection_builds.get(), builds + 1);
}

#[tokio::test]
async fn pending_delegation_cleanup_and_checkpoint_keep_the_actual_owner() {
    use oc_core::queries::{HistoryPartWindow, HistoryTurn};
    use oc_core::tool_stream::{ToolStreamEvent as Event, ToolStreamIdentity};
    for outcome in [
        "started",
        "denied",
        "clear",
        "new-round",
        "terminal",
        "presentation",
        "workspace",
        "rebind",
        "evicted",
    ] {
        let mut state = fresh_state("delegation-cleanup").await;
        let turn = WorkerTurnId("parent".into());
        state.begin_linked_turn(turn.clone());
        let identity = ToolStreamIdentity {
            round: 1,
            item_id: "i".into(),
            call_id: "c".into(),
        };
        state.apply_tool_argument_stream(
            &turn,
            &Event::Pending {
                identity: identity.clone(),
                name: "subagent".into(),
                preview: "opaque".into(),
                truncated: false,
                output_presentation: None,
            },
        );
        let now = Instant::now();
        state.tick_delegations(now);
        state.tick_delegations(now + Duration::from_millis(80));
        state.apply_tool_argument_stream(
            &turn,
            &Event::Linked {
                identity,
                op: "actual".into(),
            },
        );
        state.apply_presentation(
            &turn,
            &HistoryTurn {
                id: turn.0.clone(),
                status: "started".into(),
                window: Some(HistoryPartWindow::default()),
                ..Default::default()
            },
        );
        assert!(
            state
                .visible_transcript(80, 80, 40)
                .0
                .iter()
                .any(|line| line.plain_text() == "   ⠙ Delegating…")
        );
        match outcome {
            "started" => state.apply_tool_started(
                &turn,
                "actual",
                "subagent",
                r#"{"agent":"helper","description":"Real task"}"#,
            ),
            "denied" => {
                state.apply_tool_finished(&turn, "actual", "subagent", "denied", "Denied", 6, false)
            }
            "clear" => state.apply_tool_argument_stream(&turn, &Event::Clear { round: 1 }),
            "new-round" => state.apply_tool_argument_stream(
                &turn,
                &Event::Pending {
                    identity: ToolStreamIdentity {
                        round: 2,
                        item_id: "read".into(),
                        call_id: "read".into(),
                    },
                    name: "read".into(),
                    preview: "".into(),
                    truncated: false,
                    output_presentation: None,
                },
            ),
            "terminal" => state.apply_interrupted(&turn, "", 1),
            "presentation" => state.apply_presentation(
                &turn,
                &HistoryTurn {
                    id: turn.0.clone(),
                    status: "failed".into(),
                    revision: 1,
                    ..Default::default()
                },
            ),
            "workspace" => state.reset_workspace(),
            "rebind" => state.begin_linked_turn(WorkerTurnId("other".into())),
            "evicted" => {
                state
                    .live_parts
                    .extend((0..super::LIVE_PARTS_MAX).map(|_| LivePart::Text("tail".into())));
                state.enforce_parts();
            }
            _ => unreachable!(),
        }
        assert_eq!(state.next_delegation_deadline(), None, "{outcome}");
        assert!(
            state.live_parts.iter().all(|part| !matches!(
                part,
                LivePart::Tool {
                    delegation: Some(_),
                    ..
                }
            )),
            "{outcome}"
        );
    }
}

#[tokio::test]
async fn pending_delegation_permission_does_not_recreate_a_provider_clock() {
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    use oc_core::tool_stream::{ToolStreamEvent, ToolStreamIdentity};
    let mut state = fresh_state("delegation-permission").await;
    let turn = WorkerTurnId("parent".into());
    state.begin_linked_turn(turn.clone());
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            identity: ToolStreamIdentity {
                round: 1,
                item_id: "i".into(),
                call_id: "c".into(),
            },
            name: "subagent".into(),
            preview: "opaque".into(),
            truncated: false,
            output_presentation: None,
        },
    );
    state.tick_delegations(Instant::now());
    let request = ApprovalRequest {
        id: 1,
        binding: ApprovalBinding {
            session: "delegation-permission".into(),
            turn: turn.0.clone(),
            call: "c".into(),
            operation: "prepared".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 1,
            agent: None,
            agent_digest: None,
        },
        project: "project".into(),
        action: "subagent".into(),
        resources: vec!["helper".into()],
        save_patterns: vec![],
        preview: ApprovalPreview::Resource {
            values: vec!["helper".into()],
        },
    };
    state.project_pending_approvals(&[request]);
    assert_eq!(state.next_delegation_deadline(), None);
    state.project_pending_approvals(&[]);
    state.tick_delegations(Instant::now() + Duration::from_secs(1));
    assert_eq!(
        state.next_delegation_deadline(),
        None,
        "argument_stream after permission is not a new provider draft"
    );
    state.apply_tool_finished(&turn, "prepared", "subagent", "denied", "Denied", 6, false);
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        1
    );
    assert_eq!(state.next_delegation_deadline(), None);
}

#[tokio::test]
async fn pending_delegation_hover_is_presentation_only() {
    use crossterm::event::MouseEventKind;
    use oc_core::tool_stream::{ToolStreamEvent, ToolStreamIdentity};
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = fresh_state("pending-hover").await;
    let turn = WorkerTurnId("parent".into());
    state.begin_linked_turn(turn.clone());
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            identity: ToolStreamIdentity {
                round: 1,
                item_id: "i".into(),
                call_id: "c".into(),
            },
            name: "subagent".into(),
            preview: "opaque".into(),
            truncated: false,
            output_presentation: None,
        },
    );
    state.restore_prompt("preserved draft".into());
    state.tick_delegations(Instant::now());
    let area = Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let (lines, _, _) = state.visible_transcript_at_viewport(rect.width, area.width, rect.height);
    let row = lines
        .iter()
        .position(|line| line.plain_text().contains("Delegating…"))
        .unwrap();
    let y = rect.y + row as u16;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert_eq!(
        terminal.backend().buffer()[(rect.x + 5, y)].fg,
        Theme::dark().text_muted()
    );
    let caret = state.editor.cursor;
    for x in [rect.x, rect.x + 5, rect.x + rect.width - 1] {
        state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), area);
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(rect.x + 5, y)].fg,
            Theme::dark().text()
        );
        let down = state.handle_mouse(
            selection_mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                x,
                y,
            ),
            area,
        );
        let up = state.handle_mouse(
            selection_mouse(
                MouseEventKind::Up(crossterm::event::MouseButton::Left),
                x,
                y,
            ),
            area,
        );
        assert!(down.intent.is_none() && up.intent.is_none());
        assert_eq!(state.input(), "preserved draft");
        assert_eq!(state.editor.cursor, caret);
    }
}

#[tokio::test]
async fn pending_delegation_uses_typed_inline_opaque_presentation() {
    use oc_core::tool_stream::{ToolStreamEvent as Event, ToolStreamIdentity};
    let mut state = fresh_state("pending-delegation").await;
    let turn = WorkerTurnId("parent-contract".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    state.chrome.animations = Some(false);
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "item".into(),
        call_id: "call".into(),
    };
    for preview in [
        "",
        "{not JSON",
        r#"{"agent":"LEAK_AGENT","description":"LEAK_DESCRIPTION","model":"LEAK_MODEL","sessionID":"LEAK_CHILD"}"#,
    ] {
        state.apply_tool_argument_stream(
            &turn,
            &Event::Pending {
                output_presentation: None,
                identity: identity.clone(),
                name: "subagent".into(),
                preview: preview.into(),
                truncated: false,
            },
        );
        let rows = state.transcript_rows();
        let cards: Vec<_> = rows.iter().filter_map(|row| row.tool.as_ref()).collect();
        assert_eq!(cards.len(), 1);
        let card = cards[0];
        assert!(matches!(card.render, crate::tools::ToolRender::Subagent(_)));
        assert_eq!(card.state, "argument_stream");
        assert!(card.child_job.is_none() && card.patch_effects.is_none() && card.files.is_empty());
        assert_eq!(card.output_bytes, 0);
        assert!(card.started_at.is_none());
        for width in [80, 120] {
            let (lines, _) = state.visible_transcript(width, width, 40);
            let shown = lines
                .iter()
                .map(crate::styled::Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                lines
                    .iter()
                    .any(|line| line.plain_text() == "   ⋯ Delegating…"),
                "{shown}"
            );
            assert!(
                !shown.contains("LEAK_")
                    && !shown.contains("arguments streaming")
                    && !shown.contains("no effects yet")
            );
        }
    }
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity: identity.clone(),
            op: "actual-operation".into(),
        },
    );
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("Delegating…"))
    );
    state.apply_tool_started(
        &turn,
        "actual-operation",
        "subagent",
        r#"{"agent":"helper","description":"Actual description","prompt":"Controlled task"}"#,
    );
    state.apply_tool_argument_stream(
        &turn,
        &Event::Pending {
            output_presentation: None,
            identity,
            name: "subagent".into(),
            preview: "late".into(),
            truncated: false,
        },
    );
    let rows = state.transcript_rows();
    assert_eq!(rows.iter().filter(|row| row.tool.is_some()).count(), 1);
    let shown = state
        .visible_transcript(80, 80, 40)
        .0
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        shown.contains("Helper Subagent — Actual description") && !shown.contains("Delegating…"),
        "{shown}"
    );
}

#[tokio::test]
async fn argument_stream_exploration_groups_like_upstream_and_survives_admission() {
    use oc_core::tool_stream::{ToolStreamEvent as Event, ToolStreamIdentity as Identity};
    let mut state = fresh_state("s-streaming-exploration").await;
    let turn = WorkerTurnId("t-streaming-exploration".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    let identity = |call: &str| Identity {
        round: 1,
        item_id: format!("item-{call}"),
        call_id: call.into(),
    };
    for (call, name) in [("a", "read"), ("b", "grep"), ("c", "read"), ("d", "shell")] {
        state.apply_tool_argument_stream(
            &turn,
            &Event::Pending {
                output_presentation: None,
                identity: identity(call),
                name: name.into(),
                preview: "{\"path\":\"leak.txt\",\"command\":\"leak".into(),
                truncated: false,
            },
        );
    }
    let shown = |state: &TuiState| rendered_transcript(state, 80, 40);
    let collapsed = shown(&state);
    assert!(
        collapsed.contains(&"   ⠋ Exploring: 2 reads, 1 search".to_string()),
        "{collapsed:?}"
    );
    assert!(!collapsed.iter().any(|line| line.contains("Reading file…")));
    // Shell keeps its own BlockTool, never an empty `$` command.
    assert!(
        collapsed.contains(&"┃  ⠋ Writing command…".to_string()),
        "{collapsed:?}"
    );
    assert!(collapsed.iter().all(|line| !line.contains("leak")
        && !line.contains('$')
        && !line.contains("arguments streaming")));
    let now = Instant::now();
    assert!(!state.tick_tool_spinners(now));
    assert!(state.tick_tool_spinners(now + Duration::from_millis(80)));
    assert!(shown(&state).contains(&"   ⠙ Exploring: 2 reads, 1 search".to_string()));
    let first = state
        .transcript_rows()
        .into_iter()
        .find_map(|row| row.tool)
        .unwrap()
        .op
        .clone();
    state.exploration_expanded.insert(first);
    state.invalidate_transcript();
    let members = [
        "   ⠙ Exploring: 2 reads, 1 search",
        "   ⠙ Reading file…",
        "   ⠙ Searching content…",
        "   ⠙ Reading file…",
    ];
    let expanded = shown(&state);
    assert!(
        expanded.windows(members.len()).any(|rows| rows == members),
        "{expanded:?}"
    );
    // Admission renames the first member; its group stays expanded and open.
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity: identity("a"),
            op: "durable-a".into(),
        },
    );
    let admitted = shown(&state);
    // Linking changes operation identity but must keep the animation alive.
    assert!(
        admitted.windows(members.len()).any(|rows| rows == members),
        "{admitted:?}"
    );
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        4
    );
}

#[tokio::test]
async fn argument_stream_exact_identity_caps_reconcile_and_terminal_cleanup() {
    use oc_core::tool_stream::{
        PENDING_TOOL_MAX, ToolStreamEvent as Event, ToolStreamIdentity as Identity,
    };
    let mut state = fresh_state("s-arguments").await;
    let turn = WorkerTurnId("t-arguments".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    let identity = Identity {
        round: 1,
        item_id: "item".into(),
        call_id: "call".into(),
    };
    let pending = |identity: Identity, name: &str, preview: &str| Event::Pending {
        output_presentation: None,
        identity,
        name: name.into(),
        preview: preview.into(),
        truncated: false,
    };
    state.apply_tool_argument_stream(
        &turn,
        &pending(
            identity.clone(),
            "apply_patch",
            "{\"patchText\":\"*** Add File: fake",
        ),
    );
    let shown = state.viewport().join("\n");
    // ApplyPatch's streaming BlockTool title (`index.tsx:3440`).
    assert!(shown.contains("┃  ⠋ Patching"), "{shown}");
    assert!(!shown.contains("arguments streaming") && !shown.contains("no effects yet"));
    assert!(!shown.contains("Created") && !shown.contains("fake"));
    let card = state
        .transcript_rows()
        .into_iter()
        .find_map(|row| row.tool)
        .unwrap();
    assert!(card.patch_effects.is_none() && card.diff.is_none() && card.files.is_empty());
    assert_eq!(card.output_bytes, 0);
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity: Identity {
                round: 2,
                ..identity.clone()
            },
            op: "wrong-round".into(),
        },
    );
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity: Identity {
                call_id: "wrong".into(),
                ..identity.clone()
            },
            op: "wrong-call".into(),
        },
    );
    assert!(
        matches!(&state.live_parts[0], super::LivePart::Tool { card, .. } if card.state == "argument_stream")
    );
    state.apply_tool_argument_stream(
        &turn,
        &Event::Linked {
            identity: identity.clone(),
            op: "durable-op".into(),
        },
    );
    state.apply_tool_started(
        &turn,
        "durable-op",
        "apply_patch",
        &serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: real\n+x\n*** End Patch"})
            .to_string(),
    );
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        1
    );
    state.apply_tool_argument_stream(&turn, &pending(identity, "apply_patch", "late snapshot"));
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        1,
        "linked identity cannot reappear"
    );
    for i in 0..100 {
        state.apply_tool_argument_stream(
            &turn,
            &pending(
                Identity {
                    round: 2,
                    item_id: format!("i{i}"),
                    call_id: "call".into(),
                },
                "unknown-tool",
                &"界".repeat(4096),
            ),
        );
    }
    assert_eq!(state.pending_tool_seen.len(), PENDING_TOOL_MAX);
    assert!(state.live_parts.len() <= super::LIVE_PARTS_MAX);
    // GenericTool's streaming row is the inline spinner and bare tool name.
    state.viewport();
    state.finish_transcript_projection().await;
    let shown = state.viewport();
    assert!(
        shown
            .iter()
            .any(|line| line.trim_end() == "   ⠋ unknown-tool"),
        "{shown:?}"
    );
    assert!(
        !shown
            .iter()
            .any(|line| line.contains('┃') || line.contains('界'))
    );
    // Evicted identity snapshots cannot resurrect a second live card.
    for i in 0..super::LIVE_PARTS_MAX {
        state
            .live_parts
            .push(super::LivePart::Text(format!("tail-{i}").into()));
    }
    state.enforce_parts();
    let count = state.live_parts.len();
    state.apply_tool_argument_stream(
        &turn,
        &pending(
            Identity {
                round: 2,
                item_id: "i0".into(),
                call_id: "call".into(),
            },
            "unknown-tool",
            "late",
        ),
    );
    assert_eq!(state.live_parts.len(), count);
    assert!(!state.transcript_rows().iter().any(|row| {
        row.tool
            .as_ref()
            .is_some_and(|card| card.state == "argument_stream")
    }));
    let offset = state.live_part_offset;
    state.apply_tool_argument_stream(&turn, &Event::Clear { round: 2 });
    assert_eq!(
        state.live_part_offset, offset,
        "clearing slots does not renumber retained reasoning"
    );
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        0
    );
    state.apply_tool_argument_stream(
        &turn,
        &pending(
            Identity {
                round: 3,
                item_id: "cancel".into(),
                call_id: "call".into(),
            },
            "apply_patch",
            "{}",
        ),
    );
    let slots = state.live_parts.len();
    let offset = state.live_part_offset;
    state.apply_tool_argument_stream(&turn, &Event::Clear { round: 3 });
    assert_eq!(
        (state.live_parts.len(), state.live_part_offset),
        (slots, offset)
    );
    assert!(matches!(
        state.live_parts.last(),
        Some(super::LivePart::Vacant)
    ));
    state.apply_tool_argument_stream(
        &turn,
        &pending(
            Identity {
                round: 4,
                item_id: "cancel".into(),
                call_id: "call".into(),
            },
            "apply_patch",
            "{}",
        ),
    );
    state.apply_interrupted(&turn, "", 1);
    assert!(!state.transcript_rows().iter().any(|row| {
        row.tool
            .as_ref()
            .is_some_and(|card| card.state == "argument_stream")
    }));
    state.active_turn = Some(turn.clone());
    state.apply_tool_argument_stream(
        &turn,
        &pending(
            Identity {
                round: 1,
                item_id: "failure".into(),
                call_id: "call".into(),
            },
            "apply_patch",
            "{}",
        ),
    );
    state.apply_failed(
        &turn,
        &oc_core::session::CoreError::Application("intent refused".into()),
    );
    assert!(!state.transcript_rows().iter().any(|row| {
        row.tool
            .as_ref()
            .is_some_and(|card| card.state == "argument_stream")
    }));
}

/// A stale turn can never grow cards into the transcript.
#[tokio::test]
async fn stale_tool_events_are_ignored() {
    let mut state = fresh_state("s-tools-stale").await;
    state.active_turn = Some(WorkerTurnId("t-live".to_string()));
    state.status = TuiStatus::Streaming;
    let stale = WorkerTurnId("t-stale".to_string());
    state.apply_tool_started(
        &stale,
        "op",
        "bash",
        &serde_json::json!({"argv": ["ls"]}).to_string(),
    );
    state.apply_tool_finished(&stale, "op", "bash", "completed", "ok", 2, false);
    assert!(state.live_parts.is_empty());
    assert!(state.transcript_rows().is_empty());
}

#[tokio::test]
async fn cancel_releases_turn() {
    let (app, guard) = CoreApp::spawn(MockProvider::fixed(
        (0..20).map(|i| format!("t{i} ")).collect(),
        10,
    ));
    std::mem::forget(guard);
    app.create_session(sid("s-cancel")).await.expect("create");
    let mut state = TuiState::new(app.clone(), sid("s-cancel"));
    let mut driver = ScriptDriver::attach(&app);

    type_text(&mut state, "long").await;
    state.handle_key(KeyAction::Enter).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    state.poll_submission();
    assert_eq!(state.status(), &TuiStatus::Streaming);
    assert!(state.active_turn.is_some());
    assert!(!state.interrupt_armed());
    assert_eq!(state.handle_key(KeyAction::Cancel).await.note, None);
    assert!(state.interrupt_armed());
    assert!(state.is_busy(), "the first Esc must not cancel the owner");

    // The first press cannot remain armed forever or be replayed across
    // turns. Simulate the original five-second timeout without sleeping.
    state.interrupt_armed_until = Some(Instant::now() - Duration::from_millis(1));
    state.tick_toast(Instant::now());
    assert!(!state.interrupt_armed());
    state.handle_key(KeyAction::Cancel).await;
    assert!(
        state.interrupt_armed(),
        "expired Esc re-arms, not interrupts"
    );
    let outcome = state.handle_key(KeyAction::Cancel).await;
    assert_eq!(outcome.note, None);
    let turn = state.active_turn.clone();
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::CancelSession {
            session: sid("s-cancel"),
            turn: turn.clone(),
            cancel_compaction: false,
        })
    );
    app.cancel_turn(sid("s-cancel"), turn.unwrap())
        .await
        .unwrap();
    assert!(!state.interrupt_armed());

    let outcome = driver
        .pump_until_idle(&mut state, Duration::from_secs(5))
        .await;
    assert!(matches!(outcome, PumpOutcome::Interrupted(_)));
    assert_eq!(state.status(), &TuiStatus::Cancelled);
    assert!(!state.is_busy());
    // Interrupted turn: partial text plus the footer's `interrupted` marker
    // (upstream `Step interrupted`, `routes/session/index.tsx:1955,1977-1980`).
    assert!(
        state
            .viewport()
            .iter()
            .any(|line| line.contains("interrupted")),
        "{:?}",
        state.viewport()
    );
    // Esc after the interrupted turn is idle Esc: it neither exits nor re-arms.
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.status(), &TuiStatus::Cancelled);
    assert!(!state.interrupt_armed());

    // The same handle accepts the next prompt.
    type_text(&mut state, "after").await;
    state.handle_key(KeyAction::Enter).await;
    let outcome = driver
        .pump_until_idle(&mut state, Duration::from_secs(5))
        .await;
    let expected: String = (0..20).map(|i| format!("t{i} ")).collect();
    assert_eq!(outcome, PumpOutcome::Finished(expected));
    assert_eq!(state.status(), &TuiStatus::Idle);
}

/// Streamed text and reasoning appends repaint but never rebuild tool-card
/// spinner membership; a new card still does.
#[tokio::test]
async fn streamed_text_deltas_do_not_rescan_tool_spinner_membership() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("text-membership"));
    let turn = WorkerTurnId("parent".into());
    state.begin_linked_turn(turn.clone());
    state.apply_tool_started(&turn, "op-a", "bash", r#"{"command":"sleep 1"}"#);
    let now = Instant::now();
    state.tick_tool_spinners(now);
    let discoveries = state.tool_spinner_membership_builds;
    for index in 0..50 {
        state.apply_delta(&turn, &format!("text {index} "));
        state.tick_tool_spinners(now);
    }
    state.apply_reasoning_delta(&turn, "thinking");
    state.tick_tool_spinners(now);
    let after_freeze = state.tool_spinner_membership_builds;
    for index in 0..50 {
        state.apply_reasoning_delta(&turn, &format!("more {index} "));
        state.tick_tool_spinners(now);
    }
    assert!(
        after_freeze <= discoveries + 1,
        "only the text freeze is structural"
    );
    assert_eq!(state.tool_spinner_membership_builds, after_freeze);
    state.apply_tool_started(&turn, "op-b", "bash", r#"{"command":"sleep 2"}"#);
    state.tick_tool_spinners(now);
    assert_eq!(state.tool_spinner_membership_builds, after_freeze + 1);
    assert_eq!(state.tool_spinner_clocks.len(), 2);
}
