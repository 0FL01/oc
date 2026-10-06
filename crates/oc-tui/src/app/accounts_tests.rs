use super::*;
use oc_core::queries::{AccountAuthSource, AccountKind, ProviderAccount};

fn snapshot() -> ProviderAccounts {
    ProviderAccounts {
        provider: "opencode-go".into(),
        effective: AccountAuthSource::Stored,
        accounts: vec![ProviderAccount {
            id: "first".into(),
            label: "First".into(),
            kind: AccountKind::Key,
            method_id: None,
            active: true,
            created_at: 1,
        }],
    }
}

async fn choose_go(state: &mut TuiState) {
    state.apply_provider_connections(Ok(vec![ProviderConnection {
        provider: "opencode-go".into(),
        name: "OpenCode Go".into(),
    }]));
    let choice = state.handle_key(KeyAction::Enter).await;
    assert!(
        matches!(choice.intent, Some(PanelIntent::ProviderAccounts { provider, action: None })
        if provider == "opencode-go")
    );
}

#[tokio::test]
async fn go05_masked_connect_moves_secret_once_and_ack_never_commits_selection() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("masked".into()));
    state.input = "UNCHANGED_DRAFT".into();
    let open = state.run_command(CommandAction::OpenConnect);
    assert!(matches!(
        open.intent,
        Some(PanelIntent::LoadProviderConnections)
    ));
    choose_go(&mut state).await;
    state.apply_provider_accounts(Ok(snapshot()));
    state.handle_paste("Added");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("KEY_CANARY_MUST_NOT_RENDER");
    let lines = state.account_lines().join("\n");
    assert!(lines.contains('•') && !lines.contains("KEY_CANARY"));
    assert!(!format!("{:?}", state.accounts).contains("KEY_CANARY"));
    assert_eq!(state.input(), "UNCHANGED_DRAFT");
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert!(!format!("{outcome:?}").contains("KEY_CANARY"));
    assert!(state.accounts.key.is_empty());
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    let Some(PanelIntent::ProviderAccounts {
        action: Some(AccountAction::AddKey { key, .. }),
        ..
    }) = outcome.intent
    else {
        panic!("missing typed secret intent")
    };
    assert_eq!(key.into_secret(), "KEY_CANARY_MUST_NOT_RENDER");
    assert!(state.apply_provider_accounts(Ok(snapshot())));
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert_eq!(state.input(), "UNCHANGED_DRAFT");
    assert!(state.captured_model_commit().is_none());
    assert!(state.accounts.key.is_empty());
}

#[tokio::test]
async fn go05_accounts_cancel_error_rename_activation_and_confirmation_are_isolated() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("accounts".into()));
    state.input = "DRAFT".into();
    state.run_command(CommandAction::OpenConnect);
    choose_go(&mut state).await;
    state.handle_paste("Label");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("CANCELLED_KEY_CANARY");
    state.handle_key(KeyAction::Cancel).await;
    assert!(state.accounts.key.is_empty());
    assert_eq!(state.input(), "DRAFT");
    state.run_command(CommandAction::OpenAccounts);
    state.apply_provider_accounts(Ok(snapshot()));
    assert!(matches!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts {
            action: Some(AccountAction::Activate { .. }),
            ..
        })
    ));
    state.apply_provider_accounts(Ok(snapshot()));
    state.handle_key(KeyAction::Char('r')).await;
    state.handle_paste(" renamed");
    assert!(matches!(state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts { action: Some(AccountAction::Rename { label, .. }), .. })
        if label == "First renamed"));
    state.apply_provider_accounts(Ok(snapshot()));
    assert!(
        state
            .handle_key(KeyAction::Char('d'))
            .await
            .intent
            .is_none()
    );
    assert!(matches!(
        state.handle_key(KeyAction::Char('d')).await.intent,
        Some(PanelIntent::ProviderAccounts {
            action: Some(AccountAction::Remove {
                confirmed: true,
                ..
            }),
            ..
        })
    ));
    state.apply_provider_accounts(Err(()));
    assert!(state.account_lines().join(" ").contains("refused"));
    assert_eq!(state.input(), "DRAFT");
    state.handle_key(KeyAction::Char('a')).await;
    state.handle_paste("Retry");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("REJECTED_KEY_CANARY");
    state.handle_key(KeyAction::Enter).await;
    assert!(!state.apply_provider_accounts(Err(())));
    assert!(state.accounts.key.is_empty());
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.input(), "DRAFT");
}

#[tokio::test]
async fn go05_priority_overlay_retires_secret_without_forwarding_its_input() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("accounts".into()));
    state.run_command(CommandAction::OpenConnect);
    choose_go(&mut state).await;
    state.handle_paste("Label");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("EPHEMERAL_CANARY");
    let question = oc_core::question::QuestionRequest {
        id: 1,
        binding: oc_core::approval::ApprovalBinding {
            session: "accounts".into(),
            turn: "turn".into(),
            call: "call".into(),
            operation: "operation".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 1,
            agent: None,
            agent_digest: None,
        },
        input: oc_core::question::QuestionInput::parse(&serde_json::json!({"questions":[{
            "question":"Pick", "header":"Pick", "options":[{"label":"A","description":"first"}]
        }]}))
        .unwrap(),
    };
    state
        .questions
        .reconcile(std::slice::from_ref(&question), vec![question.clone()]);
    assert!(
        state
            .handle_paste("SECRET_PASTE_AFTER_OVERLAY")
            .intent
            .is_none()
    );
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(state.accounts.key.is_empty());
    assert!(state.input().is_empty());
    assert!(state.questions.active().is_some());
}

#[tokio::test]
async fn go05_provider_choice_is_explicit_and_never_receives_secret_paste() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("connections".into()));
    state.input = "DRAFT".into();
    state.run_command(CommandAction::OpenConnect);
    state.apply_provider_connections(Ok(vec![
        ProviderConnection {
            provider: "opencode-go".into(),
            name: "OpenCode Go".into(),
        },
        ProviderConnection {
            provider: "custom".into(),
            name: "Local connection".into(),
        },
    ]));
    state.handle_paste("PASTE_CANARY");
    assert!(state.accounts.key.is_empty() && state.accounts.label.is_empty());
    state.handle_key(KeyAction::Down).await;
    let chosen = state.handle_key(KeyAction::Enter).await;
    assert!(
        matches!(chosen.intent, Some(PanelIntent::ProviderAccounts { provider, action: None }) if provider == "custom")
    );
    assert_eq!(state.input(), "DRAFT");
    assert!(state.account_lines().join(" ").contains("Account label"));
    state.handle_key(KeyAction::Cancel).await;
    state.run_command(CommandAction::OpenConnect);
    state.apply_provider_connections(Err(()));
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(state.accounts.key.is_empty());
}
