use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{AccountAuthSource, CustomProviderAuth, NativeProtocol};

fn empty_catalog(provider: &str, generation: u64) -> CatalogSnapshot {
    CatalogSnapshot {
        chrome: oc_core::queries::TuiChrome {
            selection_generation: generation,
            ..Default::default()
        },
        provider: provider.into(),
        auto_accept: Default::default(),
        models: Vec::new(),
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }
}

async fn custom(app: CoreApp) -> (TuiState, PanelIntent) {
    let mut state = TuiState::new(app, SessionId("connect-ui".into()));
    state.handle_paste("DRAFT");
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("connect");
    state.handle_key(KeyAction::Enter).await;
    state.apply_provider_connections(Ok(Vec::new()));
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("Endpoint");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("http://127.0.0.1:8765/prefix");
    state.handle_key(KeyAction::Enter).await;
    state.handle_key(KeyAction::Down).await;
    let intent = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    (state, intent)
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
async fn conn05_closed_surface_joins_late_probe_and_cancels_exact_receipt() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let (mut state, intent) = custom(app.clone()).await;
    let mut controls = Controls::default();
    controls.start(&app, &mut state, intent).unwrap();
    let InboxMsg::CustomProvider {
        action: CustomProviderAction::Probe { input, cancel },
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("not probe")
    };
    assert_eq!(input.auth, CustomProviderAuth::Anonymous);
    assert!(!cancel.load(Ordering::SeqCst));
    // The request is held in the owner while terminal input remains responsive.
    state.handle_key(KeyAction::Cancel).await;
    assert!(cancel.load(Ordering::SeqCst));
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("connect");
    state.handle_key(KeyAction::Enter).await;
    let mounted = state.account_surface_revision();
    ack.send(Ok(CustomProviderReply::Probed {
        revision: 41,
        model_count: 7,
        suggested: None,
        diagnostic: None,
    }))
    .unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert_eq!(state.account_surface_revision(), mounted);
    assert!(state.custom_provider_revision().is_none());
    let InboxMsg::CustomProvider {
        action: CustomProviderAction::Cancel { revision: 41 },
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("not exact cancel")
    };
    ack.send(Ok(CustomProviderReply::Cancelled)).unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert!(!controls.pending());
    assert!(state.captured_model_commit().is_none());
    assert_eq!(state.input(), "DRAFT");
    assert!(inbox.try_recv().is_err());
    controls.shutdown(&app).await.unwrap();
}

#[tokio::test]
async fn final_account_removal_consumes_provider_facts_before_explicit_disconnect_feedback() {
    use oc_core::queries::{
        AccountAction, AccountKind, ProviderAccount, ProviderReadiness, ProviderStatus,
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("disconnect-ui".into()));
    state.apply_catalog(empty_catalog("opencode-go", 1));
    state.set_service_feedback_visible(true);
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("accounts");
    state.handle_key(KeyAction::Enter).await;
    let mut accounts = ProviderAccounts {
        provider: "opencode-go".into(),
        name: "OpenCode Go".into(),
        accounts: vec![ProviderAccount {
            id: "only-account".into(),
            label: "default".into(),
            kind: AccountKind::Key,
            method_id: None,
            active: true,
            created_at: 1,
        }],
        effective: AccountAuthSource::Stored,
        created: None,
    };
    state.apply_provider_accounts(Ok(accounts.clone()));
    state.handle_key(KeyAction::Down).await;
    assert!(
        state
            .handle_key(KeyAction::DeleteOrQuit)
            .await
            .intent
            .is_none()
    );
    let remove = state
        .handle_key(KeyAction::DeleteOrQuit)
        .await
        .intent
        .unwrap();
    let mut controls = Controls::default();
    controls.start(&app, &mut state, remove).unwrap();
    let InboxMsg::ProviderAccounts {
        action: Some(AccountAction::Remove {
            id,
            confirmed: true,
        }),
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("one confirmed removal")
    };
    assert_eq!(id, "only-account");
    accounts.accounts.clear();
    accounts.effective = AccountAuthSource::Missing;
    ack.send(Ok(accounts)).unwrap();
    let InboxMsg::SessionSelection {
        action: SelectionAction::Current,
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("canonical facts after mutation")
    };
    let mut facts = empty_catalog("opencode-go", 2);
    facts.chrome.provider = Some(ProviderReadiness {
        service: "opencode-go".into(),
        model: String::new(),
        status: ProviderStatus::Unavailable,
        catalog_status: ProviderStatus::Ready,
        diagnostic: None,
    });
    ack.send(Ok(facts.clone())).unwrap();
    let InboxMsg::AuthMethods { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("remaining connect methods")
    };
    ack.send(Ok(vec![AuthMethod::Key])).unwrap();
    let InboxMsg::ProviderCatalog { provider, ack } = inbox.recv().await.unwrap() else {
        panic!("same-cause provider facts")
    };
    assert_eq!(provider, "opencode-go");
    ack.send(Ok(facts.clone())).unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.note(), Some("Disconnected OpenCode Go"));
    state.apply_catalog_refresh(facts);
    assert_eq!(
        state.note(),
        Some("Disconnected OpenCode Go"),
        "late same-cause publication cannot erase the explicit ACK"
    );
    assert!(inbox.try_recv().is_err(), "no repeated credential mutation");
}

