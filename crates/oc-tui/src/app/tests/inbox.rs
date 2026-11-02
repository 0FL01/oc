use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use oc_core::core_app::InboxMsg;
use oc_core::queries::{InboxAction, InboxDelivery, InboxItem, InboxSnapshot};

const FRAME: Rect = Rect::new(0, 0, 120, 40);

fn steer(id: &str) -> InboxItem {
    InboxItem {
        id: id.into(),
        delivery: InboxDelivery::Steer,
        text: "identical pending input".into(),
        images: 0,
        error: None,
    }
}

fn open_steer(state: &mut TuiState, id: &str) {
    crate::views::render_test(state, FRAME.width, FRAME.height);
    let rect = crate::shell::transcript_area(state, FRAME);
    let (_, _, _, targets) =
        state.visible_transcript_at_viewport_with_targets(rect.width, FRAME.width, rect.height);
    let y = rect.y
        + targets
            .iter()
            .position(|target| {
                target
                    .as_ref()
                    .is_some_and(|t| t.inbox_id.as_deref() == Some(id))
            })
            .unwrap() as u16;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        assert!(
            state
                .handle_mouse(selection_mouse(kind, rect.x + 4, y), FRAME)
                .intent
                .is_none()
        );
    }
    assert_eq!(
        state.panel(),
        &TuiPanel::PendingSteer {
            id: id.into(),
            accepted: false
        }
    );
}

fn option_point(state: &TuiState) -> (u16, u16) {
    let options = state.modal_options();
    for y in 0..FRAME.height {
        for x in 0..FRAME.width {
            if state.select.hit(
                FRAME,
                crate::dialog::size_for(state.panel()),
                &options,
                x,
                y,
            ) == crate::dialog::DialogHit::Option(0)
            {
                return (x, y);
            }
        }
    }
    panic!("first option");
}

fn modal_mouse(state: &mut TuiState, kind: MouseEventKind, point: (u16, u16)) -> KeyOutcome {
    state.handle_mouse(selection_mouse(kind, point.0, point.1), FRAME)
}

fn inbox_intent(action: InboxAction) -> Option<PanelIntent> {
    Some(PanelIntent::Inbox {
        action,
        restore: false,
    })
}

#[tokio::test]
async fn pending_steer_click_uses_exact_id_and_actions_preserve_draft() {
    let (mut state, mut owner) = running("pending-modal").await;
    state.apply_inbox(&sid("pending-modal"), vec![steer("one"), steer("two")]);
    state.restore_prompt("unsent Ω draft".into());
    state.editor.move_to(7, false);
    let draft = (
        state.editor.prompt(state.input()),
        state.editor.cursor,
        state.editor.anchor,
    );
    open_steer(&mut state, "two");
    let shown = crate::views::render_test(&state, FRAME.width, FRAME.height).join("\n");
    for text in ["Pending steer", "Search", "Move to queue", "Delete"] {
        assert!(shown.contains(text), "{shown}");
    }
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        inbox_intent(InboxAction::Deliver {
            id: "two".into(),
            delivery: InboxDelivery::Queue
        })
    );
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(
        (
            state.editor.prompt(state.input()),
            state.editor.cursor,
            state.editor.anchor
        ),
        draft
    );
    // A raw terminal Down belongs to SelectList, not composer/history navigation.
    state.click = None;
    open_steer(&mut state, "two");
    for code in [KeyCode::Down, KeyCode::Up, KeyCode::Down] {
        let action = state
            .terminal_key(KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap();
        assert!(state.handle_key(action).await.intent.is_none());
    }
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        state.handle_key(action).await.intent,
        inbox_intent(InboxAction::Remove { id: "two".into() })
    );
    assert_eq!(
        (
            state.editor.prompt(state.input()),
            state.editor.cursor,
            state.editor.anchor
        ),
        draft
    );
    assert!(
        owner.try_recv().is_err(),
        "no Cancel or composer submission"
    );
}

