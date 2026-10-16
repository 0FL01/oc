use super::*;
use oc_core::queries::{AccountAuthSource, AccountKind, ProviderAccount};

fn snapshot() -> ProviderAccounts {
    ProviderAccounts {
        provider: "opencode-go".into(),
        name: String::from("opencode-go"),
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
        source: AccountAuthSource::Missing,
        summary: Vec::new(),
    }]));
    let choice = state.handle_key(KeyAction::Enter).await;
    assert!(
        matches!(choice.intent, Some(PanelIntent::ProviderAccounts { provider, action: None, .. })
        if provider == "opencode-go")
    );
    state.apply_provider_accounts(Ok(ProviderAccounts {
        provider: "opencode-go".into(),
        name: String::from("opencode-go"),
        effective: AccountAuthSource::Missing,
        accounts: Vec::new(),
    }));
    apply_key_method(state);
}

fn apply_key_method(state: &mut TuiState) {
    let (provider, revision) = state.account_methods_request().expect("method discovery");
    state.apply_auth_methods(&provider, revision, Ok(vec![AuthMethod::Key]));
}

async fn custom_form(state: &mut TuiState) {
    state.run_command(CommandAction::OpenConnect);
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(
        state.custom_provider_revision().is_none(),
        "wait for the provider-list ACK"
    );
    state.apply_provider_connections(Ok(Vec::new()));
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("My LAN endpoint");
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("http://127.0.0.1:8765/literal-prefix");
    state.handle_key(KeyAction::Enter).await;
}

#[tokio::test]
async fn conn04_custom_key_probe_api_confirmation_retry_and_save_are_revision_scoped() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("custom-ui".into()));
    state.handle_paste("UNCHANGED_DRAFT");
    custom_form(&mut state).await;
    state.handle_key(KeyAction::Enter).await; // explicit Key
    state.handle_paste("CUSTOM_KEY_CANARY");
    for (width, height) in [(100, 30), (64, 24)] {
        let frame = crate::views::render_test(&state, width, height).join("\n");
        assert!(frame.contains("Custom provider") && frame.contains("API key"));
        assert!(frame.contains('•') && frame.contains("enter submit"));
        assert!(!frame.contains("CUSTOM_KEY_CANARY"));
    }
    let submitted = state.handle_key(KeyAction::Enter).await;
    assert!(!format!("{submitted:?} {:?}", state.accounts).contains("CUSTOM_KEY_CANARY"));
    let Some(PanelIntent::ProbeCustomProvider { revision, input }) = submitted.intent else {
        panic!("missing typed probe");
    };
    assert_eq!(input.name, "My LAN endpoint");
    assert_eq!(input.base_url, "http://127.0.0.1:8765/literal-prefix");
    let CustomProviderAuth::Key(key) = input.auth else {
        panic!("not Key")
    };
    assert_eq!(key.into_secret(), "CUSTOM_KEY_CANARY");
    assert!(state.accounts.key.is_empty());
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    let cancel = state.custom_provider_cancel(revision).unwrap();
    let probed = |receipt| {
        Ok(CustomProviderReply::Probed {
            revision: receipt,
            model_count: 2,
            suggested: None,
            diagnostic: None,
        })
    };
    state.apply_custom_provider(revision + 1, probed(40));
    assert!(state.account_lines().join(" ").contains("Checking GET"));
    state.apply_custom_provider(revision, probed(41));
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("Confirm generation API")
    );
    assert!(state.captured_model_commit().is_none());
    let retry = state.handle_key(KeyAction::Char('r')).await;
    assert!(
        matches!(retry.intent, Some(PanelIntent::RetryCustomProvider { revision: current, receipt: 41 }) if current == revision)
    );
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    state.apply_custom_provider(revision, probed(42));
    state.handle_key(KeyAction::Down).await;
    let save = state.handle_key(KeyAction::Enter).await;
    assert!(
        matches!(save.intent, Some(PanelIntent::SaveCustomProvider { revision: current, receipt: 42, protocol: NativeProtocol::Chat }) if current == revision)
    );
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert_eq!(
        state.apply_custom_provider(
            revision,
            Ok(CustomProviderReply::Saved(ProviderConnection {
                provider: "custom-exact".into(),
                name: "My LAN endpoint".into(),
                source: AccountAuthSource::Stored,
                summary: Vec::new(),
            }))
        ),
        Some("custom-exact".into())
    );
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    assert!(state.captured_model_commit().is_none());
    assert_eq!(state.input(), "UNCHANGED_DRAFT");
}

