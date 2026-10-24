use super::*;

#[tokio::test]
async fn first_picker_rows_retain_enter_rename_and_unconfirmed_delete() {
    use oc_core::queries::{SessionListEntry, SessionPickerContext};
    for action in [KeyAction::Enter, KeyAction::Rename, KeyAction::DeleteOrQuit] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app.clone(), SessionId("source".into()));
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.restore_prompt("/sessions".into());
        let outcome = state.handle_key(KeyAction::Enter).await;
        apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
        let InboxMsg::PromptHistory { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("history")
        };
        ack.send(Ok(vec!["/sessions".into()])).unwrap();
        let InboxMsg::SessionPickerContext { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("context")
        };
        ack.send(Ok(SessionPickerContext {
            all_projects: false,
            project_name: None,
            canonical: None,
        }))
        .unwrap();
        let InboxMsg::SessionList { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("held first rows")
        };
        let key = match action {
            KeyAction::Enter => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyAction::Rename => KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            _ => KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        };
        handle_event(&app, &mut state, &mut deck, CEvent::Key(key))
            .await
            .unwrap();
        assert!(inbox.try_recv().is_err());
        ack.send(Ok(vec![SessionListEntry {
            id: SessionId("target".into()),
            title: "Prepared title".into(),
            directory: None,
            created_at: String::new(),
            updated_at: None,
            date_group: "Today".into(),
            running: false,
            worktree: None,
        }]))
        .unwrap();
        super::settle_picker(&app, &mut state, &mut deck).await;
        match action {
            KeyAction::Enter => {
                let InboxMsg::OpenPickerSession { session, ack, .. } = inbox.recv().await.unwrap()
                else {
                    panic!("prepared open")
                };
                assert_eq!(session.0, "target");
                ack.send(Err(CoreError::TurnBusy)).unwrap();
                super::settle_navigation(&app, &mut state, &mut deck).await;
            }
            KeyAction::Rename => assert_eq!(*state.panel(), TuiPanel::Rename),
            _ => {
                assert_eq!(*state.panel(), TuiPanel::Sessions);
                assert!(
                    oc_tui::views::render_test(&state, 100, 32)
                        .join("\n")
                        .contains("confirm")
                );
            }
        }
        assert!(
            inbox.try_recv().is_err(),
            "no unconfirmed delete or repeated action"
        );
    }
}

#[tokio::test]
async fn location_command_waits_for_success_and_preserves_newer_source_revision() {
    for (refuse, edit, typed) in [
        (true, false, true),
        (false, false, true),
        (false, true, true),
        (false, false, false),
    ] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut facts = catalog();
        facts.chrome.location = Some("/a".into());
        let mut state = TuiState::new_home(app.clone());
        state.apply_catalog(facts.clone());
        let input = if typed {
            "/location /b"
        } else {
            "ordinary unsent draft"
        };
        state.restore_prompt(input.into());
        let mut deck = LoopState {
            location: Some("/a".into()),
            ..Default::default()
        };
        let outcome = KeyOutcome {
            intent: Some(PanelIntent::SwitchLocation { path: "/b".into() }),
            ..Default::default()
        };
        apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
        assert_eq!(state.input(), input);
        let InboxMsg::SwitchLocationHome { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("captured switch")
        };
        if edit {
            state.restore_prompt("new source draft".into());
        }
        if refuse {
            ack.send(Err(CoreError::TurnBusy)).unwrap();
        } else {
            facts.chrome.location = Some("/b".into());
            ack.send(Ok(oc_core::queries::HomeLocationSnapshot {
                location: "/b".into(),
                generation: 1,
                catalog: facts,
                diagnostics: Vec::new(),
                notices: Vec::new(),
            }))
            .unwrap();
            let InboxMsg::TabDeck { ack } = inbox.recv().await.unwrap() else {
                panic!("new deck")
            };
            ack.send(Ok(TabDeckSnapshot {
                location: "/b".into(),
                revision: Some("b".into()),
                ..Default::default()
            }))
            .unwrap();
        }
        super::settle_navigation(&app, &mut state, &mut deck).await;
        if refuse {
            assert_eq!(state.input(), input);
        } else {
            assert_eq!(state.note(), Some("location: /b"));
            let mut retained = TuiState::new_home(app.clone());
            retained.restore_composer_draft(deck.picker_drafts["/a"][0].1.clone());
            assert_eq!(
                retained.input(),
                if edit {
                    "new source draft"
                } else if typed {
                    ""
                } else {
                    input
                }
            );
        }
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn native_quit_settles_same_batch_new_home_and_selection_once() {
    for refuse in [false, true] {
        let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(8, 73);
        let mut facts = catalog();
        facts.chrome.location = Some("/fixture".into());
        let mut state = TuiState::new(app.clone(), SessionId("root".into()));
        state.apply_catalog(facts.clone());
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            revision: Some("old".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        deck.authoritative_deck = Some(deck.snapshot(&state));
        apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
            .await
            .unwrap();
        state.handle_key(KeyAction::Quit).await;
        let owner = tokio::spawn(async move {
            let InboxMsg::HomeSelection { ack, .. } = inbox.recv().await.unwrap() else {
                panic!("issued Home selection")
            };
            ack.send(Ok(facts)).unwrap();
            let InboxMsg::SaveTabDeck { deck, ack } = inbox.recv().await.unwrap() else {
                panic!("selection CAS")
            };
            assert_eq!(deck.sessions, vec![SessionId("root".into())]);
            assert_eq!(deck.active, None);
            assert_eq!(deck.revision.as_deref(), Some("old"));
            ack.send(if refuse {
                Err(CoreError::TabDeckConflict)
            } else {
                Ok(TabDeckSnapshot {
                    revision: Some("saved".into()),
                    ..deck
                })
            })
            .unwrap();
            inbox
        });
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            reconcile_exit(&app, &mut state, &mut deck),
        )
        .await
        .unwrap();
        if refuse {
            assert!(result.unwrap_err().contains("not replayed"));
            assert!(deck.selection_blocked);
            assert_eq!(deck.revision.as_deref(), Some("old"));
        } else {
            result.unwrap();
            assert_eq!(deck.revision.as_deref(), Some("saved"));
            assert!(deck.unresolved_homes.is_empty());
            assert!(!deck.selection_dirty);
        }
        assert!(
            owner.await.unwrap().try_recv().is_err(),
            "no extra save or generation at shutdown"
        );
    }
}

#[tokio::test]
async fn native_quit_releases_lease_instead_of_waiting_behind_unaccepted_fresh_admission() {
    for shell in [false, true] {
        let (app, mut inbox, _control, _, _admissions) = CoreApp::native_channel(16, 74);
        let mut source = TuiState::new_home(app.clone());
        source.chrome.location = Some("/fixture".into());
        if shell {
            source.handle_key(KeyAction::Char('!')).await;
        }
        source.restore_prompt("unaccepted ask".into());
        source.handle_key(KeyAction::Enter).await;
        let admission = inbox.recv().await.unwrap();
        assert!(matches!(&admission, InboxMsg::UserShell { .. }) == shell);
        assert!(source.attached_session().is_none() && source.has_pending_submission());
        let mut state = TuiState::new_home(app.clone());
        state.chrome.location = Some("/fixture".into());
        let mut deck = LoopState {
            tabs: vec![Some(source)],
            tab_cards_before: vec![None],
            location: Some("/fixture".into()),
            revision: Some("old".into()),
            ..Default::default()
        };
        deck.home_pending = Some((state.view_identity(), SelectionAction::Current));
        sync_home(&app, &mut state, &mut deck).await;
        let home = inbox.recv().await.unwrap();
        assert!(matches!(&home, InboxMsg::HomeSelection { .. }));
        let mut requested = deck.snapshot(&state);
        requested.rail_width = Some(24);
        deck_jobs::start(&app, &mut deck, requested, deck_jobs::Effect::Selection).unwrap();
        let selection = inbox.recv().await.unwrap();
        assert!(matches!(&selection, InboxMsg::SaveTabDeck { .. }));
        deck.shared_refresh = true;
        shared::sync(&app, &mut state, &mut deck).await.unwrap();
        let workspace = inbox.recv().await.unwrap();
        assert!(matches!(&workspace, InboxMsg::Workspace { .. }));
        state.handle_key(KeyAction::Quit).await;
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            reconcile_exit(&app, &mut state, &mut deck),
        )
        .await
        .expect("Quit cannot wait for lanes behind the fresh Ask's deck gate");
        assert!(result.unwrap_err().contains("release viewer lease"));
        assert_eq!(deck.revision.as_deref(), Some("old"));
        assert!(deck.tabs[0].as_ref().unwrap().has_pending_submission());
        if let InboxMsg::UserShell { cancel, .. } = &admission {
            assert!(!cancel.load(std::sync::atomic::Ordering::SeqCst));
        }
        assert!(
            inbox.try_recv().is_err(),
            "no cancellation, replay, or replacement request"
        );
        drop((admission, home, selection, workspace));
    }
}

