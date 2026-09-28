use super::*;

#[tokio::test]
async fn vis15_finished_turn_immediately_renders_unstreamed_canonical_text_in_part_order() {
    use oc_core::queries::TranscriptPart;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::fs::write(
        project.join("opencode.json"),
        serde_json::json!({
            "model":"fixture/m", "provider":{"fixture":{"npm":"@ai-sdk/openai",
            "options":{"baseURL":format!("http://{address}/v1"),"apiKey":"fixture-key"},
            "models":{"m":{}}}}
        })
        .to_string(),
    )
    .unwrap();
    let message = |id: &str, text: &str| {
        serde_json::json!({
            "type":"message", "role":"assistant", "id":id, "status":"completed",
            "content":[{"type":"output_text", "text":text}]
        })
    };
    let first = message("m1", "between");
    let last = message("m2", "final");
    let reason = |id: &str| {
        serde_json::json!({
            "type":"reasoning", "id":id, "status":"completed",
            "encrypted_content":format!("private-{id}"), "summary":[]
        })
    };
    let r1 = reason("r1");
    let r2 = reason("r2");
    let sse = [
        serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"Inspecting"}),
        serde_json::json!({"type":"response.output_item.done","item":r1}),
        serde_json::json!({"type":"response.output_text.delta","delta":"between"}),
        serde_json::json!({"type":"response.output_item.done","output_index":1,"item":first}),
        serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"Verifying"}),
        serde_json::json!({"type":"response.output_item.done","item":r2}),
        serde_json::json!({"type":"response.output_item.done","output_index":3,"item":last}),
        serde_json::json!({"type":"response.completed","response":{"status":"completed",
                "output":[r1,first,r2,last],"usage":{"input_tokens":10,"output_tokens":5}}}),
    ]
    .iter()
    .map(|event| format!("data: {event}\n\n"))
    .collect::<String>();
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut served = false;
        while !stopping.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    counted.fetch_add(1, Ordering::SeqCst);
                    assert!(!served, "history refresh must not make a provider request");
                    served = true;
                    socket
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut chunk = [0; 4096];
                    let end = loop {
                        let n = socket.read(&mut chunk).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&chunk[..n]);
                        if let Some(pos) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                            break pos + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    while bytes.len() < end + length {
                        let n = socket.read(&mut chunk).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("fake provider: {error}"),
            }
        }
        assert!(served, "no provider request arrived");
    });
    let env = BTreeMap::from([
        ("HOME".into(), root.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &root.path().join("data"), env)
            .await
            .unwrap();
    let session = SessionId::new("vis15-complete-text").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Already titled".into())
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut rx = app.subscribe();
    state.handle_paste("show steps");
    state.handle_key(KeyAction::Enter).await;
    let mut loop_state = LoopState::default();
    let mut saw_finished = false;
    for _ in 0..32 {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("worker event timeout")
            .expect("worker event");
        state.poll_submission();
        if let CoreEvent::TurnFinished { text, .. } = &event {
            assert_eq!(text, "betweenfinal");
            assert!(!state.history().rows().iter().any(|row| row.text == "final"));
            saw_finished = true;
        }
        handle_worker_event(&app, &mut state, &mut loop_state, &session, event)
            .await
            .unwrap();
        if saw_finished {
            break;
        }
    }
    assert!(saw_finished);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    let page = app
        .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await
        .unwrap();
    let parts = &page
        .rows
        .iter()
        .find_map(|row| row.turn.as_ref())
        .unwrap()
        .parts;
    assert!(matches!(&parts[..],
            [TranscriptPart::Reasoning { text: a, .. }, TranscriptPart::Text(b),
             TranscriptPart::Reasoning { text: c, .. }, TranscriptPart::Text(d)]
            if a == "Inspecting" && b == "between" && c == "Verifying" && d == "final"));
    let rows = state.history().rows();
    let visible: Vec<_> = rows
        .iter()
        .filter_map(|row| {
            if let Some(reasoning) = &row.reasoning {
                Some(reasoning.text.as_str())
            } else if !row.text.is_empty() && row.role == "assistant" {
                Some(row.text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(visible, ["Inspecting", "between", "Verifying", "final"]);
    let painted: Vec<_> = state
        .transcript_lines(80, 80)
        .iter()
        .map(|line| line.plain_text())
        .collect();
    let between = painted
        .iter()
        .position(|line| line.contains("between"))
        .unwrap();
    let final_text = painted
        .iter()
        .position(|line| line.contains("final"))
        .unwrap();
    assert!(
        between < final_text,
        "completed frame must include both text parts in order: {painted:?}"
    );
    assert_eq!(rows.iter().filter(|row| row.text == "final").count(), 1);
    assert!(!format!("{rows:?}").contains("private-"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    // A history-page lookup is local; no second Responses call is needed.
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
}

#[test]
fn sampled_view_metrics_include_parked_routes_and_preserve_peaks() {
    let (app, _, _) = CoreApp::channel(8);
    let mut active = TuiState::new_home(app.clone());
    active.handle_paste("draft");
    let mut parked = TuiState::new(app, SessionId::new("parked").unwrap());
    parked.attach_page(&HistoryPage {
        rows: vec![HistoryMessage {
            seq: 1,
            role: Role::Assistant,
            id: oc_core::session::MessageId("cached-assistant".into()),
            text: "# cached markdown".into(),
            turn: None,
            model_switch: None,
        }],
        total: 1,
        ..Default::default()
    });
    parked.transcript_lines(60, 80);
    let cached = parked.live_view_metrics().markdown_cache_retained_bytes;
    assert!(cached > 0);
    let mut deck = LoopState {
        home: Some(parked),
        ..Default::default()
    };
    let mut metrics = FrameMetrics::default();
    metrics.sample_views(&active, &deck);
    assert_eq!(metrics.live_current.text_bytes, 0);
    assert_eq!(metrics.live_current.part_count, 0);
    assert_eq!(metrics.live_current.markdown_cache_retained_bytes, cached);
    // Sampling ignores input drafts; it is neither streamed text nor a UI queue.
    assert_eq!(metrics.live_peak.text_bytes, 0);
    deck.home = None;
    metrics.sample_views(&active, &deck);
    assert_eq!(metrics.live_current.markdown_cache_retained_bytes, 0);
    assert_eq!(metrics.live_peak.markdown_cache_retained_bytes, cached);
}

#[test]
fn worker_event_metrics_count_overwritten_broadcast_events_and_stop_drain() {
    let (sender, mut rx) = tokio::sync::broadcast::channel(2);
    let session = SessionId::new("s-lag".to_string()).expect("session");
    for i in 0..5 {
        sender
            .send(CoreEvent::TextDelta {
                session: session.clone(),
                turn: WorkerTurnId("t-lag".to_string()),
                delta: i.to_string(),
            })
            .expect("event sent");
    }
    let mut metrics = FrameMetrics::default();
    metrics.worker_event_queue_peak = metrics.worker_event_queue_peak.max(rx.len());
    assert_eq!(metrics.worker_event_queue_peak, 5);
    assert!(matches!(
        try_worker_event(&mut rx, Some(&mut metrics)),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(3))
    ));
    assert_eq!(metrics.worker_event_queue_lagged, 3);
    assert_eq!(rx.len(), 2, "remaining events wait for the next drain");
    assert!(matches!(
        try_worker_event(&mut rx, Some(&mut metrics)),
        Ok(CoreEvent::TextDelta { delta, .. }) if delta == "3"
    ));
    assert_eq!(metrics.worker_event_queue_lagged, 3);
}

#[tokio::test]
async fn immediate_quit_reconciles_only_accepted_fresh_root_before_owner_shutdown() {
    // Both receipt schedules are real channel orderings: one is already
    // delivered when the Quit key polls, the other arrives only after
    // Quit has requested cancellation through the owner.
    for ack_before_quit in [true, false] {
        for accepted in [true, false] {
            let (app, mut inbox, _) = CoreApp::channel(8);
            let mut state = TuiState::new_home(app.clone());
            state.chrome.location = Some("/fixture".into());
            let mut deck = LoopState {
                location: Some("/fixture".into()),
                ..Default::default()
            };
            state.handle_paste("first prompt");
            state.handle_key(KeyAction::Enter).await;
            let Some(InboxMsg::SubmitFresh {
                session, text, ack, ..
            }) = inbox.recv().await
            else {
                panic!("one fresh submission")
            };
            assert_eq!(text, "first prompt");
            // The user can still edit the draft while acceptance is pending.
            state.handle_key(KeyAction::Char('!')).await;
            let root = session.clone();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let worker = tokio::spawn(async move {
                let decision = || {
                    if accepted {
                        Ok(WorkerTurnId("first-turn".into()))
                    } else {
                        Err(CoreError::Application("rejected".into()))
                    }
                };
                if ack_before_quit {
                    ack.send(decision()).unwrap();
                    ready_tx.send(()).unwrap();
                } else {
                    let Some(InboxMsg::Cancel {
                        session: target,
                        ack: cancel,
                    }) = inbox.recv().await
                    else {
                        panic!("cancel must follow pending fresh submit")
                    };
                    assert_eq!(target, root);
                    ack.send(decision()).unwrap();
                    cancel
                        .send(if accepted {
                            Ok(())
                        } else {
                            Err(CoreError::TurnNotActive)
                        })
                        .unwrap();
                }
                let mut saves = 0;
                loop {
                    match inbox.recv().await.expect("owner must shut down") {
                        InboxMsg::SaveTabDeck { deck, ack } => {
                            assert!(
                                accepted && saves == 0,
                                "only the accepted root is saved once"
                            );
                            assert_eq!(deck.sessions, vec![root.clone()]);
                            assert_eq!(deck.active, Some(root.clone()));
                            assert_eq!(deck.location, "/fixture");
                            saves += 1;
                            ack.send(Ok(TabDeckSnapshot {
                                revision: Some("saved".into()),
                                ..deck
                            }))
                            .unwrap();
                        }
                        InboxMsg::Shutdown => break,
                        _ => panic!("no replay or unexpected owner work"),
                    }
                }
                saves
            });
            if ack_before_quit {
                ready_rx.await.unwrap();
            }
            state.handle_key(KeyAction::Quit).await;
            assert_eq!(state.status(), &TuiStatus::Quit);
            reconcile_exit(&app, &mut state, &mut deck).await.unwrap();
            assert_eq!(state.status(), &TuiStatus::Quit);
            assert_eq!(state.input(), "first prompt!");
            assert_eq!(state.attached_session(), accepted.then_some(&session));
            assert_eq!(deck.tabs.len(), usize::from(accepted));
            app.shutdown().await.unwrap();
            assert_eq!(worker.await.unwrap(), usize::from(accepted));
        }
    }
}

#[tokio::test]
async fn key_consumed_home_receipt_is_saved_once_by_poll_before_exit() {
    for accepted in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new_home(app.clone());
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            ..Default::default()
        };
        state.handle_paste("first prompt");
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::SubmitFresh { session, ack, .. }) = inbox.recv().await else {
            panic!("one fresh submission")
        };
        state.handle_paste("unsent draft");
        ack.send(if accepted {
            Ok(WorkerTurnId("first-turn".into()))
        } else {
            Err(CoreError::Application("rejected".into()))
        })
        .unwrap();
        // Ctrl+C polls the receipt before the next frame's poll_and_sync.
        state.handle_key(KeyAction::Interrupt).await;
        assert_eq!(state.input(), "");
        assert_eq!(
            state.status(),
            if accepted {
                &TuiStatus::Streaming
            } else {
                &TuiStatus::Idle
            }
        );
        assert_eq!(state.attached_session(), accepted.then_some(&session));
        assert!(deck.active_tab.is_none(), "no deck sync on the key");

        let worker = tokio::spawn(async move {
            let mut saves = 0;
            loop {
                match inbox.recv().await.expect("owner must shut down") {
                    InboxMsg::SaveTabDeck { deck, ack } => {
                        assert!(accepted && saves == 0, "only one accepted-root save");
                        assert_eq!(deck.location, "/fixture");
                        assert_eq!(deck.sessions, vec![session.clone()]);
                        assert_eq!(deck.active, Some(session.clone()));
                        saves += 1;
                        ack.send(Ok(TabDeckSnapshot {
                            revision: Some("saved".into()),
                            ..deck
                        }))
                        .unwrap();
                    }
                    InboxMsg::Shutdown => break,
                    _ => panic!("no replay or unexpected owner work"),
                }
            }
            saves
        });
        poll_and_sync(&app, &mut state, &mut deck).await;
        assert_eq!(deck.tabs.len(), usize::from(accepted));
        assert_eq!(deck.revision.as_deref(), accepted.then_some("saved"));
        poll_and_sync(&app, &mut state, &mut deck).await;
        state.handle_key(KeyAction::Interrupt).await;
        assert_eq!(state.status(), &TuiStatus::Quit);
        reconcile_exit(&app, &mut state, &mut deck).await.unwrap();
        app.shutdown().await.unwrap();
        assert_eq!(worker.await.unwrap(), usize::from(accepted));
    }

    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    state.handle_paste("first prompt");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { ack, .. }) = inbox.recv().await else {
        panic!("one pending fresh submission")
    };
    state.handle_paste("unsent draft");
    state.handle_key(KeyAction::Interrupt).await;
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(state.attached_session().is_none());
    assert!(deck.active_tab.is_none());
    assert!(inbox.try_recv().is_err(), "pending root cannot save");
    drop(ack);
}

#[tokio::test]
async fn quit_with_pending_existing_tab_does_not_wait_for_receipt_or_save() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let root = SessionId::new("durable").unwrap();
    let mut state = TuiState::new(app.clone(), root.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        tabs: vec![None],
        tab_cards_before: vec![None],
        active_tab: Some(0),
        ..Default::default()
    };
    state.handle_paste("followup");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { session, ack, .. }) = inbox.recv().await else {
        panic!("existing turn")
    };
    assert_eq!(session, root);
    state.handle_key(KeyAction::Quit).await;
    reconcile_exit(&app, &mut state, &mut deck).await.unwrap();
    assert_eq!(state.status(), &TuiStatus::Quit);
    assert_eq!(state.input(), "followup");
    assert_eq!(deck.tabs.len(), 1);
    assert!(
        inbox.try_recv().is_err(),
        "no cancel or tab save on existing root"
    );
    app.shutdown().await.unwrap();
    assert!(matches!(inbox.recv().await, Some(InboxMsg::Shutdown)));
    drop(ack);
}

