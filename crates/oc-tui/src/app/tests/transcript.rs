use super::*;

#[tokio::test]
async fn child_notice_order_metadata_only_refresh_paths_enforce_the_window_budget() {
    use crate::history::WINDOW_BYTES;
    use oc_core::queries::{
        HistoryNoticeOrder, HistoryNoticePlacement, HistoryTurn, NoticePlacement,
    };
    for detached in [false, true] {
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("metadata-cap"));
        let prompt = msg(1, Role::User, &"x".repeat(WINDOW_BYTES / 2));
        let other = msg(2, Role::User, &"y".repeat(WINDOW_BYTES / 2 - 4096));
        state.attach_page(&page(vec![prompt.clone(), other], 2, false, false));
        assert!(state.window.retained_bytes() <= WINDOW_BYTES);
        let mut update = if detached {
            msg(10, Role::Assistant, "")
        } else {
            let mut small = prompt.clone();
            small.text = "prompt".into();
            small
        };
        update.turn = Some(HistoryTurn {
            id: "metadata-turn".into(),
            revision: 2,
            notice_order: Some(HistoryNoticeOrder {
                accepted_prompt: prompt.id.clone(),
                notices: vec![HistoryNoticePlacement {
                    message: oc_core::session::MessageId("n".repeat(8192)),
                    placement: NoticePlacement::Tail,
                }],
            }),
            ..Default::default()
        });
        if detached {
            state
                .window
                .refresh_completed(&page(vec![update], 2, false, false), true);
        } else {
            state.refresh_child_notice_page(&page(vec![update], 2, false, false));
        }
        assert!(
            state.window.retained_bytes() <= WINDOW_BYTES,
            "sidecar-only refresh bypassed the cap: detached={detached}"
        );
    }
}

#[tokio::test]
async fn child_notice_order_tracks_the_consuming_step_live_final_and_reopen() {
    use oc_core::queries::{
        ChildHistory, ChildJob, ChildState, HistoryNoticeOrder, HistoryNoticePlacement,
        HistoryPartWindow, HistoryTurn, NoticePlacement, PartState, TranscriptPart,
    };
    use oc_core::session::MessageId;
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), sid("parent"));
    let turn = WorkerTurnId("parent-turn".into());
    let mut prompt = msg(8, Role::User, "ordinary prompt");
    let mut notice = msg(9, Role::User, "raw notice stays immutable");
    notice.id = MessageId("notice-B".into());
    notice.child = Some(ChildHistory::Notice(Box::new(ChildJob {
        parent: sid("parent"),
        child: sid("child-B"),
        operation: "B".into(),
        generation: 1,
        location: "/fixture".into(),
        agent: "explore".into(),
        model: "fixture/model".into(),
        description: "Task B".into(),
        delivery_id: "delivery-B".into(),
        state: ChildState::Completed,
        background: true,
        turn: Some("child-turn".into()),
        result: None,
        message_id: Some("notice-B".into()),
    })));
    let prompt_id = prompt.id.clone();
    let order = |placement| HistoryNoticeOrder {
        accepted_prompt: prompt_id.clone(),
        notices: vec![HistoryNoticePlacement {
            message: notice.id.clone(),
            placement,
        }],
    };
    state.attach_page(&page(vec![prompt.clone()], 1, false, false));
    state.begin_linked_turn(turn.clone());
    state.apply_delta(&turn, "open answer\n\n");
    prompt.turn = Some(HistoryTurn {
        id: turn.0.clone(),
        status: "started".into(),
        notice_order: Some(order(NoticePlacement::Tail)),
        ..Default::default()
    });
    state.refresh_child_notice_page(&page(vec![prompt.clone(), notice.clone()], 2, false, false));
    let (rows, live) = state.compose_transcript();
    assert_eq!(rows.last().unwrap().message_id.as_deref(), Some(&notice.id));
    assert_eq!(rows[live.unwrap()].text, "open answer\n\n");
    assert_ne!(live, Some(rows.len() - 1));
    let projection = |placement| HistoryTurn {
        id: turn.0.clone(),
        status: "completed".into(),
        notice_order: Some(order(placement)),
        window: Some(HistoryPartWindow {
            start: 0,
            end: 2,
            total: 2,
        }),
        parts: vec![
            TranscriptPart::Text("step before notice".into()),
            TranscriptPart::Text("step consuming notice".into()),
        ],
        part_states: (0..2)
            .map(|sequence| PartState {
                sequence,
                status: "completed".into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    state.apply_finished(&turn, "open answer\n\n", 1);
    assert!(
        state
            .transcript_rows()
            .last()
            .unwrap()
            .child_notice
            .is_some(),
        "local completed fallback keeps proven Tail before canonical refresh"
    );
    let mut answer = msg(10, Role::Assistant, "");
    let mut later_notice = notice.clone();
    later_notice.id = MessageId("notice-A".into());
    later_notice.seq = 11;
    if let Some(ChildHistory::Notice(job)) = &mut later_notice.child {
        job.child = sid("child-A");
        job.operation = "A".into();
        job.description = "Task A".into();
        job.delivery_id = "delivery-A".into();
        job.message_id = Some(later_notice.id.0.clone());
    }
    for native_successor in [false, true] {
        if native_successor {
            // A separate canonical scenario: do not replace the preceding
            // case's genuine user message with a native message at its ID.
            state = TuiState::new(app.clone(), sid("parent"));
            let mut ordinary = prompt.clone();
            ordinary.turn = None;
            answer.turn = Some(projection(NoticePlacement::Tail));
            state.attach_page(&page(
                vec![
                    ordinary,
                    notice.clone(),
                    answer.clone(),
                    later_notice.clone(),
                ],
                4,
                false,
                false,
            ));
        }
        for placement in [
            NoticePlacement::Tail,
            NoticePlacement::BeforeRequest {
                span: "next-step".into(),
                before_part_sequence: 1,
            },
        ] {
            answer.turn = Some(projection(placement.clone()));
            let mut ordinary = prompt.clone();
            ordinary.turn = None;
            let mut canonical_rows = vec![
                ordinary.clone(),
                notice.clone(),
                answer.clone(),
                later_notice.clone(),
            ];
            let mut control = msg(12, Role::User, "");
            control.child = Some(ChildHistory::NativeControl);
            let mut own_notice = notice.clone();
            own_notice.id = MessageId("notice-C".into());
            own_notice.seq = 13;
            if let Some(ChildHistory::Notice(job)) = &mut own_notice.child {
                job.child = sid("child-C");
                job.operation = "C".into();
                job.description = "Task C".into();
                job.delivery_id = "delivery-C".into();
                job.message_id = Some(own_notice.id.0.clone());
            }
            if native_successor {
                let mut followup = msg(14, Role::Assistant, "");
                followup.turn = Some(HistoryTurn {
                    id: "automatic-turn".into(),
                    status: "completed".into(),
                    notice_order: Some(HistoryNoticeOrder {
                        accepted_prompt: control.id.clone(),
                        notices: vec![HistoryNoticePlacement {
                            message: own_notice.id.clone(),
                            placement: NoticePlacement::Tail,
                        }],
                    }),
                    parts: vec![TranscriptPart::Text("native followup".into())],
                    part_states: vec![PartState {
                        sequence: 0,
                        status: "completed".into(),
                        ..Default::default()
                    }],
                    window: Some(HistoryPartWindow {
                        start: 0,
                        end: 1,
                        total: 1,
                    }),
                    ..Default::default()
                });
                canonical_rows.extend([
                    control.clone(),
                    own_notice.clone(),
                    followup,
                    msg(15, Role::User, "later prompt"),
                ]);
            } else {
                canonical_rows.push(msg(12, Role::User, "later prompt"));
            }
            let total = canonical_rows.len();
            let canonical = page(canonical_rows, total, false, false);
            state.refresh_completed_page(&canonical);
            let mut reopened = TuiState::new(app.clone(), sid("parent"));
            reopened.attach_page(&canonical);
            for view in [&state, &reopened] {
                let rows = view.transcript_rows();
                let at = rows
                    .iter()
                    .position(|row| row.child_notice.is_some())
                    .unwrap();
                let before = rows
                    .iter()
                    .position(|row| row.text == "step before notice")
                    .unwrap();
                let after = rows
                    .iter()
                    .position(|row| row.text == "step consuming notice")
                    .unwrap();
                assert!(
                    before < at,
                    "native_successor={native_successor}, placement={placement:?}"
                );
                assert_eq!(
                    at < after,
                    matches!(placement, NoticePlacement::BeforeRequest { .. })
                );
                assert!(
                    at < rows
                        .iter()
                        .position(|row| row.text == "later prompt")
                        .unwrap()
                );
                assert_eq!(
                    rows.iter().filter(|row| row.child_notice.is_some()).count(),
                    if native_successor { 3 } else { 2 }
                );
                let later = rows
                    .iter()
                    .position(|row| row.message_id.as_deref() == Some(&later_notice.id))
                    .unwrap();
                assert!(
                    at < later && after < later,
                    "an earlier Tail must not rotate past a later post-response notice"
                );
                if native_successor {
                    let boundary = rows
                        .iter()
                        .position(|row| {
                            row.message_id.as_deref() == Some(&control.id)
                                && row.role == "native_control"
                        })
                        .expect("native input retains an invisible canonical boundary");
                    let body = rows
                        .iter()
                        .position(|row| row.text == "native followup")
                        .unwrap();
                    let own = rows
                        .iter()
                        .position(|row| row.message_id.as_deref() == Some(&own_notice.id))
                        .unwrap();
                    assert!(
                        later < boundary && boundary < body && body < own,
                        "automatic turn owns its Tail without absorbing predecessor notices"
                    );
                }
                assert_eq!(view.compose_transcript().1, None);
            }
        }
    }
    // Partial/unqualified history must not guess a placement.
    answer.turn.as_mut().unwrap().notice_order = None;
    prompt.turn = None; // The settled turn belongs to its assistant message.
    state.attach_page(&page(
        vec![prompt.clone(), notice.clone(), answer],
        3,
        false,
        false,
    ));
    assert_eq!(
        state.transcript_rows()[1].message_id.as_deref(),
        Some(&notice.id)
    );
}

#[tokio::test]
async fn child_notice_order_keeps_empty_request_frontier_and_local_terminal_ownership() {
    use oc_core::queries::{
        AssistantSpan, HistoryNoticeOrder, HistoryNoticePlacement, HistoryPartWindow, HistoryTurn,
        NoticePlacement, PartState, ToolOpView, TranscriptPart,
    };
    for terminal in ["completed", "interrupted", "failed"] {
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), sid("parent"));
        let turn = WorkerTurnId("frontier".into());
        let mut prompt = msg(1, Role::User, "request");
        let mut notice = msg(2, Role::User, "notice");
        notice.child = Some(oc_core::queries::ChildHistory::Notice(Box::new(
            oc_core::queries::ChildJob {
                parent: sid("parent"),
                child: sid("child"),
                operation: "launch".into(),
                generation: 1,
                location: "/fixture".into(),
                agent: "explore".into(),
                model: "fixture/model".into(),
                description: "task".into(),
                delivery_id: "delivery".into(),
                state: oc_core::queries::ChildState::Completed,
                background: true,
                turn: None,
                result: None,
                message_id: Some(notice.id.0.clone()),
            },
        )));
        let mut projection = HistoryTurn {
            id: turn.0.clone(),
            status: "started".into(),
            revision: 1,
            parts: vec![TranscriptPart::Tool(ToolOpView {
                op:"overlap-launch".into(),name:"subagent".into(),state:"started".into(),
                input:Some(r#"{"agent":"explore","description":"Retained launch","prompt":"controlled"}"#.into()),
                child_job:None,question:None,rowid:1,output:None,output_bytes:0,output_truncated:false,output_presentation:None,patch_effects:None,dcp:None,dcp_topic:None,
            }),TranscriptPart::Text("previous step".into())],
            part_states: (0..2).map(|sequence| PartState {sequence,..Default::default()}).collect(),
            window: Some(HistoryPartWindow {
                start: 0,
                end: 2,
                total: 2,
            }),
            notice_order: Some(HistoryNoticeOrder {
                accepted_prompt: prompt.id.clone(),
                notices: vec![HistoryNoticePlacement {
                    message: notice.id.clone(),
                    placement: NoticePlacement::BeforeRequest {
                        span: "next".into(),
                        before_part_sequence: 2,
                    },
                }],
            }),
            spans: vec![AssistantSpan {
                id: "next".into(),
                status: "started".into(),
                step: 2,
                started: 1,
                completed: None,
                request: None,
                retry: None,
                error: None,
                finish: None,
            }],
            ..Default::default()
        };
        prompt.turn = Some(projection.clone());
        state.attach_page(&page(vec![prompt.clone(), notice.clone()], 2, false, false));
        state.begin_linked_turn(turn.clone());
        state.apply_presentation(&turn, &projection);
        let (rows, live) = state.compose_transcript();
        assert!(
            rows.last().unwrap().child_notice.is_some(),
            "exact empty request frontier follows the previous step"
        );
        assert_eq!(live, None);
        state.apply_delta(&turn, "next step");
        let (rows, live) = state.compose_transcript();
        assert!(
            rows[live.unwrap() - 1].child_notice.is_some(),
            "notice precedes the first delta of its consuming span"
        );
        // A delivered but unconsumed notice is also stable through each local
        // terminal fallback, which may be painted without a canonical refresh.
        projection.revision = 2;
        projection.notice_order.as_mut().unwrap().notices[0].placement = NoticePlacement::Tail;
        prompt.turn = Some(projection.clone());
        let mut state = TuiState::new(app, sid("parent"));
        state.attach_page(&page(vec![prompt, notice], 2, false, false));
        state.begin_linked_turn(turn.clone());
        state.apply_presentation(&turn, &projection);
        state.apply_delta(&turn, "next step");
        state.apply_tool_finished(
            &turn,
            "overlap-launch",
            "subagent",
            "completed",
            "latest result",
            13,
            false,
        );
        match terminal {
            "completed" => state.apply_finished(&turn, "next step", 1),
            "interrupted" => state.apply_interrupted(&turn, "next step", 1),
            "failed" => state.apply_failed(
                &turn,
                &oc_core::session::CoreError::Application("fixture failure".into()),
            ),
            _ => unreachable!(),
        }
        assert!(
            state
                .transcript_rows()
                .last()
                .unwrap()
                .child_notice
                .is_some(),
            "local {terminal} fallback lost its response ownership"
        );
        assert_eq!(state.compose_transcript().1, None);
        let rows = state.transcript_rows();
        assert_eq!(
            rows.iter()
                .filter(|row| row.text == "previous step")
                .count(),
            1,
            "canonical prefix duplicated on {terminal}"
        );
        let cards: Vec<_> = rows.iter().filter_map(|row| row.tool.as_ref()).collect();
        assert_eq!(
            cards.len(),
            1,
            "source-mapped launch duplicated on {terminal}"
        );
        assert_eq!(cards[0].state, "completed");
        assert_eq!(cards[0].output_preview, "latest result");
        assert_eq!(
            rows.iter().filter(|row| row.meta.is_some()).count(),
            1,
            "terminal footer duplicated on {terminal}"
        );
        assert_eq!(
            rows.iter().filter(|row| row.child_notice.is_some()).count(),
            1
        );
        assert!(state.window.older_part_cursor().is_none());
        assert!(state.window.newer_part_cursor().is_none());
    }
}

#[tokio::test]
async fn reverted_boundary_retires_only_on_matching_model_or_compress_acceptance() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use oc_core::core_app::InboxMsg;
    for compress in [false, true] {
        for (accepted, interrupted) in [(false, false), (true, false), (true, true)] {
            let (app, mut inbox, _) = CoreApp::channel(8);
            let mut state = TuiState::new(app, sid("reverted-acceptance"));
            let boundary = oc_core::queries::RevertedConversation {
                message: oc_core::session::MessageId("hidden-user".into()),
                user_messages: 2,
            };
            state.attach_page(&HistoryPage {
                reverted: Some(boundary.clone()),
                ..Default::default()
            });
            state.conversation_available = Some((false, true));
            state.restore_prompt("replacement".into());
            // Warm the projection before enqueue and acceptance.
            assert!(
                state
                    .visible_transcript(120, 120, 40)
                    .0
                    .iter()
                    .any(|l| l.plain_text().contains("2 messages reverted"))
            );
            let frame = Rect::new(0, 0, 120, 40);
            let area = crate::shell::transcript_area(&state, frame);
            let (lines, total, scroll, targets) = state
                .visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
            state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
            let row = targets
                .iter()
                .position(|target| target.as_ref().is_some_and(|target| target.reverted))
                .unwrap();
            let (x, y) = (area.x + 4, area.y + row as u16 + 1);
            state.handle_mouse(
                selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
                frame,
            );
            if compress {
                state.request_compress(String::new()).unwrap();
            } else {
                state.handle_key(KeyAction::Enter).await;
            }
            let ack = match inbox.recv().await.unwrap() {
                InboxMsg::Submit { ack, .. } if !compress => ack,
                InboxMsg::Compress { ack, .. } if compress => ack,
                _ => panic!("real submission receipt"),
            };
            assert_eq!(state.reverted, Some(boundary.clone()));
            type_text(&mut state, " newer draft").await;
            let turn = WorkerTurnId("replacement-turn".into());
            ack.send(if accepted {
                Ok(turn.clone())
            } else {
                Err(CoreError::TurnBusy)
            })
            .unwrap();
            state.poll_submission();
            assert_eq!(state.input(), "replacement newer draft");
            if accepted {
                assert!(state.reverted.is_none());
                assert_eq!(state.conversation_available, Some((true, false)));
                assert!(state.reverted_down.is_none());
                assert!(
                    state
                        .handle_mouse(
                            selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
                            frame
                        )
                        .intent
                        .is_none()
                );
                assert!(
                    !state
                        .visible_transcript(120, 120, 40)
                        .0
                        .iter()
                        .any(|l| l.plain_text().contains("messages reverted"))
                );
                if interrupted {
                    state.apply_interrupted(&turn, "partial", 0);
                } else {
                    state.apply_failed(&turn, &CoreError::Application("after admission".into()));
                }
                assert!(state.reverted.is_none());
            } else {
                assert_eq!(state.reverted, Some(boundary));
                assert_eq!(state.conversation_available, Some((false, true)));
            }
        }
    }
}

