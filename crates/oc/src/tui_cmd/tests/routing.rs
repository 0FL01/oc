use super::*;

#[tokio::test]
async fn direct_user_shell_notice_updates_typed_block_during_live_model_turn_without_raw_toast() {
    use oc_core::{
        queries::{HistoryMessage, HistoryPage, ShellNotice, UserShellResult},
        session::{MessageId, Role},
        tool_output::{Presentation, Shell},
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId::new("user-shell-with-live-model").unwrap();
    let current = WorkerTurnId("real-live-overlay".into());
    let mut state = TuiState::new(app.clone(), session.clone());
    state.restore_prompt("ordinary accepted prompt".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("model submit");
    };
    ack.send(Ok(current.clone())).unwrap();
    state.poll_submission();
    state.apply_delta(&current, "live assistant text");
    state.restore_prompt("unfinished newer draft".into());
    let mut output = Presentation::new("user process output", 19, false);
    output.shell = Some(Shell {
        stdout: "user process output".into(),
        stderr: String::new(),
        stdout_limited: false,
        stderr_limited: false,
        exit: Some(0),
        signal: None,
        timed_out: false,
        cancelled: false,
    });
    let owner = tokio::spawn(async move {
        let InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("bounded history query");
        };
        assert_eq!(session.0, "user-shell-with-live-model");
        assert_eq!(message, Some(MessageId("actual-shell-notice".into())));
        assert_eq!(
            limit, 1,
            "a delayed notice is not searched in the newest page"
        );
        ack.send(Ok(HistoryPage {
            rows: vec![HistoryMessage {
                id: MessageId("actual-shell-notice".into()),
                seq: 7,
                role: Role::User,
                text: "RAW technical notice must not become a toast".into(),
                turn: None,
                model_switch: None,
                user_shell: Some(UserShellResult {
                    input: false,
                    superseded_input: false,
                    operation: "owned-user-command".into(),
                    command: "printf output".into(),
                    command_limited: false,
                    state: "completed".into(),
                    output: Box::new(output),
                    diagnostic: None,
                }),
            }],
            total: 120,
            has_newer: true,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "owned-user-command".into(),
            delivery_id: "once-only".into(),
            message_id: "actual-shell-notice".into(),
            user_requested: true,
            state: "completed".into(),
            text: "RAW technical notice must not become a toast".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = owner.await.unwrap();
    let text = state
        .transcript_lines(120, 120)
        .iter()
        .map(oc_tui::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("$ printf output") && text.contains("user process output"));
    assert_eq!(text.matches("live assistant text").count(), 1);
    assert!(!text.contains("must not duplicate") && !text.contains("RAW technical"));
    assert!(state.note().is_none());
    assert_eq!(state.active_turn(), Some(&current));
    assert_eq!(state.input(), "unfinished newer draft");

    let hidden_owner = tokio::spawn(async move {
        let InboxMsg::History { message, ack, .. } = inbox.recv().await.unwrap() else {
            panic!("exact current-branch lookup");
        };
        assert_eq!(message, Some(MessageId("hidden-native-result".into())));
        ack.send(Ok(HistoryPage {
            total: 120,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "hidden-operation".into(),
            delivery_id: "hidden-delivery".into(),
            message_id: "hidden-native-result".into(),
            user_requested: true,
            state: "completed".into(),
            text: "hidden RAW warning".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = hidden_owner.await.unwrap();
    assert!(
        state.note().is_none(),
        "branch-hidden native result does not become RAW prose"
    );
    assert_eq!(state.input(), "unfinished newer draft");
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "model-operation".into(),
            delivery_id: "model-delivery".into(),
            message_id: "model-result".into(),
            user_requested: false,
            state: "completed".into(),
            text: "existing model background notice".into(),
        }),
    )
    .await
    .unwrap();
    assert!(
        state.note().is_some(),
        "model background notice retains its existing path"
    );
    assert!(
        inbox.try_recv().is_err(),
        "model notice does not query native projection"
    );
}

#[tokio::test]
async fn vis38_late_dcp_other_session_child_and_old_turn_never_touch_parent_view_or_query() {
    use oc_core::dcp_view::{DcpAccounting, DcpRunSnapshot};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let parent = SessionId::new("dcp-parent").unwrap();
    let current = WorkerTurnId("current-generation".into());
    let mut state = TuiState::new(app.clone(), parent.clone());
    state.restore_prompt("active parent request".into());
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("parent submission")
    };
    ack.send(Ok(current.clone())).unwrap();
    state.poll_submission();
    assert_eq!(state.active_turn(), Some(&current));
    let before = state.transcript_lines(100, 100);
    let mut deck = LoopState::default();
    for (owner, turn) in [
        (SessionId::new("dcp-other").unwrap(), current.clone()),
        (SessionId::new("dcp-child").unwrap(), current.clone()),
        (parent.clone(), WorkerTurnId("obsolete-generation".into())),
    ] {
        let event = CoreEvent::ToolCallFinished {
            output_presentation: None,
            question: None,
            session: owner.clone(),
            turn,
            op: "late-operation".into(),
            name: "compress".into(),
            state: "completed".into(),
            output: "successful-looking untrusted output".into(),
            output_bytes: 44,
            output_truncated: false,
            patch_effects: None,
            dcp: Some(DcpRunSnapshot {
                session: owner.0,
                operation_id: "late-operation".into(),
                ordinal: 42,
                topic: "late child must not affect parent".into(),
                block_ids: vec!["foreign-block".into()],
                removed: 10000,
                summary: 42,
                net_saved: 9958,
                method: Default::default(),
                new_messages: 2,
                new_tools: 1,
                cumulative: DcpAccounting {
                    gross_removed: 10000,
                    compressions: 42,
                    complete: true,
                    ..Default::default()
                },
                bar: "⣿".repeat(50),
            }),
        };
        tokio::time::timeout(
            Duration::from_millis(250),
            handle_worker_event(&app, &mut state, &mut deck, &parent, event),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(state.transcript_lines(100, 100), before);
        assert!(state.note().is_none());
        assert!(
            inbox.try_recv().is_err(),
            "late event must not refresh parent counters or summaries"
        );
    }
}

#[tokio::test]
async fn vis26_key_burst_only_queries_latest_and_route_swap_cancels_old_view() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    state.chrome.location = Some("/A".into());
    let mut deck = LoopState::default();
    for key in ['@', 's'] {
        state.handle_key(KeyAction::Char(key)).await;
        sync_mention(&app, &mut state, &mut deck).await;
    }
    tokio::time::sleep(Duration::from_millis(40)).await;
    state.handle_key(KeyAction::Char('r')).await;
    sync_mention(&app, &mut state, &mut deck).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "intermediate edits must not start owner traversal"
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions {
        query,
        limit,
        ack: old_ack,
    }) = inbox.recv().await
    else {
        panic!("one latest query");
    };
    assert_eq!(query, "sr");
    assert_eq!(limit, MENTION_LIMIT);
    let old_key = deck.mention_job.as_ref().unwrap().0.clone();

    // A second view can have the same draft, Location and local revision.
    let mut next = TuiState::new_home(app.clone());
    next.chrome.location = Some("/A".into());
    next.handle_paste("@sr");
    std::mem::swap(&mut state, &mut next);
    sync_mention(&app, &mut state, &mut deck).await;
    let new_key = state.mention_request().unwrap();
    assert_ne!(old_key.view_id, new_key.view_id);
    assert!(deck.mention_job.is_none());
    assert!(inbox.try_recv().is_err(), "new view also waits for quiet");
    tokio::time::sleep(MENTION_DEBOUNCE).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions { query, ack, .. }) = inbox.recv().await else {
        panic!("new view query");
    };
    assert_eq!(query, "sr");
    // The old owner request may already have started, but its receiver is
    // cancelled; it cannot populate either the old or new view.
    let _ = old_ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 1,
        paths: vec!["old".into()],
        truncated: false,
    }));
    ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 1,
        paths: vec!["fresh".into()],
        truncated: false,
    }))
    .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
        sync_mention(&app, &mut state, &mut deck).await;
        if state.mention_loaded(&new_key) {
            break;
        }
    }
    assert!(state.mention_loaded(&new_key));
    assert!(!next.mention_loaded(&old_key));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "loaded result is not queried every frame"
    );
}