#[tokio::test]
async fn accepted_quit_reports_deck_save_failure_before_owner_shutdown() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    state.handle_paste("first turn");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { ack, .. }) = inbox.recv().await else {
        panic!("first turn")
    };
    ack.send(Ok(WorkerTurnId("committed".into()))).unwrap();
    state.handle_key(KeyAction::Quit).await;
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SaveTabDeck { ack, .. }) = inbox.recv().await else {
            panic!("persist before shutdown")
        };
        ack.send(Err(CoreError::TabDeckConflict)).unwrap();
        assert!(matches!(inbox.recv().await, Some(InboxMsg::Shutdown)));
    });
    assert_eq!(
        reconcile_exit(&app, &mut state, &mut deck).await,
        Err("quit tab deck: tab deck changed; reload before saving".into())
    );
    app.shutdown().await.unwrap();
    worker.await.unwrap();
}

#[test]
fn explicit_root_id_matches_owner_tab_predicate() {
    assert!(valid_tab_id("valid-root"));
    assert!(valid_tab_id(&"é".repeat(64)));
    for id in ["", " root", "root ", "a\nb", &"é".repeat(65)] {
        assert!(!valid_tab_id(id), "admitted invalid root ID");
    }
}

#[tokio::test]
async fn unreadable_parked_tab_keeps_good_route_and_disables_saves() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("unchanged".into()),
            sessions: ["good", "broken", "last"]
                .into_iter()
                .map(|id| SessionId::new(id).unwrap())
                .collect(),
            active: Some(SessionId::new("good").unwrap()),
        }))
        .unwrap();
        for (id, fails) in [("good", false), ("broken", true), ("last", false)] {
            let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
                panic!("history")
            };
            assert_eq!(session.0, id);
            ack.send(Ok(Default::default())).unwrap();
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("catalog")
            };
            if fails {
                ack.send(Err(CoreError::StoredTabDeck)).unwrap();
            } else {
                ack.send(Ok(catalog())).unwrap();
                empty_compactions(&mut inbox).await;
            }
        }
        let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
            panic!("bare route queries Home selection")
        };
        assert_eq!(action, SelectionAction::Current);
        ack.send(Ok(catalog())).unwrap();
        assert!(inbox.try_recv().is_err(), "filtered route was written");
    });
    let (mut state, mut deck) = restore_initial(&app, None).await.unwrap();
    deck.sync_tabs(&mut state);
    assert!(
        state.attached_session().is_none(),
        "bare restart opens Home"
    );
    assert_eq!(deck.tabs.len(), 2);
    assert_eq!(
        state.note(),
        Some("saved tabs partially unavailable; review saved tabs")
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "good");
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "last");
    assert_eq!(deck.revision.as_deref(), Some("unchanged"));
    assert_eq!(
        state.note(),
        Some("tab deck could not be saved; review saved tabs")
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn existing_legacy_explicit_id_remains_readable_but_never_saved() {
    let id = SessionId::new(" legacy-root").unwrap();
    let (app, mut inbox, _) = CoreApp::channel(8);
    let expected = id.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            ..Default::default()
        }))
        .unwrap();
        let Some(InboxMsg::ProbeSession { id, ack }) = inbox.recv().await else {
            panic!("lookup legacy row")
        };
        assert_eq!(id, expected);
        ack.send(Ok(SessionProbe::Root)).unwrap();
        let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
            panic!("legacy history")
        };
        assert_eq!(session, expected);
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("legacy selection")
        };
        ack.send(Ok(catalog())).unwrap();
        empty_compactions(&mut inbox).await;
        assert!(inbox.try_recv().is_err(), "legacy ID was created or saved");
    });
    let (state, deck) = restore_initial(&app, Some(id)).await.unwrap();
    assert!(deck.save_disabled);
    assert_eq!(
        state.note(),
        Some("legacy session id cannot be saved; review saved tabs")
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn failed_active_tab_falls_back_to_home_with_surviving_parked_tab() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("keep".into()),
            sessions: ["broken", "good"]
                .into_iter()
                .map(|id| SessionId::new(id).unwrap())
                .collect(),
            active: Some(SessionId::new("broken").unwrap()),
        }))
        .unwrap();
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("broken history")
        };
        ack.send(Err(CoreError::StoredTabDeck)).unwrap();
        let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
            panic!("good history")
        };
        assert_eq!(session.0, "good");
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("good selection")
        };
        ack.send(Ok(catalog())).unwrap();
        empty_compactions(&mut inbox).await;
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("fallback Home")
        };
        ack.send(Ok(catalog())).unwrap();
        assert!(inbox.try_recv().is_err());
    });
    let (mut state, mut deck) = restore_initial(&app, None).await.unwrap();
    deck.sync_tabs(&mut state);
    assert!(state.attached_session().is_none());
    assert_eq!(
        deck.snapshot(&state).sessions,
        vec![SessionId::new("good").unwrap()]
    );
    assert!(deck.save_disabled);
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "good");
    worker.await.unwrap();
}

#[tokio::test]
async fn explicit_failed_view_is_a_startup_error() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            sessions: vec![SessionId::new("broken").unwrap()],
            active: None,
            ..Default::default()
        }))
        .unwrap();
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("explicit history")
        };
        ack.send(Err(CoreError::StoredTabDeck)).unwrap();
        assert!(inbox.try_recv().is_err());
    });
    assert!(matches!(
        restore_initial(&app, Some(SessionId::new("broken").unwrap())).await,
        Err(StartupFailure::Query)
    ));
    worker.await.unwrap();
}

#[tokio::test]
async fn vis34_manual_admission_event_race_parked_view_and_read_failure_keep_receipt() {
    use oc_core::compaction::{
        CompactionAnchor, CompactionReason, CompactionSnapshot, CompactionState,
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId("compact-view".into());
    let mut state = TuiState::new(app.clone(), session.clone());
    state.restore_prompt("/compact".into());
    let mut deck = LoopState::default();
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(outcome.intent, Some(PanelIntent::CompactSession));
    apply_intent(&app, &mut state, &mut deck, outcome.intent.unwrap())
        .await
        .unwrap();
    assert_eq!(state.input(), "/compact");
    let Some(InboxMsg::CompactSession {
        session: requested,
        ack,
    }) = inbox.recv().await
    else {
        panic!("owner admission")
    };
    assert_eq!(requested, session);
    let queued = CompactionSnapshot {
        anchor: CompactionAnchor::default(),
        id: "op".into(),
        session: session.0.clone(),
        reason: CompactionReason::Manual,
        state: CompactionState::Queued,
        summary: String::new(),
        usage: None,
        provider_native: false,
        error: None,
    };
    let mut completed = queued.clone();
    completed.state = CompactionState::Completed;
    completed.summary = "Actual owner summary".into();
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &session,
        CoreEvent::Compaction(completed.clone()),
    )
    .await
    .unwrap();
    ack.send(Ok(queued)).unwrap();
    while !deck.compaction_job.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    finish_compaction_admission(&mut state, &mut deck).await;
    assert_eq!(state.input(), "");
    assert!(
        !state.is_busy(),
        "late queued receipt cannot restart completed work"
    );
    assert!(deck.compaction_job.is_none());
    let before = state.transcript_rows();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::CompactionHistory { ack, .. }) = inbox.recv().await else {
            panic!("read only")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "refresh never repeats compaction/provider work"
        );
    });
    refresh_compactions(&app, &mut state).await;
    worker.await.unwrap();
    assert_eq!(
        state.transcript_rows(),
        before,
        "accepted receipt survives read failure"
    );
    let mut active = TuiState::new(app.clone(), SessionId("other".into()));
    deck.tabs = vec![Some(state), None];
    deck.active_tab = Some(1);
    deck.tab_cards_before = vec![None, None];
    completed.summary = "Parked owner summary".into();
    handle_worker_event(
        &app,
        &mut active,
        &mut deck,
        &session,
        CoreEvent::Compaction(completed),
    )
    .await
    .unwrap();
    assert!(active.transcript_rows().is_empty());
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().transcript_rows()[0].text,
        "Parked owner summary"
    );
}