#[tokio::test]
async fn reverted_boundary_survives_a_stale_acceptance_from_an_old_generation() {
    use oc_core::queries::{ConversationSnapshot, RevertedConversation};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("old-boundary"));
    state.restore_prompt("old submit".into());
    state.handle_key(KeyAction::Enter).await;
    let Some(oc_core::core_app::InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("old admission")
    };
    let old = state.pending.take().unwrap();
    let boundary = RevertedConversation {
        message: oc_core::session::MessageId("new-boundary".into()),
        user_messages: 1,
    };
    state.conversation_applied(
        &ConversationSnapshot {
            session: sid("new-boundary"),
            draft: Some("new draft".into()),
            images: Vec::new(),
            can_undo: false,
            can_redo: true,
            reverted: Some(boundary.clone()),
        },
        &HistoryPage {
            reverted: Some(boundary.clone()),
            ..Default::default()
        },
    );
    ack.send(Ok(WorkerTurnId("old-turn".into()))).unwrap();
    state.pending = Some(old);
    state.poll_submission();
    assert_eq!(state.reverted, Some(boundary));
    assert_eq!(state.conversation_available, Some((false, true)));
    assert_eq!(state.input(), "new draft");
}

#[tokio::test]
async fn vis34_session_tps_is_current_presentation_for_finished_and_replayed_footers() {
    use oc_core::queries::HistoryTurn;
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, sid("tps"));
    let turn = WorkerTurnId("measured-turn".into());
    state.active_turn = Some(turn.clone());
    state.active_agent = Some("build".into());
    state.live_model_label = Some("Measured model".into());
    state.apply_delta(&turn, "## actual body\n\nretained text");
    state.apply_usage(&turn, 1000, 200, 4000);
    state.apply_finished(&turn, "## actual body\n\nretained text", 1500);
    state.apply_compaction(oc_core::compaction::CompactionSnapshot {
        model: None,
        profile: None,
        anchor: Default::default(),
        id: "checkpoint".into(),
        session: "tps".into(),
        reason: oc_core::compaction::CompactionReason::Manual,
        state: oc_core::compaction::CompactionState::Completed,
        summary: "retained checkpoint".into(),
        provider_native: false,
        error: None,
        usage: Some(oc_core::compaction::CompactionUsage {
            input_tokens: 123,
            output_tokens: 45,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
        }),
    });
    let mut answer = msg(2, Role::Assistant, "## actual body\n\nretained text");
    answer.turn = Some(HistoryTurn {
        id: turn.0.clone(),
        status: "completed".into(),
        model_label: "Measured model".into(),
        agent: Some("build".into()),
        duration_ms: Some(1500),
        streamed_ms: Some(4000),
        usage: Some((1000, 200)),
        ..Default::default()
    });
    for replay in [false, true] {
        if replay {
            state.attach_page(&page(vec![answer.clone()], 1, false, false));
        }
        let stored = state
            .history()
            .rows()
            .iter()
            .find_map(|row| row.meta.clone())
            .unwrap();
        for (location, flag) in [
            ("/A", None),
            ("/A", Some(false)),
            ("/A", Some(true)),
            ("/B", Some(false)),
            ("/A", None),
        ] {
            let mut owner = snapshot();
            owner.chrome.location = Some(location.into());
            owner.chrome.session_tps = flag;
            state.apply_catalog(owner);
            let full = state.transcript_lines(80, 80);
            let (indexed, _) = state.visible_transcript(80, 80, 40);
            for lines in [full, indexed] {
                let text = lines
                    .iter()
                    .map(crate::styled::Line::plain_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(text.contains("Build · Measured model · 1.5s"), "{text}");
                assert!(
                    text.contains("Compaction · 123 in · 45 out"),
                    "TPS setting cannot hide compaction usage: {text}"
                );
                assert_eq!(
                    text.contains("50.0 tok/s"),
                    flag.unwrap_or(true),
                    "replay={replay} location={location} {text}"
                );
            }
            assert_eq!(
                state
                    .history()
                    .rows()
                    .iter()
                    .find_map(|row| row.meta.as_ref())
                    .unwrap(),
                &stored,
                "presentation does not erase stored measurements"
            );
        }
        assert_eq!(stored.input_tokens, Some(1000));
        assert_eq!(stored.output_tokens, Some(200));
        assert_eq!(stored.streamed_ms, Some(4000));
    }
}

#[tokio::test]
async fn reverted_card_owner_count_render_click_selection_and_reopen() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("reverted-card").await;
    let boundary = oc_core::queries::RevertedConversation {
        message: oc_core::session::MessageId("first-hidden".into()),
        user_messages: 3,
    };
    let owner_page = HistoryPage {
        reverted: Some(boundary.clone()),
        ..Default::default()
    };
    state.attach_page(&owner_page);
    state.restore_prompt("original prompt".into());
    let frame = Rect::new(0, 0, 120, 40);
    let area = crate::shell::transcript_area(&state, frame);
    let (rows, total, scroll, targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
    let full = state.rendered_transcript(area.width, frame.width);
    assert_eq!(rows.len(), full.len());
    for (indexed, full) in rows.iter().zip(&full) {
        assert_eq!(indexed.spans(), full.spans());
    }
    assert!(
        rows.iter()
            .any(|row| row.plain_text().contains("3 messages reverted"))
    );
    assert!(
        rows.iter()
            .any(|row| row.plain_text().contains("ctrl+x r or /redo to restore"))
    );
    state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
    let row = targets
        .iter()
        .position(|target| target.as_ref().is_some_and(|target| target.reverted))
        .unwrap();
    let x = area.x + 4;
    let y = area.y + row as u16 + 1;
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    let hovered = state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
    assert_ne!(hovered[row + 1], rows[row + 1]);
    assert_eq!(
        hovered[row + 1].spans()[0].style(),
        rows[row + 1].spans()[0].style(),
        "hover retains raised-color border"
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    let clicked = state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert_eq!(
        clicked.intent,
        Some(PanelIntent::ChangeConversation {
            action: oc_core::queries::ConversationAction::Redo
        })
    );
    assert_eq!(state.input(), "original prompt");
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    let mut changed = owner_page.clone();
    changed.reverted.as_mut().unwrap().message =
        oc_core::session::MessageId("different-boundary".into());
    state.attach_page(&changed);
    let (changed_rows, changed_total, changed_scroll, changed_targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
    state.paint_transcript_at(
        area,
        &changed_rows,
        changed_total,
        changed_scroll,
        Some(frame),
        &changed_targets,
    );
    assert!(
        state
            .handle_mouse(
                selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
                frame
            )
            .intent
            .is_none(),
        "same-count replacement cannot inherit a press"
    );
    state.attach_page(&owner_page);
    state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 3, y),
        frame,
    );
    assert!(
        state
            .handle_mouse(
                selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 3, y),
                frame
            )
            .intent
            .is_none()
    );
    state.attach_page(&owner_page);
    assert_eq!(state.reverted, Some(boundary));
    state.set_conversation_shortcuts(Some("ctrl+x z".into()), Some("ctrl+x y".into()));
    state.handle_key(KeyAction::Leader).await;
    assert!(
        state
            .handle_key(KeyAction::Char('r'))
            .await
            .intent
            .is_none(),
        "configured binding replaces default"
    );
    state.handle_key(KeyAction::Leader).await;
    assert_eq!(
        state.handle_key(KeyAction::Char('y')).await.intent,
        Some(PanelIntent::ChangeConversation {
            action: oc_core::queries::ConversationAction::Redo
        })
    );
    state.attach_page(&HistoryPage::default());
    assert!(state.reverted.is_none());
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let key = |value, modifiers| KeyEvent::new(KeyCode::Char(value), modifiers);
    let mut catalog = snapshot();
    catalog.chrome.conversation_shortcuts = oc_core::queries::ConversationShortcuts {
        leader: "ctrl+b,alt+x".into(),
        undo: "ctrl+b u,alt+x u".into(),
        redo: "ctrl+b r,alt+x r,ctrl+z".into(),
    };
    state.apply_catalog(catalog.clone());
    assert_eq!(state.terminal_key(key('x', KeyModifiers::CONTROL)), None);
    assert_eq!(
        state.terminal_key(key('z', KeyModifiers::CONTROL)),
        Some(KeyAction::RedoConversation)
    );
    let leader = state.terminal_key(key('b', KeyModifiers::CONTROL)).unwrap();
    state.handle_key(leader).await;
    assert_eq!(
        state.terminal_key(key('r', KeyModifiers::NONE)),
        Some(KeyAction::RedoConversation)
    );
    let leader = state.terminal_key(key('x', KeyModifiers::ALT)).unwrap();
    state.handle_key(leader).await;
    assert_eq!(
        state.terminal_key(key('u', KeyModifiers::NONE)),
        Some(KeyAction::UndoConversation)
    );
    catalog.chrome.location = Some("/new-location".into());
    catalog.chrome.conversation_shortcuts = oc_core::queries::ConversationShortcuts {
        leader: String::new(),
        undo: String::new(),
        redo: String::new(),
    };
    state.apply_catalog(catalog);
    assert_eq!(state.terminal_key(key('x', KeyModifiers::CONTROL)), None);
    assert_ne!(
        state.terminal_key(key('z', KeyModifiers::CONTROL)),
        Some(KeyAction::RedoConversation)
    );
    assert_eq!(
        state.terminal_key(key('-', KeyModifiers::CONTROL)),
        Some(KeyAction::Undo),
        "editor undo remains distinct"
    );
}

#[tokio::test]
async fn sent_user_messages_follow_current_profile_without_rewriting_submission() {
    let mut state = fresh_state("user-message-agent-color").await;
    let mut events = state.app.subscribe();
    let colors = crate::theme::Theme::dark().categorical_agents();

    let mut selected = snapshot();
    selected.agent_id = Some("y".to_string());
    state.apply_catalog(selected);
    submit_echo(&mut state, &mut events, "message from second profile").await;

    assert_eq!(
        state.history().rows()[0].agent.as_deref(),
        Some("y"),
        "the accepted user echo must retain its submitted profile"
    );
    assert_eq!(state.history().rows()[0].agent_color_index, Some(1));
    assert!(
        user_border_colors(&state)
            .iter()
            .all(|color| *color == colors[1]),
        "first profile's user stripe must use its categorical color"
    );

    state.apply_catalog(snapshot());
    submit_echo(&mut state, &mut events, "message from first profile").await;

    let users: Vec<_> = state
        .history()
        .rows()
        .iter()
        .filter(|row| row.role == "user")
        .collect();
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].agent.as_deref(), Some("y"));
    assert_eq!(users[0].agent_color_index, Some(1));
    assert_eq!(users[1].agent.as_deref(), Some("x"));
    assert_eq!(users[1].agent_color_index, Some(0));
    let mut stripe_runs = Vec::new();
    for color in user_border_colors(&state) {
        if stripe_runs.last() != Some(&color) {
            stripe_runs.push(color);
        }
    }
    assert_eq!(stripe_runs, [colors[0]]);
    let stored = state.history().rows().to_vec();
    let draft = "unsubmitted current-profile draft";
    state.restore_prompt(draft.into());
    let mut current = snapshot();
    current.agent_id = Some("y".into());
    current
        .chrome
        .agent_colors
        .insert("y".into(), "#12ab34".into());
    state.apply_catalog(current);
    assert!(
        user_border_colors(&state)
            .iter()
            .all(|color| *color == ratatui::style::Color::Rgb(0x12, 0xab, 0x34))
    );
    assert_eq!(state.history().rows(), stored);
    assert_eq!(state.input(), draft);
}

#[tokio::test]
async fn replayed_user_messages_use_current_profile_in_full_and_indexed_rendering() {
    use oc_core::queries::HistoryTurn;

    let mut state = fresh_state("replay-user-agent-color").await;
    state.apply_catalog(snapshot()); // current profile is `x`
    let mut old = msg(1, Role::User, "previously sent as y");
    old.turn = Some(HistoryTurn {
        agent: Some("y".into()),
        agent_color_index: Some(1),
        ..Default::default()
    });
    let mut new = msg(2, Role::User, "previously sent as x");
    new.turn = Some(HistoryTurn {
        agent: Some("x".into()),
        agent_color_index: Some(0),
        ..Default::default()
    });
    state.attach_page(&page(vec![old, new], 2, false, false));

    let stored = state.history().rows().to_vec();
    let colors = crate::theme::Theme::dark().categorical_agents();
    for (agent, explicit, expected) in [
        ("x", None, colors[0]),
        ("y", None, colors[1]),
        (
            "y",
            Some("#12ab34"),
            ratatui::style::Color::Rgb(0x12, 0xab, 0x34),
        ),
        ("y", Some("not-a-color"), colors[1]),
        ("x", None, colors[0]),
    ] {
        let mut catalog = snapshot();
        catalog.agent_id = Some(agent.into());
        if let Some(color) = explicit {
            catalog
                .chrome
                .agent_colors
                .insert(agent.into(), color.into());
        }
        state.apply_catalog(catalog);
        for width in [43, 80, 120, 160] {
            let full = state.transcript_lines(width, width);
            let (indexed, _) = state.visible_transcript(width, width, 40);
            for lines in [full, indexed] {
                let borders: Vec<_> = lines
                    .iter()
                    .flat_map(|line| line.spans())
                    .filter(|span| span.content() == "┃")
                    .map(|span| span.style().fg)
                    .collect();
                assert!(!borders.is_empty());
                assert!(
                    borders.iter().all(|color| *color == Some(expected)),
                    "agent={agent} width={width} {borders:?}"
                );
            }
        }
        assert_eq!(
            state.history().rows(),
            stored,
            "current presentation cannot rewrite either original turn profile/slot"
        );
    }
}