#[tokio::test]
async fn unresolved_home_submit_guard_does_not_eat_connection_dialog_enter() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    let mut deck = LoopState::default();
    deck.unresolved_homes.insert(state.view_identity().0);
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("connect");
    state.handle_key(KeyAction::Enter).await;
    state.apply_provider_connections(Ok(Vec::new()));
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    assert!(
        oc_tui::views::render_test(&state, 100, 32)
            .join("\n")
            .contains("Display name")
    );
    assert!(
        inbox.try_recv().is_err(),
        "dialog Enter is not a prompt or guessed selection"
    );
}

#[tokio::test]
async fn held_home_selection_survives_park_and_resume_without_replaying_enter() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut facts = catalog();
    facts.chrome.location = Some("/fixture".into());
    let mut state = TuiState::new(app.clone(), SessionId("root".into()));
    state.apply_catalog(facts.clone());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    let original = state.view_identity();
    sync_home(&app, &mut state, &mut deck).await;
    let InboxMsg::HomeSelection { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("captured Home selection")
    };
    deck.activate(&mut state, 0).unwrap();
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.view_identity().0, original.0);
    assert_ne!(state.view_identity().1, original.1);
    state.handle_paste("kept next prompt");
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    assert_eq!(state.input(), "kept next prompt");
    assert!(inbox.try_recv().is_err());
    ack.send(Ok(facts)).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.unresolved_homes.is_empty() {
            sync_home(&app, &mut state, &mut deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state.catalog_loaded());
    assert_eq!(state.input(), "kept next prompt");
    assert!(
        inbox.try_recv().is_err(),
        "no selection replay or automatic prompt submit"
    );
}