#[tokio::test]
async fn pending_steer_search_and_mouse_require_current_paint_and_undragged_press() {
    let (mut state, _) = running("pending-search").await;
    state.apply_inbox(&sid("pending-search"), vec![steer("two")]);
    open_steer(&mut state, "two");
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    let point = option_point(&state);
    // Search remaps the same physical first row from Move to Delete before paint.
    for c in "del".chars() {
        state.handle_panel_key(KeyAction::Char(c));
    }
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    modal_mouse(&mut state, MouseEventKind::Drag(MouseButton::Left), point);
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    assert_eq!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point).intent,
        inbox_intent(InboxAction::Remove { id: "two".into() })
    );

    state.panel = TuiPanel::PendingSteer {
        id: "two".into(),
        accepted: false,
    };
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    for c in "del".chars() {
        state.handle_panel_key(KeyAction::Char(c));
    }
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        inbox_intent(InboxAction::Remove { id: "two".into() })
    );
    state.panel = TuiPanel::PendingSteer {
        id: "two".into(),
        accepted: false,
    };
    for c in "zzz".chars() {
        state.handle_panel_key(KeyAction::Char(c));
    }
    assert!(state.modal_options().is_empty());
    assert!(state.handle_panel_key(KeyAction::Enter).intent.is_none());
    assert!(state.handle_panel_key(KeyAction::Cancel).intent.is_none());
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn pending_steer_delivery_is_exact_sticky_and_never_dismisses_the_modal() {
    let (mut state, _) = running("pending-accepted").await;
    let session = sid("pending-accepted");
    state.apply_inbox(&session, vec![steer("two")]);
    open_steer(&mut state, "two");
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    let point = option_point(&state);
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    state.inbox_delivered(&sid("other"), ["two"]);
    state.inbox_delivered(&session, ["one"]);
    assert_eq!(state.modal_options().len(), 2);
    state.apply_inbox(&session, Vec::new());
    assert!(
        crate::views::render_test(&state, FRAME.width, FRAME.height)
            .join("\n")
            .contains("No longer pending")
    );
    assert!(state.handle_panel_key(KeyAction::Enter).intent.is_none());
    state.inbox_delivered(&session, ["two"]);
    state.apply_inbox(&session, vec![steer("two")]);
    let shown = crate::views::render_test(&state, FRAME.width, FRAME.height).join("\n");
    assert!(shown.contains("Accepted by the agent"), "{shown}");
    assert!(!shown.contains("Move to queue") && !shown.contains("Delete"));
    assert!(state.modal_options().is_empty());
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
    assert!(state.handle_panel_key(KeyAction::Enter).intent.is_none());
    assert_eq!(
        state.panel(),
        &TuiPanel::PendingSteer {
            id: "two".into(),
            accepted: true
        }
    );
    assert!(state.handle_panel_key(KeyAction::Cancel).intent.is_none());
    assert!(state.active_turn().is_some());
}

#[tokio::test]
async fn pending_steer_defers_to_visible_approval_without_closing() {
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    let (mut state, _) = running("pending-approval").await;
    state.apply_inbox(&sid("pending-approval"), vec![steer("two")]);
    open_steer(&mut state, "two");
    crate::views::render_test(&state, FRAME.width, FRAME.height);
    let point = option_point(&state);
    modal_mouse(&mut state, MouseEventKind::Down(MouseButton::Left), point);
    state.approvals.reconcile(vec![(
        ApprovalRequest {
            id: 7,
            binding: ApprovalBinding {
                session: "pending-approval".into(),
                turn: "running".into(),
                call: "call".into(),
                operation: "op".into(),
                input_digest: "digest".into(),
                location: "location".into(),
                generation: 1,
                agent: None,
                agent_digest: None,
            },
            project: "project".into(),
            action: "fixture.tool".into(),
            resources: vec!["*".into()],
            save_patterns: vec![],
            preview: ApprovalPreview::Tool,
        },
        false,
    )]);
    let shown = crate::views::render_test(&state, FRAME.width, FRAME.height).join("\n");
    assert!(
        shown.contains("Tool: fixture.tool") && !shown.contains("Pending steer"),
        "{shown}"
    );
    assert!(!state.select.painted_matches(
        FRAME,
        crate::dialog::size_for(state.panel()),
        &state.modal_options()
    ));
    assert_eq!(
        state.mouse_down.get(),
        None,
        "hidden paint retires the old press even if another client resolves approval"
    );
    for code in [KeyCode::Esc, KeyCode::Enter] {
        let action = state
            .terminal_key(KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(
            state.handle_key(action).await.intent,
            Some(PanelIntent::ReplyApproval(_))
        ));
        assert!(matches!(state.panel(), TuiPanel::PendingSteer { .. }));
    }
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
    state.approvals.reconcile(vec![]);
    assert!(
        crate::views::render_test(&state, FRAME.width, FRAME.height)
            .join("\n")
            .contains("Pending steer")
    );
    assert!(
        modal_mouse(&mut state, MouseEventKind::Up(MouseButton::Left), point)
            .intent
            .is_none()
    );
}

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
    let theme = Theme::dark();
    for selected in [false, true] {
        let mut catalog = snapshot();
        if selected {
            catalog.agent_id = Some("y".into());
            catalog
                .chrome
                .agent_colors
                .insert("y".into(), "#12ab34".into());
        }
        state.apply_catalog(catalog);
        let agent = state.agent_color(state.active_agent.as_deref());
        let full = state.transcript_lines(80, 80);
        let (indexed, _) = state.visible_transcript(80, 80, 40);
        for lines in [full, indexed] {
            let borders: Vec<_> = lines
                .iter()
                .flat_map(|line| line.spans())
                .filter(|span| span.content() == "┃")
                .map(|span| span.style().fg)
                .collect();
            assert_eq!(
                borders,
                [Some(agent); 3]
                    .into_iter()
                    .chain([Some(theme.border()); 3])
                    .collect::<Vec<_>>(),
                "pending steer keeps its neutral border when the profile changes"
            );
        }
    }
    state.apply_inbox(&sid("steer"), Vec::new());
    assert!(
        !state
            .transcript_rows()
            .iter()
            .any(|row| row.text == "look at tests too" && row.message_id.is_none())
    );
    let mut prompt = msg(1, Role::User, "first");
    prompt.turn = Some(oc_core::queries::HistoryTurn {
        id: state.active_turn().unwrap().0.clone(),
        ..Default::default()
    });
    let delivered = msg(2, Role::User, "look at tests too");
    let recent = page(vec![prompt, delivered.clone()], 2, false, false);
    state.refresh_child_notice_page(&recent);
    state.insert_steered_rows(&recent, std::slice::from_ref(&delivered.id));
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.message_id.as_deref() == Some(&delivered.id))
            .count(),
        1
    );
    assert!(
        user_border_colors(&state)
            .iter()
            .all(|color| *color == state.agent_color(state.active_agent.as_deref())),
        "delivered steer takes the ordinary user-message color"
    );
}