#[tokio::test]
async fn conn04_custom_anonymous_cancel_edit_and_unmount_retire_the_exact_probe() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("custom-cancel".into()));
    custom_form(&mut state).await;
    state.handle_key(KeyAction::Down).await;
    let probe = state.handle_key(KeyAction::Enter).await;
    let Some(PanelIntent::ProbeCustomProvider { revision, input }) = probe.intent else {
        panic!("missing anonymous probe")
    };
    assert_eq!(input.auth, CustomProviderAuth::Anonymous);
    let cancel = state.custom_provider_cancel(revision).unwrap();
    state.apply_custom_provider(
        revision,
        Ok(CustomProviderReply::Probed {
            revision: 12,
            model_count: 0,
            suggested: Some(NativeProtocol::Messages),
            diagnostic: None,
        }),
    );
    state.handle_key(KeyAction::Char('e')).await;
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    let next = state.custom_provider_revision().unwrap();
    assert_ne!(revision, next);
    state.apply_custom_provider(
        revision,
        Ok(CustomProviderReply::Saved(ProviderConnection {
            provider: "late".into(),
            name: "Late".into(),
            source: AccountAuthSource::Anonymous,
            summary: Vec::new(),
        })),
    );
    assert_eq!(state.panel(), &TuiPanel::Accounts);
    state.handle_key(KeyAction::Down).await;
    state.handle_key(KeyAction::Enter).await;
    let cancel = state.custom_provider_cancel(next).unwrap();
    state.retire_auth_surface();
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(state.custom_provider_revision().is_none());
    assert!(state.captured_model_commit().is_none());
}