#[tokio::test]
async fn rename_local_refusal_releases_only_pending_marker_for_same_draft_retry() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("root".into()));
    state.apply_catalog(catalog());
    state.restore_prompt("/rename exact new title".into());
    let mut deck = LoopState {
        scope_uncertain: true,
        ..Default::default()
    };
    for _ in 0..2 {
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert!(matches!(
            outcome.intent,
            Some(PanelIntent::RenameSessionDirect { .. })
        ));
        apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
        assert_eq!(state.input(), "/rename exact new title");
    }
    for _ in 0..2 {
        let InboxMsg::PromptHistory { append, ack } = inbox.recv().await.unwrap() else {
            panic!("only slash history, not a rename effect")
        };
        assert_eq!(append.as_deref(), Some("/rename exact new title"));
        ack.send(Ok(vec!["/rename exact new title".into()]))
            .unwrap();
    }
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn held_picker_search_enter_uses_prepared_rows_and_newer_navigation_cancels_the_intent() {
    use oc_core::queries::{SessionListEntry, SessionPickerContext};
    let entry = |id: &str, title: &str| SessionListEntry {
        id: SessionId(id.into()),
        title: title.into(),
        directory: None,
        created_at: String::new(),
        updated_at: None,
        date_group: "Today".into(),
        running: false,
        worktree: None,
    };
    let context = || SessionPickerContext {
        all_projects: false,
        project_name: None,
        canonical: None,
    };
    for navigate in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app.clone(), SessionId("source".into()));
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        append_tab(&app, &mut deck, &mut state, "target");
        deck.activate(&mut state, 0).unwrap();
        state.restore_prompt("/sessions".into());
        let outcome = state.handle_key(KeyAction::Enter).await;
        apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
        let InboxMsg::PromptHistory { append, ack } = inbox.recv().await.unwrap() else {
            panic!("auxiliary command history")
        };
        assert_eq!(append.as_deref(), Some("/sessions"));
        ack.send(Ok(vec!["/sessions".into()])).unwrap();
        let InboxMsg::SessionPickerContext { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("initial picker context")
        };
        ack.send(Ok(context())).unwrap();
        let InboxMsg::SessionList { search, ack, .. } = inbox.recv().await.unwrap() else {
            panic!("initial rows")
        };
        assert!(search.is_empty());
        ack.send(Ok(vec![
            entry("source", "wrong root"),
            entry("target", "right root"),
        ]))
        .unwrap();
        super::settle_picker(&app, &mut state, &mut deck).await;
        state.restore_prompt("independent source draft".into());
        let outcome = state.handle_paste("right");
        apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
        let InboxMsg::SessionPickerContext { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("search context")
        };
        ack.send(Ok(context())).unwrap();
        let InboxMsg::SessionList {
            search, ack: held, ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("held filtered rows")
        };
        assert_eq!(search, "right");
        tokio::time::timeout(Duration::from_millis(100), async {
            let outcome = state.handle_key(KeyAction::Enter).await;
            assert!(matches!(
                outcome.intent,
                Some(PanelIntent::SwitchSession { .. })
            ));
            apply_outcome(&app, &mut state, &mut deck, outcome, false).await;
            handle_event(&app, &mut state, &mut deck, CEvent::Resize(100, 32))
                .await
                .unwrap();
            assert!(
                oc_tui::views::render_test(&state, 100, 32)
                    .join("\n")
                    .contains("right")
            );
        })
        .await
        .expect("Enter and first frame cannot wait for the picker owner");
        assert!(
            inbox.try_recv().is_err(),
            "no open from old rows/new search"
        );
        if navigate {
            deck.activate(&mut state, 1).unwrap();
            responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
        }
        held.send(Ok(vec![entry("target", "right root")])).unwrap();
        super::settle_picker(&app, &mut state, &mut deck).await;
        if navigate {
            assert!(deck.navigation_job.is_none());
            assert_eq!(state.session().0, "target");
            assert_eq!(state.input(), "Ж");
        } else {
            let InboxMsg::OpenPickerSession {
                session,
                search,
                ack,
                ..
            } = inbox.recv().await.unwrap()
            else {
                panic!("one exact prepared open")
            };
            assert_eq!(session.0, "target");
            assert_eq!(search, "right");
            ack.send(Err(CoreError::TurnBusy)).unwrap();
            super::settle_navigation(&app, &mut state, &mut deck).await;
            assert_eq!(state.session().0, "source");
            assert_eq!(state.input(), "independent source draft");
        }
        assert!(
            inbox.try_recv().is_err(),
            "no guessed open or effect replay"
        );
    }
}

#[tokio::test]
async fn provider_publication_keeps_original_model_receipt_until_its_exact_late_ack() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    old.chrome.selection_generation = 1;
    old.models.push(oc_core::queries::ModelEntry {
        id: old.model_id.clone(),
        ..Default::default()
    });
    let mut state = TuiState::new(app.clone(), SessionId("source".into()));
    state.apply_catalog(old.clone());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection {
        action: SelectionAction::Commit(original),
        ack: commit,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("original model receipt")
    };
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    assert!(state.has_pending_model_commit());
    deck.catalogs
        .request(&app, &mut state, catalog_jobs::Request::Provider);
    let InboxMsg::SessionSelection {
        action: SelectionAction::Current,
        ack: facts,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("prepared provider publication")
    };
    responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
    let mut next = old.clone();
    next.provider = "other".into();
    next.model_id = "other/model".into();
    next.chrome.selection_generation = 2;
    next.models = vec![oc_core::queries::ModelEntry {
        id: next.model_id.clone(),
        ..Default::default()
    }];
    facts.send(Ok(next)).unwrap();
    super::settle_catalog(&app, &mut state, &mut deck).await;
    assert!(
        state.has_pending_model_commit(),
        "new provider facts cannot discard the original owner receipt"
    );
    let current = state.captured_model_commit().unwrap();
    assert_ne!(current.binding.provider, original.binding.provider);
    assert!(deck.close_tab(&app, &mut state, 0).await.is_err());
    commit.send(Ok(old)).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(!state.has_pending_model_commit());
    assert_eq!(
        state.captured_model_commit().unwrap(),
        current,
        "old caller ACK cannot acknowledge or erase the new binding"
    );
    assert_eq!(state.input(), "Ж");
    assert!(
        inbox.try_recv().is_err(),
        "neither commit nor generation is replayed"
    );
}