#[tokio::test]
async fn vis26_parked_tab_refreshes_file_rows_only_on_reactivation() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.chrome.location = Some("/A".into());
    state.handle_paste("@sr");
    let cached = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(
        cached.clone(),
        FileSuggestionsSnapshot {
            location: "/A".into(),
            generation: 1,
            paths: vec!["src/removed.rs".into()],
            truncated: false,
        }
    ));
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "active result stays cached");
    append_tab(&app, &mut deck, &mut state, "second");
    assert!(deck.tabs[0].as_ref().unwrap().mention_loaded(&cached));
    deck.activate(&mut state, 0).unwrap();
    let refreshed = state.mention_request().unwrap();
    assert_ne!(cached, refreshed);
    assert!(!state.mention_loaded(&refreshed));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "restored tab is debounced");
    tokio::time::sleep(MENTION_DEBOUNCE).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions { query, ack, .. }) = inbox.recv().await else {
        panic!("restored tab requests a fresh snapshot");
    };
    assert_eq!(query, "sr");
    ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 2,
        paths: vec!["src/new.rs".into()],
        truncated: false,
    }))
    .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
        sync_mention(&app, &mut state, &mut deck).await;
        if state.mention_loaded(&refreshed) {
            break;
        }
    }
    assert!(state.mention_loaded(&refreshed));
    assert!(!state.mention_loaded(&cached));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "normal sync does not invalidate");
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), "@src/new.rs ");
}

#[tokio::test]
async fn accepted_conversation_survives_refresh_failure_and_retries_queries_only() {
    use oc_core::queries::{ConversationAction, ConversationSnapshot, HistoryPage};
    for action in [
        ConversationAction::Undo,
        ConversationAction::Redo,
        ConversationAction::Revert {
            message: oc_core::session::MessageId("user".into()),
        },
    ] {
        for history_failure in [true, false] {
            let (app, mut inbox, _) = CoreApp::channel(8);
            let session = SessionId::new("receipt").unwrap();
            let mut state = TuiState::new(app.clone(), session.clone());
            state.session_title = Some("future title".into());
            state.restore_prompt("draft before ACK".into());
            let mut deck = LoopState::default();
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::ChangeConversation {
                    action: action.clone(),
                },
            )
            .await
            .unwrap();
            let worker = tokio::spawn(async move {
                let Some(InboxMsg::ChangeConversation { ack, session, .. }) = inbox.recv().await
                else {
                    panic!("mutation once")
                };
                ack.send(Ok(ConversationSnapshot {
                    session,
                    draft: Some("owner restored".into()),
                    can_undo: false,
                    can_redo: true,
                    reverted: None,
                }))
                .unwrap();
                let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                    panic!("history")
                };
                ack.send(if history_failure {
                    Err(CoreError::Shutdown)
                } else {
                    Ok(HistoryPage::default())
                })
                .unwrap();
                let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                    panic!("selection")
                };
                ack.send(if history_failure {
                    Ok(catalog())
                } else {
                    Err(CoreError::Shutdown)
                })
                .unwrap();
                empty_compactions(&mut inbox).await;
                inbox
            });
            settle_conversation(&app, &mut state, &mut deck).await;
            let mut inbox = worker.await.unwrap();
            assert_eq!(state.input(), "owner restored");
            assert!(
                state.session_title.is_none(),
                "future projection invalidated"
            );
            assert_eq!(
                state.command_unavailable(&CommandAction::UndoConversation),
                Some("nothing to undo")
            );
            assert!(deck.conversation_recovery.is_some());
            deck.conversation_recovery.as_mut().unwrap().1 = std::time::Instant::now();
            finish_conversation(&app, &mut state, &mut deck).await;
            let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                panic!("refresh only")
            };
            ack.send(Ok(HistoryPage::default())).unwrap();
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("refresh only")
            };
            ack.send(Ok(catalog())).unwrap();
            empty_compactions(&mut inbox).await;
            while !deck.recovery_job.as_ref().unwrap().is_finished() {
                tokio::task::yield_now().await;
            }
            finish_conversation(&app, &mut state, &mut deck).await;
            assert!(deck.conversation_recovery.is_none());
            assert_eq!(state.input(), "owner restored");
            assert!(inbox.try_recv().is_err());
        }
    }
}