#[tokio::test]
async fn conn04_save_ack_opens_only_its_provider_picker_without_model_commit() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let (mut state, intent) = custom(app.clone()).await;
    let mut controls = Controls::default();
    controls.start(&app, &mut state, intent).unwrap();
    let InboxMsg::CustomProvider { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("not probe")
    };
    ack.send(Ok(CustomProviderReply::Probed {
        revision: 51,
        model_count: 1,
        suggested: None,
        diagnostic: None,
    }))
    .unwrap();
    finish(&mut controls, &app, &mut state).await;
    state.handle_key(KeyAction::Down).await;
    let save = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    controls.start(&app, &mut state, save).unwrap();
    let InboxMsg::CustomProvider {
        action:
            CustomProviderAction::Save {
                revision: 51,
                protocol: NativeProtocol::Chat,
            },
        ack,
    } = inbox.recv().await.unwrap()
    else {
        panic!("not explicit Chat save")
    };
    assert_eq!(state.panel(), &TuiPanel::Accounts);
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(inbox.try_recv().is_err());
    ack.send(Ok(CustomProviderReply::Saved(ProviderConnection {
        provider: "custom-exact".into(),
        name: "Endpoint".into(),
        source: AccountAuthSource::Anonymous,
        summary: Vec::new(),
    })))
    .unwrap();
    let InboxMsg::SessionSelection {
        session,
        action: SelectionAction::Current,
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("canonical selection after save publication")
    };
    assert_eq!(session.0, "connect-ui");
    ack.send(Ok(CatalogSnapshot {
        chrome: oc_core::queries::TuiChrome {
            selection_generation: 8,
            ..Default::default()
        },
        provider: "opencode-go".into(),
        auto_accept: Default::default(),
        models: Vec::new(),
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }))
    .unwrap();
    let InboxMsg::ProviderCatalog { provider, ack } = inbox.recv().await.unwrap() else {
        panic!("not provider-scoped catalog")
    };
    assert_eq!(provider, "custom-exact");
    ack.send(Ok(CatalogSnapshot {
        chrome: oc_core::queries::TuiChrome {
            selection_generation: 8,
            ..Default::default()
        },
        auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
        provider,
        models: vec![oc_core::queries::ModelEntry {
            id: "exact-model".into(),
            display_name: "Exact supplied model".into(),
            ..Default::default()
        }],
        model_id: String::new(),
        variant: None,
        agents: vec![],
        agent_id: None,
        commands: vec![],
        command_descriptions: Default::default(),
    }))
    .unwrap();
    finish(&mut controls, &app, &mut state).await;
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert_eq!(state.chrome.selection_generation, 8);
    assert_eq!(state.picker_provider_filter(), Some("custom-exact"));
    assert!(
        oc_tui::views::render_test(&state, 100, 32)
            .join("\n")
            .contains("Exact supplied model")
    );
    assert!(state.captured_model_commit().is_none());
    assert_eq!(state.input(), "DRAFT");
    assert!(!controls.pending() && controls.probe.is_none());
    assert!(inbox.try_recv().is_err());
    controls.shutdown(&app).await.unwrap();
}