#[tokio::test]
async fn withdrawal_holds_original_payload_and_merges_later_typing_on_parked_source() {
    use oc_core::queries::{InboxAction, InboxDelivery, InboxItem, InboxSnapshot};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.restore_prompt("next draft".into());
    let item = InboxItem {
        id: "queued".into(),
        delivery: InboxDelivery::Queue,
        text: "withdrawn".into(),
        images: 0,
        error: None,
    };
    state.apply_inbox(&SessionId("a".into()), vec![item.clone()]);
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    tokio::time::timeout(
        Duration::from_millis(100),
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::Inbox {
                action: InboxAction::Remove {
                    id: item.id.clone(),
                },
                restore: true,
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let InboxMsg::Inbox {
        session,
        action: InboxAction::Read { id },
        ack: peek,
    } = inbox.recv().await.unwrap()
    else {
        panic!("withdrawal preflight")
    };
    assert_eq!(session.0, "a");
    assert_eq!(id, item.id);
    responsive_frame(&app, &mut state, &mut deck, 'Ω').await;
    peek.send(Ok(InboxSnapshot {
        prompt: Some("withdrawn".into()),
        ..Default::default()
    }))
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.inbox_jobs.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    inbox_jobs::sync(&app, &mut state, &mut deck).await;
    let InboxMsg::Inbox {
        session,
        action: InboxAction::Remove { id },
        ack: remove,
    } = inbox.recv().await.unwrap()
    else {
        panic!("exact removal after local budget preflight")
    };
    assert_eq!(session.0, "a");
    assert_eq!(id, item.id);
    assert!(deck.close_tab(&app, &mut state, 0).await.is_err());
    append_tab(&app, &mut deck, &mut state, "b");
    responsive_frame(&app, &mut state, &mut deck, 'Й').await;
    let identity = state.view_identity();
    remove
        .send(Ok(InboxSnapshot {
            removed: Some(item),
            prompt: Some("withdrawn".into()),
            ..Default::default()
        }))
        .unwrap();
    super::settle_inbox(&app, &mut state, &mut deck).await;
    assert_eq!(state.view_identity(), identity);
    assert_eq!(state.input(), "Й");
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().input(),
        "next draftΩ\nwithdrawn"
    );
    assert!(deck.tabs[0].as_ref().unwrap().inbox_items().is_empty());
    assert!(
        inbox.try_recv().is_err(),
        "no repeated withdrawal or native admission"
    );
}

#[tokio::test]
async fn successful_reload_preserves_failed_original_request_for_explicit_recovery() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut facts = catalog();
    facts.chrome.location = Some("/fixture".into());
    facts.chrome.selection_generation = 1;
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(facts.clone());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    state.handle_paste("retained original payload");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SubmitFresh { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("original receipt")
    };
    ack.send(Err(CoreError::TurnNotActive)).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(state.has_occupied_submission() && !state.has_pending_submission());
    let route = state.tab_route_session().unwrap().clone();
    facts.chrome.selection_generation = 2;
    let returned = facts.clone();
    let owner = tokio::spawn(async move {
        let InboxMsg::HomeSelection {
            action: SelectionAction::Current,
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("provisional stays sessionless during reload")
        };
        ack.send(Ok(returned.clone())).unwrap();
        let InboxMsg::Catalog { ack } = inbox.recv().await.unwrap() else {
            panic!("owner binding verification")
        };
        ack.send(Ok(returned)).unwrap();
        empty_mcp_status(&mut inbox).await;
        inbox
    });
    finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/fixture".into(),
            generation: 2,
            catalog: facts,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        None,
    )
    .await
    .unwrap();
    super::settle_reload(&app, &mut state, &mut deck).await;
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(
        state.has_occupied_submission(),
        "configuration generation cannot discard a failed owned draft"
    );
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("settings");
    state.handle_panel_key(KeyAction::Enter);
    state.handle_paste("last request");
    assert_eq!(state.modal_options()[0].value, "submission-refusal");
    state.handle_panel_key(KeyAction::QueuedUndo);
    assert_eq!(state.input(), "retained original payload");
    assert_eq!(state.tab_route_session(), Some(&route));
    assert!(!state.has_occupied_submission());
    let mut inbox = owner.await.unwrap();
    assert!(
        inbox.try_recv().is_err(),
        "recovery never replays the request"
    );
}