#[tokio::test]
async fn live_footer_uses_the_turns_pinned_model_not_the_current_picker() {
    let mut state = fresh_state("pinned-model-footer").await;
    let turn = WorkerTurnId("turn-pinned-model".into());
    state.active_turn = Some(turn.clone());
    state.apply_presentation(
        &turn,
        &oc_core::queries::HistoryTurn {
            id: turn.0.clone(),
            model_label: "Muse Spark".into(),
            status: "completed".into(),
            ..Default::default()
        },
    );
    state.apply_finished(&turn, "answer", 100);
    assert_eq!(
        state
            .history()
            .rows()
            .last()
            .unwrap()
            .meta
            .as_ref()
            .unwrap()
            .model
            .as_deref(),
        Some("Muse Spark")
    );
}

#[tokio::test]
async fn v06b_reasoning_toggle_projects_public_history_without_modifying_owner() {
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("s-v06b-reasoning").await;
    let mut row = msg(2, Role::Assistant, "answer");
    row.turn = Some(HistoryTurn {
        id: "turn-1".into(),
        status: "failed".into(),
        agent: Some("build".into()),
        model_label: "fixture".into(),
        parts: vec![
            TranscriptPart::Reasoning {
                text: "**Plan**\n\nPublic detail [REDACTED]".into(),
                duration_ms: Some(1500),
            },
            TranscriptPart::Text("answer".into()),
        ],
        ..Default::default()
    });
    let owner_page = page(vec![row], 1, false, false);
    state.attach_page(&owner_page);
    let collapsed = state
        .transcript_lines(100, 120)
        .iter()
        .map(|l| l.spans().iter().map(|s| s.content()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(collapsed.contains("+ Thought: Plan · 1.5s") && !collapsed.contains("Public detail"));
    state.run_command(crate::commands::CommandAction::ToggleThinking);
    let expanded = state
        .transcript_lines(100, 120)
        .iter()
        .map(|l| l.spans().iter().map(|s| s.content()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        expanded.contains("   ┃ Thought: 1.5s")
            && expanded.contains("Public detail")
            && !expanded.contains("[REDACTED]"),
        "{expanded}"
    );
    state.attach_page(&owner_page);
    assert_eq!(owner_page.rows[0].turn.as_ref().unwrap().parts.len(), 2);
    assert!(state.transcript_lines(100, 120).iter().any(|line| {
        line.spans()
            .iter()
            .any(|s| s.content().contains("Public detail"))
    }));
    state.run_command(crate::commands::CommandAction::ToggleThinking);
    assert!(
        !state.transcript_rows()[0]
            .reasoning
            .as_ref()
            .unwrap()
            .expanded
    );
}

#[tokio::test]
async fn v06b_session_switch_discards_tool_cards_before_next_owner_load() {
    let mut state = fresh_state("owner-a").await;
    state.app.create_session(sid("owner-b")).await.unwrap();
    let card = crate::history::ToolCard {
        started_at: None,
        child_job: None,
        output_presentation: None,
        question: None,
        op: "a-only".into(),
        name: "read".into(),
        state: "completed".into(),
        input_preview: String::new(),
        output_preview: "A-only preview".into(),
        output_bytes: 14,
        output_truncated: false,
        files: Vec::new(),
        files_truncated: false,
        diff: None,
        patch_effects: None,
        diff_settings: Default::default(),
        render: crate::tools::ToolRender::Inline(crate::tools::InlineRender::Read {
            path: "a-only".into(),
            range: None,
        }),
    };
    state.apply_cards(vec![card], true);
    state.set_session(sid("owner-b"));
    let action = state.run_command(crate::commands::CommandAction::OpenCards);
    assert_eq!(action.intent, Some(PanelIntent::LoadCards));
    assert!(state.cards.is_empty());
    assert!(state.card_ops.is_empty());
    assert!(state.card_output.is_none());
    assert!(!state.cards_has_older);
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
}

#[tokio::test]
async fn v06b_detail_scrolls_every_unicode_row_before_next_owner_page() {
    use oc_core::queries::ToolOutputPage;
    for (width, height) in [(22, 10), (14, 9)] {
        let mut state = fresh_state(&format!("v06b-detail-{width}")).await;
        state.panel = TuiPanel::Cards;
        state.card_ops.push("real-op".into());
        let text = format!("{}END", "界🙂 ↵\n\\n\u{1b}[31m ".repeat(9));
        let next = text.len() as i64;
        state.apply_card_output(
            "real-op".into(),
            0,
            ToolOutputPage {
                text,
                total_bytes: next + 12,
                next_offset: Some(next),
            },
        );
        let first = crate::views::render_test(&state, width, height).join("\n");
        assert!(first.contains("\\n") && first.contains('↵'), "{first}");
        assert!(!first.contains('\u{1b}'), "no raw terminal escapes");
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        state.handle_panel_key(KeyAction::End);
        assert_eq!(
            state.handle_panel_key(KeyAction::Enter).intent,
            None,
            "jumping to the end without viewing preceding rows cannot skip the page"
        );
        state.handle_panel_key(KeyAction::Home);
        for _ in 0..300 {
            let frame = crate::views::render_test(&state, width, height).join("\n");
            let lines = crate::views::panel_lines(&state);
            let last = &lines[lines.len() - 2];
            if last.is_ascii() {
                assert!(frame.contains(last), "last visible detail row: {last:?}");
            }
            let (_, _, count) = crate::views::card_window(&state);
            if state.card_seen() == count {
                let (_, hint_width, _) = crate::dialog::card_geometry(state.detail_area());
                let hint: String = "enter next".chars().take(hint_width).collect();
                assert!(
                    frame.contains(&hint),
                    "next-page footer is truthful on the paint that reaches the end: {frame}"
                );
                break;
            }
            assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
            state.handle_panel_key(KeyAction::Down);
        }
        assert_eq!(state.card_seen(), crate::views::card_window(&state).2);
        let (_, body_width, _) = crate::dialog::card_geometry(state.detail_area());
        assert!(
            crate::views::card_body_rows(
                &state.card_output.as_ref().unwrap().page.text,
                body_width
            )
            .join("")
            .contains("END")
        );
        assert_eq!(
            state.handle_panel_key(KeyAction::Enter).intent,
            Some(PanelIntent::LoadCardOutput {
                op: "real-op".into(),
                offset: next as usize
            })
        );
    }
}

#[tokio::test]
async fn exploration_mouse_hits_only_visible_header_text_without_drag_or_modal_leak() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = fresh_state("exploration-mouse").await;
    for (i, name) in ["read", "glob", "grep"].iter().enumerate() {
        let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: i as i64 + 1,
            op: format!("op-{i}"),
            name: (*name).into(),
            state: "completed".into(),
            input: Some(
                if *name == "read" {
                    r#"{"path":"fixture-note.txt"}"#
                } else {
                    r#"{"pattern":"*.rs"}"#
                }
                .into(),
            ),
            output: Some("fixture result".into()),
            output_bytes: 14,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        state.window.push_row(crate::history::HistoryRow {
            position: None,
            message_id: None,
            seq: i as i64 + 1,
            role: "tool".into(),
            text: String::new(),
            agent: None,
            agent_color_index: None,
            chips: vec![],
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool: Some(card),
        });
    }
    let event = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let click = |state: &mut TuiState, area: ratatui::layout::Rect, x, y| {
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
    };
    for area in [
        ratatui::layout::Rect::new(0, 0, 80, 24),
        ratatui::layout::Rect::new(0, 0, 121, 40),
        ratatui::layout::Rect::new(0, 0, 25, 24),
    ] {
        state.exploration_expanded.clear();
        let transcript = crate::shell::transcript_area(&state, area);
        let (lines, total) =
            state.visible_transcript(transcript.width, area.width, transcript.height);
        state.observe_viewport(transcript.height, total);
        let row = lines
            .iter()
            .position(|line| line.plain_text().contains("Explored"))
            .expect("header visible");
        let x = transcript.x + 4;
        let y = transcript.y + row as u16;
        state.handle_mouse(event(MouseEventKind::Moved, x, y), area);
        let painted = state.paint_transcript_at(transcript, &lines, total, 0, Some(area), &[]);
        assert_ne!(painted[row], lines[row], "Explored header hover must paint");
        assert_eq!(painted[row].plain_text(), lines[row].plain_text());
        assert!(
            state.exploration_expanded.is_empty(),
            "hover does not expand"
        );
        assert!(
            !state
                .transcript_lines(80, 80)
                .iter()
                .any(|line| line.plain_text().contains("Read fixture-note.txt"))
        );
        click(&mut state, area, x, y - 1); // vertical gap
        click(&mut state, area, transcript.x, y); // left padding
        // At a narrow exact fit, the last cell belongs to the header,
        // not to blank padding. Test the actual blank continuation tail.
        let blank_row = lines
            .iter()
            .enumerate()
            .skip(row)
            .find(|(_, line)| {
                line.spans()
                    .iter()
                    .map(crate::styled::span_width)
                    .sum::<usize>()
                    < transcript.width as usize
            })
            .map(|(index, _)| transcript.y + index as u16)
            .expect("a painted blank tail");
        click(&mut state, area, transcript.right() - 1, blank_row);
        click(&mut state, area, x, transcript.bottom()); // status/prompt
        assert!(
            state.exploration_expanded.is_empty(),
            "area={area:?}, transcript={transcript:?}, header={:?}",
            lines[row].plain_text()
        );
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(
            event(MouseEventKind::Drag(MouseButton::Left), x + 1, y),
            area,
        );
        state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
        assert!(state.exploration_expanded.is_empty());
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(
            event(
                MouseEventKind::Up(MouseButton::Left),
                x,
                transcript.bottom(),
            ),
            area,
        );
        assert!(state.exploration_expanded.is_empty());
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x + 1, y), area);
        assert!(
            state.exploration_expanded.is_empty(),
            "text selection cannot toggle"
        );
        state.panel = TuiPanel::Help(None);
        assert_eq!(
            state.paint_transcript_at(transcript, &lines, total, 0, Some(area), &[]),
            lines
        );
        click(&mut state, area, x, y);
        assert!(state.exploration_expanded.is_empty());
        state.close_panel();
        click(&mut state, area, x, y);
        assert!(state.exploration_expanded.contains("op-0"));
        assert!(
            state
                .transcript_lines(80, 80)
                .iter()
                .any(|line| line.plain_text().contains("Read fixture-note.txt"))
        );
        click(&mut state, area, x, y);
        assert!(state.exploration_expanded.is_empty());
        if area.width == 25 {
            let continuation = lines[row + 1].plain_text();
            assert!(
                continuation.contains("search"),
                "header must wrap at the actual content width: {continuation}"
            );
            click(&mut state, area, transcript.x + 1, y + 1);
            assert!(state.exploration_expanded.contains("op-0"));
            click(&mut state, area, transcript.x + 1, y + 1);
            assert!(state.exploration_expanded.is_empty());
        }
    }
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    for i in 0..20 {
        state
            .window
            .push_synthetic("assistant", &format!("later message {i}"), None, None);
    }
    let rect = crate::shell::transcript_area(&state, area);
    let (_, total) = state.visible_transcript(rect.width, area.width, rect.height);
    assert!(total > rect.height as usize);
    assert!(
        state
            .exploration_hit(area, rect.x + 4, rect.y + 1)
            .is_none(),
        "offscreen group is not clickable"
    );
    state.scroll = total - rect.height as usize;
    let (lines, _) = state.visible_transcript(rect.width, area.width, rect.height);
    let row = lines
        .iter()
        .position(|line| line.plain_text().contains("Explored"))
        .unwrap();
    click(&mut state, area, rect.x + 4, rect.y + row as u16);
    assert!(state.exploration_expanded.contains("op-0"));
    let (expanded, _) = state.visible_transcript(rect.width, area.width, rect.height);
    assert!(expanded[row].plain_text().contains("Explored"));
    click(&mut state, area, rect.x + 4, rect.y + row as u16);
    assert!(state.exploration_expanded.is_empty());
    assert_eq!(state.scroll, total - rect.height as usize);
    click(&mut state, area, rect.x + 4, rect.y + row as u16);
    state.set_session(sid("other-session"));
    assert!(state.exploration_expanded.is_empty());
}