#[tokio::test]
async fn enter_during_compaction_queues_without_starting_a_turn() {
    use oc_core::compaction::{
        CompactionAnchor, CompactionReason, CompactionSnapshot, CompactionState,
    };

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, sid("compacting"));
    state.apply_catalog(snapshot());
    let revision = state.begin_compaction_admission();
    type_text(&mut state, "after compact").await;
    let (outcome, admitted) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert!(outcome.note.is_none());
    assert_eq!(admitted, ("after compact".into(), InboxDelivery::Queue));
    assert!(state.active_turn().is_none());
    assert!(!state.has_occupied_submission());
    assert!(state.input().is_empty());
    assert!(
        state
            .transcript_rows()
            .iter()
            .any(|row| { row.role == "queued" && row.text == "1 queued · after compact" })
    );

    let mut compact = CompactionSnapshot {
        model: None,
        profile: None,
        anchor: CompactionAnchor::default(),
        id: "compact".into(),
        session: "compacting".into(),
        reason: CompactionReason::Manual,
        state: CompactionState::Queued,
        summary: String::new(),
        usage: None,
        provider_native: false,
        error: None,
    };
    state.compaction_admitted(compact.clone(), revision);
    compact.state = CompactionState::Running;
    state.apply_compaction(compact.clone());
    type_text(&mut state, "then another").await;
    let (_, admitted) = tokio::join!(
        state.handle_key(KeyAction::Enter),
        admit(&mut inbox, Ok(()))
    );
    state.poll_submission();
    assert_eq!(admitted, ("then another".into(), InboxDelivery::Queue));
    assert_eq!(state.inbox_items().len(), 2);
    assert!(state.active_turn().is_none());
    assert!(!state.has_occupied_submission());

    compact.state = CompactionState::Completed;
    state.apply_compaction(compact);
    state.apply_inbox(&sid("compacting"), Vec::new());
    assert!(!state.is_busy());
    type_text(&mut state, "next ordinary prompt").await;
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { text, ack, .. } = inbox.try_recv().unwrap() else {
        panic!("ordinary submission after compaction")
    };
    assert_eq!(text.text, "next ordinary prompt");
    ack.send(Ok(WorkerTurnId("next".into()))).unwrap();
    state.poll_submission();
    assert!(!state.has_occupied_submission());
    assert!(inbox.try_recv().is_err());
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
    assert_eq!(
        &user_border_colors(&state)[3..],
        &[Theme::dark().border(); 3],
        "idle does not make an undelivered steer an accepted prompt"
    );
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