#[tokio::test]
async fn copy_message_queries_exact_owner_row_instead_of_window_preview() {
    use oc_core::queries::{HistoryMessage, HistoryPage};
    use oc_core::session::{MessageId, Role};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId::new("copy-ui").unwrap());
    let text = "full public user text\n".repeat(500);
    let expected = text.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::History {
            before_seq,
            limit,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("owner history")
        };
        assert_eq!(before_seq, Some(43));
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage {
            rows: vec![HistoryMessage {
                id: MessageId("opaque-user".into()),
                seq: 42,
                role: Role::User,
                text,
                turn: None,
                model_switch: None,
                user_shell: None,
            }],
            ..Default::default()
        }))
        .unwrap();
    });
    fn record_copy(text: &str) -> Result<(), String> {
        assert_eq!(text, "full public user text\n".repeat(500));
        Ok(())
    }
    let mut deck = LoopState {
        copy_transport: Some(record_copy),
        ..Default::default()
    };
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CopyMessage {
            message: MessageId("opaque-user".into()),
            seq: 42,
        },
    )
    .await
    .unwrap();
    assert_eq!(expected, "full public user text\n".repeat(500));
    assert!(
        state.take_copy_request().is_none(),
        "transport drained the request"
    );
    assert_eq!(state.note_variant(), Some(NoteVariant::Info));
    worker.await.unwrap();
}

#[tokio::test]
async fn mcp_detail_keyboard_copy_drains_actual_transport_before_outcome() {
    use oc_core::queries::{
        McpBinding, McpServerSnapshot, McpSnapshot, McpStatus, ServiceAction, ServiceCode,
        ServiceDiagnostic, ServiceKind, ServiceStage,
    };

    for succeeds in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app.clone(), SessionId::new("detail-copy-ui").unwrap());
        let mut chrome = catalog();
        chrome.chrome.location = Some("/fixture".into());
        state.apply_catalog(chrome);
        state.restore_prompt("unfinished Ω draft".into());
        state.apply_mcp_snapshot(McpSnapshot {
            binding: McpBinding {
                location: "/fixture".into(),
                generation: 1,
                instance: 1,
            },
            revision: 1,
            servers: vec![McpServerSnapshot {
                id: "opaque-control".into(),
                name: "safe-label".into(),
                configured_enabled: true,
                status: McpStatus::Failed,
                pending_action: None,
                tools: 0,
                actions: Vec::new(),
                diagnostic: Some(ServiceDiagnostic {
                    kind: ServiceKind::Mcp,
                    service: "opaque-diagnostic".into(),
                    source: "safe-source".into(),
                    field: vec!["mcp".into()],
                    stage: ServiceStage::Initialize,
                    code: ServiceCode::ConnectionFailed,
                    action: ServiceAction::RetryConnection,
                }),
            }],
        });
        state.restore_prompt("/mcps".into());
        let open = state.handle_key(KeyAction::Enter);
        let history_owner = async {
            let Some(oc_core::core_app::InboxMsg::PromptHistory { append, ack }) =
                inbox.recv().await
            else {
                panic!("expected bounded input-history admission");
            };
            let mut entries = Vec::new();
            oc_core::queries::append_prompt_history(&mut entries, append.as_deref().unwrap())
                .unwrap();
            assert_eq!(entries, ["/mcps"]);
            ack.send(Ok(entries)).unwrap();
        };
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(open, history_owner);
        })
        .await
        .expect("slash acceptance records before dispatch");
        state.restore_prompt("unfinished Ω draft".into());
        state.handle_key(KeyAction::Enter).await;
        fn success(text: &str) -> Result<(), String> {
            assert!(text.starts_with("MCP server: safe-label\nError: mcp opaque-diagnostic"));
            Ok(())
        }
        fn failure(text: &str) -> Result<(), String> {
            success(text)?;
            Err("transport failed".into())
        }
        let mut deck = LoopState {
            copy_transport: Some(if succeeds { success } else { failure }),
            ..Default::default()
        };
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        )
        .await
        .unwrap();
        assert!(
            state.take_copy_request().is_none(),
            "key request must reach transport"
        );
        assert_eq!(state.input(), "unfinished Ω draft");
        let detail = oc_tui::views::render_test(&state, 120, 40).join("\n");
        assert_eq!(detail.contains("✓ copied"), succeeds);
        assert_eq!(detail.contains("c copy details"), !succeeds);
        assert!(
            inbox.try_recv().is_err(),
            "copy neither submits nor reconnects"
        );
    }
}

#[tokio::test]
async fn selecting_model_updates_local_draft_without_owner_commit_or_selection_toast() {
    use oc_core::queries::ModelEntry;

    let (app, mut inbox, _) = CoreApp::channel(4);
    let session = SessionId::new("model-selection").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut selected = catalog();
    selected.models = ["first", "second"]
        .into_iter()
        .map(|id| ModelEntry {
            id: id.into(),
            display_name: id.into(),
            provider_name: "fixture".into(),
            price: None,
            variants: Vec::new(),
            context: 0,
            context_known: false,
            output: 0,
            output_known: false,
        })
        .collect();
    selected.model_id = "first".into();
    state.apply_catalog(selected.clone());
    assert_eq!(state.active_model_label(), Some(("first".into(), None)));
    assert_eq!(state.note(), None);

    apply_intent(
        &app,
        &mut state,
        &mut LoopState::default(),
        PanelIntent::SelectModel {
            id: "second".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.active_model_label(), Some(("second".into(), None)));
    assert_eq!(state.note(), None);
    assert!(inbox.try_recv().is_err(), "picker choice is only a draft");
}

#[tokio::test]
async fn selecting_agent_updates_owner_and_draft_without_selection_toast() {
    use oc_core::queries::{AgentEntry, ModelEntry};
    for existing_note in [None, Some("unrelated warning")] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("agent-selection").unwrap());
        let mut selected = catalog();
        selected.models.push(ModelEntry {
            id: selected.model_id.clone(),
            display_name: "Fixture".into(),
            provider_name: "fixture".into(),
            price: None,
            variants: Vec::new(),
            context: 0,
            context_known: false,
            output: 0,
            output_known: false,
        });
        selected.agents = ["build", "build-yolo"]
            .into_iter()
            .enumerate()
            .map(|(color_index, id)| AgentEntry {
                id: id.into(),
                color_index,
                description: String::new(),
                model: None,
                variant: None,
            })
            .collect();
        selected.agent_id = Some("build".into());
        state.apply_catalog(selected.clone());
        state.restore_prompt("preserved draft".into());
        if let Some(note) = existing_note {
            state.push_note(note);
        }
        state.handle_key(oc_tui::events::KeyAction::Agents).await;
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::SessionSelection { action, ack, .. }) = inbox.recv().await else {
                panic!("selection")
            };
            assert_eq!(action, SelectionAction::Agent("build-yolo".into()));
            selected.agent_id = Some("build-yolo".into());
            ack.send(Ok(selected)).unwrap();
            assert!(inbox.try_recv().is_err());
        });
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::SelectAgent {
                id: "build-yolo".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(state.active_agent(), Some("build-yolo"));
        assert_eq!(state.input(), "preserved draft");
        assert_eq!(state.panel(), &oc_tui::app::TuiPanel::None);
        assert_eq!(state.note(), existing_note);
        worker.await.unwrap();
    }
    // Owner failures stay errors and leave the dialog/draft available.
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(catalog());
    state.restore_prompt("retry draft".into());
    state.handle_key(oc_tui::events::KeyAction::Agents).await;
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("Home selection")
        };
        ack.send(Err(CoreError::Application("unknown agent".into())))
            .unwrap();
    });
    let error = apply_intent(
        &app,
        &mut state,
        &mut LoopState::default(),
        PanelIntent::SelectAgent {
            id: "missing".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("unknown agent"));
    assert_eq!(state.panel(), &oc_tui::app::TuiPanel::Agents);
    assert_eq!(state.input(), "retry draft");
    worker.await.unwrap();
}

