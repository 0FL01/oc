use super::*;
use oc_core::core_app::{InboxMsg, NativeControl};
use oc_core::queries::{InboxDelivery, InboxItem};
use std::collections::BTreeMap;

#[test]
fn source_default_selection_preserves_the_raw_reference_without_a_spurious_switch() {
    let raw = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "model".into(),
        variant: Some("default".into()),
    };
    let mut chosen = raw.clone();
    chosen.variant = None;
    assert!(same_model(&Some(raw.clone()), &Some(chosen.clone())));
    chosen.variant = Some("custom".into());
    assert!(!same_model(&Some(raw.clone()), &Some(chosen.clone())));
    chosen = raw.clone();
    chosen.provider = "other".into();
    assert!(!same_model(&Some(raw.clone()), &Some(chosen)));
    assert_eq!(raw.variant.as_deref(), Some("default"));
    let (mut state, _inbox, _control) = state(None);
    state.chrome.location = Some("/fixture".into());
    let source = state.opencode.as_mut().unwrap();
    let metadata = std::sync::Arc::make_mut(source.catalog.as_mut().unwrap());
    metadata.models.push(opencode::Model {
        provider: raw.provider.clone(),
        entry: oc_core::queries::ModelEntry {
            id: raw.id.clone(),
            display_name: "Fixture model".into(),
            variants_known: true,
            ..Default::default()
        },
    });
    state.apply_opencode_selection(Selection {
        agent: None,
        model: Some(raw.clone()),
    });
    assert_eq!(state.captured_model_commit().unwrap().variant, None);
    assert_eq!(state.opencode_selection().unwrap().model, Some(raw));
}

fn state(
    session: Option<SessionId>,
) -> (
    TuiState,
    tokio::sync::mpsc::Receiver<InboxMsg>,
    tokio::sync::mpsc::Receiver<NativeControl>,
) {
    let init = opencode::Init {
        location: "/fixture".into(),
        session: session.clone(),
        catalog: opencode::Catalog {
            models: Vec::new(),
            agents: Vec::new(),
            agent_models: BTreeMap::new(),
            selection: Selection {
                agent: None,
                model: None,
            },
        },
    };
    let (app, inbox, control, _) = CoreApp::opencode_channel(8, init);
    assert!(!app.shares_workspace());
    (
        session.map_or_else(
            || TuiState::new_home(app.clone()),
            |session| TuiState::new(app.clone(), session),
        ),
        inbox,
        control,
    )
}

fn session() -> opencode::Session {
    opencode::Session {
        id: SessionId("ses_created".into()),
        parent: None,
        title: None,
        location: "/fixture".into(),
        selection: Selection {
            agent: None,
            model: None,
        },
        created: "2026-11-12T00:00:00Z".into(),
        updated: "2026-11-12T00:00:00Z".into(),
        running: false,
    }
}

#[tokio::test]
async fn create_ack_survives_later_prompt_refusal_with_original_draft() {
    let (mut state, mut inbox, mut control) = state(None);
    state.handle_paste("retained draft Ω");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::OpenCode {
        request: Request::Create { .. },
        ack,
    } = inbox.try_recv().unwrap()
    else {
        panic!("separate create");
    };
    ack.send(Ok(Reply::Created { session: session() })).unwrap();
    state.poll_submission();
    assert_eq!(state.attached_session(), Some(&session().id));
    assert_eq!(state.input(), "retained draft Ω");
    let InboxMsg::OpenCode {
        request: Request::Prompt {
            session: id, text, ..
        },
        ack,
    } = inbox.try_recv().unwrap()
    else {
        panic!("prompt after create ACK");
    };
    assert_eq!(id, session().id);
    assert_eq!(text, "retained draft Ω");
    ack.send(Err(CoreError::Application("known refusal".into())))
        .unwrap();
    state.poll_submission();
    assert_eq!(state.attached_session(), Some(&session().id));
    assert_eq!(state.input(), "retained draft Ω");
    assert!(state.active_turn().is_none());
    state.reconcile_fresh_quit().await.unwrap();
    assert!(
        control.try_recv().is_err(),
        "viewer exit must not interrupt execution"
    );
    assert!(inbox.try_recv().is_err(), "no delete or automatic resend");
}