#[tokio::test]
async fn vis34_palette_enter_coalesced_admission_closes_only_on_success_and_preserves_composer() {
    use oc_core::compaction::{
        CompactionAnchor, CompactionReason, CompactionSnapshot, CompactionState,
    };
    for (accepted, edit_pending) in [(true, false), (true, true), (false, false)] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let session = SessionId("palette-compaction".into());
        let mut state = TuiState::new(app.clone(), session.clone());
        // A palette action must preserve even a literal slash-command draft.
        state.restore_prompt("/compact".into());
        let queued = CompactionSnapshot {
            anchor: CompactionAnchor::default(),
            id: "already-queued".into(),
            session: session.0.clone(),
            reason: CompactionReason::Manual,
            state: CompactionState::Queued,
            summary: String::new(),
            usage: None,
            provider_native: false,
            error: None,
        };
        state.apply_compaction(queued.clone());
        let mut deck = LoopState {
            tabs: vec![None],
            tab_cards_before: vec![None],
            active_tab: Some(0),
            ..Default::default()
        };
        // Ctrl+P belongs to the open slash autocomplete (previous item).
        // Dismiss that surface before opening the real Commands palette,
        // just as the focused keyboard route requires in production.
        state.handle_key(KeyAction::Commands).await;
        assert_eq!(state.panel(), &TuiPanel::None);
        state.handle_key(KeyAction::Cancel).await;
        state.handle_key(KeyAction::Commands).await;
        assert_eq!(state.panel(), &TuiPanel::Commands);
        state.handle_paste("Compact session");
        assert_eq!(state.modal_options().len(), 1);
        assert_eq!(state.modal_options()[0].value, "session.compact");
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        )
        .await
        .unwrap();
        assert_eq!(
            state.panel(),
            &TuiPanel::Commands,
            "wait for actual owner receipt"
        );
        assert_eq!(
            state.modal_options().len(),
            1,
            "admission must not reset palette search"
        );
        assert_eq!(state.input(), "/compact");
        if edit_pending {
            state.restore_prompt("edited while admission pending 界".into());
        }
        let Some(InboxMsg::CompactSession {
            session: requested,
            ack,
        }) = inbox.recv().await
        else {
            panic!("real compact admission job")
        };
        assert_eq!(requested, session);
        let expected_error = format!("compaction: {}", CoreError::Shutdown);
        // Same operation ID is the owner's coalesced admission receipt.
        ack.send(if accepted {
            Ok(queued)
        } else {
            Err(CoreError::Shutdown)
        })
        .unwrap();
        while !deck.compaction_job.as_ref().unwrap().is_finished() {
            tokio::task::yield_now().await;
        }
        finish_compaction_admission(&mut state, &mut deck).await;
        assert_eq!(
            state.input(),
            if edit_pending {
                "edited while admission pending 界"
            } else {
                "/compact"
            }
        );
        assert_eq!(
            state.panel(),
            if accepted {
                &TuiPanel::None
            } else {
                &TuiPanel::Commands
            }
        );
        if accepted {
            assert!(state.note().is_none(), "no synthetic success toast");
            assert_eq!(
                state
                    .transcript_rows()
                    .iter()
                    .filter(|r| r.role == "compaction_queued")
                    .count(),
                1,
                "coalesced receipt keeps one pending checkpoint"
            );
        } else {
            assert_eq!(state.note(), Some(expected_error.as_str()));
            assert_eq!(
                state.modal_options().len(),
                1,
                "owner refusal preserves searchable selection"
            );
            assert_eq!(state.modal_options()[0].value, "session.compact");
        }
        assert!(deck.compaction_job.is_none());
        assert!(
            inbox.try_recv().is_err(),
            "no extra provider turn, refresh or compaction admission"
        );
    }
}

#[tokio::test]
async fn quit_pending_fork_saves_accepted_identity_even_when_refresh_fails() {
    use oc_core::queries::{ForkSessionSnapshot, HistoryPage};
    for history_failure in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let source = SessionId::new("source").unwrap();
        let fork = SessionId::new("created-once").unwrap();
        let mut state = TuiState::new(app.clone(), source.clone());
        let mut deck = LoopState {
            tabs: vec![None],
            tab_cards_before: vec![None],
            active_tab: Some(0),
            location: Some("fixture-location".into()),
            ..Default::default()
        };
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::ForkMessage {
                message: oc_core::session::MessageId("selected".into()),
            },
        )
        .await
        .unwrap();
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        )
        .await
        .unwrap();
        assert_eq!(*state.status(), TuiStatus::Quit);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::ForkSession { ack, .. }) = inbox.recv().await else {
                panic!("one creation")
            };
            ack.send(Ok(ForkSessionSnapshot {
                session: fork,
                prompt: "unsent selected prompt".into(),
            }))
            .unwrap();
            let mut saved = None;
            let mut history_seen = false;
            let mut selection_seen = false;
            let mut compactions_seen = false;
            while saved.is_none() || !history_seen || !selection_seen || !compactions_seen {
                match inbox.recv().await.unwrap() {
                    InboxMsg::CompactionHistory { ack, .. } => {
                        compactions_seen = true;
                        ack.send(Ok(Vec::new())).unwrap();
                    }
                    InboxMsg::SaveTabDeck { mut deck, ack } => {
                        deck.revision = Some("saved-revision".into());
                        saved = Some(deck.clone());
                        ack.send(Ok(deck)).unwrap();
                    }
                    InboxMsg::History { ack, .. } => {
                        history_seen = true;
                        ack.send(if history_failure {
                            Err(CoreError::Shutdown)
                        } else {
                            Ok(HistoryPage::default())
                        })
                        .unwrap();
                    }
                    InboxMsg::SessionSelection { ack, .. } => {
                        selection_seen = true;
                        ack.send(if history_failure {
                            Ok(catalog())
                        } else {
                            Err(CoreError::Shutdown)
                        })
                        .unwrap();
                    }
                    _ => panic!("no second mutation or submission"),
                }
            }
            (saved.unwrap(), inbox)
        });
        settle_conversation(&app, &mut state, &mut deck).await;
        let (saved, mut inbox) = worker.await.unwrap();
        assert_eq!(
            saved.sessions,
            vec![source, SessionId::new("created-once").unwrap()]
        );
        assert_eq!(saved.active.as_ref(), state.attached_session());
        assert_eq!(state.input(), "unsent selected prompt");
        assert_eq!(
            deck.snapshot(&state),
            saved,
            "persisted deck is restart input"
        );
        assert!(inbox.is_empty());
        let expected_sessions = saved.sessions.clone();
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
                panic!("restart deck")
            };
            ack.send(Ok(saved)).unwrap();
            for expected in expected_sessions {
                let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
                    panic!("restart history")
                };
                assert_eq!(session, expected);
                ack.send(Ok(HistoryPage::default())).unwrap();
                let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                    panic!("restart catalog")
                };
                ack.send(Ok(catalog())).unwrap();
                empty_compactions(&mut inbox).await;
            }
            let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
                panic!("restart Home")
            };
            ack.send(Ok(catalog())).unwrap();
            assert!(inbox.try_recv().is_err(), "restart never recreates fork");
        });
        let (_, restarted) = restore_initial(&app, None).await.unwrap();
        assert_eq!(restarted.tabs.len(), 2);
        assert_eq!(
            restarted.tabs[1].as_ref().unwrap().attached_session(),
            Some(&SessionId::new("created-once").unwrap())
        );
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn five_turn_reverted_tail_all_redo_entry_points_use_real_owner_without_generation() {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use oc_core::queries::ConversationAction;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let listener = std::sync::Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    std::fs::write(project.join("opencode.json"), serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"fixture"},"models":{"m":{}}}}}).to_string()).unwrap();
    let session = SessionId::new("five-turn-redo").unwrap();
    let server_listener = listener.clone();
    let server = tokio::spawn(async move {
        for index in 0..5 {
            let (mut socket, _) = server_listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let length: usize = String::from_utf8_lossy(&bytes[..end])
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let item = serde_json::json!({"type":"message","role":"assistant","id":format!("answer-{index}"),"status":"completed","content":[{"type":"output_text","text":format!("answer {index}")}]});
            let sse = format!(
                "data: {}\n\ndata: {}\n\n",
                serde_json::json!({"type":"response.output_item.done","item":item}),
                serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item]}})
            );
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await.unwrap();
        }
    });
    let env = std::collections::BTreeMap::from([
        ("HOME".into(), root.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env)
        .await
        .unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Already titled".into())
        .await
        .unwrap();
    let mut events = app.subscribe();
    for index in 0..5 {
        app.submit(session.clone(), format!("prompt {index}"))
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    }
    server.await.unwrap();
    let first = app
        .history_page(session.clone(), None, None, 100)
        .await
        .unwrap()
        .rows[0]
        .id
        .clone();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut deck = LoopState::default();
    for entry in 0..4 {
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::ChangeConversation {
                action: ConversationAction::Revert {
                    message: first.clone(),
                },
            },
        )
        .await
        .unwrap();
        settle_conversation(&app, &mut state, &mut deck).await;
        assert_eq!(state.input(), "prompt 0");
        let page = app
            .history_page(session.clone(), None, None, 1)
            .await
            .unwrap();
        assert_eq!(page.reverted.as_ref().unwrap().user_messages, 5);
        assert!(
            state
                .transcript_lines(80, 80)
                .iter()
                .any(|line| line.plain_text().contains("5 messages reverted"))
        );
        let intent = match entry {
            0 => {
                let rows = oc_tui::views::render_test(&state, 120, 40);
                let y = rows
                    .iter()
                    .position(|row| row.contains("5 messages reverted"))
                    .unwrap() as u16;
                let x = rows[y as usize].find("5 messages").unwrap() as u16;
                let area = ratatui::layout::Rect::new(0, 0, 120, 40);
                state.handle_mouse(
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    },
                    area,
                );
                state
                    .handle_mouse(
                        MouseEvent {
                            kind: MouseEventKind::Up(MouseButton::Left),
                            column: x,
                            row: y,
                            modifiers: KeyModifiers::NONE,
                        },
                        area,
                    )
                    .intent
                    .unwrap()
            }
            1 => {
                let leader = state
                    .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
                    .unwrap();
                state.handle_key(leader).await;
                let redo = state
                    .terminal_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
                    .unwrap();
                state.handle_key(redo).await.intent.unwrap()
            }
            2 => {
                state.restore_prompt("/redo".into());
                state.handle_key(KeyAction::Enter).await.intent.unwrap()
            }
            _ => {
                state.handle_key(KeyAction::Commands).await;
                state.handle_paste("Redo");
                state.handle_key(KeyAction::Enter).await.intent.unwrap()
            }
        };
        assert_eq!(
            intent,
            PanelIntent::ChangeConversation {
                action: ConversationAction::Redo
            }
        );
        apply_intent(&app, &mut state, &mut deck, intent)
            .await
            .unwrap();
        settle_conversation(&app, &mut state, &mut deck).await;
        let page = app
            .history_page(session.clone(), None, None, 100)
            .await
            .unwrap();
        assert!(page.reverted.is_none());
        assert_eq!(
            page.rows
                .iter()
                .filter(|message| message.role == oc_core::session::Role::User)
                .count(),
            5
        );
        assert!(
            !state
                .transcript_lines(80, 80)
                .iter()
                .any(|line| line.plain_text().contains("messages reverted"))
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err(),
            "redo never calls provider"
        );
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn conversation_commands_wait_for_owner_then_refresh_without_submission() {
    use oc_core::queries::{ConversationAction, ConversationSnapshot, HistoryPage};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId::new("conversation-ui").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut deck = LoopState::default();
    state.handle_paste("/undo");
    let intent = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    assert_eq!(
        intent,
        PanelIntent::ChangeConversation {
            action: ConversationAction::Undo
        }
    );
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert_eq!(state.input(), "/undo", "draft is retained until owner ACK");
    let Some(InboxMsg::ChangeConversation { action, ack, .. }) = inbox.recv().await else {
        panic!("typed undo")
    };
    assert_eq!(action, ConversationAction::Undo);
    ack.send(Ok(ConversationSnapshot {
        session: session.clone(),
        draft: Some("original prompt".into()),
        can_undo: false,
        can_redo: true,
        reverted: Some(oc_core::queries::RevertedConversation {
            message: oc_core::session::MessageId("hidden-user".into()),
            user_messages: 3,
        }),
    }))
    .unwrap();
    let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
        panic!("history refresh")
    };
    ack.send(Ok(HistoryPage {
        title: Some("Retained title".into()),
        reverted: Some(oc_core::queries::RevertedConversation {
            message: oc_core::session::MessageId("hidden-user".into()),
            user_messages: 3,
        }),
        ..Default::default()
    }))
    .unwrap();
    let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
        panic!("metadata refresh")
    };
    ack.send(Ok(catalog())).unwrap();
    empty_compactions(&mut inbox).await;
    while !deck.conversation_job.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    finish_conversation(&app, &mut state, &mut deck).await;
    assert_eq!(state.input(), "original prompt");
    assert!(
        state
            .transcript_lines(80, 80)
            .iter()
            .any(|line| line.plain_text().contains("3 messages reverted"))
    );
    assert_eq!(state.session_title.as_deref(), Some("Retained title"));
    assert_eq!(
        state.command_unavailable(&CommandAction::UndoConversation),
        Some("nothing to undo")
    );
    assert_eq!(
        state.command_unavailable(&CommandAction::RedoConversation),
        None
    );
    state.restore_prompt("/redo".into());
    let intent = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    assert_eq!(
        intent,
        PanelIntent::ChangeConversation {
            action: ConversationAction::Redo
        }
    );
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    let Some(InboxMsg::ChangeConversation { action, ack, .. }) = inbox.recv().await else {
        panic!("typed redo, never Submit")
    };
    assert_eq!(action, ConversationAction::Redo);
    ack.send(Err(CoreError::Shutdown)).unwrap();
    while !deck.conversation_job.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    finish_conversation(&app, &mut state, &mut deck).await;
    assert_eq!(
        state.input(),
        "/redo",
        "async failure retains editable draft"
    );
    assert_eq!(state.session_title.as_deref(), Some("Retained title"));
    assert!(
        inbox.try_recv().is_err(),
        "no provider/submission or extra queries"
    );
    assert!(
        state
            .transcript_lines(80, 80)
            .iter()
            .any(|line| line.plain_text().contains("3 messages reverted")),
        "owner failure retains committed card"
    );
}

