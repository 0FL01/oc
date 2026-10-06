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

fn openai_snapshot(accounts: Vec<ProviderAccount>) -> ProviderAccounts {
    ProviderAccounts {
        provider: "openai".into(),
        effective: AccountAuthSource::Missing,
        accounts,
    }
}

async fn choose_openai(state: &mut TuiState) {
    state.run_command(CommandAction::OpenConnect);
    state.apply_provider_connections(Ok(vec![ProviderConnection {
        provider: "openai".into(),
        name: "OpenAI".into(),
    }]));
    state.handle_key(KeyAction::Enter).await;
}

#[tokio::test]
async fn auth05_openai_method_choice_pending_details_and_ack_are_isolated() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("auth-ui".into()));
    state.handle_paste("draft界");
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![])));
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(
        &provider,
        revision,
        Ok(vec![
            AuthMethod::OAuth(OAuthMethod::Browser),
            AuthMethod::OAuth(OAuthMethod::Device),
            AuthMethod::Key,
        ]),
    );
    state.handle_paste("NOT_AN_AUTH_FORM_SECRET");
    assert!(state.accounts.key.is_empty() && state.accounts.label.is_empty());
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("ChatGPT Pro/Plus (browser)")
    );
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    let rect = crate::dialog::DialogFrame::rect(area, crate::dialog::DialogSize::Medium, 16);
    let mouse = |kind| crossterm::event::MouseEvent {
        kind,
        column: rect.x + 3,
        row: rect.y + 7,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    state.handle_mouse(
        mouse(crossterm::event::MouseEventKind::Down(
            crossterm::event::MouseButton::Left,
        )),
        area,
    );
    let begin = state.handle_mouse(
        mouse(crossterm::event::MouseEventKind::Up(
            crossterm::event::MouseButton::Left,
        )),
        area,
    );
    let Some(PanelIntent::BeginAuthentication(request)) = begin.intent else {
        panic!("missing typed auth intent")
    };
    assert_eq!(request.method, OAuthMethod::Device);
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("Starting authorization")
    );
    let mut view = AuthAttempt {
        id: "attempt".into(),
        method: OAuthMethod::Device,
        state: AuthAttemptState::Pending,
        url: Some("https://auth.openai.com/codex/device".into()),
        instructions: Some("non-code prose".into()),
        user_code: Some("STRUCTURED-CODE".into()),
        created_at: 1,
        expires_at: 601,
        account_id: None,
    };
    state.apply_auth_attempt(&request, view.clone());
    assert_eq!(state.auth_detail(true), Some("STRUCTURED-CODE"));
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("Waiting for authorization")
    );
    assert!(!format!("{:?}", state.accounts).contains("STRUCTURED-CODE"));
    assert!(!format!("{view:?}").contains("STRUCTURED-CODE"));
    assert!(matches!(
        state.handle_key(KeyAction::Char('c')).await.intent,
        Some(PanelIntent::CopyAuthorization)
    ));
    assert!(matches!(
        state.handle_key(KeyAction::Char('o')).await.intent,
        Some(PanelIntent::OpenAuthorization)
    ));
    state.handle_paste("NEVER_COMPOSER");
    assert_eq!(state.input(), "draft界");
    view.state = AuthAttemptState::Complete;
    view.url = None;
    view.user_code = None;
    view.account_id = Some("ack".into());
    state.apply_auth_attempt(&request, view);
    assert!(state.auth_detail(true).is_none());
    let mut alien = request.clone();
    alien.revision += 1;
    assert!(!state.auth_connected(&alien, "ack", Ok(openai_snapshot(vec![]))));
    assert_eq!(state.panel(), &TuiPanel::Accounts);
    let account = ProviderAccount {
        id: "ack".into(),
        label: "Subscription".into(),
        kind: AccountKind::OAuth,
        method_id: Some(OAuthMethod::Device.id().into()),
        active: true,
        created_at: 1,
    };
    assert!(state.auth_connected(&request, "ack", Ok(openai_snapshot(vec![account]))));
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert!(state.captured_model_commit().is_none());
    assert_eq!(state.input(), "draft界");
    assert!(state.auth_request().is_none());
}

#[tokio::test]
async fn auth05_openai_existing_accounts_add_first_and_failed_auth_close_retire_surface() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("accounts-ui".into()));
    choose_openai(&mut state).await;
    let row = |id: &str, label: &str| ProviderAccount {
        id: id.into(),
        label: label.into(),
        kind: AccountKind::Key,
        method_id: None,
        active: id == "a",
        created_at: 1,
    };
    state.apply_provider_accounts(Ok(openai_snapshot(vec![
        row("z", "Zulu"),
        row("a", "Alpha"),
    ])));
    assert_eq!(
        state.accounts.snapshot.as_ref().unwrap().accounts[0].id,
        "a"
    );
    assert!(state.account_lines()[2].contains("> Add account"));
    // The painted eight-row window, not the whole inventory, owns mouse hits.
    let large = (0..12)
        .map(|i| row(&format!("id-{i:02}"), &format!("Label {i:02}")))
        .collect();
    state.apply_provider_accounts(Ok(openai_snapshot(large)));
    let area = Rect::new(0, 0, 120, 40);
    let rect = crate::dialog::DialogFrame::rect(area, crate::dialog::DialogSize::Medium, 16);
    let mouse = |kind| MouseEvent {
        kind,
        column: rect.x + 3,
        row: rect.y + 14,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent
            .is_none()
    );
    assert!(!state.accounts.pending && state.accounts.cursor == 0);
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Down).await;
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(!state.accounts.pending);
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("z", "Zulu")])));
    assert!(matches!(state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts { action: Some(AccountAction::Activate { id }), .. }) if id == "z"));
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Char('a')).await;
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(
        &provider,
        revision,
        Ok(vec![
            AuthMethod::OAuth(OAuthMethod::Browser),
            AuthMethod::Key,
        ]),
    );
    state.handle_key(KeyAction::Enter).await;
    let old = state.auth_request().unwrap().clone();
    state.apply_auth_attempt(
        &old,
        AuthAttempt {
            id: "old".into(),
            method: OAuthMethod::Browser,
            state: AuthAttemptState::Expired,
            url: None,
            instructions: None,
            user_code: None,
            created_at: 1,
            expires_at: 2,
            account_id: None,
        },
    );
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(
        state
            .handle_key(KeyAction::Char('o'))
            .await
            .intent
            .is_none()
    );
    assert!(state.auth_request().is_none() && state.auth_detail(true).is_none());
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![])));
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(&provider, revision, Ok(vec![AuthMethod::Key]));
    state.handle_paste("Key account");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("KEY_FORM_CANARY");
    assert!(!state.account_lines().join(" ").contains("KEY_FORM_CANARY"));
    state.handle_key(KeyAction::Cancel).await;
    assert!(state.accounts.key.is_empty());
    assert_ne!(state.accounts.revision, old.revision);
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Down).await;
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
    assert!(!state.apply_provider_accounts(Ok(openai_snapshot(vec![]))));
    assert_eq!(state.panel(), &TuiPanel::None);
}