#[tokio::test]
async fn go05_masked_connect_moves_secret_once_and_ack_never_commits_selection() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("masked".into()));
    state.input = "UNCHANGED_DRAFT".into();
    let open = state.run_command(CommandAction::OpenConnect);
    assert!(matches!(
        open.intent,
        Some(PanelIntent::LoadProviderConnections { .. })
    ));
    choose_go(&mut state).await;
    // Pinned KeyMethod DialogPrompt: `Connect <name>` with an `API key` field.
    assert_eq!(
        state.account_lines()[..2],
        ["Connect opencode-go", "API key"]
    );
    assert!(!state.account_lines().join(" ").contains("Account label"));
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
        action: Some(AccountAction::AddKey { key, label }),
        ..
    }) = outcome.intent
    else {
        panic!("missing typed secret intent")
    };
    assert_eq!(key.into_secret(), "KEY_CANARY_MUST_NOT_RENDER");
    assert_eq!(label, "default");
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
    state.handle_paste("CANCELLED_KEY_CANARY");
    state.handle_key(KeyAction::Cancel).await;
    assert!(state.accounts.key.is_empty());
    assert_eq!(state.input(), "DRAFT");
    state.run_command(CommandAction::OpenAccounts);
    let mut inactive = snapshot();
    inactive.accounts[0].active = false;
    state.apply_provider_accounts(Ok(inactive));
    state.handle_key(KeyAction::Down).await;
    assert!(matches!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts {
            action: Some(AccountAction::Activate { .. }),
            ..
        })
    ));
    state.apply_provider_accounts(Ok(snapshot()));
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    // ctrl+r / ctrl+d (`dialog.integration.*`); letters belong to Search.
    state.handle_key(KeyAction::Rename).await;
    state.handle_paste(" renamed");
    assert!(matches!(state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts { action: Some(AccountAction::Rename { label, .. }), .. })
        if label == "First renamed"));
    state.apply_provider_accounts(Ok(snapshot()));
    assert!(
        state
            .handle_key(KeyAction::DeleteOrQuit)
            .await
            .intent
            .is_none()
    );
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("Press ctrl+d again to confirm")
    );
    assert!(matches!(
        state.handle_key(KeyAction::DeleteOrQuit).await.intent,
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
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::Enter).await;
    apply_key_method(&mut state);
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
            source: AccountAuthSource::Environment,
            summary: vec!["$OPENCODE_API_KEY".into()],
        },
        ProviderConnection {
            provider: "custom".into(),
            name: "Local connection".into(),
            source: AccountAuthSource::Configured,
            summary: Vec::new(),
        },
    ]));
    let providers = state.account_lines().join(" ");
    assert!(providers.contains("[Popular]") && providers.contains("[Services]"));
    assert!(providers.contains("✓ OpenCode Go | $OPENCODE_API_KEY"));
    assert!(
        !providers.contains("✓ Local"),
        "configured keys are not connections"
    );
    state.handle_paste("Local");
    assert!(state.accounts.key.is_empty() && state.accounts.label.is_empty());
    assert_eq!(state.select.query, "Local");
    assert!(!state.account_lines().join(" ").contains("OpenCode Go"));
    let chosen = state.handle_key(KeyAction::Enter).await;
    assert!(
        matches!(chosen.intent, Some(PanelIntent::ProviderAccounts { provider, action: None, .. }) if provider == "custom")
    );
    assert_eq!(state.input(), "DRAFT");
    state.apply_provider_accounts(Ok(ProviderAccounts {
        provider: "custom".into(),
        name: String::from("custom"),
        effective: AccountAuthSource::Missing,
        accounts: Vec::new(),
    }));
    apply_key_method(&mut state);
    assert_eq!(state.account_lines()[..2], ["Connect custom", "API key"]);
    state.handle_key(KeyAction::Cancel).await;
    state.run_command(CommandAction::OpenConnect);
    state.apply_provider_connections(Err(()));
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(state.accounts.key.is_empty());
}

fn openai_snapshot(accounts: Vec<ProviderAccount>) -> ProviderAccounts {
    ProviderAccounts {
        provider: "openai".into(),
        name: "OpenAI".into(),
        effective: AccountAuthSource::Missing,
        accounts,
    }
}