#[tokio::test]
async fn adjacent_reasoning_click_opens_and_recloses_one_group_across_live_boundary() {
    use crate::messages::ReasoningIdentity;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("reasoning-adjacent-click").await;
    let mut message = msg(9, Role::Assistant, "answer");
    message.turn = Some(HistoryTurn {
        parts: vec![
            TranscriptPart::Reasoning {
                text: "**Inspecting**\n\nfirst body".into(),
                duration_ms: Some(5),
            },
            TranscriptPart::Reasoning {
                text: "**Verifying**\n\nsecond body".into(),
                duration_ms: Some(7),
            },
            TranscriptPart::Text("answer".into()),
        ],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let header = |state: &TuiState| {
        state
            .visible_transcript(rect.width, area.width, rect.height)
            .0
            .iter()
            .position(|line| line.plain_text().contains("Thought"))
            .unwrap() as u16
            + rect.y
    };
    let click = |state: &mut TuiState, y| {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(
                MouseEvent {
                    kind,
                    column: rect.x + 4,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
        }
    };
    let y = header(&state);
    assert!(state.transcript_lines(80, 80).iter().any(|line| {
        line.plain_text()
            .contains("Thought: Verifying · 2 steps · 12ms")
    }));
    click(&mut state, y);
    assert_eq!(
        state.reasoning_expanded.iter().copied().collect::<Vec<_>>(),
        [ReasoningIdentity::Durable(9, 0)]
    );
    let lines: Vec<_> = state
        .transcript_lines(80, 80)
        .iter()
        .map(|line| line.plain_text())
        .collect();
    assert!(
        lines
            .iter()
            .any(|s| s.contains("- Thought · 2 steps · 12ms"))
    );
    assert!(lines.iter().any(|s| s.contains("first body")));
    assert!(lines.iter().any(|s| s.contains("second body")));
    state.click = None;
    let y = header(&state);
    click(&mut state, y);
    assert!(state.reasoning_expanded.is_empty());
    let first = ReasoningIdentity::Durable(9, 0);
    state.reasoning_expanded.insert(first);
    state.prepend_page(&page(vec![msg(8, Role::User, "earlier")], 9, false, true));
    state.append_page(&page(vec![msg(10, Role::User, "later")], 10, true, false));
    assert!(state.reasoning_expanded.contains(&first));
    assert!(
        state
            .transcript_lines(80, 80)
            .iter()
            .any(|line| line.plain_text().contains("- Thought · 2 steps"))
    );
    state.reasoning_expanded.clear();
    let turn = WorkerTurnId("live-group".into());
    state.active_turn = Some(turn.clone());
    state.apply_reasoning_delta(&turn, "**Live one**\n\nbody one");
    state.apply_reasoning_item_ended(&turn);
    state.apply_reasoning_delta(&turn, "**Live two**\n\nbody two");
    let live = state.transcript_lines(80, 80);
    assert!(
        live.iter()
            .any(|line| line.plain_text().contains("Thinking: Live two"))
    );
    let id = ReasoningIdentity::Live(state.reasoning_epoch, state.live_part_offset);
    state.reasoning_expanded.insert(id);
    let live = state.transcript_lines(80, 80);
    assert!(
        live.iter()
            .any(|line| line.plain_text().contains("body one"))
    );
    assert!(
        live.iter()
            .any(|line| line.plain_text().contains("body two"))
    );
    state.apply_reasoning_item_ended(&turn);
    assert!(
        state
            .transcript_lines(80, 80)
            .iter()
            .any(|line| line.plain_text().contains("2 steps"))
    );
}

#[tokio::test]
async fn reasoning_click_is_independent_across_same_seq_parts_and_owned_by_viewport() {
    use crate::messages::ReasoningIdentity;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("reasoning-mouse").await;
    let mut message = msg(9, Role::Assistant, "aggregate");
    message.turn = Some(HistoryTurn {
        parts: vec![
            TranscriptPart::Reasoning {
                text: "first".into(),
                duration_ms: None,
            },
            TranscriptPart::Text("between".into()),
            TranscriptPart::Reasoning {
                text: "second".into(),
                duration_ms: None,
            },
        ],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let event = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let headers = |state: &TuiState, area: ratatui::layout::Rect| {
        let rect = crate::shell::transcript_area(state, area);
        state
            .visible_transcript(rect.width, area.width, rect.height)
            .0
            .iter()
            .enumerate()
            .filter(|(_, line)| line.plain_text().contains("Thought"))
            .map(|(i, _)| rect.y + i as u16)
            .collect::<Vec<_>>()
    };
    let click = |state: &mut TuiState, area, x, y| {
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
    };
    let y = headers(&state, area)[0];
    let x = rect.x + 4;
    state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
    assert!(
        state.reasoning_expanded.is_empty(),
        "press alone does not toggle"
    );
    state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
    assert_eq!(state.reasoning_expanded.len(), 1);
    state.click = None;
    click(&mut state, area, x, y);
    assert!(state.reasoning_expanded.is_empty());
    state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
    state.handle_mouse(event(MouseEventKind::Drag(MouseButton::Left), x, y), area);
    state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
    assert!(state.reasoning_expanded.is_empty());
    state.panel = TuiPanel::Help(None);
    click(&mut state, area, x, y);
    assert!(state.reasoning_expanded.is_empty(), "dialog owns mouse");
    state.close_panel();
    click(&mut state, area, x, y - 1);
    click(&mut state, area, rect.x, y);
    click(&mut state, area, rect.right() - 1, y);
    assert!(state.reasoning_expanded.is_empty());
    click(&mut state, area, x, y);
    assert_eq!(
        state.reasoning_expanded.iter().copied().collect::<Vec<_>>(),
        [ReasoningIdentity::Durable(9, 0)]
    );
    assert!(
        state
            .visible_transcript(rect.width, area.width, rect.height)
            .0[y as usize - rect.y as usize]
            .plain_text()
            .contains("-")
    );
    assert_eq!(state.reasoning_hit(area, rect.x + 2, y), None, "padding");
    assert_eq!(
        state.reasoning_hit(area, rect.x + 3, y),
        Some(ReasoningIdentity::Durable(9, 0)),
        "expanded group header stays at the collapsed x"
    );
    let second_y = *headers(&state, area).last().unwrap();
    click(&mut state, area, x, second_y);
    assert_eq!(state.reasoning_expanded.len(), 2);
    click(&mut state, area, rect.x + 6, y);
    assert_eq!(
        state.reasoning_expanded.iter().copied().collect::<Vec<_>>(),
        [ReasoningIdentity::Durable(9, 2)]
    );
    state.run_command(crate::commands::CommandAction::ToggleThinking);
    assert!(
        state.transcript_rows()[2]
            .reasoning
            .as_ref()
            .unwrap()
            .expanded,
        "show mode is open even for a locally collapsed part"
    );
    assert!(
        !state.transcript_rows()[2]
            .reasoning
            .as_ref()
            .unwrap()
            .toggleable
    );
    let in_show = state.reasoning_expanded.clone();
    let show_y = *headers(&state, area).last().unwrap();
    click(&mut state, area, x, show_y);
    assert_eq!(
        state.reasoning_expanded, in_show,
        "show mode ignores header clicks"
    );
    state.run_command(crate::commands::CommandAction::ToggleThinking);
    assert!(
        state.transcript_rows()[2]
            .reasoning
            .as_ref()
            .unwrap()
            .expanded,
        "hide mode restores its prior per-part toggle"
    );

    // A press on an old frame cannot act after scrolling or resizing.
    let second_y = *headers(&state, area).last().unwrap();
    state.handle_mouse(
        event(MouseEventKind::Down(MouseButton::Left), x, second_y),
        area,
    );
    state.scroll = 1;
    state.handle_mouse(
        event(MouseEventKind::Up(MouseButton::Left), x, second_y),
        area,
    );
    assert_eq!(state.reasoning_expanded.len(), 1);
    state.scroll = 0;
    state.handle_mouse(
        event(MouseEventKind::Down(MouseButton::Left), x, second_y),
        area,
    );
    state.handle_mouse(
        event(MouseEventKind::Up(MouseButton::Left), x, second_y),
        ratatui::layout::Rect::new(0, 0, 81, 24),
    );
    assert_eq!(state.reasoning_expanded.len(), 1);
    state.set_session(sid("reasoning-next"));
    assert!(state.reasoning_expanded.is_empty());
    assert!(state.reasoning_down.is_none());
}

#[tokio::test]
async fn reasoning_painted_release_only_toggles_and_preserves_anchor() {
    use crate::messages::ReasoningIdentity;
    use crossterm::event::{MouseButton, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("reasoning-release-only").await;
    let mut message = msg(9, Role::Assistant, "aggregate");
    message.turn = Some(HistoryTurn {
        parts: vec![TranscriptPart::Reasoning {
            text: "**a thought**\n\nmore thought".into(),
            duration_ms: None,
        }],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let frame = Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, frame);
    let x = rect.x + 4;
    let paint = |state: &TuiState| {
        let (rows, total, scroll) =
            state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let y = rect.y
            + rows
                .iter()
                .position(|line| line.plain_text().contains("Thought"))
                .unwrap() as u16;
        state.paint_transcript(rect, &rows, total, scroll);
        y
    };
    let y = paint(&state);
    let release = selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y);
    let id = ReasoningIdentity::Durable(9, 0);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), rect.x + 1, y),
        frame,
    );
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.is_empty(),
        "a non-header press must not become a standalone header release"
    );
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.contains(&id));
    assert_eq!(paint(&state), y, "expanded header stays at its painted row");
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.is_empty());
    assert_eq!(paint(&state), y, "collapsed header returns to the same row");

    state.panel = TuiPanel::Help(None);
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.is_empty(), "modal owns release");
    state.close_panel();
    paint(&state);

    state.clear_mouse_position();
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.is_empty(),
        "unpainted resize is stale"
    );
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.contains(&id),
        "fresh resize is live"
    );
    paint(&state);
    state.handle_mouse(selection_mouse(MouseEventKind::ScrollDown, x, y), frame);
    state.scroll_transcript(false);
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.contains(&id),
        "unpainted wheel is stale"
    );
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.is_empty(), "fresh wheel is live");

    // Repainting does not turn an outstanding pre-resize press into a
    // standalone release, even when the header stays at the same cell.
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.clear_mouse_position();
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.is_empty(),
        "pre-resize press remains stale after repaint"
    );
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.contains(&id));
    paint(&state);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(selection_mouse(MouseEventKind::ScrollDown, x, y), frame);
    state.scroll_transcript(false);
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.contains(&id),
        "wheel cancels old press"
    );
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.is_empty());

    // A drag back onto its starting cell can end with empty selection.
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 2, y),
        frame,
    );
    state.handle_mouse(release, frame);
    assert_eq!(state.selection_text(), Ok(None));
    assert!(state.reasoning_expanded.is_empty());

    // The subsequent release with no press still works after a fresh paint.
    paint(&state);
    state.handle_mouse(release, frame);
    assert!(state.reasoning_expanded.contains(&id));
}

#[tokio::test]
async fn completed_thought_hover_tracks_painted_header_and_release_only_recollapse() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    use ratatui::{Terminal, backend::TestBackend};

    let mut state = fresh_state("thought-hover").await;
    let mut message = msg(9, Role::Assistant, "after thought");
    message.turn = Some(HistoryTurn {
        parts: vec![TranscriptPart::Reasoning {
            text: "**Tracing**\n\nbody".into(),
            duration_ms: Some(1500),
        }],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let area = Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let (rows, _, _) = state.visible_transcript_at_viewport(rect.width, area.width, rect.height);
    let y = rect.y
        + rows
            .iter()
            .position(|line| line.plain_text().contains("Thought"))
            .unwrap() as u16;
    let x = rect.x + 5;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    let mut color = |state: &TuiState| {
        terminal
            .draw(|frame| crate::shell::render(frame, state))
            .unwrap();
        terminal.backend().buffer()[(x, y)].fg
    };
    let faded = ratatui::style::Color::Rgb(0x97, 0x68, 0x2c);
    let bright = super::Theme::dark().warning();
    assert_eq!(color(&state), faded, "initial no-hover");
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), area);
    assert_eq!(color(&state), bright);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        area,
    );
    assert_eq!(color(&state), bright, "expanded");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        area,
    );
    assert_eq!(
        color(&state),
        bright,
        "recollapsed under stationary pointer"
    );
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        area,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        area,
    );
    assert_eq!(color(&state), bright, "normal click expands");
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        area,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        area,
    );
    assert_eq!(color(&state), bright, "normal click recollapses");
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y + 1), area);
    assert_eq!(color(&state), faded, "non-header row");
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), area);
    state.panel = TuiPanel::Help(None);
    assert_ne!(color(&state), bright, "modal owns pointer");
    state.close_panel();
    state.clear_mouse_position();
    assert_eq!(color(&state), faded, "resize invalidates pointer");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Moved, x, y),
        Rect::new(0, 0, 81, 24),
    );
    assert_eq!(color(&state), faded, "old terminal area is not owner");
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), area);
    assert_eq!(color(&state), bright);
    for i in 0..30 {
        state
            .window
            .push_synthetic("assistant", &format!("later message {i}"), None, None);
    }
    assert_ne!(color(&state), bright, "scrolled-off header cannot hover");
}

#[tokio::test]
async fn reasoning_selected_text_blocks_release_but_new_click_replaces_selection() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("reasoning-selected-text").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_reasoning = "**thinking**".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "Thinking");
    assert!(state.reasoning_hit(frame, x, y).is_some());
    let down = selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y);
    let release = selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y);
    state.handle_mouse(down, frame);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 4, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 4, y),
        frame,
    );
    assert!(matches!(state.selection_text(), Ok(Some(_))));
    state.handle_mouse(release, frame);
    assert!(
        state.reasoning_expanded.is_empty(),
        "release cannot erase selection"
    );
    state.handle_mouse(down, frame);
    assert_eq!(
        state.selection_text(),
        Ok(None),
        "new down replaces highlight"
    );
    state.handle_mouse(release, frame);
    assert_eq!(state.reasoning_expanded.len(), 1, "new click toggles");

    paint_selection_fixture(&mut state, frame, "Thinking");
    state.handle_mouse(down, frame);
    assert!(matches!(state.selection_text(), Ok(Some(_))));
    state.handle_mouse(release, frame);
    assert_eq!(state.reasoning_expanded.len(), 1, "selected word owns UP");
}

#[tokio::test]
async fn redacted_only_durable_reasoning_leaves_no_mouse_target_or_spacer() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("empty-reasoning-click").await;
    let mut message = msg(9, Role::Assistant, "aggregate");
    message.turn = Some(HistoryTurn {
        parts: vec![
            TranscriptPart::Reasoning {
                text: " [REDACTED] \n".into(),
                duration_ms: None,
            },
            TranscriptPart::Reasoning {
                text: "actual".into(),
                duration_ms: None,
            },
        ],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 1, false, false));
    let area = Rect::new(0, 0, 120, 40);
    let rect = crate::shell::transcript_area(&state, area);
    let (lines, total) = state.visible_transcript(rect.width, area.width, rect.height);
    assert_eq!(total, 5, "root blank + one visible header + footer");
    assert_eq!(lines[2].plain_text(), "   + Thought");
    let x = rect.x + 4;
    let y = rect.y + 2;
    assert_eq!(
        state.reasoning_hit(area, x, y),
        Some(crate::messages::ReasoningIdentity::Durable(9, 1))
    );
    let event = |kind| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), area);
    state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), area);
    assert_eq!(
        state.reasoning_expanded.iter().copied().collect::<Vec<_>>(),
        [crate::messages::ReasoningIdentity::Durable(9, 1)]
    );
    assert_eq!(
        state.reasoning_hit(area, x, y),
        Some(crate::messages::ReasoningIdentity::Durable(9, 1)),
        "expanded group header remains at its collapsed position"
    );
}

#[tokio::test]
async fn reasoning_addresses_survive_paging_and_live_eviction_then_reset() {
    use crate::messages::ReasoningIdentity;
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let mut state = fresh_state("reasoning-paging").await;
    let mut message = msg(90, Role::Assistant, "aggregate");
    message.turn = Some(HistoryTurn {
        parts: vec![
            TranscriptPart::Reasoning {
                text: "one".into(),
                duration_ms: None,
            },
            TranscriptPart::Text("between".into()),
            TranscriptPart::Reasoning {
                text: "two".into(),
                duration_ms: None,
            },
        ],
        status: "completed".into(),
        ..Default::default()
    });
    state.attach_page(&page(vec![message], 90, true, true));
    state
        .reasoning_expanded
        .insert(ReasoningIdentity::Durable(90, 0));
    state
        .reasoning_expanded
        .insert(ReasoningIdentity::Durable(90, 2));
    state.prepend_page(&page(vec![msg(1, Role::User, "earlier")], 90, false, true));
    state.append_page(&page(vec![msg(91, Role::User, "later")], 91, true, false));
    assert_eq!(state.reasoning_expanded.len(), 2);
    assert!(
        state
            .transcript_rows()
            .iter()
            .filter_map(|row| row.reasoning.as_ref())
            .all(|r| r.expanded)
    );
    let newer = (100..100 + WINDOW_ROWS)
        .map(|i| msg(i as i64, Role::User, "next"))
        .collect();
    state.append_page(&page(newer, 500, true, false));
    assert!(
        state.reasoning_expanded.is_empty(),
        "evicted owner parts release local state"
    );

    state.active_turn = Some(WorkerTurnId("reasoning-live".into()));
    state.live_reasoning = "open".into();
    state.freeze_reasoning();
    let first = state
        .transcript_rows()
        .last()
        .unwrap()
        .reasoning
        .as_ref()
        .unwrap()
        .identity
        .unwrap();
    assert_eq!(first, ReasoningIdentity::Live(state.reasoning_epoch, 0));
    state.live_reasoning = "retained".into();
    state.freeze_reasoning();
    let retained = ReasoningIdentity::Live(state.reasoning_epoch, 1);
    for i in 0..LIVE_PARTS_MAX - 1 {
        state
            .live_parts
            .push(super::LivePart::Text(format!("segment {i}")));
    }
    state.enforce_parts();
    assert_eq!(state.live_part_offset, 1);
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .find_map(|row| row.reasoning.as_ref().and_then(|r| r.identity)),
        Some(retained),
        "retained part keeps its ordinal after head eviction"
    );
    state.live_reasoning = "next".into();
    state.freeze_reasoning();
    let last = state
        .transcript_rows()
        .iter()
        .rev()
        .find_map(|row| row.reasoning.as_ref().and_then(|r| r.identity))
        .unwrap();
    assert_ne!(first, last);
    assert_eq!(
        last,
        ReasoningIdentity::Live(
            state.reasoning_epoch,
            state.live_part_offset + state.live_parts.len() - 1
        )
    );
    state.reasoning_expanded.insert(last);
    state.reset_workspace();
    assert!(state.reasoning_expanded.is_empty());
}