#[tokio::test]
async fn message_copy_keyboard_drains_transport_and_preserves_dialog_on_failure() {
    use crossterm::event::{MouseButton, MouseEvent};
    use oc_core::queries::{HistoryMessage, HistoryPage};
    use oc_core::session::{MessageId, Role};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    for succeeds in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app.clone(), SessionId::new("copy-dialog").unwrap());
        let row = HistoryMessage {
            id: MessageId("copy-id".into()),
            seq: 4,
            role: Role::User,
            text: "COPY STORED PROMPT".into(),
            turn: None,
            model_switch: None,
        };
        state.attach_page(&HistoryPage {
            rows: vec![row.clone()],
            ..Default::default()
        });
        state.restore_prompt("unfinished draft".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| render_frame(frame, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let y = (0..30)
            .find(|&y| {
                (0..100)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains("COPY STORED PROMPT")
            })
            .unwrap();
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(
                MouseEvent {
                    kind,
                    column: 6,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                },
                Rect::new(0, 0, 100, 30),
            );
        }
        assert!(matches!(state.panel(), TuiPanel::MessageActions { .. }));
        fn success(text: &str) -> Result<(), String> {
            assert_eq!(text, "COPY STORED PROMPT");
            Ok(())
        }
        fn failure(text: &str) -> Result<(), String> {
            assert_eq!(text, "COPY STORED PROMPT");
            Err("transport failed".into())
        }
        let mut deck = LoopState {
            copy_transport: Some(if succeeds { success } else { failure }),
            ..Default::default()
        };
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                panic!("exact text query")
            };
            ack.send(Ok(HistoryPage {
                rows: vec![row],
                ..Default::default()
            }))
            .unwrap();
        });
        for code in [KeyCode::Down, KeyCode::Down, KeyCode::Enter] {
            handle_event(
                &app,
                &mut state,
                &mut deck,
                CEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            )
            .await
            .unwrap();
        }
        worker.await.unwrap();
        assert!(state.take_copy_request().is_none());
        assert_eq!(state.input(), "unfinished draft");
        assert_eq!(
            matches!(state.panel(), TuiPanel::MessageActions { .. }),
            !succeeds
        );
        assert_eq!(
            state.note_variant(),
            Some(if succeeds {
                NoteVariant::Info
            } else {
                NoteVariant::Error
            })
        );
    }
}

#[tokio::test]
async fn fork_owner_result_adopts_real_root_with_unsent_prompt_and_preserved_source() {
    use oc_core::queries::{ForkSessionSnapshot, HistoryPage};
    use oc_core::session::MessageId;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let source = SessionId::new("fork-source").unwrap();
    let fork = SessionId::new("fork-owner-root").unwrap();
    let mut state = TuiState::new(app.clone(), source.clone());
    state.restore_prompt("unfinished source draft".into());
    let mut deck = LoopState {
        tabs: vec![None],
        tab_cards_before: vec![Some(7)],
        active_tab: Some(0),
        cards_before: Some(7),
        save_disabled: true,
        ..Default::default()
    };
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ForkMessage {
            message: MessageId("selected-user".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.attached_session(), Some(&source));
    let Some(InboxMsg::ForkSession {
        source: received,
        before: message,
        ack,
    }) = inbox.recv().await
    else {
        panic!("real fork")
    };
    assert_eq!(received, source);
    assert_eq!(message.0, "selected-user");
    ack.send(Ok(ForkSessionSnapshot {
        session: fork.clone(),
        prompt: "selected public prompt".into(),
    }))
    .unwrap();
    let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
        panic!("fork history")
    };
    assert_eq!(session, fork);
    ack.send(Ok(HistoryPage {
        title: Some("Fork title".into()),
        ..Default::default()
    }))
    .unwrap();
    let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
        panic!("fork selection")
    };
    ack.send(Ok(catalog())).unwrap();
    empty_compactions(&mut inbox).await;
    while !deck.conversation_job.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    finish_conversation(&app, &mut state, &mut deck).await;
    assert_eq!(state.attached_session(), Some(&fork));
    assert_eq!(state.input(), "selected public prompt");
    assert_eq!(state.session_title.as_deref(), Some("Fork title"));
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().input(),
        "unfinished source draft"
    );
    assert_eq!(deck.tab_cards_before[0], Some(7));
    assert_eq!(deck.snapshot(&state).sessions, vec![source, fork]);
    assert!(
        inbox.try_recv().is_err(),
        "unsent prompt and disabled deck safeguard"
    );
}

#[tokio::test]
async fn vis39_retained_view_activation_moves_mounted_tab_clocks_and_keeps_busy_guard() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut running = TuiState::new(app.clone(), SessionId::new("running").unwrap());
    running.apply_catalog(catalog());
    running.chrome.location = Some("/owner/actual-project".into());
    running.handle_paste("held");
    running.handle_key(KeyAction::Enter).await;
    assert!(running.is_busy());
    let request = inbox.try_recv().expect("one pending typed submission");
    assert!(matches!(&request, InboxMsg::Submit { .. }));
    let mut state = TuiState::new(app.clone(), SessionId::new("current").unwrap());
    state.apply_catalog(catalog());
    state.chrome.location = Some("/owner/actual-project".into());
    let mut deck = LoopState {
        tabs: vec![Some(running), None],
        tab_cards_before: vec![None, None],
        active_tab: Some(1),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    assert_eq!(
        state.tab_presentation().0[0].detail.as_deref(),
        Some("actual-project")
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    let at = state.next_ui_deadline().unwrap() + Duration::from_millis(500);
    state.tick_ui(at);
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    let before = terminal.backend().buffer()[(1, 0)].clone();
    assert_eq!(before.symbol(), "⠦");
    deck.activate(&mut state, 0).unwrap();
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    let after = &terminal.backend().buffer()[(1, 0)];
    assert_eq!(
        after.symbol(),
        before.symbol(),
        "same visible session keeps its mounted dots phase"
    );
    assert_eq!(
        after.fg, before.fg,
        "same visible session keeps its sweep whitecap phase"
    );
    assert_eq!(
        deck.tabs[1].as_ref().unwrap().next_ui_deadline(),
        None,
        "parked view holds no second deck clock"
    );
    assert_eq!(
        deck.activate(&mut state, 1),
        Err("turn active; session switch refused".into())
    );
    assert!(inbox.try_recv().is_err(), "clock transfer cannot resubmit");
}

#[tokio::test]
async fn vis41_real_mouse_activation_preserves_running_and_completed_marquee_deadlines() {
    const TITLE: &str = "abcdefghijklmnopqrstuvwxyz123456789ABCDEFGHIJKLMNO";
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.chrome.animations = Some(false);
    state.chrome.location = Some("/owner/real-project".into());
    let mut other = TuiState::new(app.clone(), SessionId::new("second").unwrap());
    other.chrome = state.chrome.clone();
    other.session_title = Some(TITLE.into());
    let mut deck = LoopState {
        tabs: vec![None, Some(other)],
        tab_cards_before: vec![None, None],
        active_tab: Some(0),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let x = oc_tui::shell::tab_strip(&state, area).unwrap().tabs[1]
        .rect
        .x
        + 3;
    let moved = crossterm::event::MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(moved, area);
    let first = state.next_ui_deadline().unwrap();
    state.tick_ui(first + Duration::from_millis(80));
    let deadline = state.next_ui_deadline();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(x, 0)].symbol(), "c");
    for kind in [
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        MouseEventKind::Up(crossterm::event::MouseButton::Left),
    ] {
        let outcome = state.handle_mouse(crossterm::event::MouseEvent { kind, ..moved }, area);
        apply_mouse_outcome(&app, &mut state, &mut deck, outcome, area, x, 0).await;
    }
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(state.next_ui_deadline(), deadline);
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    assert_eq!(terminal.backend().buffer()[(x, 0)].symbol(), "c");
    assert_eq!(
        state.tab_presentation().0[1].detail.as_deref(),
        Some("real-project")
    );

    state.tick_ui(first + Duration::from_secs(6));
    assert_eq!(state.next_ui_deadline(), None);
    for kind in [
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        MouseEventKind::Up(crossterm::event::MouseButton::Left),
    ] {
        let outcome = state.handle_mouse(crossterm::event::MouseEvent { kind, ..moved }, area);
        apply_mouse_outcome(&app, &mut state, &mut deck, outcome, area, x, 0).await;
    }
    assert_eq!(
        state.next_ui_deadline(),
        None,
        "clicking the same completed cycle cannot restart it"
    );
    handle_event(&app, &mut state, &mut deck, CEvent::Resize(120, 48))
        .await
        .unwrap();
    assert_eq!(state.next_ui_deadline(), None);
    assert!(
        inbox.try_recv().is_err(),
        "mouse/resize cannot enqueue provider work"
    );
}