#[tokio::test]
async fn variant_cycle_drafts_catalog_order_and_stale_default_rules_in_home_and_session() {
    use oc_core::queries::{ModelEntry, VariantEntry};
    for home in [true, false] {
        for (current, expected) in [
            (None, Some("zeta")),
            (Some("default"), Some("zeta")),
            (Some("zeta"), Some("alpha")),
            (Some("alpha"), None),
            (Some("retired"), None),
        ] {
            let (app, mut inbox, _) = CoreApp::channel(4);
            let mut state = if home {
                TuiState::new_home(app.clone())
            } else {
                TuiState::new(app.clone(), SessionId::new("cycle-owner").unwrap())
            };
            state.restore_prompt("next accepted prompt".into());
            let mut selected = catalog();
            selected.models = vec![ModelEntry {
                id: selected.model_id.clone(),
                display_name: "Dynamic model".into(),
                provider_name: "fixture".into(),
                price: None,
                variants: [
                    ("default", false),
                    ("zeta", false),
                    ("disabled", true),
                    ("alpha", false),
                ]
                .into_iter()
                .map(|(name, disabled)| VariantEntry {
                    name: name.into(),
                    disabled,
                    reasoning_effort: Some(name.into()),
                })
                .collect(),
                context: 0,
                context_known: false,
                output: 0,
                output_known: false,
            }];
            selected.variant = current.map(str::to_string);
            state.apply_catalog(selected);
            state.push_note("unrelated warning");
            let intent = state
                .handle_key(oc_tui::events::KeyAction::CycleVariant)
                .await
                .intent
                .unwrap();
            apply_intent(&app, &mut state, &mut LoopState::default(), intent)
                .await
                .unwrap();
            assert_eq!(
                state.active_model_label(),
                Some(("Dynamic model".into(), expected.map(str::to_string)))
            );
            assert_eq!(state.input(), "next accepted prompt");
            assert_eq!(state.panel(), &oc_tui::app::TuiPanel::None);
            assert_eq!(state.note(), Some("unrelated warning"));
            assert!(
                inbox.try_recv().is_err(),
                "cycle must neither commit nor submit"
            );
        }
    }
}

#[tokio::test]
async fn variant_cycle_missing_models_or_named_variants_is_noop_and_child_is_readonly() {
    use oc_core::queries::{ModelEntry, VariantEntry};
    for missing_model in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app.clone());
        let mut selected = catalog();
        if !missing_model {
            selected.models.push(ModelEntry {
                id: selected.model_id.clone(),
                display_name: "No variants".into(),
                provider_name: "fixture".into(),
                price: None,
                variants: vec![
                    VariantEntry {
                        name: "default".into(),
                        disabled: false,
                        reasoning_effort: None,
                    },
                    VariantEntry {
                        name: "disabled".into(),
                        disabled: true,
                        reasoning_effort: None,
                    },
                ],
                context: 0,
                context_known: false,
                output: 0,
                output_known: false,
            });
        }
        state.apply_catalog(selected);
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::CycleVariant,
        )
        .await
        .unwrap();
        assert!(inbox.try_recv().is_err());
        let mut deck = LoopState {
            read_only: true,
            ..Default::default()
        };
        assert!(
            apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
                .await
                .unwrap_err()
                .contains("read-only")
        );
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn variant_cycle_persists_and_next_real_request_uses_selected_overlay() {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;

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
                "models":{"m":{"variants":{"custom":{"reasoningEffort":"high"}}}}}}
        })
        .to_string(),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut requests = Vec::<serde_json::Value>::new();
        while requests.len() < 2 && std::time::Instant::now() < deadline {
            let mut socket = match listener.accept() {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fake provider: {error}"),
            };
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
            let length: usize = String::from_utf8_lossy(&bytes[..end])
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
            requests.push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
            let item = serde_json::json!({"type":"message","role":"assistant","id":"a","status":"completed","content":[{"type":"output_text","text":"done"}]});
            let sse = format!(
                "data: {}\n\ndata: {}\n\n",
                serde_json::json!({"type":"response.output_item.done","item":item}),
                serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1}}})
            );
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).unwrap();
        }
        assert_eq!(requests.len(), 2);
        requests
    });
    let env = BTreeMap::from([
        ("HOME".into(), root.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let data = root.path().join("data");
    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env.clone())
        .await
        .unwrap();
    let session = SessionId::new("cycle-wire").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Already titled".into())
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut deck = LoopState::default();
    state.apply_catalog(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap(),
    );
    apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
        .await
        .unwrap();
    assert_eq!(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap()
            .variant,
        None,
        "variant cycle remains local until composer admission"
    );
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("custom")
    );
    state.poll_submission();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env)
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    state.apply_catalog(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap(),
    );
    assert_eq!(
        state.active_model_label().unwrap().1.as_deref(),
        Some("custom")
    );
    let mut events = app.subscribe();
    for (index, prompt) in ["named variant request", "default variant request"]
        .into_iter()
        .enumerate()
    {
        if index == 1 {
            apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
                .await
                .unwrap();
        }
        state.handle_paste(prompt);
        state.handle_key(KeyAction::Enter).await;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap();
            let finished = matches!(event, CoreEvent::TurnFinished { .. });
            assert!(!matches!(event, CoreEvent::TurnFailed { .. }), "{event:?}");
            state.poll_submission();
            handle_worker_event(&app, &mut state, &mut deck, &session, event)
                .await
                .unwrap();
            if finished {
                break;
            }
        }
    }
    assert_eq!(
        app.session_selection(session, false, SelectionAction::Current)
            .await
            .unwrap()
            .variant,
        None
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests[0]["model"], "m");
    assert_eq!(requests[1]["model"], "m");
    assert_eq!(requests[0]["reasoning"]["effort"], "high");
    assert!(requests[1]["reasoning"]["effort"].is_null());
}