#[tokio::test]
async fn late_foreign_open_restores_known_scope_without_replacing_newer_local_home() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut facts = catalog();
    facts.chrome.location = Some("/a".into());
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.apply_catalog(facts.clone());
    state.restore_prompt("root A draft".into());
    let mut deck = LoopState {
        location: Some("/a".into()),
        revision: Some("before".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    let root_identity = state.view_identity().0;
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession {
            id: "foreign-b".into(),
        },
    )
    .await
    .unwrap();
    let InboxMsg::OpenPickerSession {
        old_deck,
        ack: open,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("one captured open")
    };
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
    let home_identity = state.view_identity();
    sync_home(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "new Home effect waits for its original Location context"
    );
    let mut foreign = facts.clone();
    foreign.chrome.location = Some("/b".into());
    open.send(Ok(oc_core::queries::SessionPickerOpen {
        session: SessionId("foreign-b".into()),
        location: "/b".into(),
        catalog: foreign,
        page: Default::default(),
        previous_deck: old_deck,
        deck: TabDeckSnapshot {
            location: "/b".into(),
            revision: Some("b-opened".into()),
            sessions: vec![SessionId("foreign-b".into())],
            active: Some(SessionId("foreign-b".into())),
            ..Default::default()
        },
    }))
    .unwrap();
    empty_compactions(&mut inbox).await;
    let returned = facts.clone();
    let owner = tokio::spawn(async move {
        let InboxMsg::SwitchLocationHome { path, ack } = inbox.recv().await.unwrap() else {
            panic!("restore known A context, never repeat foreign open")
        };
        assert_eq!(path, "/a");
        ack.send(Ok(oc_core::queries::HomeLocationSnapshot {
            location: "/a".into(),
            generation: 2,
            catalog: returned,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        }))
        .unwrap();
        let InboxMsg::TabDeck { ack } = inbox.recv().await.unwrap() else {
            panic!("authoritative return token")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/a".into(),
            revision: Some("a-restored".into()),
            sessions: vec![SessionId("a".into())],
            active: None,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    super::settle_navigation(&app, &mut state, &mut deck).await;
    let mut inbox = owner.await.unwrap();
    assert_eq!(state.view_identity(), home_identity);
    assert_eq!(state.input(), "Ж");
    assert_eq!(deck.location.as_deref(), Some("/a"));
    assert_eq!(deck.revision.as_deref(), Some("a-restored"));
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().view_identity().0,
        root_identity
    );
    assert_eq!(deck.tabs[0].as_ref().unwrap().input(), "root A draft");
    assert!(!deck.scope_uncertain);
    // Complete only authoritative fact reads. Healthy retained history is not
    // rebuilt and neither the accepted foreign open nor a Home submit is replayed.
    let owner = tokio::spawn(async move {
        for _ in 0..3 {
            match inbox.recv().await.unwrap() {
                InboxMsg::SessionSelection {
                    session,
                    action: SelectionAction::Current,
                    ack,
                    ..
                } => {
                    assert_eq!(session.0, "a");
                    ack.send(Ok(facts.clone())).unwrap();
                }
                InboxMsg::HomeSelection {
                    action: SelectionAction::Current,
                    ack,
                } => {
                    ack.send(Ok(facts.clone())).unwrap();
                }
                InboxMsg::McpStatus { ack } => {
                    ack.send(Ok(oc_core::queries::McpSnapshot {
                        binding: oc_core::queries::McpBinding {
                            location: "/a".into(),
                            generation: 0,
                            instance: 1,
                        },
                        revision: 1,
                        servers: Vec::new(),
                    }))
                    .unwrap();
                }
                _ => panic!("only return-context facts, no effects/history"),
            }
        }
        inbox
    });
    super::settle_provider(&app, &mut state, &mut deck).await;
    let mut inbox = owner.await.unwrap();
    assert!(inbox.try_recv().is_err());
    assert_eq!(state.view_identity(), home_identity);
    assert_eq!(state.input(), "Ж");
}

#[tokio::test]
async fn typed_new_parks_pending_fresh_source_even_when_auxiliary_history_queue_is_full() {
    let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(1, 73);
    let mut state = TuiState::new_home(app.clone());
    state.handle_paste("original owned capture");
    state.handle_key(KeyAction::Enter).await;
    let source = state.tab_route_session().unwrap().clone();
    let mut deck = LoopState::default();
    state.handle_paste("/new");
    tokio::time::timeout(
        Duration::from_millis(100),
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ),
    )
    .await
    .expect("New is local even when auxiliary persistence cannot enqueue")
    .unwrap();
    assert!(state.attached_session().is_none() && state.tab_route_session().is_none());
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().tab_route_session(),
        Some(&source)
    );
    assert!(deck.tabs[0].as_ref().unwrap().has_pending_submission());
    responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
    let InboxMsg::SubmitFresh { session, ack, .. } = inbox.recv().await.unwrap() else {
        panic!("only original captured admission")
    };
    assert_eq!(session, source);
    ack.send(Err(CoreError::TurnNotActive)).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.input(), "Ж");
    assert!(deck.tabs[0].as_ref().unwrap().has_occupied_submission());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn typed_new_and_prompt_recall_do_not_wait_for_history_persistence_or_erase_newer_input() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut state = TuiState::new(app.clone(), SessionId("source".into()));
    state.apply_catalog(catalog());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.restore_prompt("/new".into());
    tokio::time::timeout(
        Duration::from_millis(100),
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ),
    )
    .await
    .expect("typed New cannot await auxiliary persistence")
    .unwrap();
    let InboxMsg::PromptHistory {
        append,
        ack: history,
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured slash history append")
    };
    assert_eq!(append.as_deref(), Some("/new"));
    assert!(state.attached_session().is_none());
    sync_home(&app, &mut state, &mut deck).await;
    let InboxMsg::HomeSelection { ack: home, .. } = inbox.recv().await.unwrap() else {
        panic!("authoritative Home preparation")
    };
    responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
    deck.activate(&mut state, 0).unwrap();
    state.restore_prompt("independent source draft".into());
    history
        .send(Err(CoreError::Application("safe history failure".into())))
        .unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.input(), "independent source draft");
    assert!(state.note().unwrap().contains("retained input"));
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::PromptHistoryPrevious).await;
    assert_eq!(
        state.input(),
        "/new",
        "a refused append remains locally recoverable without effect replay"
    );
    state.handle_key(KeyAction::End).await;
    assert!(state.handle_key(KeyAction::Cancel).await.intent.is_none());
    state.handle_key(KeyAction::PromptHistoryNext).await;
    assert_eq!(state.input(), "independent source draft");
    home.send(Ok(catalog())).unwrap();
    super::settle_home(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "source");
    assert_eq!(state.input(), "independent source draft");
    assert_eq!(deck.home.as_ref().unwrap().input(), "Ж");
    assert!(
        inbox.try_recv().is_err(),
        "unknown auxiliary write is never replayed"
    );

    let mut fresh = TuiState::new(app.clone(), SessionId("recall".into()));
    fresh.restore_prompt("new".into());
    fresh.handle_key(KeyAction::Home).await;
    tokio::time::timeout(
        Duration::from_millis(100),
        fresh.handle_key(KeyAction::PromptHistoryPrevious),
    )
    .await
    .expect("history read starts locally");
    let InboxMsg::PromptHistory { append, ack } = inbox.recv().await.unwrap() else {
        panic!("one history read")
    };
    assert_eq!(append, None);
    fresh.handle_key(KeyAction::End).await;
    responsive_frame(&app, &mut fresh, &mut LoopState::default(), 'Ω').await;
    ack.send(Ok(vec!["older input".into()])).unwrap();
    fresh.poll_prompt_history();
    assert_eq!(
        fresh.input(),
        "newΩ",
        "late recall cannot overwrite a newer editor revision"
    );
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn close_ack_retains_a_source_that_gained_an_owned_control_receipt() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.chrome.location = Some("/fixture".into());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("before".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "b");
    state.chrome.location = Some("/fixture".into());
    deck.activate(&mut state, 0).unwrap();
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    let InboxMsg::SaveTabDeck {
        deck: before,
        ack: preflight,
    } = inbox.recv().await.unwrap()
    else {
        panic!("preclose save")
    };
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SetPermissionMode { auto_once: true },
    )
    .await
    .unwrap();
    let InboxMsg::RegisterApprovalConsumer {
        ack: permission, ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured exact-source control receipt")
    };
    preflight
        .send(Ok(TabDeckSnapshot {
            revision: Some("preflight".into()),
            ..before
        }))
        .unwrap();
    let InboxMsg::SaveTabDeck { deck: removed, ack } = inbox.recv().await.unwrap() else {
        panic!("committed removal")
    };
    assert_eq!(removed.sessions, [SessionId("b".into())]);
    ack.send(Ok(TabDeckSnapshot {
        revision: Some("removed".into()),
        ..removed
    }))
    .unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    assert!(deck.closed_owners.contains_key(&SessionId("a".into())));
    assert_eq!(
        deck.tabs.len(),
        2,
        "unresolved receipt owner cannot be discarded by late close ACK"
    );
    assert_eq!(
        deck.snapshot(&state).sessions,
        [SessionId("b".into())],
        "retained closed owner cannot reauthor durable membership"
    );
    deck.activate(&mut state, 1).unwrap();
    responsive_frame(&app, &mut state, &mut deck, 'Й').await;
    let identity = state.view_identity();
    permission.send(Ok(())).unwrap();
    let owner = tokio::spawn(async move {
        let InboxMsg::PendingApprovals { ack } = inbox.recv().await.unwrap() else {
            panic!("one controls inventory")
        };
        ack.send(Ok(Vec::new())).unwrap();
        let InboxMsg::PendingQuestions { ack } = inbox.recv().await.unwrap() else {
            panic!("one questions inventory")
        };
        ack.send(Ok(Vec::new())).unwrap();
        inbox
    });
    super::settle_controls(&app, &mut state, &mut deck).await;
    deck_jobs::sync(&app, &mut state, &mut deck).await;
    assert!(deck.closed_owners.is_empty());
    assert_eq!(deck.tabs.len(), 1);
    assert_eq!(state.view_identity(), identity);
    assert_eq!(state.session().0, "b");
    assert_eq!(state.input(), "Й");
    let mut inbox = owner.await.unwrap();
    assert!(
        inbox.try_recv().is_err(),
        "no cancel, admission replay or re-add of removed source"
    );
}