async fn choose_openai(state: &mut TuiState) {
    state.run_command(CommandAction::OpenConnect);
    state.apply_provider_connections(Ok(vec![ProviderConnection {
        provider: "openai".into(),
        name: "OpenAI".into(),
        source: AccountAuthSource::Missing,
        summary: Vec::new(),
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
    // DialogSelect paste is Search text only, never a credential field.
    assert_eq!(state.select.query, "NOT_AN_AUTH_FORM_SECRET");
    for _ in 0.."NOT_AN_AUTH_FORM_SECRET".len() {
        state.handle_key(KeyAction::Backspace).await;
    }
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("ChatGPT Pro/Plus (browser)")
    );
    assert_eq!(state.account_lines()[0], "Connect OpenAI");
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    crate::views::render_test(&state, area.width, area.height);
    // DialogSelect rows start five cells below the title; Device is the second.
    let rect = crate::dialog::DialogFrame::rect(area, crate::dialog::DialogSize::Medium, 3 + 7);
    let mouse = |kind| crossterm::event::MouseEvent {
        kind,
        column: rect.x + 3,
        row: rect.y + 6,
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
    assert_eq!(
        state.account_lines()[..4],
        ["OpenAI", ">Add account", "[Connected accounts]", " ● Alpha"]
    );
    // Painted DialogSelect rows, not separators/headings, own mouse hits.
    let large = (0..12)
        .map(|i| row(&format!("id-{i:02}"), &format!("Label {i:02}")))
        .collect();
    state.apply_provider_accounts(Ok(openai_snapshot(large)));
    let area = Rect::new(0, 0, 120, 40);
    crate::views::render_test(&state, area.width, area.height);
    // Add row, separator, heading and 12 accounts in a 14-row viewport.
    let rect = crate::dialog::DialogFrame::rect(area, crate::dialog::DialogSize::Medium, 14 + 7);
    let mouse = |kind| MouseEvent {
        kind,
        column: rect.x + 3,
        row: rect.y + 6,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent
            .is_none()
    );
    assert!(!state.accounts.pending && state.select.cursor == 0);
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Down).await;
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    assert!(!state.accounts.pending);
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("z", "Zulu")])));
    assert!(matches!(state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ProviderAccounts { action: Some(AccountAction::Activate { id }), .. }) if id == "z"));
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::Enter).await;
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
    state.handle_paste("KEY_FORM_CANARY");
    assert!(!state.accounts.key.is_empty());
    assert!(!state.account_lines().join(" ").contains("KEY_FORM_CANARY"));
    state.handle_key(KeyAction::Cancel).await;
    assert!(state.accounts.key.is_empty());
    assert_ne!(state.accounts.revision, old.revision);
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![row("a", "Alpha")])));
    state.handle_key(KeyAction::Down).await;
    assert!(
        state
            .handle_key(KeyAction::DeleteOrQuit)
            .await
            .intent
            .is_none()
    );
    assert!(
        state
            .account_lines()
            .join(" ")
            .contains("Press ctrl+d again to confirm")
    );
    assert!(matches!(
        state.handle_key(KeyAction::DeleteOrQuit).await.intent,
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

#[tokio::test]
async fn vis45_integration_list_matches_pinned_dialog_integration_rows() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("integrations".into()));
    state.run_command(CommandAction::OpenConnect);
    let connection = |provider: &str, name: &str, source, summary: &[&str]| ProviderConnection {
        provider: provider.into(),
        name: name.into(),
        source,
        summary: summary.iter().map(|s| (*s).into()).collect(),
    };
    state.apply_provider_connections(Ok(vec![
        connection("ludka2", "ludka2", AccountAuthSource::Configured, &[]),
        connection("openai", "OpenAI", AccountAuthSource::Missing, &[]),
        connection("a6api", "A6API", AccountAuthSource::Configured, &[]),
        connection(
            "opencode-go",
            "OpenCode Go",
            AccountAuthSource::Stored,
            &["API key"],
        ),
    ]));
    let frame = crate::views::render_test(&state, 100, 30);
    let row = |text: &str| frame.iter().position(|r| r.contains(text)).unwrap();
    assert!(frame[row("Connect an integration")].contains("esc"));
    assert!(row("Search") < row("Popular"));
    assert!(row("Popular") < row("OpenCode Go") && row("OpenCode Go") < row("OpenAI"));
    assert!(row("OpenAI") < row("Services") && row("Services") < row("A6API"));
    assert!(row("A6API") < row("ludka2") && row("ludka2") < row("Custom provider…"));
    assert!(frame[row("OpenCode Go")].contains("API key"));
    assert!(frame[row("OpenCode Go")].contains("✓ OpenCode Go"));
    assert!(!frame[row("A6API")].contains('✓'));
    // Typed search keeps the matching rows under their category.
    state.handle_paste("lud");
    let frame = crate::views::render_test(&state, 100, 30);
    let services = frame.iter().position(|r| r.contains("Services")).unwrap();
    assert!(frame[services + 1].contains("ludka2"));
    assert!(!frame.iter().any(|r| r.contains("Popular")));
}