#[tokio::test]
async fn sequential_public_reasoning_items_freeze_as_separate_live_and_interrupted_rows() {
    let mut state = fresh_state("reasoning-items-live").await;
    let turn = WorkerTurnId("active".into());
    let stale = WorkerTurnId("stale".into());
    state.active_turn = Some(turn.clone());
    state.apply_reasoning_item_ended(&turn);
    assert!(
        state.live_parts.is_empty(),
        "no empty part on early boundary"
    );
    state.apply_reasoning_delta(&turn, "Inspecting");
    state.apply_reasoning_item_ended(&stale);
    assert_eq!(state.live_reasoning, "Inspecting");
    state.apply_reasoning_item_ended(&turn);
    state.apply_reasoning_item_ended(&turn);
    assert_eq!(state.live_parts.len(), 1, "duplicate boundary has no part");
    state.apply_reasoning_delta(&turn, "Verifying");
    let live = state.transcript_rows();
    let parts: Vec<_> = live
        .iter()
        .filter_map(|row| row.reasoning.as_ref())
        .collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].text, "Inspecting");
    assert!(!parts[0].running);
    assert!(parts[0].duration_ms.is_some());
    assert_eq!(parts[1].text, "Verifying");
    assert!(parts[1].running);
    state.apply_reasoning_item_ended(&turn);
    state.apply_delta(&turn, "answer");
    let live = state.transcript_rows();
    assert_eq!(live.iter().filter(|row| row.reasoning.is_some()).count(), 2);
    assert_eq!(live.last().unwrap().text, "answer");
    state.apply_interrupted(&turn, "answer", 42);
    let rows = state.transcript_rows();
    assert_eq!(rows.iter().filter(|row| row.reasoning.is_some()).count(), 2);
    assert!(
        rows.iter()
            .any(|row| row.meta.as_ref().is_some_and(|meta| meta.interrupted))
    );
    state.apply_reasoning_item_ended(&turn);
    assert_eq!(
        state.transcript_rows().len(),
        rows.len(),
        "stale turn ignored"
    );

    let mut failed = fresh_state("reasoning-items-failed").await;
    failed.active_turn = Some(turn.clone());
    failed.apply_reasoning_delta(&turn, "Inspecting");
    failed.apply_reasoning_item_ended(&turn);
    failed.apply_reasoning_delta(&turn, "Verifying");
    failed.apply_failed(&turn, &CoreError::Application("safe failure".into()));
    let rows = failed.transcript_rows();
    assert_eq!(rows.iter().filter(|row| row.reasoning.is_some()).count(), 2);
    assert!(rows.iter().any(|row| {
        row.meta
            .as_ref()
            .is_some_and(|meta| meta.status.as_deref() == Some("failed"))
    }));
    assert!(rows.iter().any(|row| row.text.contains("safe failure")));
}

#[tokio::test]
async fn failed_and_interrupted_open_reasoning_do_not_display_an_elapsed_duration() {
    let turn = WorkerTurnId("open-reasoning".into());
    for interrupted in [false, true] {
        let mut state = fresh_state("reasoning-open-terminal").await;
        state.active_turn = Some(turn.clone());
        state.apply_reasoning_delta(&turn, "**Inspecting**\n\nstill open");
        state.reasoning_started = Some(Instant::now() - Duration::from_millis(2500));
        if interrupted {
            state.apply_interrupted(&turn, "", 3000);
        } else {
            state.apply_failed(&turn, &CoreError::Application("failed".into()));
        }
        let reasoning = state
            .transcript_rows()
            .into_iter()
            .find_map(|row| row.reasoning)
            .expect("visible partial reasoning");
        assert_eq!(reasoning.duration_ms, None);
    }
}

#[tokio::test]
async fn running_reasoning_click_survives_freeze_but_resize_releases_press() {
    use crate::messages::ReasoningIdentity;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = fresh_state("running-reasoning-click").await;
    let turn = WorkerTurnId("running-reasoning-turn".into());
    state.active_turn = Some(turn.clone());
    state.live_reasoning = "**Streaming**\n\nstep".into();
    let area = Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let (lines, _) = state.visible_transcript(rect.width, area.width, rect.height);
    let x = rect.x + 5;
    let y = rect.y
        + lines
            .iter()
            .position(|line| line.plain_text().contains("Thinking"))
            .unwrap() as u16;
    let mouse = |kind| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let down = mouse(MouseEventKind::Down(MouseButton::Left));
    let up = mouse(MouseEventKind::Up(MouseButton::Left));
    let id = ReasoningIdentity::Live(state.reasoning_epoch, 0);
    assert_eq!(state.reasoning_hit(area, x, y), Some(id));
    state.thinking_expanded = true;
    assert!(
        state
            .transcript_rows()
            .last()
            .unwrap()
            .reasoning
            .as_ref()
            .unwrap()
            .expanded
    );
    assert_eq!(
        state.reasoning_hit(area, x, y),
        None,
        "show mode has no running toggle"
    );
    state.handle_mouse(down, area);
    state.handle_mouse(up, area);
    assert!(state.reasoning_expanded.is_empty());
    state.thinking_expanded = false;
    state.handle_mouse(down, area);
    assert!(state.reasoning_down.is_some());
    state.clear_mouse_position();
    state.handle_mouse(up, area);
    assert!(
        state.reasoning_expanded.is_empty(),
        "resize invalidates press even at original size"
    );
    state.handle_mouse(down, area);
    state.clear_mouse_position();
    let resized = Rect::new(0, 0, 81, 24);
    state.handle_mouse(up, resized);
    state.handle_mouse(up, area);
    assert!(
        state.reasoning_expanded.is_empty(),
        "resize roundtrip does not revive the press"
    );
    state.handle_mouse(down, area);
    state.handle_mouse(up, area);
    assert!(state.reasoning_expanded.contains(&id));
    state.freeze_reasoning();
    assert_eq!(
        state
            .transcript_rows()
            .last()
            .unwrap()
            .reasoning
            .as_ref()
            .unwrap()
            .identity,
        Some(id)
    );
    assert!(
        state
            .transcript_rows()
            .last()
            .unwrap()
            .reasoning
            .as_ref()
            .unwrap()
            .expanded
    );
    state.apply_finished(&turn, "", 100);
    assert!(state.transcript_rows().iter().any(|row| {
        row.reasoning
            .as_ref()
            .is_some_and(|r| r.identity == Some(id) && r.expanded && !r.running)
    }));
    state.handle_mouse(down, area);
    state.set_session(sid("another-running-session"));
    state.handle_mouse(up, area);
    assert!(state.reasoning_down.is_none());
    assert!(state.reasoning_expanded.is_empty());
}

#[tokio::test]
async fn expandable_shell_hover_repeated_toggle_and_selection_respect_painted_surface() {
    use crate::theme::Theme;
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("shell-mouse").await;
    let result = format!(
        "exit 0\n{}",
        (0..30)
            .map(|i| format!("recorded output {i}\n"))
            .collect::<String>()
    );
    let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        rowid: 1,
        op: "shell-op".into(),
        name: "bash".into(),
        state: "completed".into(),
        input: Some(r#"{"argv":["fixture"]}"#.into()),
        output_bytes: result.len() as i64,
        output: Some(result),
        output_truncated: false,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    });
    state.window.push_row(crate::history::HistoryRow {
        position: None,
        message_id: None,
        seq: 1,
        role: "tool".into(),
        text: String::new(),
        agent: None,
        agent_color_index: None,
        chips: vec![],
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    });
    let frame = ratatui::layout::Rect::new(0, 0, 80, 48);
    let area = crate::shell::transcript_area(&state, frame);
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
    state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]);
    let row = rows
        .iter()
        .position(|line| line.plain_text().contains("$ fixture"))
        .unwrap();
    let (x, y) = (area.x + 5, area.y + row as u16);
    let mouse = |kind| selection_mouse(kind, x, y);
    state.handle_mouse(mouse(MouseEventKind::Moved), frame);
    let hover = state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]);
    assert_eq!(
        hover[row].style().bg,
        Some(Theme::dark().decrease(Theme::dark().background_raised()))
    );
    assert_eq!(hover[row].plain_text(), rows[row].plain_text());
    assert!(state.exploration_expanded.is_empty());
    // Hover is owned by the exact frame and is blocked by painted overlays.
    let other_frame = ratatui::layout::Rect::new(0, 0, 81, 48);
    assert_eq!(
        state.paint_transcript_at(area, &rows, total, scroll, Some(other_frame), &[]),
        rows
    );
    state.panel = TuiPanel::Help(None);
    assert_eq!(
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]),
        rows
    );
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(mouse(kind), frame);
    }
    assert!(state.exploration_expanded.is_empty());
    state.close_panel();
    state.push_note("cover shell");
    let toast = crate::shell::toast_rect(&state, frame).unwrap();
    if area.contains((toast.x + 1, toast.y + 1).into()) {
        state.handle_mouse(
            selection_mouse(MouseEventKind::Moved, toast.x + 1, toast.y + 1),
            frame,
        );
        assert_eq!(
            state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]),
            rows
        );
    }
    state.note = None;
    for expanded in [true, false, true, false] {
        state.click = None; // independent clicks, rather than word/line selection
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), frame);
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), frame);
        assert_eq!(state.exploration_expanded.contains("shell-op"), expanded);
        let (visible, count, offset) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        assert_eq!(
            visible
                .iter()
                .any(|line| line.plain_text().contains("recorded output 0")),
            expanded
        );
        state.paint_transcript_at(area, &visible, count, offset, Some(frame), &[]);
    }
    state.click = None;
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), frame);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 3, y),
        frame,
    );
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), frame);
    assert!(
        state.exploration_expanded.is_empty(),
        "selection owns the release"
    );

    // A limited Read still belongs to an exploration group. Opening that group
    // reveals the real label, never a fabricated full-file output expansion.
    state.attach_page(&page(vec![], 0, false, false));
    let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        rowid: 2,
        op: "read-op".into(),
        name: "read".into(),
        state: "completed".into(),
        input: Some(r#"{"path":"fixture.txt"}"#.into()),
        output_bytes: 1000,
        output: Some("private file body".into()),
        output_truncated: true,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    });
    state.window.push_row(crate::history::HistoryRow {
        position: None,
        message_id: None,
        seq: 2,
        role: "tool".into(),
        text: String::new(),
        agent: None,
        agent_color_index: None,
        chips: vec![],
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    });
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
    let header = rows
        .iter()
        .position(|line| line.plain_text().contains("Explored: 1 read"))
        .unwrap();
    assert!(
        !rows
            .iter()
            .any(|line| line.plain_text().contains("Read fixture.txt"))
    );
    state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]);
    state.click = None;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(
            selection_mouse(kind, area.x + 5, area.y + header as u16),
            frame,
        );
    }
    assert!(state.exploration_expanded.contains("read-op"));
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
    let row = rows
        .iter()
        .position(|line| line.plain_text().contains("Read fixture.txt"))
        .unwrap();
    let y = area.y + row as u16;
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    assert_eq!(
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]),
        rows
    );
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(selection_mouse(kind, x, y), frame);
    }
    assert!(state.exploration_expanded.contains("read-op"));
    assert!(
        !state
            .transcript_lines(80, 80)
            .iter()
            .any(|line| line.plain_text().contains("private file body"))
    );
}