#[tokio::test]
async fn closing_tab_with_pending_title_cancels_only_title_and_survivor_can_submit() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let title = SessionId::new("title-tab").unwrap();
    let survivor = SessionId::new("survivor").unwrap();
    let mut state = TuiState::new(app.clone(), title.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    state.handle_paste("/rename");
    // Dismiss inline argument completion to submit the bare owner action.
    state.handle_key(KeyAction::Cancel).await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(outcome.intent, Some(PanelIntent::RegenerateTitle));
    apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
    let Some(InboxMsg::RegenerateTitle {
        session,
        ack: title_ack,
    }) = tokio::time::timeout(Duration::from_secs(1), inbox.recv())
        .await
        .unwrap()
    else {
        panic!("title work must start before close")
    };
    assert_eq!(session, title);
    append_tab(&app, &mut deck, &mut state, "survivor");
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "/rename");

    let worker = tokio::spawn(async move {
        for (expected, active, revision) in [
            (vec!["title-tab", "survivor"], Some("title-tab"), "old"),
            (vec!["survivor"], Some("survivor"), "preflight"),
        ] {
            let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
                panic!("close must save before cancelling title")
            };
            assert_eq!(
                deck.sessions
                    .iter()
                    .map(|id| id.0.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), active);
            assert_eq!(deck.revision.as_deref(), Some(revision));
            ack.send(Ok(TabDeckSnapshot {
                revision: Some(
                    if revision == "old" {
                        "preflight"
                    } else {
                        "closed"
                    }
                    .into(),
                ),
                ..deck
            }))
            .unwrap();
        }
        let Some(InboxMsg::CancelTitle { session, ack }) = inbox.recv().await else {
            panic!("successful close must cancel title, not a turn")
        };
        assert_eq!(session.0, "title-tab");
        title_ack
            .send(Err(CoreError::Application("cancelled title".into())))
            .unwrap();
        ack.send(Ok(())).unwrap();
        let Some(InboxMsg::Submit {
            session, text, ack, ..
        }) = inbox.recv().await
        else {
            panic!("surviving tab must be able to submit after title cancellation")
        };
        assert_eq!(session.0, "survivor");
        assert_eq!(text, "survivor prompt");
        ack.send(Ok(WorkerTurnId("survivor-turn".into()))).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "no conversational turn cancellation"
        );
    });
    tokio::time::timeout(
        Duration::from_secs(1),
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        ),
    )
    .await
    .expect("close stalled on title provider")
    .unwrap();
    assert_eq!(state.attached_session(), Some(&survivor));
    assert_eq!(deck.snapshot(&state).sessions, vec![survivor.clone()]);
    let (_, job) = deck
        .title_job
        .take()
        .expect("title job tracked until drained");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), job)
            .await
            .expect("title cancellation did not finish")
            .unwrap()
            .is_err()
    );
    state.handle_paste("survivor prompt");
    state.handle_key(KeyAction::Enter).await;
    tokio::time::timeout(Duration::from_secs(1), worker)
        .await
        .expect("survivor submission blocked")
        .unwrap();
    state.poll_submission();
    assert_eq!(state.attached_session(), Some(&survivor));
}

#[tokio::test]
async fn refused_close_leaves_pending_title_uncancelled_and_tab_intact() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let title = SessionId::new("title-tab").unwrap();
    let mut state = TuiState::new(app.clone(), title.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    state.handle_paste("/rename");
    state.handle_key(KeyAction::Cancel).await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(outcome.intent, Some(PanelIntent::RegenerateTitle));
    apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
    let Some(InboxMsg::RegenerateTitle {
        session,
        ack: title_ack,
    }) = tokio::time::timeout(Duration::from_secs(1), inbox.recv())
        .await
        .unwrap()
    else {
        panic!("title job must reach owner")
    };
    assert_eq!(session, title);
    append_tab(&app, &mut deck, &mut state, "survivor");
    deck.activate(&mut state, 0).unwrap();
    let (release_title, resume_title) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("full deck preflight")
        };
        assert_eq!(deck.sessions.len(), 2);
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("preflight".into()),
            ..deck
        }))
        .unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("candidate save")
        };
        assert_eq!(deck.sessions, vec![SessionId::new("survivor").unwrap()]);
        ack.send(Err(CoreError::TabDeckConflict)).unwrap();
        resume_title.await.unwrap();
        assert!(inbox.try_recv().is_err(), "refusal cannot cancel title");
        title_ack.send(Ok("Still generating".into())).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "title completion adds no turn or cancellation"
        );
    });
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(1),
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::CloseTab { index: 0 }
            )
        )
        .await
        .expect("refused close stalled on title provider"),
        Err("tab close refused; saved tabs unavailable".into())
    );
    assert_eq!(state.attached_session(), Some(&title));
    assert_eq!(deck.snapshot(&state).sessions.len(), 2);
    assert!(
        deck.title_job
            .as_ref()
            .is_some_and(|(_, job)| !job.is_finished()),
        "refused close must leave title work running"
    );
    release_title.send(()).unwrap();
    worker.await.unwrap();
    let (_, job) = deck.title_job.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(1), job)
        .await
        .expect("title stopped after refused close")
        .unwrap()
        .unwrap();
    state.regenerated_title(Ok(result));
    assert_eq!(state.session_title.as_deref(), Some("Still generating"));
    assert_eq!(state.input(), "");
}

#[tokio::test]
async fn restore_home_with_parked_views_keeps_order_and_failed_save_keeps_route() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read deck first")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("rev-1".into()),
            sessions: vec![
                SessionId::new("one").unwrap(),
                SessionId::new("two").unwrap(),
            ],
            active: None,
        }))
        .unwrap();
        for id in ["one", "two"] {
            let Some(InboxMsg::History {
                session,
                limit,
                ack,
                ..
            }) = inbox.recv().await
            else {
                panic!("read bounded history")
            };
            assert_eq!(session.0, id);
            assert_eq!(limit, HISTORY_PAGE_LIMIT);
            ack.send(Ok(Default::default())).unwrap();
            let Some(InboxMsg::SessionSelection {
                session,
                action,
                ack,
                ..
            }) = inbox.recv().await
            else {
                panic!("read selection")
            };
            assert_eq!(session.0, id);
            assert_eq!(action, SelectionAction::Current);
            ack.send(Ok(catalog())).unwrap();
            empty_compactions(&mut inbox).await;
        }
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("Home selection")
        };
        ack.send(Ok(catalog())).unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("activate saves route")
        };
        assert_eq!(
            deck.sessions
                .iter()
                .map(|id| id.0.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), Some("one"));
        assert_eq!(deck.location, "/fixture");
        assert_eq!(deck.revision.as_deref(), Some("rev-1"));
        ack.send(Err(CoreError::TabDeckConflict)).unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("another action retains the expected token")
        };
        assert_eq!(deck.location, "/fixture");
        assert_eq!(deck.revision.as_deref(), Some("rev-1"));
        assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), Some("two"));
        ack.send(Err(CoreError::TabDeckConflict)).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "restore/switch never creates or submits"
        );
    });
    let (mut state, mut deck) = restore_initial(&app, None).await.unwrap();
    deck.sync_tabs(&mut state);
    assert!(state.attached_session().is_none());
    assert_eq!(deck.tabs.len(), 2);
    assert_eq!(state.tab_presentation().1, 2);
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "one");
    assert_eq!(deck.tabs.len(), 2);
    assert_eq!(deck.revision.as_deref(), Some("rev-1"));
    assert_eq!(
        state.note(),
        Some("tab deck could not be saved; saved tabs changed or storage unavailable")
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "two");
    assert_eq!(deck.revision.as_deref(), Some("rev-1"));
    worker.await.unwrap();
}

#[tokio::test]
async fn pruned_home_deck_keeps_all_owner_projected_tabs() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("rev-home".into()),
            sessions: (0..MAX_TABS - 1)
                .map(|i| SessionId::new(format!("tab-{i}")).unwrap())
                .collect(),
            active: None,
        }))
        .unwrap();
        for i in 0..MAX_TABS - 1 {
            let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
                panic!("history")
            };
            assert_eq!(session.0, format!("tab-{i}"));
            ack.send(Ok(Default::default())).unwrap();
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("selection")
            };
            ack.send(Ok(catalog())).unwrap();
            empty_compactions(&mut inbox).await;
        }
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("Home selection")
        };
        ack.send(Ok(catalog())).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "no Home root or automatic repair write"
        );
    });
    let (mut state, mut deck) = restore_initial(&app, None).await.unwrap();
    deck.sync_tabs(&mut state);
    assert_eq!(state.tab_presentation().0.len(), MAX_TABS - 1);
    assert_eq!(state.tab_presentation().1, MAX_TABS - 1);
    assert_eq!(deck.snapshot(&state).sessions.len(), MAX_TABS - 1);
    assert!(state.attached_session().is_none());
    assert_eq!(deck.revision.as_deref(), Some("rev-home"));
    assert!(!deck.can_open_session(), "Home consumes the remaining slot");
    worker.await.unwrap();
}