#[tokio::test]
async fn queued_prompt_ack_is_inbox_admission_not_execution_start() {
    let (mut state, mut inbox, _control) = state(Some(session().id));
    state.handle_paste("queue this");
    state.queue_prompt().await;
    let InboxMsg::OpenCode {
        request: Request::Prompt { delivery, .. },
        ack,
    } = inbox.try_recv().unwrap()
    else {
        panic!("queued prompt");
    };
    assert_eq!(delivery, InboxDelivery::Queue);
    ack.send(Ok(Reply::Admitted {
        item: InboxItem {
            id: "msg_inbox".into(),
            delivery,
            text: "queue this".into(),
            images: 0,
            error: None,
        },
    }))
    .unwrap();
    state.poll_submission();
    assert!(state.input().is_empty());
    assert!(state.active_turn().is_none());
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(
        state.inbox_items().is_empty(),
        "do not fabricate a pending item after it may already have delivered"
    );
    assert!(matches!(
        inbox.try_recv().unwrap(),
        InboxMsg::OpenCode {
            request: Request::Open { .. },
            ..
        }
    ));
}

#[tokio::test]
async fn unsupported_surfaces_do_not_change_the_composer_or_request_native_work() {
    let (mut state, mut inbox, _control) = state(None);
    assert!(state.run_command(CommandAction::OpenConnect).note.is_some());
    assert!(
        state
            .run_command(CommandAction::OpenAccounts)
            .note
            .is_some()
    );
    assert_eq!(state.panel, TuiPanel::None);
    assert!(
        state
            .paste_clipboard_image(std::sync::Arc::from([0_u8]))
            .note
            .is_some()
    );
    assert!(!state.editor.has_images());
    state.panel = TuiPanel::Model;
    assert!(state.model_dialog_provider().note.is_some());
    assert_eq!(state.panel, TuiPanel::Model);
    state.panel = TuiPanel::Sessions;
    state.sessions = vec![session().id.0];
    assert!(state.handle_key(KeyAction::Rename).await.note.is_some());
    assert_eq!(state.panel, TuiPanel::Sessions);
    assert!(state.rename_pending.is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn fresh_create_captures_the_selected_home_composer_agent() {
    let (mut state, mut inbox, _control) = state(None);
    state.apply_catalog_refresh(oc_core::queries::CatalogSnapshot {
        agent_id: Some("plan".into()),
        chrome: Default::default(),
        auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
        provider: String::new(),
        models: Vec::new(),
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        commands: Vec::new(),
        command_descriptions: Default::default(),
    });
    assert_eq!(state.composer_agent(), Some("plan"));
    assert!(state.staged_agent().is_none());
    state.handle_paste("use the selected agent");
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::OpenCode {
        request: Request::Create { selection },
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("fresh create");
    };
    assert_eq!(selection.agent.as_deref(), Some("plan"));
}

#[tokio::test]
async fn source_execution_keeps_the_existing_two_escape_interrupt_guard() {
    let id = session().id;
    let (mut state, _inbox, _control) = state(Some(id.clone()));
    state
        .opencode_event(&Event::Running {
            session: id.clone(),
            running: true,
        })
        .unwrap();
    assert!(state.is_busy());
    state.handle_paste("queued");
    state.queue_prompt().await;
    assert!(state.opencode.as_ref().unwrap().submitting());
    assert!(state.handle_key(KeyAction::Cancel).await.intent.is_none());
    assert!(
        matches!(state.handle_key(KeyAction::Cancel).await.intent, Some(PanelIntent::CancelSession { session, turn: None, cancel_compaction: false }) if session == id)
    );
}

#[test]
fn source_ordinals_merge_live_and_settled_history_without_native_watermarks() {
    let id = session().id;
    let (mut state, _inbox, _control) = state(Some(id.clone()));
    state
        .opencode_event(&Event::Started {
            session: id.clone(),
            message: empty_message("assistant_original"),
            epoch: 0,
        })
        .unwrap();
    // Pinned OpenCode numbers reasoning and text independently: both first
    // parts are ordinal zero, and their public starts establish order.
    for (reasoning, delta, epoch) in [(true, "reason", 1), (false, "answer", 3)] {
        state
            .opencode_event(&Event::PartStarted {
                session: id.clone(),
                message: "assistant_original".into(),
                ordinal: 0,
                reasoning,
                epoch,
            })
            .unwrap();
        state
            .opencode_event(&Event::Delta {
                session: id.clone(),
                message: "assistant_original".into(),
                ordinal: 0,
                reasoning,
                delta: delta.into(),
                epoch: epoch + 1,
            })
            .unwrap();
        if reasoning {
            state
                .opencode_event(&Event::Part {
                    session: id.clone(),
                    message: "assistant_original".into(),
                    ordinal: 0,
                    part: Part::Reasoning {
                        text: "reason".into(),
                    },
                    epoch: 3,
                })
                .unwrap();
        }
    }
    let visible = rows(&state.opencode.as_ref().unwrap().messages[0]);
    assert_eq!(&*visible[0].reasoning.as_ref().unwrap().text, "reason");
    assert_eq!(
        &*visible[1].text, "answer",
        "text is visible before text.ended"
    );
    let source = state.opencode.as_mut().unwrap();
    source.page(
        Page {
            epoch: 5,
            messages: vec![Message {
                parts: vec![Part::Reasoning {
                    text: "reason".into(),
                }],
                ..empty_message("assistant_original")
            }],
            older: Some("opaque:older".into()),
            newer: None,
        },
        false,
    );
    assert_eq!(source.messages.len(), 1);
    assert_eq!(
        source.messages[0].parts[1].1,
        Part::Text {
            text: "answer".into()
        }
    );
    state
        .opencode_event(&Event::ToolStarted {
            session: id.clone(),
            message: "assistant_original".into(),
            epoch: 6,
            id: "tool_original".into(),
            name: "read".into(),
        })
        .unwrap();
    state
        .opencode_event(&Event::PartStarted {
            session: id.clone(),
            message: "assistant_original".into(),
            epoch: 7,
            ordinal: 1,
            reasoning: false,
        })
        .unwrap();
    state
        .opencode_event(&Event::Delta {
            session: id,
            message: "assistant_original".into(),
            epoch: 8,
            ordinal: 1,
            reasoning: false,
            delta: "after tool".into(),
        })
        .unwrap();
    let parts = vec![
        Part::Reasoning {
            text: "reason".into(),
        },
        Part::Text {
            text: "answer".into(),
        },
        Part::Tool {
            id: "tool_original".into(),
            name: "read".into(),
            input: "{}".into(),
            output: "saved output".into(),
            status: "completed".into(),
        },
        Part::Text {
            text: "after tool".into(),
        },
    ];
    let message = Message {
        parts: parts.clone(),
        status: "completed".into(),
        ..empty_message("assistant_original")
    };
    let source = state.opencode.as_mut().unwrap();
    assert_eq!(
        source.messages[0]
            .parts
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>(),
        vec![
            PartKey::Reasoning(0),
            PartKey::Text(0),
            PartKey::Tool("tool_original".into()),
            PartKey::Text(1)
        ]
    );
    source.merge(message.clone(), 9);
    let live = rows(&source.messages[0]);
    source.page(
        Page {
            epoch: 10,
            messages: vec![message],
            older: None,
            newer: None,
        },
        true,
    );
    let reopened = rows(&source.messages[0]);
    assert_eq!(
        source.messages[0]
            .parts
            .iter()
            .map(|(_, part)| part.clone())
            .collect::<Vec<_>>(),
        parts
    );
    assert_eq!(live, reopened);
    assert!(
        matches!(reopened[0].reasoning.as_ref().unwrap().identity, Some(crate::messages::ReasoningIdentity::OpenCode(ref id, 0)) if id == "assistant_original")
    );
}

#[test]
fn already_active_partial_history_does_not_double_append_unfenced_deltas() {
    let id = session().id;
    let (mut state, _inbox, _control) = state(Some(id.clone()));
    state.opencode.as_mut().unwrap().page(
        Page {
            epoch: 0,
            messages: vec![Message {
                parts: vec![Part::Text {
                    text: "partial".into(),
                }],
                ..empty_message("active_message")
            }],
            older: None,
            newer: None,
        },
        true,
    );
    state
        .opencode_event(&Event::Delta {
            session: id.clone(),
            message: "active_message".into(),
            ordinal: 0,
            reasoning: false,
            delta: "partial".into(),
            epoch: 1,
        })
        .unwrap();
    assert_eq!(
        state.opencode.as_ref().unwrap().messages[0].parts[0].1,
        Part::Text {
            text: "partial".into()
        }
    );
    state
        .opencode_event(&Event::Part {
            session: id.clone(),
            message: "active_message".into(),
            ordinal: 0,
            part: Part::Text {
                text: "partial and settled".into(),
            },
            epoch: 2,
        })
        .unwrap();
    // Already-active content waits for the canonical read triggered by the
    // ended event; its live ordinal is not a global history-array index.
    assert_eq!(
        state.opencode.as_ref().unwrap().messages[0].parts[0].1,
        Part::Text {
            text: "partial".into()
        }
    );
    state
        .opencode_event(&Event::Message {
            session: id,
            epoch: 3,
            message: Message {
                parts: vec![Part::Text {
                    text: "partial and settled".into(),
                }],
                status: "completed".into(),
                ..empty_message("active_message")
            },
        })
        .unwrap();
    assert_eq!(
        state.opencode.as_ref().unwrap().messages[0].parts[0].1,
        Part::Text {
            text: "partial and settled".into()
        }
    );
}