#[test]
fn vis27_parked_tabs_and_home_use_current_owner_mode_on_activation() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, _inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    old.chrome.session_tps = Some(true);
    let measured_page = tps_page();
    let mut current = old.clone();
    current.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    current.chrome.session_tps = Some(false);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&measured_page);
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(current);
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    assert!(
        !state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );

    let mut home = TuiState::new_home(app.clone());
    home.apply_catalog(old);
    deck.home = Some(home);
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    // A now-unconfigured owner snapshot resets every restored route.
    state.apply_catalog(catalog());
    deck.activate(&mut state, 1).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::default());
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::default());
    deck.activate(&mut state, 0).unwrap();
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s")),
        "missing setting restores pinned default without losing stats"
    );
}

#[tokio::test]
async fn pending_owner_reload_allows_resize_and_quit_without_dropping_owner_receipt() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    state.handle_paste("/reload");
    let mut deck = LoopState::default();
    let owner = app.clone();
    deck.reload_job = Some(tokio::spawn(async move { owner.reload_location().await }));
    let Some(InboxMsg::ReloadLocation { ack }) = inbox.recv().await else {
        panic!("owner reload must be in flight")
    };
    let pointer = (2, 3, ratatui::layout::Rect::new(0, 0, 80, 24));
    state.restore_mouse_hover(pointer);
    assert_eq!(state.mouse_position(), Some(pointer));
    handle_event(&app, &mut state, &mut deck, CEvent::Resize(100, 30))
        .await
        .unwrap();
    assert_eq!(state.mouse_position(), None);
    assert_eq!(state.status(), &TuiStatus::Idle);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    )
    .await
    .unwrap();
    assert_eq!(state.status(), &TuiStatus::Quit);
    assert_eq!(state.input(), "/reload");
    assert!(!deck.reload_job.as_ref().unwrap().is_finished());
    // The owner can complete its transaction after terminal quit: no
    // dropped receiver or optimistic success/selection replacement.
    ack.send(Err(CoreError::Shutdown)).unwrap();
    assert!(deck.reload_job.take().unwrap().await.unwrap().is_err());
}

#[tokio::test]
async fn failed_selection_after_owner_reload_keeps_every_view_and_draft() {
    use oc_core::queries::{
        McpBinding, McpServerSnapshot, McpSnapshot, McpStatus, TerminalCopyMode,
    };
    use oc_tui::app::ClipboardMode;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    old.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    old.chrome.session_tps = Some(true);
    old.commands = vec!["old-command".into()];
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(old.clone());
    state.handle_paste("/reload");
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    let mut fresh = old.clone();
    fresh.commands = vec!["fresh-command".into()];
    fresh.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    fresh.chrome.session_tps = Some(false);
    let published = fresh.clone();
    let current_mcp = McpSnapshot {
        binding: McpBinding {
            location: "/fixture".into(),
            generation: 1,
            instance: 1,
        },
        revision: 2,
        servers: vec![McpServerSnapshot {
            id: "coalesced-server".into(),
            name: "safe-service".into(),
            configured_enabled: true,
            status: McpStatus::Failed,
            pending_action: None,
            tools: 0,
            diagnostic: None,
            actions: Vec::new(),
        }],
    };
    let worker_mcp = current_mcp.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("parked session selection")
        };
        ack.send(Ok(fresh)).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("active session selection")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        let Some(InboxMsg::McpStatus { ack }) = inbox.recv().await else {
            panic!("refused reload consumes the surviving owner's current facts")
        };
        ack.send(Ok(worker_mcp)).unwrap();
    });
    let error = finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/fixture".into(),
            generation: 2,
            catalog: published,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        Some("/reload".into()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("selection refresh failed"));
    state.apply_intent_error(error);
    assert!(state.note().unwrap().contains("reload incomplete"));
    assert_eq!(state.input(), "/reload");
    assert!(state.is_workspace_command("/old-command"));
    assert!(!state.is_workspace_command("/fresh-command"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    for view in [&state, deck.tabs[0].as_ref().unwrap()] {
        let text = view
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .map(oc_tui::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Build · Measured model · 1.5s"), "{text}");
        assert!(
            !text.contains("tok/s"),
            "published setting survives read failure: {text}"
        );
    }
    assert!(
        deck.tabs[0]
            .as_ref()
            .unwrap()
            .is_workspace_command("/old-command")
    );
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Manual
    );
    deck.reload_job = Some(tokio::spawn(async { Err(CoreError::Shutdown) }));
    apply_mcp_to_views(&mut state, &mut deck, current_mcp);
    state.handle_key(KeyAction::Cancel).await; // dismiss /reload autocomplete, retaining the draft
    state.handle_key(KeyAction::Commands).await;
    assert_eq!(state.panel(), &TuiPanel::Commands);
    state.handle_paste("mcp");
    state.handle_panel_key(KeyAction::Enter);
    assert_eq!(state.panel(), &TuiPanel::Mcps);
    assert!(
        state.modal_options().is_empty(),
        "hint is coalesced while rebuilding"
    );
    let refusal = deck.reload_job.take().unwrap().await.unwrap().unwrap_err();
    finish_reload_refusal(&app, &mut state, &mut deck, reload_error(refusal)).await;
    worker.await.unwrap();
    assert!(
        state
            .note()
            .unwrap()
            .contains("Configuration reload failed")
    );
    for view in [&mut state, deck.tabs[0].as_mut().unwrap()] {
        if view.panel() != &TuiPanel::Mcps {
            view.handle_key(KeyAction::Commands).await;
            view.handle_paste("mcp");
            view.handle_panel_key(KeyAction::Enter);
        }
        assert_eq!(view.panel(), &TuiPanel::Mcps);
        assert!(
            view.modal_options()
                .iter()
                .any(|row| { row.title == "safe-service" && row.footer.contains("Failed") })
        );
    }
    assert!(
        state
            .note()
            .unwrap()
            .contains("Configuration reload failed")
    );
    assert_eq!(state.input(), "/reload");
}

#[tokio::test]
async fn vis27_mismatched_published_reload_disables_copy_until_owner_catalog() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, _inbox, _) = CoreApp::channel(8);
    let mut current = catalog();
    current.chrome.location = Some("/B".into());
    current.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(current.clone());
    let mut deck = LoopState {
        location: Some("/B".into()),
        ..Default::default()
    };
    let error = finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/A".into(),
            generation: 2,
            catalog: current.clone(),
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(error.contains("Location changed"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Disabled);
    state.apply_catalog(current);
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
}