#[tokio::test]
async fn bare_restart_with_full_real_deck_keeps_all_ids_and_selected_route() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read full deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("full".into()),
            sessions: (0..MAX_TABS)
                .map(|i| SessionId::new(format!("tab-{i}")).unwrap())
                .collect(),
            active: Some(SessionId::new("tab-12").unwrap()),
        }))
        .unwrap();
        for i in 0..MAX_TABS {
            let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
                panic!("read full history")
            };
            assert_eq!(session.0, format!("tab-{i}"));
            ack.send(Ok(Default::default())).unwrap();
            let Some(InboxMsg::SessionSelection { session, ack, .. }) = inbox.recv().await else {
                panic!("read full selection")
            };
            assert_eq!(session.0, format!("tab-{i}"));
            ack.send(Ok(catalog())).unwrap();
            empty_compactions(&mut inbox).await;
        }
        assert!(
            inbox.try_recv().is_err(),
            "no Home or preference write at capacity"
        );
    });
    let (mut state, mut deck) = restore_initial(&app, None).await.unwrap();
    deck.sync_tabs(&mut state);
    assert_eq!(state.session().0, "tab-12");
    assert_eq!(deck.active_tab, Some(12));
    assert_eq!(state.tab_presentation().0.len(), MAX_TABS);
    assert_eq!(deck.snapshot(&state).sessions.len(), MAX_TABS);
    assert!(!deck.can_open_session());
    assert_eq!(
        state.note(),
        Some("tab limit reached; Home unavailable until a tab is closed")
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn explicit_restore_adopts_successful_revision_for_next_save() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read deck")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("loaded".into()),
            sessions: vec![SessionId::new("one").unwrap()],
            active: None,
        }))
        .unwrap();
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("read history")
        };
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("read catalog")
        };
        ack.send(Ok(catalog())).unwrap();
        empty_compactions(&mut inbox).await;
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("explicit session saves")
        };
        assert_eq!(deck.revision.as_deref(), Some("loaded"));
        assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), Some("one"));
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("after-explicit".into()),
            ..deck
        }))
        .unwrap();
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("open Home")
        };
        ack.send(Ok(catalog())).unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("save Home route")
        };
        assert_eq!(deck.location, "/fixture");
        assert_eq!(deck.revision.as_deref(), Some("after-explicit"));
        assert!(deck.active.is_none());
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("after-home".into()),
            ..deck
        }))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    let (mut state, mut deck) = restore_initial(&app, Some(SessionId::new("one").unwrap()))
        .await
        .unwrap();
    assert_eq!(deck.revision.as_deref(), Some("after-explicit"));
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    assert_eq!(deck.revision.as_deref(), Some("after-home"));
    worker.await.unwrap();
}

#[tokio::test]
async fn invalid_owner_home_layout_or_location_is_not_silently_saved() {
    for invalid in [
        TabDeckSnapshot {
            location: "/fixture".into(),
            sessions: (0..MAX_TABS)
                .map(|i| SessionId::new(format!("tab-{i}")).unwrap())
                .collect(),
            ..Default::default()
        },
        TabDeckSnapshot {
            sessions: vec![SessionId::new("one").unwrap()],
            ..Default::default()
        },
    ] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
                panic!("read deck")
            };
            ack.send(Ok(invalid)).unwrap();
            assert!(inbox.try_recv().is_err(), "invalid read never saves");
        });
        assert!(matches!(
            restore_initial(&app, None).await,
            Err(StartupFailure::Query)
        ));
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn mouse_close_home_holds_survivor_at_the_pointer_but_keyboard_switch_does_not() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};
    use ratatui::layout::Rect;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
    state.session_title = Some("Какие инструменты доступны ассистенту".into());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    let area = Rect::new(0, 0, 120, 40);
    let before = oc_tui::shell::tab_strip(&state, area).unwrap();
    let close = before.tabs[1].rect.right() - 2;
    let mouse = |kind| MouseEvent {
        kind,
        column: close,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Moved), area);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    let outcome = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area);
    assert_eq!(outcome.intent, Some(PanelIntent::CloseTab { index: 1 }));
    apply_mouse_outcome(&app, &mut state, &mut deck, outcome, area, close, 0).await;
    let after = oc_tui::shell::tab_strip(&state, area).unwrap();
    assert!(deck.home.is_none());
    assert_eq!(state.session().0, "kept");
    assert_eq!(after.tabs[0].rect.right() - 2, close);
    assert_eq!(
        state.tab_close_cell(area, 0, after.tabs[0].rect),
        Some(close)
    );
    assert_eq!(after.add.unwrap().x, close + 2);
    assert!(inbox.try_recv().is_err());

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    let cells = terminal.backend().buffer();
    let row: String = (0..120).map(|x| cells[(x, 0)].symbol()).collect();
    assert!(
        row.starts_with("   Какие инструменты доступны ассистенту"),
        "{row}"
    );
    assert_eq!(cells[(close, 0)].symbol(), "✕");
    assert_eq!(
        cells[(close, 0)].fg,
        ratatui::style::Color::Rgb(238, 238, 238)
    );
    assert_eq!(cells[(close + 3, 0)].symbol(), "+");

    state.clear_mouse_position(); // PTY resize invalidates the held frame.
    let resized = oc_tui::shell::tab_strip(&state, area).unwrap();
    assert_eq!(resized.tabs[0].rect.width, 32);
    assert_eq!(state.tab_close_cell(area, 0, resized.tabs[0].rect), None);

    // Clicking a real tab replaces Home's view too: re-hit-test the
    // release, rather than relying on the parked view's stale hover.
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    let point = |kind| MouseEvent {
        kind,
        column: 3,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(point(MouseEventKind::Moved), area);
    state.handle_mouse(point(MouseEventKind::Down(MouseButton::Left)), area);
    let activate = state.handle_mouse(point(MouseEventKind::Up(MouseButton::Left)), area);
    assert_eq!(activate.intent, Some(PanelIntent::ActivateTab { index: 0 }));
    apply_mouse_outcome(&app, &mut state, &mut deck, activate, area, 3, 0).await;
    let clicked = oc_tui::shell::tab_strip(&state, area).unwrap().tabs[0].rect;
    assert_eq!(
        state.tab_close_cell(area, 0, clicked),
        Some(clicked.right() - 2)
    );

    // A keyboard switch has no pointer event and must not manufacture hover.
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    deck.activate(&mut state, 0).unwrap();
    let normal = oc_tui::shell::tab_strip(&state, area).unwrap();
    assert_eq!(normal.tabs[0].rect.width, 32);
    assert_eq!(state.tab_close_cell(area, 0, normal.tabs[0].rect), None);
}

#[tokio::test]
async fn keyboard_close_after_mouse_add_retests_real_pointer_without_mouse_hold() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    let area = Rect::new(0, 0, 120, 40);
    let add = oc_tui::shell::tab_strip(&state, area).unwrap().add.unwrap();
    let pointer = add.x + 1;
    let mouse = |kind| MouseEvent {
        kind,
        column: pointer,
        row: add.y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Moved), area);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    let outcome = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area);
    assert_eq!(outcome.intent, Some(PanelIntent::NewSession));
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("owner Home selection")
        };
        ack.send(Ok(catalog())).unwrap();
        inbox
    });
    apply_mouse_outcome(&app, &mut state, &mut deck, outcome, area, pointer, add.y).await;
    let mut inbox = worker.await.unwrap();
    assert!(state.home);
    assert_eq!(state.mouse_position(), Some((pointer, add.y, area)));
    assert_eq!(state.handle_key(KeyAction::Leader).await.intent, None);
    let close = state.handle_key(KeyAction::Char('w')).await;
    assert_eq!(close.intent, Some(PanelIntent::CloseTab { index: 1 }));
    apply_outcome(&app, &mut state, &mut deck, close, false).await;
    assert!(!state.home);
    assert_eq!(state.session().0, "kept");
    assert_eq!(state.mouse_position(), Some((pointer, add.y, area)));
    let strip = oc_tui::shell::tab_strip(&state, area).unwrap();
    assert_eq!(strip.add.unwrap(), add);
    assert_eq!(
        strip.tabs[0].rect.width, 32,
        "keyboard close cannot hold mouse geometry"
    );
    assert_eq!(state.tab_close_cell(area, 0, strip.tabs[0].rect), None);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| render_frame(frame, &state)).unwrap();
    for x in add.x..add.right() {
        let cell = &terminal.backend().buffer()[(x, add.y)];
        assert_eq!(cell.fg, Color::Rgb(238, 238, 238));
        assert_eq!(cell.bg, Color::Rgb(20, 20, 20));
    }
    assert!(
        inbox.try_recv().is_err(),
        "close Home needs no provider or owner query"
    );
}

#[tokio::test]
async fn vis11_binary_events_preserve_focused_lifecycle_then_submit_exact_draft() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("leader-binary").unwrap());
    let mut catalog = catalog();
    catalog.chrome.conversation_shortcuts.leader = "alt+x".into();
    catalog.chrome.leader_timeout_ms = Some(80);
    state.apply_catalog(catalog);
    state.handle_paste("draft\nwith\nchip\n");
    let draft = state.input().to_string();
    let caret = state.prompt_layout(80).1;
    let mut deck = LoopState::default();
    let event = |code, modifiers| CEvent::Key(KeyEvent::new(code, modifiers));
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    assert!(state.leader_pending());
    let deadline = state.next_ui_deadline().unwrap();
    assert!(state.tick_ui(deadline));
    assert!(!state.leader_pending());
    assert!(state.next_ui_deadline().is_none());
    assert_eq!(state.input(), draft);
    assert_eq!(state.prompt_layout(80).1, caret);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Backspace, KeyModifiers::NONE),
    )
    .await
    .unwrap();
    assert!(!state.leader_pending());
    assert_eq!(state.input(), draft);
    assert_eq!(state.prompt_layout(80).1, caret);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    let hidden = state.chrome.sidebar_hidden;
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('b'), KeyModifiers::NONE),
    )
    .await
    .unwrap();
    assert_ne!(state.chrome.sidebar_hidden, hidden);
    assert!(!state.leader_pending());
    assert_eq!(state.input(), draft);
    assert!(inbox.try_recv().is_err());
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('c'), KeyModifiers::CONTROL),
    )
    .await
    .unwrap();
    assert!(!state.leader_pending());
    assert_eq!(state.input(), draft);
    assert_eq!(state.prompt_layout(80).1, caret);
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(inbox.try_recv().is_err());
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('p'), KeyModifiers::CONTROL),
    )
    .await
    .unwrap();
    assert_eq!(state.panel(), &TuiPanel::Commands);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    assert!(state.leader_pending());
    let deadline = state.next_ui_deadline().unwrap();
    assert!(state.tick_ui(deadline));
    assert_eq!(state.panel(), &TuiPanel::Commands);
    assert_eq!(state.input(), draft);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Esc, KeyModifiers::NONE),
    )
    .await
    .unwrap();
    assert_eq!(state.panel(), &TuiPanel::None);
    state.handle_paste("Unicode αβ 🦊 unchanged full draft");
    let submitted = state.input().to_string();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Char('x'), KeyModifiers::ALT),
    )
    .await
    .unwrap();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event(KeyCode::Enter, KeyModifiers::NONE),
    )
    .await
    .unwrap();
    assert!(!state.leader_pending());
    let Some(InboxMsg::Submit { text, .. }) = inbox.try_recv().ok() else {
        panic!("pending Enter must route an actual owner Submit request")
    };
    assert_eq!(text, submitted);
}