async fn settle_deck(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.deck_job.as_ref().is_some_and(deck_jobs::Job::ready) {
            tokio::task::yield_now().await;
        }
        deck_jobs::sync(app, state, deck).await;
    })
    .await
    .expect("captured deck receipt");
}

async fn responsive_frame(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState, glyph: char) {
    tokio::time::timeout(Duration::from_millis(100), async {
        handle_event(app, state, deck, CEvent::Resize(100, 32))
            .await
            .unwrap();
        handle_event(
            app,
            state,
            deck,
            CEvent::Key(KeyEvent::new(KeyCode::Char(glyph), KeyModifiers::NONE)),
        )
        .await
        .unwrap();
        assert!(
            oc_tui::views::render_test(state, 100, 32)
                .iter()
                .any(|line| line.contains(glyph))
        );
    })
    .await
    .expect("input, resize and draw must not wait for owner acknowledgement");
}

#[tokio::test]
async fn fresh_receipts_can_park_reverse_ack_without_focus_steal_or_next_draft_loss() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut state = TuiState::new_home(app.clone());
    let mut deck = LoopState::default();
    state.handle_paste("first A");
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    let InboxMsg::SubmitFresh {
        session: a,
        ack: ack_a,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("fresh A")
    };
    let identity_a = state.view_identity();
    assert!(state.input().is_empty());
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(deck.tabs.len(), 1);
    assert!(
        deck.snapshot(&state).sessions.is_empty(),
        "local candidate is not durable membership"
    );
    state.handle_paste("next A");
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    assert!(state.home && state.attached_session().is_none());
    sync_controller_jobs(&app, &mut state, &mut deck).await;
    let InboxMsg::HomeSelection { ack: home_ack, .. } = inbox.recv().await.unwrap() else {
        panic!("prepared Home catalog")
    };
    responsive_frame(&app, &mut state, &mut deck, '界').await;
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    assert_eq!(
        state.input(),
        "界",
        "unresolved Home retains input, not a guessed model binding"
    );
    assert!(inbox.try_recv().is_err());
    state.restore_prompt("/unresolved-custom-command".into());
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    assert_eq!(state.input(), "/unresolved-custom-command");
    assert!(
        inbox.try_recv().is_err(),
        "unknown slash text cannot bypass unresolved Home admission"
    );
    state.restore_prompt("界".into());
    home_ack.send(Ok(catalog())).unwrap();
    while !deck.home_job.as_ref().unwrap().1.is_finished() {
        tokio::task::yield_now().await;
    }
    sync_controller_jobs(&app, &mut state, &mut deck).await;
    state.handle_paste(" first B");
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    let InboxMsg::SubmitFresh {
        session: b,
        ack: ack_b,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("fresh B")
    };
    assert_ne!(a, b);
    state.handle_paste("next B");
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(deck.tabs.len(), 2);
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.view_identity().0, identity_a.0);
    let reactivated_a = state.view_identity();
    ack_b.send(Ok(WorkerTurnId("turn-B".into()))).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.view_identity(), reactivated_a);
    assert!(state.attached_session().is_none(), "B ACK cannot attach A");
    assert_eq!(deck.tabs[1].as_ref().unwrap().attached_session(), Some(&b));
    assert_eq!(deck.tabs[1].as_ref().unwrap().input(), "next B");
    ack_a.send(Ok(WorkerTurnId("turn-A".into()))).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.attached_session(), Some(&a));
    assert_eq!(state.input(), "next A");
    assert_eq!(deck.active_tab, Some(0));
    assert_eq!(deck.tabs.len(), 2, "ACK promotes existing slots in place");
}

#[tokio::test]
async fn unrelated_settled_close_is_allowed_while_exact_target_admission_stays_owned() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("pending".into()));
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "settled");
    deck.activate(&mut state, 0).unwrap();
    state.handle_paste("once");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("pending submit")
    };
    assert!(deck.close_tab(&app, &mut state, 0).await.is_err());
    deck.close_tab(&app, &mut state, 1).await.unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "pending");
    assert!(state.has_pending_submission());
    assert_eq!(deck.tabs.len(), 1);
    assert!(
        inbox.try_recv().is_err(),
        "close does not cancel or replay the pending target"
    );
    ack.send(Ok(WorkerTurnId("original".into()))).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("original".into())));
}