#[tokio::test]
async fn exploration_toggle_anchors_long_result_and_attach_page_discards_expansion() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = fresh_state("exploration-anchor").await;
    for i in 0..16 {
        let result = format!("loaded result {i}: {}", "contents ".repeat(30));
        let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: i + 1,
            op: format!("read-{i}"),
            name: "read".into(),
            state: "completed".into(),
            input: Some(format!(r#"{{"path":"file-{i}.txt"}}"#)),
            output_bytes: result.len() as i64,
            output: Some(result),
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        state.window.push_row(crate::history::HistoryRow {
            position: None,
            message_id: None,
            seq: i + 1,
            role: "tool".into(),
            text: String::new(),
            agent: None,
            agent_color_index: None,
            chips: vec![],
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool: Some(card),
        });
    }
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let rect = crate::shell::transcript_area(&state, area);
    let (collapsed, before) = state.visible_transcript(rect.width, area.width, rect.height);
    assert!(before < rect.height as usize);
    let header_y = collapsed
        .iter()
        .position(|line| line.plain_text().contains("Explored"))
        .unwrap() as u16;
    let x = rect.x + 4;
    let y = rect.y + header_y;
    let click = |state: &mut TuiState| {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(
                MouseEvent {
                    kind,
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
        }
    };
    click(&mut state);
    assert!(state.exploration_expanded.contains("read-0"));
    let (expanded, after) = state.visible_transcript(rect.width, area.width, rect.height);
    assert!(after > rect.height as usize);
    assert!(
        state.scroll() > 0,
        "expansion must leave the bottom to keep the header visible"
    );
    assert!(
        expanded[header_y as usize]
            .plain_text()
            .contains("Explored")
    );
    assert_eq!(state.exploration_hit(area, x, y).as_deref(), Some("read-0"));
    click(&mut state);
    assert!(state.exploration_expanded.is_empty());
    assert_eq!(state.scroll(), 0);
    let (restored, total) = state.visible_transcript(rect.width, area.width, rect.height);
    assert_eq!((restored, total), (collapsed, before));

    click(&mut state);
    assert!(state.exploration_expanded.contains("read-0"));
    state.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    state.attach_page(&page(vec![], 0, false, false));
    assert!(state.exploration_expanded.is_empty());
    assert!(state.exploration_down.is_none());
    state.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(state.exploration_expanded.is_empty());
}

#[tokio::test]
async fn vis32_wheel_displacement_elapsed_motion_and_edges() {
    use std::time::{Duration, Instant};
    // Donor fixture: OC2 util/scroll.ts CustomSpeedScroll(3), OpenTUI
    // v0.5.10 ScrollBox.onMouseEvent: unit delta * 3, truncation, clamp.
    // These endpoints are donor semantics; intermediate rows explicitly
    // qualify our one-paint-budget native presentation, not donor inertia.
    for (budget, expected_rows) in [
        (Duration::from_micros(6060), &[1, 2, 3][..]),
        (Duration::from_millis(2), &[0, 0, 1, 1, 1, 2, 2, 2, 3][..]),
    ] {
        let mut state = fresh_state("wheel-timeline").await;
        state.observe_viewport(10, 110);
        state.handle_paste("draft 界👩‍💻");
        let now = Instant::now();
        assert_eq!(state.next_scroll_animation_deadline(), None);
        state.wheel_transcript_at(true, 1, now);
        assert_eq!(state.scroll(), 0);
        assert_eq!(
            state.next_scroll_animation_deadline(),
            Some(now + Duration::from_nanos(5_555_556))
        );
        let mut elapsed = Duration::ZERO;
        for expected in expected_rows {
            elapsed += budget;
            state.tick_scroll_animation(now + elapsed);
            assert_eq!(state.scroll(), *expected);
        }
        assert_eq!(state.scroll(), 3);
        assert_eq!(state.input(), "draft 界👩‍💻");
        assert_eq!(state.next_scroll_animation_deadline(), None);
        assert!(!state.tick_scroll_animation(now + Duration::from_secs(1)));

        // Same-direction burst is accumulated without a per-event queue.
        let now = now + Duration::from_secs(2);
        state.wheel_transcript_at(true, 4, now);
        state.wheel_transcript_at(true, 2, now + Duration::from_millis(4));
        assert!(state.tick_scroll_animation(now + Duration::from_millis(17)));
        assert_eq!(state.scroll(), 21);
        assert_eq!(state.next_scroll_animation_deadline(), None);

        // Reverse before settling: obsolete upward debt cannot drag the
        // reader past the position visible when downward input arrives.
        let now = now + Duration::from_secs(1);
        state.wheel_transcript_at(true, 10, now);
        state.tick_scroll_animation(now + Duration::from_millis(6));
        let reversed_from = state.scroll();
        state.wheel_transcript_at(false, 1, now + Duration::from_millis(6));
        state.tick_scroll_animation(now + Duration::from_millis(23));
        assert_eq!(state.scroll(), reversed_from - 3);

        let now = now + Duration::from_secs(1);
        state.wheel_transcript_at(true, usize::MAX, now);
        state.tick_scroll_animation(now + Duration::from_millis(17));
        assert_eq!(state.scroll(), 100);
        state.wheel_transcript_at(false, usize::MAX, now + Duration::from_millis(20));
        state.tick_scroll_animation(now + Duration::from_millis(37));
        assert_eq!(state.scroll(), 0);
        assert_eq!(state.next_scroll_animation_deadline(), None);

        state.observe_viewport(10, 3);
        state.wheel_transcript_at(true, 1, now + Duration::from_secs(2));
        assert_eq!(
            state.next_scroll_animation_deadline(),
            None,
            "short transcript needs no timer"
        );
    }
}

#[tokio::test]
async fn vis32_live_tail_anchor_unicode_paging_and_resize() {
    fn paint(state: &super::TuiState, width: u16, height: u16) -> Vec<String> {
        let (lines, total, scroll) = state.visible_transcript_at_viewport(width, width, height);
        state.observe_transcript_viewport(width, width, height, total, scroll);
        lines
            .iter()
            .map(|line| line.spans().iter().map(|span| span.content()).collect())
            .collect()
    }
    let mut state = fresh_state("wheel-anchor").await;
    let text = (0..90)
        .map(|i| format!("ROW-{i:03} 界👩‍💻 é\n"))
        .collect::<String>();
    state.attach_page(&page(vec![msg(1, Role::Assistant, &text)], 1, true, false));
    state.active_turn = Some(WorkerTurnId("wheel-live".into()));
    let turn = state.active_turn.clone().unwrap();
    state.apply_delta(&turn, "LIVE-ONE\n");
    paint(&state, 80, 10);
    let now = std::time::Instant::now();
    state.wheel_transcript_at(true, 10, now);
    state.tick_scroll_animation(now + std::time::Duration::from_millis(17));
    let before = paint(&state, 80, 10);
    assert!(
        before.iter().any(|line| line.contains("ROW-")),
        "{before:?}"
    );
    let top = before[0].clone();
    state.apply_delta(&turn, "LIVE-TWO 界\nLIVE-THREE 👩‍💻\n");
    let after = paint(&state, 80, 10);
    assert_eq!(
        after, before,
        "stream growth must preserve all painted detached rows"
    );
    let resized = paint(&state, 60, 6);
    assert_eq!(resized[0], top, "resize retains the painted top row");
    state.prepend_page(&page(
        vec![msg(0, Role::Assistant, "OLDER 界")],
        2,
        false,
        false,
    ));
    assert_eq!(
        paint(&state, 60, 6)[0],
        top,
        "older paging retains the loaded anchor"
    );

    // Reengage sticky follow using the visible, resize-adjusted offset.
    let now = now + std::time::Duration::from_secs(1);
    state.wheel_transcript_at(false, usize::MAX, now);
    state.tick_scroll_animation(now + std::time::Duration::from_millis(17));
    paint(&state, 60, 6);
    state.apply_delta(&turn, "LIVE-LATEST 界\n");
    let pinned = paint(&state, 60, 6);
    assert!(
        pinned.iter().any(|line| line.contains("LIVE-LATEST")),
        "{pinned:?}"
    );
    assert_eq!(state.scroll(), 0);
    assert_eq!(state.next_scroll_animation_deadline(), None);
}

#[tokio::test]
async fn vis32_completion_preserves_cached_part_expansion_paging_and_resize() {
    use oc_core::queries::{HistoryTurn, ToolOpView, TranscriptPart};
    fn paint(state: &super::TuiState, width: u16, height: u16) -> Vec<String> {
        let (lines, total, scroll) = state.visible_transcript_at_viewport(width, width, height);
        state.observe_transcript_viewport(width, width, height, total, scroll);
        lines.iter().map(|line| line.plain_text()).collect()
    }
    let mut state = fresh_state("completion-parts").await;
    let mut answer = msg(2, Role::Assistant, "");
    answer.turn = Some(HistoryTurn {
        id: "parts".into(),
        status: "completed".into(),
        parts: vec![
            TranscriptPart::Text(
                "## Markdown before tool\n\n| A | B |\n|---|---|\n| cell | value |\n".into(),
            ),
            TranscriptPart::Tool(ToolOpView {
                child_job: None,
                output_presentation: None,
                question: None,
                rowid: 1,
                op: "anchor-shell".into(),
                name: "bash".into(),
                state: "completed".into(),
                input: Some(r#"{"command":"fixture"}"#.into()),
                output: Some((0..60).map(|i| format!("SHELL-{i:03}\n")).collect()),
                output_bytes: 600,
                output_truncated: false,
                patch_effects: None,
                dcp: None,
                dcp_topic: None,
            }),
            TranscriptPart::Text((0..60).map(|i| format!("TAIL-{i:03}\n")).collect()),
        ],
        ..HistoryTurn::default()
    });
    let prefix = msg(1, Role::Assistant, &"prefix ".repeat(70));
    state.attach_page(&page(vec![prefix, answer.clone()], 2, true, false));
    state.exploration_expanded.insert("anchor-shell".into());
    let all = state.rendered_transcript(80, 80);
    let top = all
        .iter()
        .position(|line| line.plain_text().contains("SHELL-020"))
        .unwrap();
    state.scroll = all.len() - 8 - top;
    let before = paint(&state, 80, 8);
    assert!(before[0].contains("SHELL-020"));

    // Both the prefix and tail change height. A total-row delta or old
    // global row index would land on the wrong content inside this message.
    let refreshed = page(
        vec![
            msg(1, Role::Assistant, &"changed prefix ".repeat(140)),
            answer.clone(),
            msg(3, Role::Assistant, "NEWEST\n"),
        ],
        3,
        true,
        false,
    );
    state.refresh_completed_page(&refreshed);
    assert_eq!(paint(&state, 80, 8), before);
    assert!(state.exploration_expanded.contains("anchor-shell"));
    assert_eq!(
        paint(&state, 60, 6)[0].trim_end(),
        before[0].trim_end(),
        "resize must reindex the same part"
    );
    assert_eq!(
        paint(&state, 60, 6)[0].trim_end(),
        before[0].trim_end(),
        "cached repaint must remain anchored"
    );

    // An overlapping newest page retains the already loaded older message.
    state.refresh_completed_page(&page(
        vec![answer.clone(), msg(3, Role::Assistant, "NEWEST\n")],
        3,
        true,
        false,
    ));
    assert!(state.history().rows().iter().any(|row| row.seq == 1));
    assert_eq!(paint(&state, 60, 6)[0].trim_end(), before[0].trim_end());

    state.scroll = 0;
    state.refresh_completed_page(&refreshed);
    assert_eq!(state.scroll(), 0);
    assert!(
        paint(&state, 80, 8)
            .iter()
            .any(|line| line.contains("NEWEST"))
    );
    // Explicit conversation/session replacement still discards all view state.
    state.scroll = 20;
    state.attach_page(&page(vec![answer], 1, false, false));
    assert_eq!(state.scroll(), 0);
    assert!(state.exploration_expanded.is_empty());
    assert!(state.completion_anchor.borrow().is_none());

    // A far-detached window and a disjoint newest page cannot be joined
    // across missing messages. Retain the reader and expose newer paging.
    let old = (0..60)
        .map(|i| format!("OLDER-{i:03}\n"))
        .collect::<String>();
    state.attach_page(&page(vec![msg(1, Role::Assistant, &old)], 100, false, true));
    state.scroll = 20;
    let before = paint(&state, 80, 8);
    state.refresh_completed_page(&page(
        vec![msg(100, Role::Assistant, "NEW TAIL")],
        100,
        true,
        false,
    ));
    assert_eq!(paint(&state, 80, 8), before);
    assert!(state.needs_newer());
    assert!(state.history().rows().iter().all(|row| row.seq == 1));
    state.set_session(oc_core::domain::SessionId("other-route".into()));
    assert_eq!(state.scroll(), 0);
    assert!(state.completion_anchor.borrow().is_none());
}

#[tokio::test]
async fn checkpointed_active_prefix_and_live_parts_are_painted_once() {
    use oc_core::queries::{HistoryPartWindow, HistoryTurn, PartState, ToolOpView, TranscriptPart};
    let mut state = fresh_state("checkpoint-overlap").await;
    let turn = WorkerTurnId("checkpoint-turn".into());
    let mut accepted = msg(1, Role::User, "request");
    accepted.turn = Some(HistoryTurn {
        id: turn.0.clone(),
        status: "started".into(),
        agent: Some("build".into()),
        model_label: "fixture-model".into(),
        window: Some(HistoryPartWindow {
            start: 0,
            end: 3,
            total: 3,
        }),
        parts: vec![
            TranscriptPart::Reasoning {
                text: "canonical prefix outside the live tail".into(),
                duration_ms: Some(10),
            },
            TranscriptPart::Reasoning {
                text: "checkpointed thought".into(),
                duration_ms: None,
            },
            TranscriptPart::Tool(ToolOpView {
                op: "overlap-read".into(),
                name: "read".into(),
                state: "started".into(),
                input: Some(r#"{"path":"note.txt"}"#.into()),
                child_job: None,
                output_presentation: None,
                question: None,
                dcp_topic: None,
                dcp: None,
                patch_effects: None,
                rowid: 1,
                output: None,
                output_bytes: 0,
                output_truncated: false,
            }),
        ],
        part_states: (0..3)
            .map(|sequence| PartState {
                sequence,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let mut future = accepted.clone();
    state.attach_page(&page(vec![accepted], 1, false, false));
    let canonical_rows = state.window.len();
    state.begin_linked_turn(turn.clone());
    state.live_part_offset = 1;
    state.apply_reasoning_delta(&turn, "checkpointed thought");
    state.apply_reasoning_item_ended(&turn);
    state.apply_tool_started(&turn, "overlap-read", "read", r#"{"path":"note.txt"}"#);
    // The producer publishes the frozen generation immediately after Started,
    // before the next generation's public suffix.
    state.apply_presentation(&turn, future.turn.as_ref().unwrap());
    state.apply_delta(&turn, "new stream suffix");
    let rows = state.transcript_rows();
    assert_eq!(
        state.window.len(),
        canonical_rows,
        "painting does not erase recoverable canonical rows"
    );
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.reasoning.as_ref())
            .filter(|reasoning| reasoning.text == "checkpointed thought")
            .count(),
        1
    );
    assert!(rows.iter().any(|row| {
        row.reasoning
            .as_ref()
            .is_some_and(|reasoning| reasoning.text == "canonical prefix outside the live tail")
    }));
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.tool.as_ref())
            .filter(|card| card.op == "overlap-read")
            .count(),
        1
    );
    assert!(rows.iter().any(|row| row.text == "new stream suffix"));
    assert!(
        !rows.iter().any(|row| row.meta.is_some()),
        "a checkpoint is not a terminal footer between live parts"
    );

    // A later canonical read can be ahead of queued stream events. Repeated
    // public text is not a reason to move an older live tool behind a future one.
    let parts = &mut future.turn.as_mut().unwrap().parts;
    let TranscriptPart::Tool(mut later) = parts[2].clone() else {
        unreachable!()
    };
    later.op = "future-read".into();
    later.state = "completed".into();
    parts.extend([
        TranscriptPart::Text("new stream suffix".into()),
        TranscriptPart::Reasoning {
            text: "checkpointed thought".into(),
            duration_ms: Some(20),
        },
        TranscriptPart::Tool(later),
    ]);
    let projected = future.turn.as_mut().unwrap();
    projected.window = Some(HistoryPartWindow {
        start: 0,
        end: 6,
        total: 6,
    });
    projected.part_states = (0..6)
        .map(|sequence| PartState {
            sequence,
            ..Default::default()
        })
        .collect();
    state.window.reset(&page(vec![future], 1, false, false));
    let rows = state.transcript_rows();
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.tool.as_ref())
            .map(|card| card.op.as_str())
            .collect::<Vec<_>>(),
        ["overlap-read", "future-read"]
    );
    assert_eq!(
        rows.iter()
            .filter_map(|row| row.reasoning.as_ref())
            .filter(|part| part.text == "checkpointed thought")
            .count(),
        2
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.text == "new stream suffix")
            .count(),
        1
    );
}

#[tokio::test]
async fn exploration_and_its_failure_are_clickable_above_an_open_child_composer() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = fresh_state("composer-group-hit").await;
    state.parent_id = Some("parent".into());
    state.input = "preserved draft".into();
    state.open_composer(crate::composer::Tab::Subagents);
    state.begin_linked_turn(WorkerTurnId("child-active".into()));
    state.window.push_row(crate::history::HistoryRow {
        role: "tool".into(),
        tool: Some(crate::history::card_from_row(
            &oc_core::queries::ToolOpView {
                child_job: None,
                output_presentation: None,
                question: None,
                rowid: 1,
                op: "composer-read".into(),
                name: "read".into(),
                state: "failed".into(),
                input: Some(r#"{"path":"note.txt"}"#.into()),
                output: Some("actual file error".into()),
                output_bytes: 17,
                output_truncated: false,
                patch_effects: None,
                dcp: None,
                dcp_topic: None,
            },
        )),
        ..Default::default()
    });
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let (x, y) = (0..area.height)
        .find_map(|y| {
            let line = (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            line.contains("Explored").then(|| {
                (
                    (0..area.width)
                        .find(|x| buffer[(*x, y)].symbol() == "E")
                        .unwrap(),
                    y,
                )
            })
        })
        .unwrap();
    assert_eq!(
        state.exploration_hit(area, x, y).as_deref(),
        Some("composer-read")
    );
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(selection_mouse(kind, x, y), area);
    }
    assert!(state.exploration_expanded.contains("composer-read"));
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert!(!state.viewport().join("\n").contains("actual file error"));
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(selection_mouse(kind, x, y + 1), area);
    }
    assert!(state.inline_errors_expanded.contains("composer-read"));
    assert!(
        state.exploration_expanded.contains("composer-read"),
        "error and group have distinct keys"
    );
    assert!(state.viewport().join("\n").contains("actual file error"));
    assert_eq!(state.input, "preserved draft");
    assert!(state.composer_open());
}

#[tokio::test]
async fn scroll_edges_request_pages() {
    let mut state = fresh_state("s-scroll").await;

    // Fewer rows than the viewport: the top edge is already reached.
    state.attach_page(&page(
        vec![msg(1, Role::User, "m1"), msg(2, Role::Assistant, "m2")],
        9,
        true,
        false,
    ));
    assert!(state.needs_older());
    let outcome = state.scroll_transcript(true);
    assert_eq!(outcome.intent, Some(PanelIntent::LoadOlder));

    // A window that evicted its newest rows asks for a newer page at the
    // bottom edge, never while scrolling inside the window.
    let bulk: Vec<HistoryMessage> = (0..WINDOW_ROWS + 2)
        .map(|i| msg(100 + i as i64, Role::User, "older"))
        .collect();
    state.prepend_page(&page(bulk, 500, true, true));
    assert!(state.needs_newer());
    let outcome = state.scroll_transcript(false);
    assert_eq!(outcome.intent, Some(PanelIntent::LoadNewer));

    state.append_page(&page(
        vec![msg(999, Role::User, "newest")],
        500,
        true,
        false,
    ));
    assert!(!state.needs_newer());
    let outcome = state.scroll_transcript(true);
    assert_eq!(outcome.intent, None, "only the top edge loads older");

    let mut requested = None;
    // Blocks render as multiple lines (blank/border rows), so walk the
    // whole rendered transcript to reach the top edge.
    let max_scroll = state.max_scroll();
    for _ in 0..=max_scroll {
        let outcome = state.scroll_transcript(true);
        if outcome.intent.is_some() {
            requested = outcome.intent;
            break;
        }
    }
    assert_eq!(requested, Some(PanelIntent::LoadOlder));

    // Nothing older exists: the clamp keeps scrolling usable.
    let mut state = fresh_state("s-scroll2").await;
    state.attach_page(&page(
        (0..WINDOW_ROWS)
            .map(|i| msg(i as i64, Role::Assistant, "filler"))
            .collect(),
        100,
        false,
        false,
    ));
    let max_scroll = state.max_scroll();
    for _ in 0..=max_scroll {
        assert_eq!(state.scroll_transcript(true).intent, None);
    }
    assert_eq!(state.scroll, state.max_scroll());
    assert!(!state.needs_older());
}

#[tokio::test]
async fn retained_bytes_are_bounded_after_many_pages() {
    let mut state = fresh_state("s-retained").await;
    assert!(state.retained_bytes() < WINDOW_BYTES);
    let blob = "z".repeat(2048);
    for round in 0..60 {
        let older: Vec<HistoryMessage> = (0..8)
            .map(|i| msg(round as i64 * 16 + i, Role::User, &blob))
            .collect();
        let newer: Vec<HistoryMessage> = (0..8)
            .map(|i| msg(10_000 + round as i64 * 16 + i, Role::Assistant, &blob))
            .collect();
        state.prepend_page(&page(older, 100_000, true, true));
        state.append_page(&page(newer, 100_000, true, true));
        assert!(
            state.retained_bytes() <= WINDOW_BYTES + MAX_INPUT_BYTES,
            "round {round}: {} bytes",
            state.retained_bytes()
        );
        assert!(state.viewport().len() <= VIEWPORT_LINES);
    }
}

#[tokio::test]
async fn viewport_is_bounded_with_unicode() {
    let mut state = fresh_state("s-uni").await;
    let rows: Vec<HistoryMessage> = (0..80)
        .map(|i| {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            msg(i, role, "привет 🌍 мир")
        })
        .collect();
    state.attach_page(&page(rows, 80, true, false));
    type_text(&mut state, "next…").await;

    let view = state.viewport();
    assert!(view.len() <= VIEWPORT_LINES);
    assert!(
        view.iter().any(|line| line.contains("привет 🌍")),
        "{view:?}"
    );
    assert!(view.iter().any(|line| line == "┃  привет 🌍 мир"));
    assert_eq!(state.input(), "next…");
}

#[tokio::test]
async fn v02_multiround_reasoning_keeps_arrival_order() {
    let mut state = fresh_state("ordered").await;
    let turn = WorkerTurnId("turn".into());
    state.active_turn = Some(turn.clone());
    state.apply_reasoning_delta(&turn, "first thought");
    state.apply_tool_started(&turn, "op", "read", "{}");
    state.apply_reasoning_delta(&turn, "second thought");
    state.apply_delta(&turn, "answer");
    state.apply_finished(&turn, "answer", 1);
    let kinds: Vec<_> = state
        .history()
        .rows()
        .iter()
        .map(|r| {
            if let Some(reasoning) = &r.reasoning {
                reasoning.text.as_str()
            } else if r.tool.is_some() {
                "tool"
            } else if r.meta.is_some() {
                "footer"
            } else {
                r.text.as_str()
            }
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "first thought",
            "tool",
            "second thought",
            "answer",
            "footer"
        ]
    );
}

/// Tool-call events build transcript cards from real event payloads, in
/// upstream part order (text, tool, text) and survive the turn finish as
/// committed rows with the footer after them.
#[tokio::test]
async fn tool_events_build_transcript_cards_in_part_order() {
    let mut state = fresh_state("s-tools-ui").await;
    state.active_agent = Some("build".to_string());
    state.active_turn = Some(WorkerTurnId("t-ui-tools".to_string()));
    state.status = TuiStatus::Streaming;

    state.apply_delta(&WorkerTurnId("t-ui-tools".to_string()), "working");
    let patch = "*** Begin Patch\n*** Update File: a.txt\n@@\n-old\n+new\n*** End Patch";
    let input = serde_json::json!({"patchText": patch}).to_string();
    state.apply_tool_started(
        &WorkerTurnId("t-ui-tools".to_string()),
        "op-1",
        "apply_patch",
        &input,
    );
    // The running card is already a transcript row with the parsed diff.
    let rows = state.transcript_rows();
    assert_eq!(rows.len(), 2, "text part then tool card: {rows:?}");
    assert_eq!(rows[0].role, "assistant");
    assert_eq!(rows[0].text, "working");
    assert_eq!(rows[1].role, "tool");
    assert_eq!(rows[1].tool.as_ref().expect("card").state, "started");
    let lines = state.transcript_lines(0, 80);
    assert!(
        lines.iter().any(|line| line
            .plain_text()
            .contains("Request preview (not confirmed): a.txt")),
        "the diff card is visible while running"
    );

    state.apply_tool_finished_with_effects(
        &WorkerTurnId("t-ui-tools".to_string()),
        "op-1",
        "apply_patch",
        "completed",
        "Update a.txt (hash_before=x, hash_after=y)",
        42,
        false,
        Some(crate::patch_view::tests::effects()),
    );
    state.apply_delta(&WorkerTurnId("t-ui-tools".to_string()), "after tool");
    let rows = state.transcript_rows();
    assert_eq!(rows.len(), 3, "text, card, open text: {rows:?}");
    assert_eq!(rows[1].tool.as_ref().expect("card").state, "completed");
    assert_eq!(rows[2].text, "after tool");

    state.apply_finished(
        &WorkerTurnId("t-ui-tools".to_string()),
        "workingafter tool",
        2500,
    );
    assert!(!state.is_busy());
    let window = state.history().rows();
    assert_eq!(window.len(), 4, "three parts plus the footer: {window:?}");
    assert_eq!(window[0].role, "assistant");
    assert_eq!(window[0].text, "working");
    assert_eq!(window[1].role, "tool");
    assert_eq!(window[1].tool.as_ref().expect("card").state, "completed");
    assert_eq!(
        window[1].tool.as_ref().unwrap().patch_effects,
        Some(crate::patch_view::tests::effects())
    );
    assert_eq!(window[2].text, "after tool");
    assert_eq!(window[3].role, "assistant");
    assert!(window[3].text.is_empty(), "footer row carries no text");
    assert_eq!(
        window[3].meta.as_ref().expect("meta").duration_ms,
        Some(2500)
    );
    // Footer renders after every part (`routes/session/index.tsx:1934-1985`).
    let lines = state.transcript_lines(80, 80);
    let texts: Vec<String> = lines.iter().map(|line| line.plain_text()).collect();
    let footer = texts
        .iter()
        .position(|text| text.contains("Build"))
        .expect("footer");
    let diff = texts
        .iter()
        .position(|text| text.contains("← Patched"))
        .expect("diff");
    assert!(footer > diff, "footer follows the parts: {texts:?}");
}

#[tokio::test]
async fn vis10_user_hover_and_click_follow_painted_identity_through_wrap_and_scroll() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    let mut state = fresh_state("user-hover-target").await;
    let frame = Rect::new(0, 0, 40, 25);
    state.attach_page(&page(
        vec![
            msg(10, Role::User, &"same content ".repeat(12)),
            msg(11, Role::Assistant, "between"),
            msg(20, Role::User, &"same content ".repeat(12)),
        ],
        3,
        false,
        false,
    ));
    let rect = crate::shell::transcript_area(&state, frame);
    let (rows, total, scroll, targets) =
        state.visible_transcript_at_viewport_with_targets(rect.width, frame.width, rect.height);
    assert_eq!(rows.len(), targets.len());
    let second = targets
        .iter()
        .enumerate()
        .position(|(i, target)| {
            target.as_ref().is_some_and(|target| target.seq == 20)
                && rows[i].plain_text().contains("same")
        })
        .expect("second user block in painted viewport");
    let first = targets
        .iter()
        .position(|target| target.as_ref().is_some_and(|target| target.seq == 10));
    assert!(
        targets
            .iter()
            .filter(|target| target.as_ref().is_some_and(|target| target.seq == 20))
            .count()
            > 2,
        "wrapped content and both padding rows retain one identity"
    );
    let x = rect.x + 3;
    let y = rect.y + second as u16;
    state.observe_transcript_viewport(rect.width, frame.width, rect.height, total, scroll);
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    let painted = state.paint_transcript_at(rect, &rows, total, scroll, Some(frame), &targets);
    let theme = super::Theme::dark();
    let hover = theme.decrease(theme.user_message_background());
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                Paragraph::new(crate::styled::Lines::from(painted).into_text()),
                rect,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(rect.x, y)].bg,
        theme.user_message_background(),
        "agent border stays raised"
    );
    assert_eq!(buffer[(x, y)].bg, hover);
    for (row, target) in targets.iter().enumerate() {
        if target.as_ref().is_some_and(|target| target.seq == 20) {
            assert_eq!(
                buffer[(x, rect.y + row as u16)].bg,
                hover,
                "every visible line in the same user block is hovered"
            );
        }
    }
    if let Some(first) = first {
        assert_eq!(
            buffer[(x, rect.y + first as u16)].bg,
            theme.user_message_background(),
            "hover cannot bleed into an earlier message with the same text"
        );
    }
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert_eq!(
        state.panel(),
        &TuiPanel::MessageActions {
            message: oc_core::session::MessageId("fixture-20".into()),
            seq: 20
        }
    );
    assert_eq!(
        state
            .modal_options()
            .iter()
            .map(|o| o.title.as_str())
            .collect::<Vec<_>>(),
        ["Jump to", "Revert", "Copy", "Fork"]
    );
    state.select.cursor = 1;
    assert_eq!(
        state.panel_enter().intent,
        Some(PanelIntent::ChangeConversation {
            action: oc_core::queries::ConversationAction::Revert {
                message: oc_core::session::MessageId("fixture-20".into())
            }
        })
    );
    assert!(
        matches!(state.panel(), TuiPanel::MessageActions { .. }),
        "owner result owns dismissal"
    );
    state.close_panel();
    state.paint_transcript_at(rect, &rows, total, scroll, Some(frame), &targets);
    assert_eq!(
        state.user_message_target_at(frame, rect.x, y),
        None,
        "border is not the inner box"
    );
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 2, y),
        frame,
    );
    assert_eq!(
        state.user_message_target_at(frame, x, y),
        None,
        "selection suppresses actions"
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 2, y),
        frame,
    );
    assert_eq!(state.user_message_target_at(frame, x, y), None);
    assert_eq!(
        state.panel(),
        &TuiPanel::None,
        "drag cannot open Message Actions"
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("sa"));

    // Even a previously painted coordinate cannot target a new viewport.
    state.scroll = total;
    assert_eq!(state.user_message_target_at(frame, x, y), None);
    state.scroll = 0;
    state.clear_mouse_position();
    assert_eq!(state.user_message_target_at(frame, x, y), None);
}

