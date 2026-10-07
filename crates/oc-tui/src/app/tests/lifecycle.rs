use super::*;

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
    assert_eq!(text, "retry me");
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
    assert_eq!(state.input(), "retry me");
    assert!(state.history().rows().is_empty());
    assert_eq!(state.status(), &TuiStatus::Idle);
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
    assert_eq!(state.input(), "first prompt edited");
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
    let cancel = tokio::spawn(async move {
        let Some(InboxMsg::Cancel { session, ack }) = inbox.recv().await else {
            panic!("candidate cancel")
        };
        assert_eq!(session, candidate);
        ack.send(Ok(())).unwrap();
        submit_ack
            .send(Ok(WorkerTurnId("accepted-after-cancel".into())))
            .unwrap();
        session
    });
    assert_eq!(state.handle_key(KeyAction::Cancel).await.note, None);
    let candidate = cancel.await.unwrap();
    await_submission(&mut state).await;
    assert_eq!(state.attached_session(), Some(&candidate));
    assert_eq!(
        state.input(),
        "cancel me",
        "cancel keeps the editable draft"
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
    assert_eq!(text, "original");
    state.handle_key(KeyAction::Enter).await;
    assert!(inbox.try_recv().is_err(), "duplicate not enqueued");
    type_text(&mut state, " edited").await;
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(
        state.input(),
        "original edited",
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
    assert_eq!(state.input(), "new generation draft edited after enqueue");
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
    state.reset_workspace();
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(!state.is_busy());
    assert_eq!(state.input(), "retained draft");
    assert!(ack.send(Ok(WorkerTurnId("old".into()))).is_err());
    state.poll_submission();
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "retained draft");
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
    assert_eq!(state.input(), "retry me");
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("retry")
    };
    state.handle_key(KeyAction::Quit).await;
    ack.send(Ok(WorkerTurnId("accepted-at-quit".into())))
        .unwrap();
    state.poll_submission();
    assert_eq!(state.status(), &TuiStatus::Quit);
    assert_eq!(state.input(), "retry me");
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
    assert_eq!(state.input(), "hi");
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
    assert_eq!(normal_work, 1, "one indexed viewport traversal per draw");
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
        (normal_work, bright),
        "streaming hover keeps the clipped header and re-indexes once"
    );
    state.live_text.clear();
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, rect.x + 2, y), frame);
    assert_eq!(draw(&state), (normal_work, faded), "padding is not a hit");
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
    assert!(state.live_preview_limited());
    assert!(
        !state
            .viewport()
            .iter()
            .any(|line| line.contains("Live preview truncated"))
    );
    assert!(state.retained_bytes() <= 2 * WINDOW_BYTES + MAX_INPUT_BYTES);
    assert!(state.viewport().len() <= VIEWPORT_LINES);
    state.active_turn = None;
    assert!(
        !state.live_preview_limited(),
        "a completed answer does not retain a live viewing alert"
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
    let LivePart::Tool { card, input } = &state.live_parts[0] else {
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
    assert!(
        shown.contains("Patch · arguments streaming · no effects yet"),
        "{shown}"
    );
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
    assert!(
        state
            .viewport()
            .join("\n")
            .contains("unknown-tool · arguments streaming · no effects yet")
    );
    // Evicted identity snapshots cannot resurrect a second live card.
    for i in 0..super::LIVE_PARTS_MAX {
        state
            .live_parts
            .push(super::LivePart::Text(format!("tail-{i}")));
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