#[tokio::test]
async fn provider_publication_preserves_discarded_fresh_candidate_and_mounted_dialog() {
    let (app, mut inbox, _control, _, _admissions) = CoreApp::native_channel(16, 42);
    let mut state = TuiState::new_home(app.clone());
    state.chrome.location = Some("/fixture".into());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    state.handle_paste("refused fresh");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SubmitFresh {
        session: candidate,
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("fresh candidate")
    };
    ack.send(Err(CoreError::Application("fixture unavailable".into())))
        .unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("Settings");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("Last request");
    state.handle_key(KeyAction::DeleteOrQuit).await;
    assert!(!state.has_occupied_submission());
    assert_eq!(state.tab_presentation().1, deck.active_tab.unwrap());
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("Close tab");
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::CloseTab { index: 0 }),
        "a retained fresh route is not the synthetic Home slot"
    );
    state.handle_key(KeyAction::Cancel).await;
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("connect");
    state.handle_key(KeyAction::Enter).await;
    state.apply_provider_connections(Ok(Vec::new()));
    let identity = state.view_identity();
    let surface = state.account_surface_revision();
    assert!(surface.is_some());
    assert_eq!(state.tab_route_session(), Some(&candidate));
    deck.selection_dirty = false;
    deck.shared_refresh = true;
    shared::sync(&app, &mut state, &mut deck).await.unwrap();
    let InboxMsg::Workspace { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("workspace publication")
    };
    ack.send(Ok(oc_core::queries::WorkspaceSnapshot {
        revision: 1,
        deck: TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("published".into()),
            ..Default::default()
        },
        turns: Vec::new(),
        inbox: Vec::new(),
        activity: Vec::new(),
        selected_root: None,
    }))
    .unwrap();
    let InboxMsg::HomeSelection { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("canonical Home facts only")
    };
    let mut facts = catalog();
    facts.chrome.location = Some("/fixture".into());
    ack.send(Ok(facts)).unwrap();
    while !deck.workspace_job.as_ref().unwrap().ready() {
        tokio::task::yield_now().await;
    }
    shared::sync(&app, &mut state, &mut deck).await.unwrap();
    assert_eq!(state.view_identity(), identity);
    assert_eq!(state.tab_route_session(), Some(&candidate));
    assert!(state.attached_session().is_none());
    assert_eq!(state.panel(), &TuiPanel::Accounts);
    assert_eq!(state.account_surface_revision(), surface);
    assert!(
        deck.home.is_none(),
        "no replacement Home behind the retained dialog"
    );
    assert!(
        inbox.try_recv().is_err(),
        "publication never submits the candidate"
    );
}

#[tokio::test]
async fn workspace_before_submit_ack_keeps_candidate_unattached_and_selection_preserves_membership()
{
    let (app, mut inbox, _control, _, _admissions) = CoreApp::native_channel(16, 41);
    let mut state = TuiState::new_home(app.clone());
    state.chrome.location = Some("/fixture".into());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("old".into()),
        ..Default::default()
    };
    state.handle_paste("fresh");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SubmitFresh {
        session: candidate,
        ack: submit_ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("fresh")
    };
    poll_and_sync(&app, &mut state, &mut deck).await;
    append_tab(&app, &mut deck, &mut state, "settled");
    state.chrome.location = Some("/fixture".into());
    deck.activate(&mut state, 0).unwrap();
    deck.activate(&mut state, 1).unwrap();
    deck.shared_refresh = true;
    shared::sync(&app, &mut state, &mut deck).await.unwrap();
    let InboxMsg::Workspace { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("one prepared workspace read")
    };
    responsive_frame(&app, &mut state, &mut deck, 'Ω').await;
    ack.send(Ok(oc_core::queries::WorkspaceSnapshot {
        revision: 1,
        deck: TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("adopted".into()),
            sessions: vec![candidate.clone(), SessionId("settled".into())],
            active: Some(candidate.clone()),
            ..Default::default()
        },
        turns: Vec::new(),
        inbox: Vec::new(),
        activity: Vec::new(),
        selected_root: Some(SessionId("settled".into())),
    }))
    .unwrap();
    let InboxMsg::SessionSelection { session, ack, .. } = inbox.recv().await.unwrap() else {
        panic!("settled catalog only")
    };
    assert_eq!(
        session.0, "settled",
        "provisional must not become a canonical read source"
    );
    let mut snapshot = catalog();
    snapshot.chrome.location = Some("/fixture".into());
    ack.send(Ok(snapshot.clone())).unwrap();
    let InboxMsg::HomeSelection { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("Home catalog")
    };
    ack.send(Ok(snapshot)).unwrap();
    while !deck.workspace_job.as_ref().unwrap().ready() {
        tokio::task::yield_now().await;
    }
    let area = ratatui::layout::Rect::new(0, 0, 100, 32);
    let mouse = |kind| crossterm::event::MouseEvent {
        kind,
        column: 10,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        area,
    );
    shared::sync(&app, &mut state, &mut deck).await.unwrap();
    assert_eq!(
        state
            .handle_mouse(
                mouse(MouseEventKind::Up(crossterm::event::MouseButton::Left)),
                area
            )
            .intent,
        Some(PanelIntent::ActivateTab { index: 0 }),
        "unchanged workspace geometry cannot cancel an in-progress tab click"
    );
    assert_eq!(state.session().0, "settled");
    assert_eq!(state.input(), "Ω");
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().tab_route_session(),
        Some(&candidate)
    );
    assert!(deck.tabs[0].as_ref().unwrap().attached_session().is_none());
    deck_jobs::sync(&app, &mut state, &mut deck).await;
    let InboxMsg::SaveTabDeck {
        deck: requested,
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("selection CAS")
    };
    assert_eq!(
        requested.sessions,
        vec![candidate.clone(), SessionId("settled".into())]
    );
    assert_eq!(requested.active, Some(SessionId("settled".into())));
    assert_eq!(requested.revision.as_deref(), Some("adopted"));
    ack.send(Ok(TabDeckSnapshot {
        revision: Some("focused".into()),
        ..requested
    }))
    .unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "settled");
    submit_ack
        .send(Ok(WorkerTurnId("fresh-turn".into())))
        .unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "settled");
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().attached_session(),
        Some(&candidate)
    );
}