#[tokio::test]
async fn vis27_reload_projects_owner_mode_to_active_parked_and_home() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    old.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    old.chrome.session_tps = Some(true);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut home = TuiState::new_home(app.clone());
    home.apply_catalog(old.clone());
    deck.home = Some(home);

    let mut fresh = old.clone();
    fresh.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    fresh.chrome.session_tps = Some(false);
    let returned = fresh.clone();
    let worker = tokio::spawn(async move {
        for expected in ["first", "second"] {
            let Some(InboxMsg::SessionSelection { session, ack, .. }) = inbox.recv().await else {
                panic!("refresh session selection")
            };
            assert_eq!(session.0, expected);
            ack.send(Ok(fresh.clone())).unwrap();
        }
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("refresh Home selection")
        };
        ack.send(Ok(fresh.clone())).unwrap();
        let Some(InboxMsg::Catalog { ack }) = inbox.recv().await else {
            panic!("verify Location catalog")
        };
        ack.send(Ok(fresh)).unwrap();
        empty_mcp_status(&mut inbox).await;
        assert!(inbox.try_recv().is_err(), "reload does not save tab deck");
    });
    finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/fixture".into(),
            generation: 2,
            catalog: returned,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        None,
    )
    .await
    .unwrap();
    worker.await.unwrap();
    for view in [&state, deck.tabs[0].as_ref().unwrap()] {
        let text = view
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .map(oc_tui::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Build · Measured model · 1.5s"), "{text}");
        assert!(
            !text.contains("tok/s"),
            "fresh owner setting must replace warmed footer: {text}"
        );
        let meta = view
            .history()
            .rows()
            .iter()
            .find_map(|row| row.meta.as_ref())
            .unwrap();
        assert_eq!(meta.streamed_ms, Some(4000));
        assert_eq!(meta.output_tokens, Some(200));
    }
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Select
    );
    assert_eq!(
        deck.home.as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Select
    );
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    assert!(
        !state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
}

#[tokio::test]
async fn vis27_invalid_owner_location_does_not_replace_mode_or_write_tabs() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut current = catalog();
    current.chrome.location = Some("/A".into());
    current.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(current);
    let mut deck = LoopState {
        location: Some("/A".into()),
        ..Default::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SwitchLocationHome { ack, .. }) = inbox.recv().await else {
            panic!("owner Location switch")
        };
        ack.send(Err(CoreError::Application("invalid configuration".into())))
            .unwrap();
        let Some(InboxMsg::ReloadLocation { ack }) = inbox.recv().await else {
            panic!("owner reload")
        };
        ack.send(Err(CoreError::Application("invalid configuration".into())))
            .unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "failed config must not save tabs"
        );
    });
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchLocation { path: "/B".into() },
        )
        .await
        .is_err()
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ReloadConfiguration,
    )
    .await
    .unwrap();
    assert!(deck.reload_job.take().unwrap().await.unwrap().is_err());
    worker.await.unwrap();
    assert_eq!(state.chrome.location.as_deref(), Some("/A"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.location.as_deref(), Some("/A"));
}

#[tokio::test]
async fn published_location_uses_new_owner_binding_and_token_for_next_save() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut b_catalog = catalog();
    b_catalog.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    let b_session_catalog = b_catalog.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read B preference")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/B".into(),
            revision: Some("B-loaded".into()),
            new_session_titles: Vec::new(),
            sessions: vec![SessionId::new("b-root").unwrap()],
            active: None,
        }))
        .unwrap();
        let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
            panic!("read B root")
        };
        assert_eq!(session.0, "b-root");
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("read B catalog")
        };
        ack.send(Ok(b_session_catalog)).unwrap();
        empty_compactions(&mut inbox).await;
        empty_mcp_status_at(&mut inbox, "/B").await;
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("save B route")
        };
        assert_eq!(deck.location, "/B");
        assert_eq!(deck.revision.as_deref(), Some("B-loaded"));
        assert_eq!(deck.sessions, vec![SessionId::new("b-root").unwrap()]);
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("B-saved".into()),
            ..deck
        }))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    let mut state = TuiState::new(app.clone(), SessionId::new("a-root").unwrap());
    let mut a_catalog = catalog();
    a_catalog.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    state.apply_catalog(a_catalog);
    let mut deck = LoopState {
        location: Some("/A".into()),
        revision: Some("A-stale".into()),
        tabs: vec![None],
        tab_cards_before: vec![None],
        active_tab: Some(0),
        ..Default::default()
    };
    adopt_location(&app, &mut state, &mut deck, b_catalog, "/B").await;
    assert!(state.attached_session().is_none());
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.location.as_deref(), Some("/B"));
    assert_eq!(deck.revision.as_deref(), Some("B-loaded"));
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "b-root");
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.revision.as_deref(), Some("B-saved"));
    worker.await.unwrap();
}

#[tokio::test]
async fn close_inactive_tab_reindexes_active_view_and_cursor_without_owner_query() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("first draft");
    state.attach_page(&HistoryPage {
        rows: vec![HistoryMessage {
            seq: 1,
            role: Role::User,
            id: oc_core::session::MessageId("viewport-user".into()),
            text: "first viewport marker".into(),
            turn: None,
            model_switch: None,
            user_shell: None,
        }],
        total: 1,
        ..Default::default()
    });
    deck.cards_before = Some(11);
    append_tab(&app, &mut deck, &mut state, "b");
    state.handle_paste("middle draft");
    deck.cards_before = Some(22);
    append_tab(&app, &mut deck, &mut state, "c");
    state.handle_paste("active draft");
    deck.cards_before = Some(33);

    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(deck.tab_cards_before, [Some(11), None]);
    assert_eq!(deck.cards_before, Some(33));
    assert_eq!(state.input(), "active draft");
    assert_eq!(state.tab_presentation().0.len(), 2);
    assert_eq!(state.tab_presentation().1, 1);
    assert!(inbox.try_recv().is_err(), "close cannot delete a session");
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "first draft");
    assert!(
        state
            .viewport()
            .join("\n")
            .contains("first viewport marker")
    );
    assert_eq!(deck.cards_before, Some(11));
}

