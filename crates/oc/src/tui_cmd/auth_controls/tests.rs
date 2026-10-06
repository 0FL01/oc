use super::*;
use oc_core::{
    core_app::InboxMsg,
    queries::{AuthMethod, OAuthMethod, ProviderConnection},
};

async fn starting(app: CoreApp) -> (TuiState, AuthRequest) {
    let mut state = TuiState::new(app, SessionId("ui".into()));
    state.handle_paste("PARKED_DRAFT");
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("connect");
    state.handle_key(KeyAction::Enter).await;
    state.apply_provider_connections(Ok(vec![ProviderConnection {
        provider: "openai".into(),
        name: "OpenAI".into(),
    }]));
    state.handle_key(KeyAction::Enter).await;
    state.apply_provider_accounts(Ok(ProviderAccounts {
        provider: "openai".into(),
        accounts: vec![],
        effective: oc_core::queries::AccountAuthSource::Missing,
    }));
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(
        &provider,
        revision,
        Ok(vec![AuthMethod::OAuth(OAuthMethod::Browser)]),
    );
    state.handle_key(KeyAction::Enter).await;
    let request = state.auth_request().unwrap().clone();
    (state, request)
}

fn pending() -> AuthAttempt {
    AuthAttempt {
        id: "owned-attempt".into(),
        method: OAuthMethod::Browser,
        state: AuthAttemptState::Pending,
        url: Some("https://auth.openai.com/oauth/authorize?state=STATE_CANARY".into()),
        instructions: Some("Complete authorization".into()),
        user_code: None,
        created_at: 1,
        expires_at: 601,
        account_id: None,
    }
}

async fn finish(controls: &mut Controls, app: &CoreApp, state: &mut TuiState) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !controls.ready() {
            tokio::task::yield_now().await;
        }
        controls.sync(app, state).await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn auth05_closed_surface_joins_late_begin_and_cancels_exact_owned_id() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let (mut state, _) = starting(app.clone()).await;
    let mut controls = Controls::default();
    assert!(controls.sync(&app, &mut state).await);
    let InboxMsg::Authenticate {
        provider,
        action: AuthAction::Begin { method, .. },
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("not begin")
    };
    assert_eq!(provider, "openai");
    assert_eq!(method, OAuthMethod::Browser);
    let mut deck = LoopState {
        home: Some(state),
        ..Default::default()
    };
    deck.retire_parked_auth();
    state = deck.home.take().unwrap();
    assert!(state.auth_request().is_none() && state.auth_detail(false).is_none());
    ack.send(Ok(pending())).unwrap();
    finish(&mut controls, &app, &mut state).await;
    let InboxMsg::Authenticate {
        provider,
        action: AuthAction::Cancel { attempt },
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("not cancel")
    };
    assert_eq!(provider, "openai");
    assert_eq!(attempt, "owned-attempt");
    let mut closed = pending();
    closed.state = AuthAttemptState::Failed(oc_core::queries::AuthAttemptFailure::Cancelled);
    closed.url = None;
    ack.send(Ok(closed)).unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert!(controls.deadline().is_none() && !controls.pending());
    assert!(controls.active.is_none());
    assert_eq!(state.input(), "PARKED_DRAFT");
    controls.shutdown(&app).await.unwrap();
}

#[tokio::test]
async fn auth05_polling_uses_owned_status_and_stops_on_terminal_failure_without_picker() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let (mut state, _) = starting(app.clone()).await;
    let mut controls = Controls::default();
    controls.sync(&app, &mut state).await;
    let InboxMsg::Authenticate { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("not begin")
    };
    ack.send(Ok(pending())).unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert!(controls.deadline().is_some());
    assert!(state.auth_detail(false).unwrap().contains("STATE_CANARY"));
    controls.poll_at = Some(Instant::now());
    controls.sync(&app, &mut state).await;
    let InboxMsg::Authenticate {
        action: AuthAction::Status { attempt },
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("not status")
    };
    assert_eq!(attempt, "owned-attempt");
    let mut failed = pending();
    failed.state = AuthAttemptState::Expired;
    failed.url = None;
    ack.send(Ok(failed)).unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert!(controls.deadline().is_none());
    assert!(state.auth_detail(false).is_none());
    assert!(state.auth_request().is_none());
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(state.captured_model_commit().is_none());
    assert_eq!(state.input(), "PARKED_DRAFT");
    controls.shutdown(&app).await.unwrap();
}

#[tokio::test]
async fn auth05_completed_auth_requires_account_ack_then_browses_exact_provider_without_commit() {
    for acknowledged in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let (mut state, _) = starting(app.clone()).await;
        let mut controls = Controls::default();
        controls.sync(&app, &mut state).await;
        let InboxMsg::Authenticate { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("not begin")
        };
        let mut view = pending();
        view.state = AuthAttemptState::Complete;
        view.url = None;
        view.account_id = Some("durable-account".into());
        ack.send(Ok(view)).unwrap();
        finish(&mut controls, &app, &mut state).await;
        let InboxMsg::ProviderAccounts {
            provider,
            action: None,
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("missing account acknowledgement")
        };
        assert_eq!(provider, "openai");
        ack.send(Ok(ProviderAccounts {
            provider: "openai".into(),
            effective: oc_core::queries::AccountAuthSource::Stored,
            accounts: if acknowledged {
                vec![oc_core::queries::ProviderAccount {
                    id: "durable-account".into(),
                    label: "Subscription".into(),
                    kind: oc_core::queries::AccountKind::OAuth,
                    method_id: Some(OAuthMethod::Browser.id().into()),
                    active: true,
                    created_at: 1,
                }]
            } else {
                vec![]
            },
        }))
        .unwrap();
        if acknowledged {
            let InboxMsg::ProviderCatalog { provider, ack } = inbox.recv().await.unwrap() else {
                panic!("wrong provider model view")
            };
            assert_eq!(provider, "openai");
            ack.send(Ok(CatalogSnapshot {
                chrome: Default::default(),
                auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
                provider: "openai".into(),
                model_id: String::new(),
                models: vec![],
                variant: None,
                agents: vec![],
                agent_id: None,
                commands: vec![],
                command_descriptions: Default::default(),
            }))
            .unwrap();
        }
        finish(&mut controls, &app, &mut state).await;
        assert_eq!(
            state.panel(),
            if acknowledged {
                &TuiPanel::Model
            } else {
                &TuiPanel::None
            }
        );
        assert!(state.captured_model_commit().is_none());
        assert!(state.auth_request().is_none());
        assert!(!controls.pending() && controls.deadline().is_none());
        assert_eq!(state.input(), "PARKED_DRAFT");
        assert!(inbox.try_recv().is_err());
        controls.shutdown(&app).await.unwrap();
    }
}