#[tokio::test]
async fn vis45_connect_flow_surfaces_follow_pinned_select_prompt_and_oauth_views() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("flow".into()));
    let frame = |state: &TuiState| crate::views::render_test(state, 100, 30);
    let show = |label: &str, rows: &[String]| {
        if std::env::var_os("OC_SHOW_FRAMES").is_some() {
            eprintln!("--- {label}\n{}", rows.join("\n"));
        }
    };
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![ProviderAccount {
        id: "a".into(),
        label: "Alpha".into(),
        kind: AccountKind::Key,
        method_id: None,
        active: true,
        created_at: 1,
    }])));
    let rows = frame(&state);
    show("accounts", &rows);
    let at = |rows: &[String], text: &str| rows.iter().position(|r| r.contains(text)).unwrap();
    assert!(rows[at(&rows, "OpenAI")].trim_end().ends_with("esc"));
    assert!(at(&rows, "Add account") < at(&rows, "Connected accounts"));
    assert!(rows[at(&rows, "Alpha")].contains("● Alpha"));
    // rename/delete are hidden on the Add row and shown for an account.
    assert!(!rows.iter().any(|r| r.contains("rename ctrl+r")));
    state.handle_key(KeyAction::Down).await;
    let rows = frame(&state);
    assert!(
        rows.iter()
            .any(|r| r.contains("rename ctrl+r  delete ctrl+d"))
    );
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::Enter).await;
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(
        &provider,
        revision,
        Ok(vec![
            AuthMethod::Key,
            AuthMethod::OAuth(OAuthMethod::Browser),
        ]),
    );
    let rows = frame(&state);
    show("methods", &rows);
    assert!(
        rows[at(&rows, "Connect OpenAI")]
            .trim_end()
            .ends_with("esc")
    );
    // connectMethods: the key method sorts last.
    assert!(at(&rows, "ChatGPT Pro/Plus (browser)") < at(&rows, "API key"));
    state.handle_key(KeyAction::Down).await;
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("PROMPT_KEY_CANARY");
    let rows = frame(&state);
    show("key", &rows);
    let title = at(&rows, "Connect OpenAI");
    assert!(rows[title + 2].contains("•••••") && rows[title + 4].contains("enter submit"));
    assert!(!rows.join("\n").contains("PROMPT_KEY_CANARY"));
    state.handle_key(KeyAction::Enter).await;
    let rows = frame(&state);
    show("key busy", &rows);
    assert!(rows.iter().any(|r| r.contains("processing…")));
    state.apply_provider_accounts(Err(()));
    let rows = frame(&state);
    show("key error", &rows);
    let title = at(&rows, "Connect OpenAI");
    assert!(rows[title + 2].contains("Account operation refused"));
    assert!(rows[title + 4].contains("API key"));
    // OAuthView: title, link, instructions, message and o/c hints.
    state.handle_key(KeyAction::Cancel).await;
    choose_openai(&mut state).await;
    state.apply_provider_accounts(Ok(openai_snapshot(vec![])));
    let (provider, revision) = state.account_methods_request().unwrap();
    state.apply_auth_methods(
        &provider,
        revision,
        Ok(vec![AuthMethod::OAuth(OAuthMethod::Device)]),
    );
    let request = state.auth_request().unwrap().clone();
    let rows = frame(&state);
    assert!(rows.iter().any(|r| r.contains("Starting authorization…")));
    state.apply_auth_attempt(
        &request,
        AuthAttempt {
            id: "attempt".into(),
            method: OAuthMethod::Device,
            state: AuthAttemptState::Pending,
            url: Some("https://auth.openai.com/codex/device".into()),
            instructions: Some("Enter code: ABCD-EFGH".into()),
            user_code: Some("ABCD-EFGH".into()),
            created_at: 1,
            expires_at: 601,
            account_id: None,
        },
    );
    let rows = frame(&state);
    show("oauth", &rows);
    let title = at(&rows, "ChatGPT Pro/Plus (headless)");
    assert!(rows[title].trim_end().ends_with("esc"));
    assert!(rows[title + 2].contains("https://auth.openai.com/codex/device"));
    assert!(rows[title + 4].contains("Enter code: ABCD-EFGH"));
    assert!(rows[title + 6].contains("Waiting for authorization…"));
    assert!(rows[title + 8].contains("o open  c copy"));
}