#[tokio::test]
async fn reorder_ack_uses_stable_ids_and_coalesces_newer_focus_without_reactivating_capture() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.chrome.location = Some("/fixture".into());
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        revision: Some("before".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "b");
    append_tab(&app, &mut deck, &mut state, "c");
    state.chrome.location = Some("/fixture".into());
    deck.authoritative_deck = Some(deck.snapshot(&state));
    deck.activate(&mut state, 0).unwrap();
    deck_jobs::sync(&app, &mut state, &mut deck).await;
    let InboxMsg::SaveTabDeck {
        deck: selection,
        ack: selection_ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("selection before structural request")
    };
    deck.move_tab(&app, &state, SessionId("a".into()), 2)
        .await
        .unwrap();
    assert!(
        deck.move_tab(&app, &state, SessionId("b".into()), 2)
            .await
            .is_err(),
        "only one bounded staged structural action"
    );
    assert!(
        inbox.try_recv().is_err(),
        "structural save waits for exact selection acknowledgement"
    );
    selection_ack
        .send(Ok(TabDeckSnapshot {
            revision: Some("selected".into()),
            ..selection
        }))
        .unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    let InboxMsg::SaveTabDeck {
        deck: requested,
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured reorder")
    };
    assert_eq!(requested.revision.as_deref(), Some("selected"));
    assert_eq!(
        deck.snapshot(&state).sessions,
        ["a", "b", "c"].map(|id| SessionId(id.into()))
    );
    deck.activate(&mut state, 1).unwrap();
    responsive_frame(&app, &mut state, &mut deck, 'Ж').await;
    ack.send(Ok(TabDeckSnapshot {
        revision: Some("reordered".into()),
        ..requested
    }))
    .unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "b");
    assert_eq!(state.input(), "Ж");
    assert_eq!(deck.active_tab, Some(0));
    assert_eq!(
        deck.snapshot(&state).sessions,
        ["b", "c", "a"].map(|id| SessionId(id.into()))
    );
    let InboxMsg::SaveTabDeck {
        deck: requested,
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("latest focus CAS")
    };
    assert_eq!(requested.revision.as_deref(), Some("reordered"));
    assert_eq!(requested.active, Some(SessionId("b".into())));
    ack.send(Ok(TabDeckSnapshot {
        revision: Some("focused".into()),
        ..requested
    }))
    .unwrap();
    settle_deck(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn held_history_coalesces_reruns_and_old_completion_cannot_erase_a_new_turn() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let mut state = TuiState::new(app.clone(), SessionId("history".into()));
    state.begin_linked_turn(WorkerTurnId("old-turn".into()));
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    deck.reads.request(&state, true);
    read_jobs::sync(&app, &mut state, &mut deck).await;
    let InboxMsg::History { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("canonical page")
    };
    for _ in 0..64 {
        deck.reads.request(&state, true);
        read_jobs::sync(&app, &mut state, &mut deck).await;
    }
    assert!(
        inbox.try_recv().is_err(),
        "one concrete read per view, not an unbounded queue"
    );
    responsive_frame(&app, &mut state, &mut deck, 'λ').await;
    state.begin_linked_turn(WorkerTurnId("new-turn".into()));
    ack.send(Ok(tps_page())).unwrap();
    let InboxMsg::ChildJobs { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("resources")
    };
    ack.send(Ok(Vec::new())).unwrap();
    empty_compactions(&mut inbox).await;
    empty_shell_inventory(&mut inbox, "history").await;
    // The read result is deliberately older than the currently painted turn.
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.reads.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    read_jobs::sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("new-turn".into())));
    assert_eq!(state.input(), "λ");
    assert!(
        !state
            .history()
            .rows()
            .iter()
            .any(|row| row.text == "cached body")
    );
    let InboxMsg::History { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("one coalesced repair against the current route")
    };
    assert!(inbox.try_recv().is_err());
    drop(ack);
}

#[tokio::test]
async fn deferred_blank_promotion_dispatches_once_to_parked_source_without_waiting_for_ack() {
    use oc_core::queries::{InboxAction, InboxDelivery, InboxItem, SessionSelectionAction};
    let (app, mut inbox, _) = CoreApp::channel(16);
    let source = SessionId("blank-source".into());
    let mut state = TuiState::new(app.clone(), source.clone());
    let mut snapshot = catalog();
    snapshot.models.push(oc_core::queries::ModelEntry {
        id: snapshot.model_id.clone(),
        ..Default::default()
    });
    state.apply_catalog(snapshot.clone());
    snapshot.agent_id = Some("prepared-agent".into());
    state.apply_agent_preview(snapshot.clone()).unwrap();
    state.apply_inbox(
        &source,
        vec![InboxItem {
            id: "original-item".into(),
            delivery: InboxDelivery::Queue,
            text: "queued once".into(),
            images: 0,
            error: None,
        }],
    );
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection {
        ack,
        action: SessionSelectionAction::AgentCommit(_),
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("preparation")
    };
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "focused");
    ack.send(Ok(snapshot)).unwrap();
    tokio::time::timeout(
        Duration::from_millis(100),
        poll_and_sync(&app, &mut state, &mut deck),
    )
    .await
    .unwrap();
    let InboxMsg::Inbox {
        session,
        action,
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured promotion")
    };
    assert_eq!(session, source);
    assert_eq!(
        action,
        InboxAction::Deliver {
            id: "original-item".into(),
            delivery: InboxDelivery::Steer
        }
    );
    responsive_frame(&app, &mut state, &mut deck, 'δ').await;
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "delayed intent is consumed exactly once"
    );
    drop(ack);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.inbox_jobs.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    inbox_jobs::sync(&app, &mut state, &mut deck).await;
    assert_eq!(state.session().0, "focused");
    assert_eq!(state.input(), "δ");
    assert!(
        deck.tabs[0]
            .as_ref()
            .unwrap()
            .note()
            .unwrap()
            .contains("outcome unknown")
    );
    assert!(
        inbox.try_recv().is_err(),
        "unknown delivery is not replayed"
    );
}

#[tokio::test]
async fn pending_fresh_cancel_uses_native_priority_lane_while_work_queue_is_full() {
    let (app, mut inbox, mut controls, _, _admissions) = CoreApp::native_channel(1, 73);
    let mut state = TuiState::new_home(app.clone());
    state.handle_paste("original capture");
    state.handle_key(KeyAction::Enter).await;
    let source = state.tab_route_session().unwrap().clone();
    let mut deck = LoopState::default();
    let intent = state.handle_key(KeyAction::Cancel).await.intent.unwrap();
    assert!(
        matches!(&intent, PanelIntent::CancelSession { session, turn: None, .. } if session == &source)
    );
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    let oc_core::core_app::NativeControl::Cancel {
        session,
        turn,
        admission,
        ack,
    } = tokio::time::timeout(Duration::from_millis(100), controls.recv())
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("priority cancellation, not InboxMsg::Cancel")
    };
    assert_eq!(session, source);
    assert!(turn.is_none() && admission.is_some());
    let InboxMsg::SubmitFresh {
        session,
        ack: original,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("original receipt still owned")
    };
    assert_eq!(session, source);
    assert!(state.has_pending_submission());
    ack.send(Ok(())).unwrap();
    original.send(Err(CoreError::TurnNotActive)).unwrap();
    poll_and_sync(&app, &mut state, &mut deck).await;
    assert!(state.has_occupied_submission() && !state.has_pending_submission());
    assert!(inbox.try_recv().is_err(), "no replay after cancellation");
}