#[tokio::test]
async fn close_last_real_tab_queries_current_home_before_discarding_and_reopens() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("durable").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("saved draft");
    let worker = tokio::spawn(async move {
        for result in [Err(CoreError::Shutdown), Ok(catalog())] {
            let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
                panic!("close must query Home only")
            };
            assert_eq!(action, SelectionAction::Current);
            ack.send(result).unwrap();
        }
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("Sessions reopen must query existing selection")
        };
        assert_eq!(session.0, "durable");
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: String::new(),
            catalog: catalog(),
            page: Default::default(),
            deck: TabDeckSnapshot {
                sessions: vec![session.clone()],
                active: Some(session),
                ..Default::default()
            },
            previous_deck: old_deck,
        }))
        .unwrap();
    });
    let intent = PanelIntent::CloseTab { index: 0 };
    assert!(
        apply_intent(&app, &mut state, &mut deck, intent.clone())
            .await
            .is_err()
    );
    assert_eq!(state.input(), "saved draft");
    assert_eq!(state.session().0, "durable");
    assert_eq!(deck.tabs.len(), 1);
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert!(state.attached_session().is_none());
    assert_eq!(deck.active_tab, None);
    assert!(deck.tabs.is_empty() && deck.tab_cards_before.is_empty());
    assert!(state.tab_presentation().0.is_empty());
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession {
            id: "durable".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "durable");
    assert_eq!(deck.tabs.len(), 1);
    assert_eq!(deck.active_tab, Some(0));
    assert!(deck.home.is_some(), "reopening parks synthetic Home");
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert!(state.attached_session().is_none());
    assert!(deck.home.is_none() && deck.tabs.is_empty());
    worker.await.unwrap();
}

#[tokio::test]
async fn closing_last_real_tab_restores_parked_home_draft_without_query() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    state.handle_paste("parked Home draft");
    deck.activate(&mut state, 0).unwrap();
    assert!(deck.home.is_some());
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert!(state.attached_session().is_none());
    assert_eq!(state.input(), "parked Home draft");
    assert!(deck.tabs.is_empty() && deck.home.is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn sessions_query_uses_owner_metadata_and_scope_without_legacy_id_enumeration() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("root").unwrap());
    let mut deck = LoopState::default();
    state.handle_paste("/sessions");
    let mut input_history = Vec::new();
    tokio::join!(
        state.handle_key(KeyAction::Enter),
        accept_prompt_input(&mut inbox, &mut input_history, "/sessions")
    );
    state.handle_panel_key(KeyAction::Char('N'));
    state.handle_panel_key(KeyAction::CtrlA);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SessionPickerContext { all_projects, ack }) = inbox.recv().await else {
            panic!("expected picker preferences");
        };
        assert_eq!(all_projects, Some(false));
        ack.send(Ok(oc_core::queries::SessionPickerContext {
            all_projects: false,
            project_name: Some("project".into()),
            canonical: Some("/project".into()),
        }))
        .unwrap();
        let Some(InboxMsg::SessionList {
            search,
            all_projects,
            ack,
        }) = inbox.recv().await
        else {
            panic!("expected bounded metadata query");
        };
        assert_eq!(search, "N");
        assert!(!all_projects);
        ack.send(Ok(vec![oc_core::queries::SessionListEntry {
            id: SessionId("root".into()),
            title: "Named owner title".into(),
            directory: Some("/project".into()),
            created_at: "1".into(),
            updated_at: Some("2".into()),
            date_group: "Today".into(),
            running: false,
            worktree: None,
        }]))
        .unwrap();
    });
    apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert_eq!(state.modal_options()[0].title, "Named owner title");
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        Some(PanelIntent::SwitchSession { id: "root".into() })
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn picker_open_refusal_keeps_old_view_and_accepted_foreign_receipt_survives_refresh_failure()
{
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.restore_prompt("kept A draft".into());
    let mut deck = LoopState {
        location: Some("/a".into()),
        revision: Some("old".into()),
        ..LoopState::default()
    };
    deck.sync_tabs(&mut state);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker open")
        };
        assert_eq!(session.0, "b");
        assert_eq!(old_deck.location, "/a");
        ack.send(Err(CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            detail: "invalid target".into(),
            diagnostic: None,
        }))
        .unwrap();
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker open")
        };
        let mut target = catalog();
        target.chrome.location = Some("/b".into());
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: "/b".into(),
            catalog: target,
            page: Default::default(),
            deck: TabDeckSnapshot {
                location: "/b".into(),
                revision: Some("accepted-b".into()),
                new_session_titles: vec![true, false],
                sessions: vec![session.clone(), SessionId("parked-b".into())],
                active: Some(session),
            },
            previous_deck: old_deck,
        }))
        .unwrap();
        empty_compactions(&mut inbox).await;
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("parked history")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("parked selection")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("return route")
        };
        assert_eq!(old_deck.sessions.len(), 2);
        assert_eq!(session.0, "a");
        let mut target = catalog();
        target.chrome.location = Some("/a".into());
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: "/a".into(),
            catalog: target,
            page: Default::default(),
            deck: TabDeckSnapshot {
                location: "/a".into(),
                revision: Some("accepted-a".into()),
                new_session_titles: vec![true],
                sessions: vec![session.clone()],
                active: Some(session),
            },
            previous_deck: old_deck,
        }))
        .unwrap();
        empty_compactions(&mut inbox).await;
    });
    let intent = PanelIntent::SwitchSession { id: "b".into() };
    assert!(
        apply_intent(&app, &mut state, &mut deck, intent.clone())
            .await
            .is_err()
    );
    assert_eq!(state.session().0, "a");
    assert_eq!(state.input(), "kept A draft");
    assert_eq!(deck.location.as_deref(), Some("/a"));
    assert_eq!(deck.revision.as_deref(), Some("old"));
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert_eq!(state.session().0, "b");
    assert_eq!(deck.location.as_deref(), Some("/b"));
    assert_eq!(deck.revision.as_deref(), Some("accepted-b"));
    assert_eq!(deck.tabs.len(), 2);
    assert!(
        deck.picker_pending_tabs
            .contains(&SessionId("parked-b".into()))
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession { id: "a".into() },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "a");
    assert_eq!(state.input(), "kept A draft");
    assert_eq!(deck.tabs.len(), 1);
    worker.await.unwrap();
}