#[tokio::test]
async fn vis10_resize_rehits_physical_hover_without_retaining_message_press() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};

    let mut state = fresh_state("resize-user-hover").await;
    let text = "same accepted user with café 中文 and wrapping ".repeat(2);
    state.attach_page(&page(vec![msg(1, Role::User, &text)], 1, false, false));
    let mut current = snapshot();
    current.agent_id = Some("y".into());
    current
        .chrome
        .agent_colors
        .insert("y".into(), "#12ab34".into());
    state.apply_catalog(current);
    state.restore_prompt("unsent resize draft".into());
    let stored = state.history().rows().to_vec();
    let theme = super::Theme::dark();
    let stripe = ratatui::style::Color::Rgb(0x12, 0xab, 0x34);
    let mut frame = Rect::new(0, 0, 120, 40);
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let area = crate::shell::transcript_area(&state, frame);
    let (_, _, _, targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
    let x = area.x + 8;
    let y = area.y + targets.iter().position(Option::is_some).unwrap() as u16 + 1;
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(x, y)].bg,
        theme.decrease(theme.user_message_background())
    );

    for next in [Rect::new(0, 0, 120, 320), Rect::new(0, 0, 80, 24)] {
        state.handle_mouse(
            selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
            frame,
        );
        assert!(state.message_down.is_some());
        state.resize_mouse_position(next);
        assert_eq!(state.mouse_position(), Some((x, y, next)));
        assert!(state.message_down.is_none());
        assert!(state.selection.is_none());
        assert!(state.painted_user_message_target_at(next, x, y).is_none());
        terminal.backend_mut().resize(next.width, next.height);
        terminal.resize(next).unwrap();
        terminal
            .draw(|f| {
                assert_eq!(f.area(), next);
                crate::shell::render(f, &state);
            })
            .unwrap();
        let area = crate::shell::transcript_area(&state, next);
        let (_, _, _, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, next.width, area.height);
        for (row, target) in targets.iter().enumerate() {
            if target.as_ref().is_some_and(|target| target.seq == 1) {
                let buffer = terminal.backend().buffer();
                assert_eq!(
                    buffer[(x, area.y + row as u16)].bg,
                    theme.decrease(theme.user_message_background())
                );
                assert_eq!(
                    buffer[(area.x, area.y + row as u16)].bg,
                    theme.user_message_background()
                );
                assert_eq!(buffer[(area.x, area.y + row as u16)].fg, stripe);
            }
        }
        state.handle_mouse(
            selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
            next,
        );
        assert_eq!(
            state.panel(),
            &TuiPanel::None,
            "resize cannot authorize an old press"
        );
        assert_eq!(state.history().rows(), stored);
        assert_eq!(state.input(), "unsent resize draft");
        frame = next;
    }

    // The pointer is not a message receipt: a new paint resolves the current ID.
    let replacement = msg(9, Role::User, &text);
    state
        .window
        .reset(&page(vec![replacement.clone()], 1, false, false));
    state.clear_transcript_selection();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(x, y)].bg,
        theme.decrease(theme.user_message_background())
    );
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(selection_mouse(kind, x, y), frame);
    }
    assert_eq!(
        state.panel(),
        &TuiPanel::MessageActions {
            message: replacement.id.clone(),
            seq: 9
        }
    );
    state.select.cursor = 1;
    assert_eq!(
        state.panel_enter().intent,
        Some(PanelIntent::ChangeConversation {
            action: oc_core::queries::ConversationAction::Revert {
                message: replacement.id
            }
        })
    );
    state.close_panel();
    state.handle_mouse(selection_mouse(MouseEventKind::Moved, x, y), frame);
    state.resize_mouse_position(Rect::new(0, 0, 5, 24));
    assert_eq!(
        state.mouse_position(),
        None,
        "a pointer outside the resized frame is gone"
    );
    assert_eq!(state.input(), "unsent resize draft");
}