#[tokio::test]
async fn vis11_binary_obsolete_leader_is_inert_but_explicit_direct_binding_wins() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use oc_core::queries::ConversationShortcuts;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("leader-remap").unwrap());
    let mut deck = LoopState::default();
    state.handle_paste("entire Unicode αβ 🦊 draft");
    state.handle_key(KeyAction::Left).await;
    let draft = state.input().to_string();
    let caret = state.prompt_layout(80).1;
    let event = |value, modifiers| CEvent::Key(KeyEvent::new(KeyCode::Char(value), modifiers));
    // These snapshots contain final resolved bindings, as the owner admits
    // them; no stale Ctrl+X Undo/Redo prefix is silently retained.
    for shortcuts in [
        ConversationShortcuts {
            leader: "ctrl+g".into(),
            undo: "ctrl+g u".into(),
            redo: "ctrl+g r".into(),
        },
        ConversationShortcuts {
            leader: String::new(),
            undo: String::new(),
            redo: String::new(),
        },
    ] {
        let mut configured = catalog();
        configured.chrome.conversation_shortcuts = shortcuts;
        state.apply_catalog(configured);
        handle_event(
            &app,
            &mut state,
            &mut deck,
            event('x', KeyModifiers::CONTROL),
        )
        .await
        .unwrap();
        assert!(!state.leader_pending());
        assert_eq!(state.next_ui_deadline(), None);
        assert_eq!(state.input(), draft);
        assert_eq!(state.prompt_layout(80).1, caret);
        assert_eq!(state.panel(), &TuiPanel::None);
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(inbox.try_recv().is_err());
    }
    let mut configured = catalog();
    configured.chrome.conversation_shortcuts = ConversationShortcuts {
        leader: "ctrl+g".into(),
        undo: "ctrl+g u".into(),
        redo: "ctrl+g r".into(),
    };
    configured.chrome.command_palette_shortcut = Some("ctrl+x".into());
    state.apply_catalog(configured);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event('x', KeyModifiers::CONTROL),
    )
    .await
    .unwrap();
    assert_eq!(state.panel(), &TuiPanel::Commands);
    assert!(!state.leader_pending());
    assert_eq!(state.next_ui_deadline(), None);
    assert_eq!(state.input(), draft);
    assert!(inbox.try_recv().is_err());
    state.close_panel();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        event('g', KeyModifiers::CONTROL),
    )
    .await
    .unwrap();
    assert!(state.leader_pending());
    assert!(state.next_ui_deadline().is_some());
    let hidden = state.chrome.sidebar_hidden;
    handle_event(&app, &mut state, &mut deck, event('b', KeyModifiers::NONE))
        .await
        .unwrap();
    assert_ne!(state.chrome.sidebar_hidden, hidden);
    assert!(!state.leader_pending());
    assert_eq!(state.next_ui_deadline(), None);
    assert_eq!(state.input(), draft);
    assert_eq!(state.prompt_layout(80).1, caret);
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn failed_preclose_save_keeps_accepted_home_root_and_parked_cursor() {
    // Home acceptance attached a real root, but the first preference
    // write failed. Neither a stale CAS nor storage failure may turn a
    // subsequent close into a local removal before marker retirement.
    for failure in [CoreError::TabDeckConflict, CoreError::TabDeckStorage] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("fresh").unwrap());
        state.handle_paste("retained draft");
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            revision: Some("old-token".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state); // accepted fresh Home receipt
        deck.cards_before = Some(42);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
                panic!("Home query before any writes or removal")
            };
            ack.send(Ok(catalog())).unwrap();
            let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
                panic!("pre-close save must precede removal")
            };
            assert_eq!(deck.sessions, vec![SessionId::new("fresh").unwrap()]);
            assert_eq!(deck.active, Some(SessionId::new("fresh").unwrap()));
            assert_eq!(deck.revision.as_deref(), Some("old-token"));
            ack.send(Err(failure)).unwrap();
            assert!(
                inbox.try_recv().is_err(),
                "no candidate save after failed preflight"
            );
        });
        apply_outcome(
            &app,
            &mut state,
            &mut deck,
            KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index: 0 }),
                ..Default::default()
            },
            false,
        )
        .await;
        assert_eq!(
            state.note(),
            Some("tab close refused; saved tabs unavailable")
        );
        assert_eq!(state.input(), "retained draft");
        assert_eq!(state.session().0, "fresh");
        assert_eq!(deck.snapshot(&state).sessions.len(), 1);
        assert_eq!(deck.cards_before, Some(42));
        assert_eq!(deck.tab_cards_before, [None]);
        assert_eq!(deck.revision.as_deref(), Some("old-token"));
        worker.await.unwrap();
    }

    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("active").unwrap());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        save_disabled: true,
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "other");
    deck.cards_before = Some(12);
    deck.tab_cards_before[0] = Some(9);
    assert_eq!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 }
        )
        .await
        .unwrap_err(),
        "tab close refused; saved tabs unavailable"
    );
    assert_eq!(deck.snapshot(&state).sessions.len(), 2);
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(deck.cards_before, Some(12));
    assert_eq!(deck.tab_cards_before, [Some(9), None]);
    assert!(inbox.try_recv().is_err(), "disabled save never calls owner");
}

#[tokio::test]
async fn real_close_commits_candidate_before_removal_or_retains_full_deck_on_failure() {
    for failure in [
        Some(CoreError::TabDeckConflict),
        Some(CoreError::TabDeckStorage),
        None,
    ] {
        let failed = failure.is_some();
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("fresh").unwrap());
        state.handle_paste("retained draft");
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            revision: Some("before".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        deck.cards_before = Some(42);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
                panic!("Home queried before either save")
            };
            assert_eq!(action, SelectionAction::Current);
            ack.send(Ok(catalog())).unwrap();
            let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
                panic!("full deck preflight")
            };
            assert_eq!(deck.sessions, vec![SessionId::new("fresh").unwrap()]);
            assert_eq!(deck.active, Some(SessionId::new("fresh").unwrap()));
            assert_eq!(deck.revision.as_deref(), Some("before"));
            let persisted_full = deck.clone();
            ack.send(Ok(TabDeckSnapshot {
                revision: Some("marker-retired".into()),
                ..deck
            }))
            .unwrap();
            let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
                panic!("candidate saved before visible removal")
            };
            assert!(deck.sessions.is_empty());
            assert!(deck.active.is_none());
            assert_eq!(deck.location, "/fixture");
            assert_eq!(deck.revision.as_deref(), Some("marker-retired"));
            ack.send(match failure {
                Some(error) => Err(error),
                None => Ok(TabDeckSnapshot {
                    revision: Some("closed".into()),
                    ..deck
                }),
            })
            .unwrap();
            assert!(inbox.try_recv().is_err(), "no redundant post-close save");
            persisted_full
        });
        let result = apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await;
        if failed {
            assert_eq!(
                result.unwrap_err(),
                "tab close refused; saved tabs unavailable"
            );
            assert_eq!(state.session().0, "fresh");
            assert_eq!(state.input(), "retained draft");
            assert_eq!(deck.snapshot(&state).sessions.len(), 1);
            assert_eq!(deck.active_tab, Some(0));
            assert_eq!(deck.cards_before, Some(42));
            assert_eq!(deck.tab_cards_before, [None]);
            assert_eq!(deck.revision.as_deref(), Some("marker-retired"));
        } else {
            result.unwrap();
            assert!(state.attached_session().is_none());
            assert!(deck.tabs.is_empty());
            assert_eq!(deck.revision.as_deref(), Some("closed"));
        }
        let persisted_full = worker.await.unwrap();
        if failed {
            assert_eq!(
                persisted_full.sessions,
                vec![SessionId::new("fresh").unwrap()]
            );
            assert_eq!(
                persisted_full.active,
                Some(SessionId::new("fresh").unwrap())
            );
        }
    }
}

#[tokio::test]
async fn selected_middle_close_saves_order_and_previous_survivor_before_changing_cursors() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    deck.cards_before = Some(10);
    append_tab(&app, &mut deck, &mut state, "b");
    deck.cards_before = Some(20);
    append_tab(&app, &mut deck, &mut state, "c");
    deck.cards_before = Some(30);
    deck.activate(&mut state, 1).unwrap();
    state.handle_paste("middle draft");
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("full ordered preflight")
        };
        assert_eq!(
            deck.sessions
                .iter()
                .map(|id| id.0.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), Some("b"));
        assert_eq!(deck.revision.as_deref(), Some("old"));
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("preflight".into()),
            ..deck
        }))
        .unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("survivors saved before mutation")
        };
        assert_eq!(
            deck.sessions
                .iter()
                .map(|id| id.0.as_str())
                .collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert_eq!(deck.active.as_ref().map(|id| id.0.as_str()), Some("a"));
        assert_eq!(deck.revision.as_deref(), Some("preflight"));
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("closed".into()),
            ..deck
        }))
        .unwrap();
        assert!(inbox.try_recv().is_err(), "no third write");
    });
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "a");
    assert_eq!(deck.cards_before, Some(10));
    assert_eq!(deck.tab_cards_before, [Some(10), Some(30)]);
    assert_eq!(deck.active_tab, Some(0));
    assert_eq!(
        deck.snapshot(&state)
            .sessions
            .iter()
            .map(|id| id.0.as_str())
            .collect::<Vec<_>>(),
        ["a", "c"]
    );
    assert_eq!(deck.revision.as_deref(), Some("closed"));
    worker.await.unwrap();
}

#[tokio::test]
async fn rejected_candidate_close_keeps_parked_tab_draft_and_cursor() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    state.handle_paste("parked draft");
    deck.cards_before = Some(11);
    append_tab(&app, &mut deck, &mut state, "b");
    state.handle_paste("active draft");
    deck.cards_before = Some(22);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("full deck preflight")
        };
        assert_eq!(
            deck.sessions
                .iter()
                .map(|id| id.0.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        let persisted = deck.clone();
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("preflight".into()),
            ..deck
        }))
        .unwrap();
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("candidate write")
        };
        assert_eq!(deck.sessions, vec![SessionId::new("b").unwrap()]);
        assert_eq!(deck.active, Some(SessionId::new("b").unwrap()));
        assert_eq!(deck.revision.as_deref(), Some("preflight"));
        ack.send(Err(CoreError::TabDeckConflict)).unwrap();
        assert!(inbox.try_recv().is_err());
        persisted
    });
    apply_outcome(
        &app,
        &mut state,
        &mut deck,
        KeyOutcome {
            intent: Some(PanelIntent::CloseTab { index: 0 }),
            ..Default::default()
        },
        false,
    )
    .await;
    assert_eq!(
        state.note(),
        Some("tab close refused; saved tabs unavailable")
    );
    assert_eq!(state.session().0, "b");
    assert_eq!(state.input(), "active draft");
    assert_eq!(deck.cards_before, Some(22));
    assert_eq!(deck.tab_cards_before, [Some(11), None]);
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(deck.revision.as_deref(), Some("preflight"));
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "parked draft");
    assert_eq!(deck.cards_before, Some(11));
    let persisted = worker.await.unwrap();
    assert_eq!(
        persisted
            .sessions
            .iter()
            .map(|id| id.0.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[tokio::test]
async fn close_selected_real_tab_prefers_previous_and_preserves_survivors() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("first draft");
    append_tab(&app, &mut deck, &mut state, "b");
    state.handle_paste("middle draft");
    append_tab(&app, &mut deck, &mut state, "c");
    state.handle_paste("discarded draft");
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 2 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "b");
    assert_eq!(state.input(), "middle draft");
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(deck.tabs.len(), 2);
    assert_eq!(state.tab_presentation().0.len(), 2);
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "b");
    assert_eq!(deck.active_tab, Some(0));
    assert_eq!(state.tab_presentation().0.len(), 1);
    assert!(inbox.try_recv().is_err());
    assert_eq!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 2 }
        )
        .await
        .unwrap_err(),
        "tab unavailable"
    );
    assert_eq!(state.input(), "middle draft");
}