#[tokio::test]
async fn picker_open_preserves_child_read_only_policy_without_owner_or_deck_changes() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    state.attach_page(&oc_core::queries::HistoryPage {
        parent_id: Some("parent".into()),
        ..Default::default()
    });
    let mut deck = LoopState {
        location: Some("/a".into()),
        read_only: true,
        save_disabled: true,
        ..LoopState::default()
    };
    deck.sync_tabs(&mut state);
    let before = deck.snapshot(&state);
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchSession {
                id: "parent".into(),
            },
        )
        .await
        .is_err()
    );
    assert_eq!(state.session().0, "child");
    assert!(deck.read_only && deck.save_disabled);
    assert_eq!(deck.snapshot(&state), before);
    assert!(matches!(
        inbox.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn picker_open_existing_tab_keeps_older_history_window_and_draft() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let row = |seq| oc_core::queries::HistoryMessage {
        id: oc_core::session::MessageId(format!("m{seq}")),
        seq,
        role: Role::User,
        text: format!("row {seq}"),
        turn: None,
        model_switch: None,
        user_shell: None,
    };
    let mut state = TuiState::new(app.clone(), SessionId("kept".into()));
    state.attach_page(&oc_core::queries::HistoryPage {
        rows: vec![row(1)],
        total: 200,
        has_newer: true,
        ..Default::default()
    });
    state.restore_prompt("retained draft".into());
    let mut deck = LoopState {
        location: Some("/a".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker route")
        };
        let mut catalog = catalog();
        catalog.chrome.location = Some("/a".into());
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session,
            location: "/a".into(),
            catalog,
            page: oc_core::queries::HistoryPage {
                rows: vec![row(200)],
                total: 200,
                has_older: true,
                title: Some("real title".into()),
                ..Default::default()
            },
            deck: old_deck.clone(),
            previous_deck: old_deck,
        }))
        .unwrap();
    });
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession { id: "kept".into() },
    )
    .await
    .unwrap();
    assert_eq!(state.history().rows()[0].seq, 1);
    assert!(state.history().has_newer());
    assert_eq!(state.input(), "retained draft");
    assert_eq!(state.session_title.as_deref(), Some("real title"));
    worker.await.unwrap();
}

#[tokio::test]
async fn sessions_retained_views_read_latest_scope_and_only_ctrl_a_writes() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut a = TuiState::new(app.clone(), SessionId("a".into()));
    let mut b = TuiState::new(app.clone(), SessionId("b".into()));
    let mut deck = LoopState::default();
    let worker = tokio::spawn(async move {
        let mut scope = false;
        let mut input_history = Vec::new();
        for (index, expected) in [None, None, Some(true), None, Some(false), None]
            .into_iter()
            .enumerate()
        {
            if matches!(index, 0 | 1 | 3 | 5) {
                accept_prompt_input(&mut inbox, &mut input_history, "/sessions").await;
            }
            let Some(InboxMsg::SessionPickerContext { all_projects, ack }) = inbox.recv().await
            else {
                panic!("scope")
            };
            assert_eq!(
                all_projects, expected,
                "only the explicit toggle may persist"
            );
            if let Some(value) = all_projects {
                scope = value;
            }
            ack.send(Ok(oc_core::queries::SessionPickerContext {
                all_projects: scope,
                project_name: None,
                canonical: None,
            }))
            .unwrap();
            let Some(InboxMsg::SessionList {
                all_projects, ack, ..
            }) = inbox.recv().await
            else {
                panic!("query")
            };
            assert_eq!(all_projects, scope);
            ack.send(Ok(Vec::new())).unwrap();
        }
    });
    for state in [&mut a, &mut b] {
        state.handle_paste("/sessions");
        state.handle_key(KeyAction::Enter).await;
        apply_intent(&app, state, &mut deck, PanelIntent::LoadSessions)
            .await
            .unwrap();
    }
    a.handle_panel_key(KeyAction::CtrlA);
    apply_intent(&app, &mut a, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    b.close_panel();
    b.handle_paste("/sessions");
    b.handle_key(KeyAction::Enter).await;
    apply_intent(&app, &mut b, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert!(b.sessions_all_projects());
    b.handle_panel_key(KeyAction::CtrlA);
    apply_intent(&app, &mut b, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    a.close_panel();
    a.handle_paste("/sessions");
    a.handle_key(KeyAction::Enter).await;
    apply_intent(&app, &mut a, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert!(!a.sessions_all_projects());
    worker.await.unwrap();
}

#[tokio::test]
async fn v03_catalog_failure_is_not_an_empty_usable_session() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::Create { ack, .. }) = inbox.recv().await else {
            panic!("create")
        };
        ack.send(Ok(())).unwrap();
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("history")
        };
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("catalog")
        };
        ack.send(Err(oc_core::session::CoreError::Shutdown))
            .unwrap();
    });
    let result = initial_state(&app, Some(SessionId::new("catalog-failure").unwrap())).await;
    assert!(
        matches!(result, Err(StartupFailure::QueryDiagnostic(diagnostic))
        if diagnostic.code == oc_core::queries::ServiceCode::QueryFailed)
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn bare_home_queries_selection_without_creating_or_reading_history() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
            panic!("Home must query selection first")
        };
        assert_eq!(action, SelectionAction::Current);
        ack.send(Err(CoreError::Shutdown)).unwrap();
        assert!(inbox.try_recv().is_err(), "no root or history query");
    });
    assert!(matches!(
        initial_state(&app, None).await,
        Err(StartupFailure::QueryDiagnostic(diagnostic))
            if diagnostic.code == oc_core::queries::ServiceCode::QueryFailed
    ));
    worker.await.unwrap();
}

#[tokio::test]
async fn home_refuses_session_scoped_queries() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    let mut loop_state = LoopState::default();
    for intent in [
        PanelIntent::LoadCards,
        PanelIntent::LoadCardOutput {
            op: "op".into(),
            offset: 0,
        },
        PanelIntent::LoadOlder,
        PanelIntent::LoadNewer,
        PanelIntent::Compress {
            focus: String::new(),
        },
    ] {
        assert!(
            apply_intent(&app, &mut state, &mut loop_state, intent)
                .await
                .unwrap_err()
                .contains("no active session")
        );
        assert!(
            inbox.try_recv().is_err(),
            "no session-scoped query reached worker"
        );
    }
}