#[tokio::test]
async fn vis10_painted_target_rejects_identity_change_and_distinguishes_live_echo() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("identity-ownership").await;
    let frame = Rect::new(0, 0, 70, 28);
    let mut message = msg(9, Role::User, "identical visible text");
    message.id = oc_core::session::MessageId("owner-original".into());
    state.attach_page(&page(vec![message.clone()], 1, false, false));
    let area = crate::shell::transcript_area(&state, frame);
    let (lines, total, scroll, targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
    state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
    state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
    let y = area.y + targets.iter().position(Option::is_some).unwrap() as u16;
    let x = area.x + 3;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert_eq!(
        state.panel(),
        &TuiPanel::MessageActions {
            message: message.id.clone(),
            seq: 9
        }
    );
    let click = state.click;
    state.close_panel();
    state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
    state.click = click;
    assert_eq!(
        state
            .user_message_target_at(frame, x, y)
            .unwrap()
            .message_id
            .as_deref(),
        Some(&message.id)
    );

    message.id = oc_core::session::MessageId("owner-replacement".into());
    // Keep the old paint and click intact: only durable identity changes.
    state.window.reset(&page(vec![message], 1, false, false));
    assert!(
        state.user_message_target_at(frame, x, y).is_none(),
        "identical pixels/sequence cannot authorize a stale owner identity"
    );
    state.window.push_synthetic("user", "live echo", None, None);
    let (lines, total, scroll, targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
    state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
    state.paint_transcript_at(area, &lines, total, scroll, Some(frame), &targets);
    let live_row = targets
        .iter()
        .position(|t| t.as_ref().is_some_and(|t| t.seq == i64::MAX))
        .unwrap();
    state.click = None;
    state.handle_mouse(
        selection_mouse(
            MouseEventKind::Down(MouseButton::Left),
            x,
            area.y + live_row as u16,
        ),
        frame,
    );
    state.handle_mouse(
        selection_mouse(
            MouseEventKind::Up(MouseButton::Left),
            x,
            area.y + live_row as u16,
        ),
        frame,
    );
    let target = state
        .user_message_target_at(frame, x, area.y + live_row as u16)
        .unwrap();
    assert!(
        target.message_id.is_none(),
        "echo may hover but has no durable action owner"
    );
    assert_eq!(
        state.panel(),
        &TuiPanel::None,
        "live echo cannot open owner actions"
    );
    assert!(
        targets
            .iter()
            .flatten()
            .filter(|t| t.seq == 9)
            .all(|t| t.message_id.as_ref().unwrap().0 == "owner-replacement")
    );
}

#[tokio::test]
async fn message_press_receipt_rejects_other_release_repaint_and_history_replacement() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("press-identity").await;
    let frame = Rect::new(0, 0, 90, 35);
    let first = msg(1, Role::User, "first target");
    let second = msg(2, Role::User, "second target");
    for replacement in [0, 1, 2] {
        state.attach_page(&page(vec![first.clone(), second.clone()], 2, false, false));
        let area = crate::shell::transcript_area(&state, frame);
        let (rows, total, scroll, targets) =
            state.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
        let y1 = area.y
            + targets
                .iter()
                .position(|t| t.as_ref().is_some_and(|t| t.seq == 1))
                .unwrap() as u16;
        let y2 = area.y
            + targets
                .iter()
                .position(|t| t.as_ref().is_some_and(|t| t.seq == 2))
                .unwrap() as u16;
        let x = area.x + 4;
        state.handle_mouse(
            selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y1),
            frame,
        );
        let release_y = if replacement != 0 {
            let mut swapped = first.clone();
            swapped.id = oc_core::session::MessageId("different-owner".into());
            let next_page = page(vec![swapped, second.clone()], 2, false, false);
            if replacement == 1 {
                state.attach_page(&next_page);
            } else {
                // Repaint replaces A with B before release without a view reset.
                state.window.reset(&next_page);
            }
            let (rows, total, scroll, targets) = state.visible_transcript_at_viewport_with_targets(
                area.width,
                frame.width,
                area.height,
            );
            state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
            state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &targets);
            y1
        } else {
            y2
        };
        state.handle_mouse(
            selection_mouse(MouseEventKind::Up(MouseButton::Left), x, release_y),
            frame,
        );
        assert_eq!(state.panel(), &TuiPanel::None);
    }
}

#[tokio::test]
async fn message_actions_focus_intents_clipboard_result_and_escape() {
    use oc_core::session::MessageId;
    let mut state = fresh_state("message-actions").await;
    let message = msg(4, Role::User, "stored prompt");
    state.attach_page(&page(vec![message.clone()], 1, false, false));
    state.restore_prompt("unfinished draft".into());
    state.panel = TuiPanel::MessageActions {
        message: message.id.clone(),
        seq: 4,
    };
    state.handle_key(KeyAction::Down).await;
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.select.cursor, 2);
    for (key, cursor) in [
        (KeyAction::Home, 0),
        (KeyAction::End, 3),
        (KeyAction::Up, 2),
        // Shared Select source wraps an overshooting page to an endpoint,
        // rather than retaining the modulo remainder.
        (KeyAction::PageUp, 3),
        (KeyAction::PageDown, 0),
        (KeyAction::Down, 1),
        (KeyAction::Down, 2),
    ] {
        state.handle_key(key).await;
        assert_eq!(state.select.cursor, cursor);
    }
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::CopyMessage {
            message: message.id.clone(),
            seq: 4
        })
    );
    state
        .copy_message_text("exact owner text\n".into())
        .unwrap();
    assert_eq!(
        state.take_copy_request().as_deref(),
        Some("exact owner text\n")
    );
    state.report_clipboard_result(Err("unavailable".into()));
    assert!(matches!(state.panel(), TuiPanel::MessageActions { .. }));
    assert_eq!(state.input(), "unfinished draft");
    state.report_clipboard_result(Ok(()));
    assert_eq!(state.panel(), &TuiPanel::None);
    state.panel = TuiPanel::MessageActions {
        message: message.id.clone(),
        seq: 4,
    };
    state.handle_key(KeyAction::End).await;
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ForkMessage {
            message: message.id.clone()
        })
    );
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.input(), "unfinished draft");
    state.panel = TuiPanel::MessageActions {
        message: MessageId("stale".into()),
        seq: 4,
    };
    assert!(state.panel_enter().intent.is_none());
    assert!(
        state
            .copy_message_text("x".repeat(super::MAX_SELECTION_BYTES + 1))
            .is_err()
    );
    assert!(state.take_copy_request().is_none());
}

#[tokio::test]
async fn vis27_paired_word_and_line_selection_paint_explicit_source_cells() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend, style::Modifier, widgets::Paragraph};

    let mut state = fresh_state("selection-paired-cells").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.set_clipboard_mode(super::ClipboardMode::Select);
    state.live_text = "GEOMETRY-SHORT: tool read completed.".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "GEOMETRY-SHORT");
    let theme = super::Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    for (clicks, selected_text) in [
        (2, "GEOMETRY-SHORT"),
        (3, "GEOMETRY-SHORT: tool read completed."),
    ] {
        state.click = None;
        for _ in 0..clicks {
            state.handle_mouse(
                selection_mouse(MouseEventKind::Down(MouseButton::Left), x + 3, y),
                frame,
            );
            state.handle_mouse(
                selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 3, y),
                frame,
            );
        }
        assert_eq!(state.take_copy_request().as_deref(), Some(selected_text));
        let rect = crate::shell::transcript_area(&state, frame);
        let (rows, total, scroll) =
            state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let highlighted = state.paint_transcript(rect, &rows, total, scroll);
        terminal
            .draw(|frame| {
                frame.render_widget(
                    ratatui::widgets::Block::default().style(
                        ratatui::style::Style::default()
                            .fg(theme.text())
                            .bg(theme.background()),
                    ),
                    frame.area(),
                );
                frame.render_widget(
                    Paragraph::new(crate::styled::Lines::from(highlighted).into_text()),
                    rect,
                );
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        for col in x..x + selected_text.len() as u16 {
            let cell = &cells[(col, y)];
            assert_eq!(cell.fg, theme.background(), "fg at {col}");
            assert_eq!(cell.bg, theme.text(), "bg at {col}");
            assert!(!cell.modifier.contains(Modifier::REVERSED), "at {col}");
        }
        let outside = &cells[(x + selected_text.len() as u16, y)];
        assert_eq!(outside.bg, theme.background());
        if x > rect.x {
            let leading = &cells[(x - 1, y)];
            assert_eq!(leading.bg, theme.background(), "indent stays unselected");
            assert!(!leading.modifier.contains(Modifier::REVERSED));
        }
    }
}

#[tokio::test]
async fn vis27_drag_and_repeated_click_copy_only_selected_painted_text() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-drag").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.set_clipboard_mode(super::ClipboardMode::Select);
    state.live_text = "alpha beta gamma".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "beta");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None, "ordinary click is empty");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("beta"));
    let rect = crate::shell::transcript_area(&state, frame);
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
    let highlighted = state.paint_transcript(rect, &rows, total, scroll);
    assert!(
        highlighted
            .iter()
            .flat_map(|line| line.spans())
            .any(|span| {
                span.content() == "beta"
                    && span.style().fg == Some(super::Theme::dark().background())
                    && span.style().bg == Some(super::Theme::dark().text())
                    && !span
                        .style()
                        .add_modifier
                        .contains(ratatui::style::Modifier::REVERSED)
            })
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert!(
        state
            .take_copy_request()
            .unwrap()
            .contains("alpha beta gamma")
    );

    // A fresh press followed by a true drag selects exact visible cells.
    state.click = None;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 4, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 4, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("beta"));
    assert!(state.selection.is_some(), "copy preserves highlight");
    state.report_clipboard_result(Err("clipboard unavailable".into()));
    assert_eq!(state.note_variant(), Some(NoteVariant::Error));
    assert_eq!(state.note(), Some("clipboard unavailable"));
    state.report_clipboard_result(Ok(()));
    assert_eq!(state.note_variant(), Some(NoteVariant::Info));
    assert_eq!(state.note(), Some("Copied to clipboard"));
}

#[tokio::test]
async fn vis27_manual_right_click_and_resize_do_not_copy_unseen_rows() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-manual").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.set_clipboard_mode(super::ClipboardMode::Manual);
    state.live_text = "word after".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "word");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 4, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 4, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("word"));
    state.live_text = "changed private content".into();
    let _ = paint_selection_fixture(&mut state, frame, "changed");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(
        state.take_copy_request(),
        None,
        "stale rows must never leak"
    );
    state.clear_mouse_position();
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
}

#[tokio::test]
async fn vis27_drag_on_reasoning_header_cannot_toggle_or_steal_modal() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-reasoning-owner").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_reasoning = "**thinking**".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "Thinking");
    let id = state
        .reasoning_hit(frame, x, y)
        .expect("painted reasoning header");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 1, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
        frame,
    );
    assert!(!state.reasoning_expanded.contains(&id));
    state.handle_key(KeyAction::Commands).await;
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    assert_eq!(state.panel(), &TuiPanel::Commands);
}

#[tokio::test]
async fn vis27_wrapped_wide_graphemes_copy_complete_visible_rows() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use unicode_width::UnicodeWidthStr;
    let mut state = fresh_state("selection-wide-wrap").await;
    let frame = Rect::new(0, 0, 40, 25);
    state.set_clipboard_mode(super::ClipboardMode::Select);
    state.live_text = "🧑‍💻".repeat(36);
    let (x, y) = paint_selection_fixture(&mut state, frame, "🧑‍💻");
    let rect = crate::shell::transcript_area(&state, frame);
    let (rows, _, _) = state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
    let next = rows
        .iter()
        .enumerate()
        .skip((y - rect.y) as usize + 1)
        .find_map(|(row, line)| {
            line.plain_text().find("🧑‍💻").map(|byte| {
                (
                    rect.x + UnicodeWidthStr::width(&line.plain_text()[..byte]) as u16,
                    rect.y + row as u16,
                )
            })
        })
        .expect("wrapped emoji row");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x + 1, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), next.0 + 2, next.1),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), next.0 + 2, next.1),
        frame,
    );
    let copied = state.take_copy_request().expect("wrapped selection");
    assert!(copied.contains('\n'));
    assert!(copied.contains("🧑‍💻"));
    assert!(!copied.contains('�'));
}

#[tokio::test]
async fn vis27_double_click_word_is_clipped_to_painted_wrap_row() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-word-wrap").await;
    let frame = Rect::new(0, 0, 40, 25);
    state.live_text = "q".repeat(200);
    let (x, y) = paint_selection_fixture(&mut state, frame, "qqqq");
    let rect = crate::shell::transcript_area(&state, frame);
    let row = (y - rect.y) as usize;
    let visible_word = state.painted_transcript.borrow().as_ref().unwrap().rows[row].plain_text();
    assert!(visible_word.len() < 200, "word must wrap on screen");
    for _ in 0..2 {
        state.handle_mouse(
            selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
            frame,
        );
        state.handle_mouse(
            selection_mouse(MouseEventKind::Up(MouseButton::Left), x, y),
            frame,
        );
    }
    assert_eq!(
        state.take_copy_request().as_deref(),
        Some(visible_word.trim_start())
    );
    assert!(!visible_word.contains('\n'));
}

#[tokio::test]
async fn vis27_nonowned_drag_cannot_change_completed_selection_or_copy() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-owner").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_text = "alpha beta gamma".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "beta");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 4, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Right), x + 4, y),
        frame,
    );
    assert_eq!(
        state.take_copy_request().as_deref(),
        Some("beta"),
        "release button is immaterial"
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Right), x + 10, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Right), x + 10, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    assert_eq!(state.selection_text(), Ok(Some("beta".into())));
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Right), x + 8, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 8, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    assert_eq!(state.selection_text(), Ok(None));
}

#[tokio::test]
async fn vis27_modifier_and_surface_click_preserve_completed_highlight() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-surface").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_text = "alpha beta gamma".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "beta");
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 4, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 4, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("beta"));
    let mut modified = selection_mouse(MouseEventKind::Down(MouseButton::Left), x + 6, y);
    modified.modifiers = KeyModifiers::SHIFT;
    state.handle_mouse(modified, frame);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 7, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 7, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    assert_eq!(state.selection_text(), Ok(Some("beta".into())));

    let rect = crate::shell::transcript_area(&state, frame);
    state.handle_mouse(
        selection_mouse(
            MouseEventKind::Down(MouseButton::Left),
            rect.x,
            rect.bottom(),
        ),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), rect.x, rect.bottom()),
        frame,
    );
    assert_eq!(state.selection_text(), Ok(Some("beta".into())));
    state.set_clipboard_mode(super::ClipboardMode::Manual);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("beta"));
}

#[tokio::test]
async fn vis27_oversized_selection_reports_error_without_clipboard_request() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-size").await;
    let frame = Rect::new(0, 0, 100, 28);
    let (x, y) = paint_selection_fixture(&mut state, frame, "");
    let large = "z".repeat(super::MAX_SELECTION_BYTES + 1);
    let rect = crate::shell::transcript_area(&state, frame);
    let painted = super::PaintedTranscript {
        area: rect,
        rows: vec![crate::styled::Line::plain(&large)],
        user_targets: vec![],
        child_targets: vec![],
        total: 1,
        scroll: 0,
    };
    state.selection = Some(super::TranscriptSelection {
        anchor: super::TextPoint { row: 0, byte: 0 },
        focus: super::TextPoint {
            row: 0,
            byte: large.len(),
        },
        painted: painted.clone(),
        dragging: false,
    });
    *state.painted_transcript.borrow_mut() = Some(painted);
    state.set_clipboard_mode(super::ClipboardMode::Manual);
    // The same snapshot is intentionally used for the byte-budget check;
    // no clipboard request is produced even when the selection is nonempty.
    assert_eq!(
        state.selection_text(),
        Err("Selection exceeds clipboard size limit")
    );
    state.pending_copy = Some("previous request".into());
    state.request_selection_copy();
    assert_eq!(state.take_copy_request(), None);
    assert_eq!(state.note_variant(), Some(NoteVariant::Error));
    assert_eq!(state.note(), Some("Selection exceeds clipboard size limit"));
    // A right-down over the real fixture's unchanged text remains empty.
    state.clear_transcript_selection();
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
}

#[tokio::test]
async fn vis27_unrelated_mouse_events_do_not_rebuild_painted_transcript() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let mut state = fresh_state("selection-motion-cost").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_text = "before".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "before");
    let before = state
        .painted_transcript
        .borrow()
        .as_ref()
        .unwrap()
        .rows
        .clone();
    state.live_text = "after".into();
    for kind in [
        MouseEventKind::Moved,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
        MouseEventKind::Drag(MouseButton::Right),
        MouseEventKind::Up(MouseButton::Right),
    ] {
        state.handle_mouse(selection_mouse(kind, x, y), frame);
    }
    assert_eq!(
        state.painted_transcript.borrow().as_ref().unwrap().rows,
        before
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    assert!(
        state.selection.is_none(),
        "changed transcript invalidates stale paint"
    );
}