#[tokio::test]
async fn close_selected_or_parked_synthetic_home_never_creates_a_root() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("kept draft");
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    state.handle_paste("home draft");
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "kept");
    assert_eq!(state.input(), "kept draft");
    assert!(deck.home.is_none());
    assert_eq!(state.tab_presentation().0.len(), 1);
    assert_eq!(state.tab_presentation().1, 0);
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    deck.activate(&mut state, 0).unwrap();
    assert!(deck.home.is_some());
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 1 },
    )
    .await
    .unwrap();
    assert!(deck.home.is_none());
    assert_eq!(state.input(), "kept draft");
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert!(state.attached_session().is_none());
    assert!(deck.tabs.is_empty() && deck.tab_cards_before.is_empty());
    assert!(state.tab_presentation().0.is_empty());
    assert_eq!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 }
        )
        .await
        .unwrap_err(),
        "tab unavailable"
    );
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn close_refuses_pending_turn_and_preserves_deck() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("busy").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("pending draft");
    state.handle_key(KeyAction::Enter).await;
    let _request = inbox.recv().await.unwrap();
    let error = apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap_err();
    assert_eq!(error, "turn active; tab close refused");
    assert_eq!(state.input(), "pending draft");
    assert_eq!(state.session().0, "busy");
    assert_eq!(deck.tabs.len(), 1);
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn home_reserves_sixteenth_slot_against_new_session_selection() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("tab-0").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    for index in 1..MAX_TABS - 1 {
        let next = TuiState::new(app.clone(), SessionId::new(format!("tab-{index}")).unwrap());
        deck.tabs[deck.active_tab.unwrap()] = Some(std::mem::replace(&mut state, next));
        deck.active_tab = Some(deck.tabs.len());
        deck.tabs.push(None);
        deck.tab_cards_before.push(None);
    }
    assert_eq!(deck.tabs.len(), MAX_TABS - 1);
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    state.handle_paste("Home draft");
    deck.activate(&mut state, 0).unwrap();
    state.handle_paste("parked draft");
    let error = apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession {
            id: "tab-15".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error, "tab limit reached");
    assert!(
        inbox.try_recv().is_err(),
        "rejection must precede app selection"
    );
    assert_eq!(state.input(), "parked draft");
    assert_eq!(deck.tabs.len(), MAX_TABS - 1);
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    assert_eq!(state.input(), "Home draft");
    assert!(state.attached_session().is_none());
    // The actual first-turn receipt binds Home to its reserved slot.
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { text, ack, .. }) = inbox.recv().await else {
        panic!("expected fresh Home submission")
    };
    assert_eq!(text, "Home draft");
    ack.send(Ok(WorkerTurnId("accepted-home-turn".into())))
        .unwrap();
    state.poll_submission();
    deck.sync_tabs(&mut state);
    assert_eq!(deck.tabs.len(), MAX_TABS);
    assert!(state.attached_session().is_some());
    assert_eq!(deck.tabs[0].as_ref().unwrap().input(), "parked draft");
}

#[tokio::test]
async fn typed_new_consumes_only_successful_command_before_parking() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("old").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("ordinary draft");
    // Mouse + leaves the parked editor untouched.
    let worker = tokio::spawn(async move {
        for _ in 0..2 {
            let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
                panic!("expected Home selection")
            };
            ack.send(Ok(catalog())).unwrap();
        }
    });
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "ordinary draft");
    state.accept_intent();
    state.handle_paste("/new");
    // The parked Home is restored without querying a new selection.
    let outcome = state.handle_key(KeyAction::Enter).await;
    apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "");
    // An initial attached view follows the selection-success path too.
    let mut state = TuiState::new(app.clone(), SessionId::new("another").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("/new");
    let outcome = state.handle_key(KeyAction::Enter).await;
    apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "");
    worker.await.unwrap();
}

#[tokio::test]
async fn rejected_typed_new_keeps_editable_command() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("old").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("/new");
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("expected Home selection")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
    });
    let outcome = state.handle_key(KeyAction::Enter).await;
    apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
    assert_eq!(state.input(), "/new");
    assert_eq!(deck.tabs.len(), 1);
    assert!(deck.home.is_none());
    worker.await.unwrap();
}

#[tokio::test]
async fn cards_cursor_follows_parked_view_and_pages_from_oldest_loaded_row() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("cards-a").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    let worker = tokio::spawn(async move {
        for (before, ids, has_older) in [(None, vec![30, 20], true), (Some(20), vec![10], false)] {
            let Some(InboxMsg::ToolOps {
                before_rowid, ack, ..
            }) = inbox.recv().await
            else {
                panic!("expected cards query")
            };
            assert_eq!(before_rowid, before);
            ack.send(Ok(ToolOpPage {
                rows: ids
                    .into_iter()
                    .map(|rowid| ToolOpView {
                        op: format!("op-{rowid}"),
                        rowid,
                        name: "read".into(),
                        state: "completed".into(),
                        input: None,
                        output: None,
                        output_bytes: 0,
                        output_truncated: false,
                        patch_effects: None,
                        dcp: None,
                        dcp_topic: None,
                    })
                    .collect(),
                total: 3,
                has_older,
            }))
            .unwrap();
        }
    });
    apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadCards)
        .await
        .unwrap();
    assert_eq!(deck.cards_before, Some(20));
    assert!(state.cards_need_older());
    let next = TuiState::new(app.clone(), SessionId::new("cards-b").unwrap());
    deck.tabs[0] = Some(std::mem::replace(&mut state, next));
    deck.tab_cards_before[0] = deck.cards_before;
    deck.active_tab = Some(1);
    deck.tabs.push(None);
    deck.tab_cards_before.push(None);
    deck.cards_before = None;
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(deck.cards_before, Some(20));
    assert!(state.cards_need_older());
    apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadCards)
        .await
        .unwrap();
    assert_eq!(deck.cards_before, Some(10));
    assert!(!state.cards_need_older());
    worker.await.unwrap();
}

#[tokio::test]
async fn sessions_delete_changes_deck_only_after_owner_acceptance_and_adopts_survivor() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("current".into()));
    state.restore_prompt("current draft".into());
    let mut parked = TuiState::new(app.clone(), SessionId("survivor".into()));
    parked.restore_prompt("survivor draft".into());
    let mut deck = LoopState {
        tabs: vec![Some(parked), None],
        tab_cards_before: vec![None, None],
        active_tab: Some(1),
        ..LoopState::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::PickerSessionAction { session, ack, .. }) = inbox.recv().await else {
            panic!("delete action")
        };
        assert_eq!(session.0, "current");
        ack.send(Err(CoreError::SessionNotFound)).unwrap();
        let Some(InboxMsg::PickerSessionAction { session, ack, .. }) = inbox.recv().await else {
            panic!("delete action")
        };
        assert_eq!(session.0, "current");
        ack.send(Ok(oc_core::queries::SessionPickerResult::Deleted(
            TabDeckSnapshot {
                location: "/project".into(),
                revision: Some("accepted".into()),
                sessions: vec![SessionId("survivor".into())],
                active: Some(SessionId("survivor".into())),
            },
        )))
        .unwrap();
        let Some(InboxMsg::SessionList { ack, .. }) = inbox.recv().await else {
            panic!("refresh")
        };
        ack.send(Ok(Vec::new())).unwrap();
    });
    let intent = PanelIntent::DeleteSelectedSession {
        id: "current".into(),
    };
    apply_intent(&app, &mut state, &mut deck, intent.clone())
        .await
        .unwrap();
    assert_eq!(state.attached_session().unwrap().0, "current");
    assert_eq!(state.input(), "current draft");
    assert_eq!(deck.tabs.len(), 2);
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert_eq!(state.attached_session().unwrap().0, "survivor");
    assert_eq!(state.input(), "survivor draft");
    assert_eq!(deck.tabs.len(), 1);
    assert_eq!(deck.active_tab, Some(0));
    assert_eq!(deck.revision.as_deref(), Some("accepted"));
    worker.await.unwrap();
}

#[tokio::test]
async fn committed_title_event_refreshes_only_the_open_session() {
    let (app, _inbox, _) = CoreApp::channel(4);
    let session = SessionId::new("open-title").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut loop_state = LoopState::default();
    handle_worker_event(
        &app,
        &mut state,
        &mut loop_state,
        &session,
        CoreEvent::SessionTitleUpdated {
            session: SessionId::new("other-title").unwrap(),
            title: "foreign".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.session_title, None);
    handle_worker_event(
        &app,
        &mut state,
        &mut loop_state,
        &session,
        CoreEvent::SessionTitleUpdated {
            session: session.clone(),
            title: "Real title".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.session_title.as_deref(), Some("Real title"));
}

#[tokio::test]
async fn pending_submission_refuses_session_and_location_switches() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let session = SessionId::new("pending").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    state.handle_paste("immutable draft");
    state.handle_key(KeyAction::Enter).await;
    let _request = inbox.recv().await.unwrap(); // retain acceptance sender
    let mut loop_state = LoopState::default();
    for intent in [
        PanelIntent::SwitchSession { id: "other".into() },
        PanelIntent::SwitchLocation {
            path: "/fixture/other".into(),
        },
    ] {
        let error = apply_intent(&app, &mut state, &mut loop_state, intent)
            .await
            .unwrap_err();
        assert!(error.contains("switch refused"));
        assert_eq!(state.session(), &session);
        assert_eq!(state.input(), "immutable draft");
        assert!(inbox.try_recv().is_err(), "switch reached runtime");
    }
    // Session-scoped events cannot match even a coincident turn id.
    state.begin_compress_turn(WorkerTurnId("same-id".into()));
    handle_worker_event(
        &app,
        &mut state,
        &mut loop_state,
        &session,
        CoreEvent::TextDelta {
            session: SessionId::new("other").unwrap(),
            turn: WorkerTurnId("same-id".into()),
            delta: "wrong session".into(),
        },
    )
    .await
    .unwrap();
    assert!(!state.viewport().join("\n").contains("wrong session"));
}
