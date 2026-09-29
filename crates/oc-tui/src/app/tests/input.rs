use super::*;

#[tokio::test]
async fn cfg10_diagnostic_details_copy_and_investigation_never_touch_prompt_or_submit() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use oc_core::queries::{
        ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
    };
    let mut state = fresh_state("cfg10-details").await;
    state.handle_paste("keep the unsent local prompt");
    let diagnostic = ServiceDiagnostic {
        kind: ServiceKind::Definition,
        service: "definition-opaque-fixture".into(),
        source: "source-123/config".into(),
        field: vec!["command".into(), "entry".into()],
        stage: ServiceStage::Config,
        code: ServiceCode::InvalidDefinition,
        action: ServiceAction::ReviewConfiguration,
    };
    let mut catalog = snapshot();
    catalog.chrome.service_diagnostics.push(diagnostic.clone());
    state.apply_catalog(catalog);
    state.panel = TuiPanel::Settings;
    state.handle_panel_key(KeyAction::Down);
    assert!(state.handle_panel_key(KeyAction::Right).intent.is_none());
    let details = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(details.note, Some(diagnostic.to_string()));
    assert!(details.intent.is_none());
    let modifiers = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('c'), modifiers)),
        None
    );
    assert_eq!(state.take_copy_request(), Some(diagnostic.to_string()));
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('i'), modifiers)),
        None
    );
    assert_eq!(
        state.note(),
        Some(diagnostic.investigation_draft().as_str())
    );
    assert_eq!(state.input, "keep the unsent local prompt");
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(!state.has_pending_submission());
}

#[tokio::test]
async fn ui07_provider_settings_are_read_only_owner_facts_and_preserve_draft() {
    use oc_core::queries::{
        ProviderReadiness, ProviderStatus, ServiceAction, ServiceCode, ServiceDiagnostic,
        ServiceKind, ServiceStage,
    };
    let mut state = fresh_state("ui07-settings").await;
    state.handle_paste("keep this local draft");
    let mut catalog = snapshot();
    let readiness = ProviderReadiness {
        service: "provider-opaque-fixture".into(),
        model: "model-opaque-fixture".into(),
        status: ProviderStatus::Unavailable,
        catalog_status: ProviderStatus::Unavailable,
        diagnostic: Some(ServiceDiagnostic {
            kind: ServiceKind::Provider,
            service: "provider-opaque-fixture".into(),
            source: "source-123/opencode.json".into(),
            field: vec!["options".into(), "apiKey".into()],
            stage: ServiceStage::Config,
            code: ServiceCode::MissingCredential,
            action: ServiceAction::ReviewConfiguration,
        }),
    };
    catalog.chrome.provider = Some(readiness.clone());
    state.apply_catalog(catalog);
    assert_eq!(state.input, "keep this local draft");
    state.panel = TuiPanel::Settings;
    state.handle_panel_key(KeyAction::Down);
    let rows = state.modal_options();
    assert_eq!(rows[1].title, "Provider request — unavailable");
    assert_eq!(rows[1].footer, readiness.to_string());
    assert!(rows[1].footer.contains("missing_credential"));
    assert!(state.handle_panel_key(KeyAction::Right).intent.is_none());
    let detail = state.handle_panel_key(KeyAction::Enter);
    assert!(
        detail.intent.is_none(),
        "read-only provider facts cannot trigger effects"
    );
    assert_eq!(detail.note.as_deref(), Some(rows[1].footer.as_str()));
    assert_eq!(state.input, "keep this local draft");
}

#[tokio::test]
async fn cfg09_plugin_settings_rows_show_owner_status_and_are_read_only() {
    use oc_core::queries::{
        PluginEntry, PluginStatus, ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind,
        ServiceStage,
    };
    let mut state = fresh_state("cfg09-settings").await;
    let mut catalog = snapshot();
    let diagnostic = ServiceDiagnostic {
        kind: ServiceKind::Plugin,
        service: "plugin-opaque-fixture".into(),
        source: "source-123/opencode.json".into(),
        field: vec!["plugin".into(), "0".into()],
        stage: ServiceStage::Capability,
        code: ServiceCode::UnsupportedPlugin,
        action: ServiceAction::ReviewConfiguration,
    };
    catalog.chrome.plugins.entries.push(PluginEntry {
        requested: diagnostic.service.clone(),
        current: None,
        module: None,
        status: PluginStatus::Failed,
        source: diagnostic.source.clone(),
        field: diagnostic.field.clone(),
        diagnostic: Some(diagnostic),
    });
    state.apply_catalog(catalog);
    state.panel = TuiPanel::Settings;
    state.handle_panel_key(KeyAction::Down);
    assert_eq!(state.select.cursor, 1);
    let rows = state.modal_options();
    assert_eq!(rows[1].title, "Unsupported plugin — failed");
    assert!(rows[1].footer.contains("current=none"));
    assert!(rows[1].footer.contains("retryable=false"));
    assert!(rows[1].footer.contains("plugin.0"));
    assert!(state.handle_panel_key(KeyAction::Right).intent.is_none());
    let result = state.handle_panel_key(KeyAction::Enter);
    assert!(
        result.intent.is_none(),
        "read-only facts never toggle permissions"
    );
    assert_eq!(result.note.as_deref(), Some(rows[1].footer.as_str()));
}

#[tokio::test]
async fn vis26_trigger_caret_tab_and_escape_keep_textual_draft() {
    let mut state = fresh_state("mention-editor").await;
    state.chrome.location = Some("/A".into());
    state.handle_paste("email@host and @sr tail");
    state.handle_key(KeyAction::Left).await;
    for _ in 0..4 {
        state.handle_key(KeyAction::Left).await;
    }
    let request = state
        .mention_request()
        .expect("space-separated mention before caret");
    assert_eq!(request.query, "sr");
    assert_eq!(&state.input()[request.caret..], " tail");
    assert!(state.apply_file_suggestions(
        request.clone(),
        file_result("/A", 7, &["src/a.rs", "src/b.rs"])
    ));
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.mention_selected(2), 1);
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let previous =
        crate::events::map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)).unwrap();
    state.handle_key(previous).await;
    assert_eq!(state.mention_selected(2), 0);
    let next =
        crate::events::map_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)).unwrap();
    state.handle_key(next).await;
    assert_eq!(state.mention_selected(2), 1);
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), "email@host and @src/b.rs tail");
    assert_eq!(state.editor.cursor, "email@host and @src/b.rs".len());
    assert!(state.mention_options().is_none());
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.input(), "email@host and @sr tail");
    state.handle_key(KeyAction::End).await;
    state.handle_paste(" @");
    let empty = state.mention_request().unwrap();
    assert_eq!(empty.query, "");
    assert!(state.apply_file_suggestions(empty, file_result("/A", 7, &[])));
    assert!(state.mention_options().unwrap().paths.is_empty());
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(state.input().ends_with(" @"));
    assert!(state.mention_request().is_none());
    state.handle_key(KeyAction::Char('x')).await;
    assert_eq!(state.mention_request().unwrap().query, "x");
    state.handle_key(KeyAction::SelectLeft).await;
    assert!(
        state.mention_request().is_none(),
        "selection owns the editor"
    );

    for (text, cursor) in [("foo@bar", 7), ("@a b", 4), ("@a\nb", 4), ("x @a", 1)] {
        assert!(
            crate::autocomplete::mention(text, cursor).is_none(),
            "{text:?} {cursor}"
        );
    }
    assert_eq!(
        crate::autocomplete::mention("next\n@src", 9),
        Some((5, "src"))
    );
}

#[tokio::test]
async fn vis26_stale_edits_location_roundtrip_and_view_instances() {
    let mut state = fresh_state("mention-stale").await;
    state.chrome.location = Some("/A".into());
    state.handle_paste("@");
    let first = state.mention_request().unwrap();
    state.handle_key(KeyAction::Char('x')).await;
    assert!(!state.apply_file_suggestions(first.clone(), file_result("/A", 1, &["old"])));
    let current = state.mention_request().unwrap();
    assert!(!state.apply_file_suggestions(current.clone(), file_result("/B", 1, &["old"])));
    assert!(state.apply_file_suggestions(current.clone(), file_result("/A", 1, &["new"])));
    state.handle_key(KeyAction::Backspace).await;
    state.handle_key(KeyAction::Char('x')).await;
    assert!(!state.apply_file_suggestions(current, file_result("/A", 1, &["old"])));
    let later = state.mention_request().unwrap();
    assert!(!state.apply_file_suggestions(later.clone(), file_result("/A", 2, &["wrong epoch"])));
    state.reset_workspace();
    state.chrome.location = Some("/B".into());
    state.chrome.location = Some("/A".into());
    assert!(!state.apply_file_suggestions(later, file_result("/A", 1, &["old"])));
    let returned = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(returned.clone(), file_result("/A", 3, &["fresh"])));
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    let mut other = TuiState::new_home(app);
    other.chrome.location = Some("/A".into());
    other.handle_paste("@x");
    assert!(!other.apply_file_suggestions(returned, file_result("/A", 3, &["old"])));
    assert_ne!(
        state.mention_request().unwrap().view_id,
        other.mention_request().unwrap().view_id
    );
}

#[tokio::test]
async fn vis26_enter_submits_unmodified_text_without_implicit_file_read() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app);
    state.chrome.location = Some("/A".into());
    state.handle_paste("inspect @src/main.rs");
    let key = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(key, file_result("/A", 1, &["src/main.rs"])));
    state.handle_key(KeyAction::Enter).await;
    let Some(oc_core::core_app::InboxMsg::SubmitFresh { text, .. }) = inbox.recv().await else {
        panic!("ordinary Home submission");
    };
    assert_eq!(text, "inspect @src/main.rs");
}

#[tokio::test]
async fn vis26_tab_never_replaces_an_atomic_paste_chip() {
    let mut state = fresh_state("mention-chip").await;
    state.chrome.location = Some("/A".into());
    state.handle_paste("first\nsecond\n@sr");
    let original = state.input().to_string();
    let key = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(key, file_result("/A", 1, &["src/main.rs"])));
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), original);
    assert_eq!(state.editor.cursor, original.len());
}

#[tokio::test]
async fn vis26_tab_separator_depends_on_next_character_and_keeps_suffix() {
    for (suffix, expected) in [
        ("", "@src/main.rs "),
        (" more", "@src/main.rs more"),
        ("\nmore", "@src/main.rs\nmore"),
        ("more", "@src/main.rs more"),
    ] {
        let mut state = fresh_state("mention-space").await;
        state.chrome.location = Some("/A".into());
        state.handle_key(KeyAction::Char('@')).await;
        state.handle_key(KeyAction::Char('s')).await;
        state.handle_key(KeyAction::Char('r')).await;
        state.handle_paste(suffix);
        for _ in suffix.chars() {
            state.handle_key(KeyAction::Left).await;
        }
        let key = state.mention_request().unwrap();
        assert!(state.apply_file_suggestions(key, file_result("/A", 1, &["src/main.rs"])));
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), expected, "suffix {suffix:?}");
        assert_eq!(
            state.editor.cursor,
            expected.len() - suffix.len(),
            "caret stays before original suffix"
        );
    }
}

#[tokio::test]
async fn vis25_slash_focus_navigation_completion_and_real_actions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = fresh_state("slash-focus").await;
    let mut catalog = snapshot();
    catalog.commands = vec!["project-check".into()];
    state.apply_catalog(catalog);
    state.handle_paste("/side");
    assert_eq!(state.slash_options().unwrap()[0].name, "sidebar");
    assert_eq!(state.handle_key(KeyAction::Enter).await.note, None);
    assert!(
        state.chrome.sidebar_hidden,
        "selected built-in ran its real action"
    );
    assert_eq!(state.input(), "");

    state.handle_paste("/ne");
    assert_eq!(state.slash_options().unwrap()[0].name, "new");
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::NewSession)
    );
    assert_eq!(
        state.input(),
        "/new",
        "owner refusal must retain the selected command"
    );
    state.editor.clear();
    state.input.clear();

    state.handle_paste("/project");
    assert_eq!(state.slash_options().unwrap()[0].name, "project-check");
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(state.input(), "/project-check ");
    assert!(state.slash_options().is_none());
    assert!(state.is_workspace_command(state.input()));
    assert_eq!(state.editor.cursor, state.input().len());

    state.editor.clear();
    state.input.clear();
    state.handle_paste("/ren");
    assert_eq!(state.slash_options().unwrap()[0].name, "rename");
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), "/rename ");
    state.handle_paste("title");
    assert!(state.slash_options().is_none());
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::RenameSessionDirect {
            title: "title".into()
        })
    );

    state.editor.clear();
    state.input.clear();
    state.handle_paste("/");
    let options = state.slash_options().unwrap();
    assert!(options.len() > 1);
    assert_eq!(state.slash_selected, 0);
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.slash_selected, 1);
    let prev =
        crate::events::map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)).unwrap();
    state.handle_key(prev).await;
    assert_eq!(state.slash_selected, 0);
    let next =
        crate::events::map_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)).unwrap();
    state.handle_key(next).await;
    assert_eq!(state.slash_selected, 1);
    state.handle_key(KeyAction::Up).await;
    assert_eq!(state.slash_selected, 0);
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.input(), "/");
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(state.slash_options().is_none());
    state.handle_key(KeyAction::Char('x')).await;
    assert!(
        state.slash_options().is_some(),
        "editing reopens the overlay"
    );
    state.handle_key(KeyAction::Char(' ')).await;
    assert!(state.slash_options().is_none());
    state.handle_key(KeyAction::Backspace).await;
    assert!(state.slash_options().is_some());
    state.reset_workspace();
    assert!(state.commands.is_empty(), "old Location commands are gone");
}

#[tokio::test]
async fn review_is_offered_as_a_workspace_command_on_home_and_session() {
    let mut catalog = snapshot();
    catalog.commands = vec!["review".into()];
    let description = "review changes [commit|branch|pr], defaults to uncommitted";
    catalog
        .command_descriptions
        .insert("review".into(), description.into());
    let (app, _, _) = CoreApp::channel(4);
    let mut home = TuiState::new_home(app);
    home.apply_catalog(catalog.clone());
    let mut session = fresh_state("review-command").await;
    session.apply_catalog(catalog);
    for state in [&mut home, &mut session] {
        state.handle_paste("/rev");
        let options = state.slash_options().expect("slash options");
        assert_eq!(options[0].name, "review");
        assert_eq!(options[0].description, description);
        state.input.clear();
        state.editor.clear();
        state.handle_paste("/ren");
        assert!(
            state
                .slash_options()
                .expect("/ren options")
                .iter()
                .any(|option| option.name == "review" && option.description == description)
        );
        assert!(options[0].arguments);
        assert!(
            options[0].action.is_none(),
            "application owns review execution"
        );
        state.input.clear();
        state.editor.clear();
        state.handle_paste("/rev");
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), "/review ");
        assert!(state.is_workspace_command(state.input()));
    }
}

#[tokio::test]
async fn command_descriptions_follow_reload_and_location_without_fallback() {
    let mut state = fresh_state("description-refresh").await;
    let mut fallback = snapshot();
    fallback.chrome.location = Some("/A".into());
    fallback.commands = vec!["review".into()];
    fallback.command_descriptions.insert(
        "review".into(),
        "review changes [commit|branch|pr], defaults to uncommitted".into(),
    );
    state.apply_catalog(fallback);
    state.handle_paste("/rev");
    assert!(
        state.slash_options().unwrap()[0]
            .description
            .contains("uncommitted")
    );

    let mut overridden = snapshot();
    overridden.chrome.location = Some("/A".into());
    overridden.commands = vec!["review".into()];
    overridden
        .command_descriptions
        .insert("review".into(), "workspace review".into());
    state.refresh_configuration(overridden);
    assert_eq!(state.input(), "/rev");
    assert_eq!(
        state.slash_options().unwrap()[0].description,
        "workspace review"
    );

    let mut other = snapshot();
    other.chrome.location = Some("/B".into());
    other.commands = vec!["review".into()];
    // A definition with no description must not inherit the bundled one.
    other
        .command_descriptions
        .insert("review".into(), String::new());
    state.reset_workspace();
    assert!(state.command_descriptions.is_empty());
    state.apply_catalog(other);
    assert_eq!(state.slash_options().unwrap()[0].description, "");
}

#[tokio::test]
async fn reload_refresh_invalidates_generation_options_without_erasing_draft() {
    let mut state = fresh_state("reload-view").await;
    let mut old = snapshot();
    old.chrome.location = Some("/A".into());
    old.commands = vec!["retired-command".into()];
    state.apply_catalog(old);
    state.handle_paste("@src");
    let stale = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(stale.clone(), file_result("/A", 1, &["src/old.rs"])));
    state.apply_sessions(vec!["stale-session".into()]);
    let mut next = snapshot();
    next.chrome.location = Some("/A".into());
    next.commands = vec!["fresh-command".into()];
    state.refresh_configuration(next);
    assert_eq!(state.input(), "@src");
    assert_eq!(state.attached_session().unwrap().0, "reload-view");
    assert!(state.sessions.is_empty());
    assert!(!state.sessions_loaded);
    assert_eq!(state.commands, ["fresh-command"]);
    assert!(!state.mention_loaded(&stale));
    assert!(!state.apply_file_suggestions(stale, file_result("/A", 1, &["src/old.rs"])));
    assert_eq!(state.mention_request().unwrap().query, "src");
}

#[tokio::test]
async fn home_ren_filter_selects_real_reload_without_stealing_workspace_ren() {
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    let mut home = TuiState::new_home(app);
    let mut catalog = snapshot();
    catalog.commands = vec!["ren".into()];
    home.apply_catalog(catalog);
    home.handle_paste("/ren");
    let options = home.slash_options().unwrap();
    assert_eq!(options[0].name, "ren");
    assert!(!options.iter().any(|option| option.name == "rename"));
    assert!(options.iter().any(|option| option.name == "reload"));
    assert!(
        options[0].action.is_none(),
        "workspace /ren owns exact dispatch"
    );
    home.handle_key(KeyAction::Tab).await;
    assert_eq!(home.input(), "/ren ");
    // The session-only rename is absent from Home suggestions; Tab on
    // the genuine argument-free reload asks the application owner.
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    let mut home = TuiState::new_home(app);
    home.apply_catalog(snapshot());
    home.handle_paste("/ren");
    let options = home.slash_options().unwrap();
    assert_eq!(options[0].name, "reload");
    assert_eq!(
        home.handle_key(KeyAction::Tab).await.intent,
        Some(PanelIntent::ReloadConfiguration)
    );
    assert_eq!(home.input(), "/reload");
}

#[tokio::test]
async fn vis25_home_no_match_and_history_keep_editor_ownership() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    let mut state = TuiState::new_home(app);
    state.handle_paste("/zzzznotacommand");
    assert!(state.slash_options().unwrap().is_empty());
    assert_eq!(
        state.handle_key(KeyAction::Enter).await,
        KeyOutcome::default()
    );
    assert_eq!(
        state.input(),
        "/zzzznotacommand",
        "no-match Enter selects nothing"
    );
    assert_eq!(state.panel(), &TuiPanel::None);
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.input(), "/zzzznotacommand");
    state.editor.clear();
    state.input.clear();
    state.handle_paste("/sessions");
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::LoadSessions)
    );
    assert_eq!(state.panel(), &TuiPanel::Sessions);
    assert_eq!(state.input(), "");
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.panel(), &TuiPanel::None);
    state.handle_paste("/cards");
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.note.as_deref(),
        Some("no session yet")
    );
    assert_eq!(
        state.input(),
        "/cards",
        "unavailable action keeps the draft"
    );
    state.handle_key(KeyAction::SelectHome).await;
    state.handle_paste("/location elsewhere");
    assert!(state.slash_options().is_none());
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::SwitchLocation {
            path: "elsewhere".into()
        })
    );
    assert_eq!(
        state.input(),
        "/location elsewhere",
        "intent awaits owner ACK"
    );
}

#[tokio::test]
async fn vis25_up_owns_selection_then_history_recovers_after_escape() {
    let mut state = fresh_state("slash-history").await;
    state.attach_page(&page(
        vec![msg(1, Role::User, "older prompt")],
        1,
        false,
        false,
    ));
    state.handle_paste("/");
    state.handle_key(KeyAction::Up).await;
    assert_eq!(state.input(), "/");
    assert!(state.slash_selected > 0);
    state.handle_key(KeyAction::Cancel).await;
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        "/",
        "dismissed overlay yields to the raw start first"
    );
    assert_eq!(state.editor.cursor, 0);
    state.handle_key(KeyAction::Up).await;
    assert_eq!(state.input(), "older prompt");
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.input(), "older prompt");
    assert_eq!(state.editor.cursor, "older prompt".len());
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.input(), "/", "history returns to saved draft");
}

#[tokio::test]
async fn vis25_selection_matches_highlight_after_caret_motion_and_catalog_refresh() {
    let mut state = fresh_state("slash-stale-selection").await;
    state.handle_paste("/sidebar");
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::Right).await; // The caret is just after `/`.
    state.handle_key(KeyAction::Up).await; // Last row in the unfiltered list.
    let old = state.slash_selected;
    state.handle_key(KeyAction::End).await;
    assert!(old >= state.slash_options().unwrap().len());
    assert_eq!(state.slash_options().unwrap().len(), 1);
    assert_eq!(
        state.slash_options().unwrap()[state.slash_selected(1)].name,
        "sidebar"
    );
    assert_eq!(
        state.handle_key(KeyAction::Enter).await,
        KeyOutcome {
            consumed_input: true,
            ..KeyOutcome::default()
        }
    );
    assert!(
        state.chrome.sidebar_hidden,
        "Enter activates the highlighted row"
    );
    assert_eq!(state.input(), "");

    let mut catalog = snapshot();
    catalog.commands = vec!["zzzz-project".into()];
    state.apply_catalog(catalog);
    state.handle_paste("/");
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.slash_options().unwrap()[state.slash_selected].name,
        "zzzz-project"
    );
    state.apply_catalog(snapshot()); // This generation no longer has that command.
    let options = state.slash_options().unwrap();
    assert!(state.slash_selected >= options.len());
    let highlighted = options[state.slash_selected(options.len())].name.clone();
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), format!("/{highlighted} "));
}

#[tokio::test]
async fn sidebar_palette_title_tracks_rendered_visibility_and_runs_same_action() {
    use crate::commands::CommandAction;
    use ratatui::{Terminal, backend::TestBackend};

    let mut state = fresh_state("sidebar-palette").await;
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("sidebar");
    let options = state.modal_options();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].value, "session.sidebar.toggle");
    assert_eq!(options[0].title, "Hide sidebar");
    assert_eq!(options[0].footer, "ctrl+x b");
    assert_eq!(
        state.command_unavailable(&CommandAction::ToggleSidebar),
        None
    );
    assert!(state.handle_panel_key(KeyAction::Enter).consumed_input);
    assert!(state.chrome.sidebar_hidden);
    assert_eq!(state.panel(), &TuiPanel::None);

    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("sidebar");
    let options = state.modal_options();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].title, "Show sidebar");
    assert_eq!(options[0].value, "session.sidebar.toggle");
    assert!(state.handle_panel_key(KeyAction::Enter).consumed_input);
    assert!(!state.chrome.sidebar_hidden);
}

#[tokio::test]
async fn sidebar_palette_uses_effective_width_home_and_child() {
    use ratatui::{Terminal, backend::TestBackend};

    let mut state = fresh_state("sidebar-breakpoint").await;
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("sidebar");
    let title = |state: &TuiState| {
        state
            .modal_options()
            .iter()
            .find(|o| o.value == "session.sidebar.toggle")
            .expect("sidebar palette option")
            .title
            .clone()
    };
    assert_eq!(title(&state), "Show sidebar", "no width observed yet");
    for (width, rail, expected) in [
        (120, 0, "Show sidebar"),
        (121, 0, "Hide sidebar"),
        (160, 40, "Show sidebar"),
        (161, 40, "Hide sidebar"),
    ] {
        state.chrome.vertical_tabs_width = rail;
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        assert_eq!(
            title(&state),
            expected,
            "terminal width {width}, rail {rail}"
        );
    }
    state.parent_id = Some("parent".into());
    state.chrome.vertical_tabs_width = 0;
    let mut wide = Terminal::new(TestBackend::new(160, 40)).unwrap();
    wide.draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert_eq!(title(&state), "Show sidebar");

    let (app, _, _) = CoreApp::channel(4);
    let mut home = TuiState::new_home(app);
    wide.draw(|frame| crate::shell::render(frame, &home))
        .unwrap();
    home.handle_key(KeyAction::Commands).await;
    home.handle_paste("sidebar");
    assert_eq!(title(&home), "Show sidebar");
}

#[tokio::test]
async fn rename_shortcut_edits_graphemes_and_waits_for_owner_ack() {
    use crate::commands::CommandAction;
    let mut state = fresh_state("rename-shortcut").await;
    state.session_title = Some("е\u{301}🧑‍💻界".into());
    state.set_tab_strip(
        vec![TabPresentation {
            title: state.session_title.clone(),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("rename-shortcut"))
        }],
        0,
        true,
    );
    type_text(&mut state, "unsent prompt").await;
    let (draft, caret) = (state.input.clone(), state.editor.cursor);
    assert_eq!(state.handle_key(KeyAction::Rename).await.intent, None);
    assert_eq!(state.panel(), &TuiPanel::Rename);
    assert_eq!(state.rename_title(), Some("е\u{301}🧑‍💻界"));
    assert_eq!(state.rename_cursor(), "е\u{301}🧑‍💻界".len());
    state.handle_key(KeyAction::Left).await;
    state.handle_key(KeyAction::Backspace).await;
    assert_eq!(state.rename_title(), Some("е\u{301}界"));
    state.handle_paste("  🦊\n\tnew  ");
    assert_eq!(state.rename_title(), Some("е\u{301}  🦊  new  界"));
    let outcome = state.handle_key(KeyAction::Enter).await;
    let title = "е\u{301}  🦊  new  界".to_string();
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::RenameSession {
            title: title.clone()
        })
    );
    assert_eq!(state.panel(), &TuiPanel::Rename);
    assert_eq!(state.session_title.as_deref(), Some("е\u{301}🧑‍💻界"));
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        None,
        "no duplicate while waiting"
    );
    state.rename_session_rejected("rename failed".into());
    assert_eq!(state.note(), Some("rename failed"));
    assert_eq!(state.rename_title(), Some(title.as_str()));
    assert_eq!(state.input(), draft);
    assert_eq!(state.editor.cursor, caret);
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::RenameSession {
            title: title.clone()
        })
    );
    state.rename_session_applied("wrong title".into());
    assert_eq!(state.panel(), &TuiPanel::Rename);
    state.rename_session_applied(title.clone());
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.session_title.as_deref(), Some(title.as_str()));
    assert_eq!(
        state.tab_presentation().0[0].title.as_deref(),
        Some(title.as_str())
    );
    assert_eq!(state.input(), draft);
    assert_eq!(state.editor.cursor, caret);
    assert_eq!(
        state.command_unavailable(&CommandAction::RenameSession { title: None }),
        None
    );
}

#[tokio::test]
async fn rename_palette_slash_blank_cancel_and_refusal() {
    use crate::commands::CommandAction;
    let (app, _, _) = CoreApp::channel(4);
    let mut home = TuiState::new_home(app);
    assert_eq!(
        home.handle_key(KeyAction::Rename).await.note.as_deref(),
        Some("no session yet")
    );
    home.handle_key(KeyAction::Commands).await;
    home.handle_paste("Rename session");
    assert!(home.modal_options().is_empty());

    let mut state = fresh_state("rename-palette").await;
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("Rename session");
    assert_eq!(state.modal_options()[0].value, "session.rename");
    assert_eq!(state.modal_options()[0].footer, "ctrl+r");
    state.handle_panel_key(KeyAction::Enter);
    assert_eq!(state.rename_title(), Some(""));
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    state.handle_paste("  \n\t");
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    assert_eq!(state.panel(), &TuiPanel::Rename);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.rename_title(), None);
    assert_eq!(state.session_title, None);

    type_text(&mut state, "/rename").await;
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    assert_eq!(
        state.input(),
        "/rename ",
        "upstream slash.arguments completes first"
    );
    state.handle_key(KeyAction::Backspace).await;
    state.handle_key(KeyAction::Cancel).await; // Dismiss to submit the owner-backed bare action.
    let bare = state.handle_key(KeyAction::Enter).await;
    assert_eq!(bare.intent, Some(PanelIntent::RegenerateTitle));
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.input(), "/rename");
    state.regenerated_title(Err("provider unavailable".into()));
    assert_eq!(state.input(), "/rename");
    assert_eq!(state.note(), Some("provider unavailable"));
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::RegenerateTitle)
    );
    state.regenerated_title(Ok("New generated title".into()));
    assert_eq!(state.input(), "");
    assert_eq!(state.session_title.as_deref(), Some("New generated title"));

    state.parent_id = Some("parent".into());
    assert_eq!(
        state
            .run_command(CommandAction::RenameSession { title: None })
            .note
            .as_deref(),
        Some("child session is read-only")
    );
    state.parent_id = None;
    state.set_tab_strip(
        vec![TabPresentation {
            title: None,
            home: false,
            busy: true,
            ..TabPresentation::new(sid("busy"))
        }],
        0,
        false,
    );
    assert_eq!(
        state.handle_key(KeyAction::Rename).await.note.as_deref(),
        Some("tab busy; action unavailable")
    );
}

#[tokio::test]
async fn ctrl_c_clears_focused_rename_prefill_before_dismissing_dialog() {
    let mut state = fresh_state("rename-interrupt").await;
    state.session_title = Some("Existing title".into());
    type_text(&mut state, "untouched prompt").await;
    state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.rename_title(), Some("Existing title"));
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.rename_title(), Some(""));
    assert_eq!(state.rename_cursor(), 0);
    assert_eq!(state.input(), "untouched prompt");
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(
        state.rename_title(),
        Some(""),
        "old prefill must not return"
    );
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.session_title.as_deref(), Some("Existing title"));
    assert_eq!(state.input(), "untouched prompt");

    state.handle_key(KeyAction::Rename).await;
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(
        state.panel(),
        &TuiPanel::None,
        "Esc still closes immediately"
    );
}

#[tokio::test]
async fn rename_oversized_prefill_never_submits_a_prefix_and_can_be_replaced() {
    let mut state = fresh_state("rename-generated").await;
    let generated = "🙂".repeat(100);
    assert_eq!(generated.len(), 400);
    state.session_title = Some(generated.clone());
    let opened = state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.rename_title(), Some(generated.as_str()));
    assert!(
        opened
            .note
            .as_deref()
            .unwrap()
            .contains("shorten it or select all")
    );
    assert_eq!(state.rename_cursor(), generated.len());
    assert!(state.rename_editor.retained_bytes() <= 400 * 32);
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));

    state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.handle_key(KeyAction::Char('x')).await.intent, None);
    assert_eq!(state.rename_title(), Some(generated.as_str()));
    state.handle_key(KeyAction::Backspace).await;
    assert_eq!(state.rename_title(), Some("🙂".repeat(99).as_str()));
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    assert_eq!(state.panel(), &TuiPanel::Rename);
    assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));
    state.handle_key(KeyAction::SelectHome).await;
    state.handle_paste("е\u{301}🧑‍💻 renamed");
    let title = "е\u{301}🧑‍💻 renamed".to_string();
    assert_eq!(state.rename_title(), Some(title.as_str()));
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::RenameSession {
            title: title.clone()
        })
    );
    assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));
    state.rename_session_applied(title.clone());
    assert_eq!(state.session_title.as_deref(), Some(title.as_str()));
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn rename_full_unicode_prefill_stays_on_one_row_at_narrow_width() {
    use ratatui::{
        Terminal,
        backend::{Backend, TestBackend},
        layout::Rect,
    };

    let mut state = fresh_state("rename-narrow").await;
    let title = "🙂".repeat(100);
    state.session_title = Some(title.clone());
    state.handle_key(KeyAction::Rename).await;
    let mut terminal = Terminal::new(TestBackend::new(43, 24)).unwrap();
    terminal
        .draw(|frame| crate::dialog::render(frame, &state))
        .unwrap();
    let rect = crate::dialog::rename_geometry(Rect::new(0, 0, 43, 24));
    let buffer = terminal.backend().buffer();
    assert!((rect.x + 2..rect.right() - 2).any(|x| buffer[(x, rect.y + 3)].symbol() == "🙂"));
    assert!((rect.x + 2..rect.right() - 2).all(|x| buffer[(x, rect.y + 4)].symbol() == " "));
    assert_eq!(
        terminal.backend_mut().get_cursor_position().unwrap().y,
        rect.y + 3
    );
    assert_eq!(state.rename_title(), Some(title.as_str()));
}

#[tokio::test]
async fn rename_limits_prefill_typing_and_paste_to_owner_bytes_without_splitting_graphemes() {
    let mut state = fresh_state("rename-limits").await;
    state.session_title = Some(format!("{}🧑‍💻", "a".repeat(252)));
    let opened = state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.rename_title(), state.session_title.as_deref());
    assert!(
        opened
            .note
            .as_deref()
            .unwrap()
            .contains("shorten it or select all")
    );
    state.handle_key(KeyAction::Backspace).await;
    assert_eq!(state.rename_title(), Some("a".repeat(252).as_str()));
    assert_eq!(state.handle_key(KeyAction::Char('界')).await.intent, None);
    assert_eq!(state.rename_title().unwrap().len(), 255);
    assert_eq!(
        state
            .handle_key(KeyAction::Char('界'))
            .await
            .note
            .as_deref(),
        Some("session title truncated at 256 bytes")
    );
    let outcome = state.handle_paste("🦊界");
    assert_eq!(
        outcome.note.as_deref(),
        Some("session title truncated at 256 bytes")
    );
    assert_eq!(state.rename_title().unwrap().len(), 255);
    state.handle_key(KeyAction::Backspace).await;
    let outcome = state.handle_paste("界🦊");
    assert_eq!(
        outcome.note.as_deref(),
        Some("session title truncated at 256 bytes")
    );
    assert_eq!(
        state.rename_title(),
        Some(format!("{}界", "a".repeat(252)).as_str())
    );
    let intent = state.handle_key(KeyAction::Enter).await.intent;
    assert_eq!(
        intent,
        Some(PanelIntent::RenameSession {
            title: format!("{}界", "a".repeat(252))
        })
    );
    assert!(state.rename_title().unwrap().len() <= MAX_SESSION_TITLE_BYTES);
}

#[tokio::test]
async fn rename_combined_unicode_prefill_and_editor_history_stay_bounded() {
    let mut state = fresh_state("rename-combined").await;
    // 100 Unicode scalar values, with combined graphemes and UTF-8 >256.
    let original = format!("{}{}", "🧑‍💻".repeat(20), "е\u{301}".repeat(20));
    assert_eq!(original.chars().count(), 100);
    assert!(original.len() > MAX_SESSION_TITLE_BYTES);
    assert!(original.len() <= 400);
    state.session_title = Some(original.clone());
    state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.rename_title(), Some(original.as_str()));
    state.handle_key(KeyAction::Backspace).await;
    assert_eq!(
        state.rename_title(),
        Some(format!("{}{}", "🧑‍💻".repeat(20), "е\u{301}".repeat(19)).as_str())
    );
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.rename_title(), Some(original.as_str()));
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    assert_eq!(state.session_title.as_deref(), Some(original.as_str()));

    // Even malformed/older oversized titles do not create an unbounded
    // modal copy or retained undo snapshots.
    state.session_title = Some("🙂".repeat(10_000));
    state.handle_key(KeyAction::Rename).await;
    assert_eq!(state.rename_title(), Some(""));
    for _ in 0..40 {
        state.handle_paste(&"🧑‍💻".repeat(100));
        state.handle_key(KeyAction::Undo).await;
    }
    assert!(state.rename_title().unwrap().len() <= 400);
    assert!(state.rename_editor.retained_bytes() <= 32 * 400);
    state.close_panel();
    assert_eq!(state.rename_editor.retained_bytes(), 0);
}

#[tokio::test]
async fn slash_rename_direct_waits_for_owner_and_retains_draft_on_async_failure() {
    let mut state = fresh_state("rename-direct").await;
    state.set_tab_strip(
        vec![TabPresentation {
            title: None,
            home: false,
            busy: false,
            ..TabPresentation::new(sid("rename-direct"))
        }],
        0,
        true,
    );
    state.input = "/rename  🦊 edited  ".into();
    state.editor.cursor = state.input.len();
    let original = state.input.clone();
    let intent = state.handle_key(KeyAction::Enter).await.intent;
    assert_eq!(
        intent,
        Some(PanelIntent::RenameSessionDirect {
            title: "🦊 edited".into()
        })
    );
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.input(), original);
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
    state.rename_session_applied("🦊 edited".into());
    assert_eq!(
        state.session_title, None,
        "dialog ACK must not accept a direct request"
    );
    state.rename_session_direct_rejected("storage failed".into());
    assert_eq!(state.note(), Some("storage failed"));
    assert_eq!(state.input(), original);
    assert_eq!(state.handle_key(KeyAction::Enter).await.intent, intent);
    state.rename_session_direct_applied("wrong title".into());
    assert_eq!(state.input(), original);
    state.rename_session_direct_applied("🦊 edited".into());
    assert_eq!(state.input(), "");
    assert_eq!(state.session_title.as_deref(), Some("🦊 edited"));
    assert_eq!(
        state.tab_presentation().0[0].title.as_deref(),
        Some("🦊 edited")
    );

    state.input = format!("/rename {}🦊", "a".repeat(MAX_SESSION_TITLE_BYTES));
    state.editor.cursor = state.input.len();
    let invalid = state.handle_key(KeyAction::Enter).await;
    assert_eq!(invalid.intent, None);
    assert!(invalid.note.as_deref().unwrap().contains("256 bytes"));
    assert!(!state.input().is_empty());

    state.input = "/rename short".into();
    state.editor.cursor = state.input.len();
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::RenameSessionDirect {
            title: "short".into()
        })
    );
    state.handle_key(KeyAction::Char('!')).await;
    state.rename_session_direct_applied("short".into());
    assert_eq!(state.input(), "/rename short!");
}

#[tokio::test]
async fn aud33_paste_is_never_silently_cut_below_the_input_budget() {
    let mut state = fresh_state("s-aud33-paste").await;
    // 100 KiB of multibyte text: above the old 64 KiB transport cap and
    // well inside the configured input budget, so nothing may be dropped.
    let big = "п".repeat(50 * 1024);
    let outcome = state.handle_paste(&big);
    assert_eq!(state.input().len(), big.len(), "paste must be kept whole");
    assert!(outcome.note.is_none(), "no diagnostic without a drop");

    // Above the input budget the cut is bounded and reported.
    let huge = "x".repeat(MAX_INPUT_BYTES + 4096);
    let outcome = state.handle_paste(&huge);
    assert_eq!(state.input().len(), MAX_INPUT_BYTES);
    let note = outcome.note.expect("a dropped paste must be reported");
    assert!(
        note.contains("paste") && note.contains(&MAX_INPUT_BYTES.to_string()),
        "note must name the paste and the budget: {note}"
    );
    state.handle_key(KeyAction::Interrupt).await;
    let full = &huge[..MAX_INPUT_BYTES];
    state.handle_paste(full);
    assert_eq!(state.editor.chip_count(), 1);
    assert!(state.handle_paste(&huge).note.is_some());
    assert_eq!(
        state.editor.chip_count(),
        1,
        "truncated prefix is not an identical paste"
    );
    assert_eq!(state.input(), full);
    assert!(state.handle_paste(full).note.is_none());
    assert_eq!(
        state.editor.chip_count(),
        0,
        "identical full-budget paste can expand"
    );
    assert_eq!(state.input(), full);
}

#[tokio::test]
async fn accepted_text_only_mention_recalled_in_same_session() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    let session = sid("mention-recall");
    app.create_session(session.clone()).await.unwrap();
    let mut state = TuiState::new(app.clone(), session);
    state.handle_paste("  @file.rs  ");
    state.editor.mark_file_mention(2, 10, &state.input);
    state.handle_key(KeyAction::Enter).await;
    await_submission(&mut state).await;
    assert!(state.input.is_empty());
    assert_eq!(state.window.rows()[0].text, "@file.rs");
    assert!(state.recall_history(true));
    assert_eq!(state.input, "@file.rs");
    assert_eq!(
        state.editor.layout(&state.input, 80).0[0].spans[0],
        ("@file.rs".into(), false, true)
    );
    state.set_session(sid("another-session"));
    assert_eq!(state.editor.retained_bytes(), 0);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn sessionless_home_keeps_bounded_editor_and_refuses_session_actions() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    assert!(state.home);
    assert!(HOME_EXAMPLES.contains(&state.home_example));
    assert_eq!(state.attached_session(), None);
    assert!(state.history().rows().is_empty());
    assert!(
        crate::views::render_test(&state, 80, 24)
            .join("\n")
            .contains("Ask anything")
    );
    state.handle_key(KeyAction::Enter).await;
    state.handle_paste("  \n ");
    state.handle_key(KeyAction::Enter).await;
    assert!(
        inbox.try_recv().is_err(),
        "empty Home has no candidate session"
    );
    assert!(state.attached_session().is_none());
    assert_eq!(
        state.request_compress(String::new()),
        Err(CoreError::SessionNotFound)
    );
    for action in [
        crate::commands::CommandAction::OpenCards,
        crate::commands::CommandAction::OpenDcp,
        crate::commands::CommandAction::DcpCompress {
            focus: String::new(),
        },
    ] {
        let result = state.run_command(action);
        assert_eq!(result.note.as_deref(), Some("no session yet"));
        assert_eq!(result.intent, None);
        assert_eq!(state.panel(), &TuiPanel::None);
    }
    state.panel = TuiPanel::Dcp;
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    state.panel = TuiPanel::Cards;
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    state.close_panel();
    state.handle_paste(&"x".repeat(MAX_INPUT_BYTES + 1));
    assert_eq!(state.input().len(), MAX_INPUT_BYTES);
    assert!(state.attached_session().is_none());
}

#[tokio::test]
async fn v05_review_overlimit_selection_during_pending_cannot_lose_draft() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("overlimit-pending"));
    let mut full = "a".repeat(MAX_INPUT_BYTES - 1);
    assert_eq!(state.handle_paste(&full).note, None);
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
    state.handle_key(KeyAction::Char('z')).await;
    full.push('z');
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    assert_eq!(text.len(), MAX_INPUT_BYTES);
    state.handle_key(KeyAction::SelectLeft).await;
    let outcome = state.handle_key(KeyAction::Char('🦊')).await;
    assert!(outcome.note.is_some(), "replacement cannot fit");
    assert_eq!(
        state.input(),
        full,
        "rejected replacement must not delete selection"
    );
    state.handle_key(KeyAction::Backspace).await;
    assert_eq!(state.input().len(), MAX_INPUT_BYTES - 1);
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(
        state.input().len(),
        MAX_INPUT_BYTES - 1,
        "acceptance must not clear revised draft"
    );
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("accepted".into())));
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
}

#[tokio::test]
async fn v05_review_chip_trim_keeps_visible_and_submitted_draft_in_sync() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("chip-trim"));
    let outcome = state.handle_paste("a\nb\nc\n");
    assert!(
        outcome
            .note
            .as_deref()
            .is_some_and(|n| n.contains("trimmed"))
    );
    assert_eq!(state.input(), "a\nb\nc");
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~3 lines] ");
    state.handle_key(KeyAction::Char('Z')).await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    assert_eq!(text, "a\nb\ncZ");
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.history().rows()[0].text, text);

    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("plain-paste"));
    state.handle_paste("a\nb\n");
    assert_eq!(state.input(), "a\nb\n", "non-chip paste retains whitespace");
    assert_eq!(state.prompt_layout(80).0.len(), 3);
}

#[tokio::test]
async fn vis07_paste_mouse_expands_only_live_painted_prompt_cells() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let area = Rect::new(0, 0, 120, 40);
    let prefix = "VIS11 full draft αβ caret-middle preserving every wzord";
    let pasted = "VIS11-PASTE-0\nVIS11-PASTE-1\nVIS11-PASTE-2";
    state.handle_paste(prefix);
    state.handle_paste(pasted);
    let mut actual = state.input().to_owned();
    let chip_end = actual.len();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mouse = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let down = MouseEventKind::Down(MouseButton::Left);
    state.handle_mouse(mouse(down, 81, 21), area);
    assert_eq!(
        state.editor.chip_count(),
        1,
        "unpainted prompt has no targets"
    );
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let (first, second) = {
        let paint = state.painted_prompt.borrow();
        let paint = paint.as_ref().unwrap();
        assert_eq!(state.prompt_width.get(), Some(70));
        assert_eq!(paint.chips.len(), 2);
        (paint.chips[0].0, paint.chips[1].0)
    };
    assert_eq!(
        (first.x, first.width, second.x, second.width),
        (81, 11, 26, 6)
    );
    // Styles and caret come from the exact observed 70-cell projection.
    let buffer = terminal.backend().buffer();
    for rect in [first, second] {
        for x in rect.x..rect.right() {
            let cell = &buffer[(x, rect.y)];
            assert_eq!(cell.fg, ratatui::style::Color::Rgb(10, 10, 10));
            assert_eq!(cell.bg, ratatui::style::Color::Rgb(245, 167, 66));
            assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
        }
    }
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        (33, second.y).into()
    );
    for _ in 0..20 {
        state.handle_key(KeyAction::Newline).await;
    }
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    assert!(
        state
            .painted_prompt
            .borrow()
            .as_ref()
            .unwrap()
            .chips
            .is_empty()
    );
    state.handle_mouse(mouse(down, first.x, first.y), area);
    assert_eq!(state.editor.chip_count(), 1, "offscreen chip has no target");
    for _ in 0..20 {
        state.handle_key(KeyAction::Undo).await;
    }
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    state.handle_mouse(mouse(MouseEventKind::Moved, first.x, first.y), area);
    state.handle_mouse(
        mouse(MouseEventKind::Down(MouseButton::Right), first.x, first.y),
        area,
    );
    state.handle_mouse(mouse(down, second.right(), second.y), area);
    assert_eq!(
        state.editor.chip_count(),
        1,
        "virtual space is not an extmark cell"
    );
    state.handle_mouse(mouse(down, first.x, first.y), Rect::new(0, 0, 119, 40));
    assert_eq!(
        state.editor.chip_count(),
        1,
        "resize invalidates coordinates"
    );
    state.handle_key(KeyAction::Commands).await;
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    state.handle_mouse(mouse(down, first.x, first.y), area);
    state.close_panel();
    state.handle_mouse(mouse(down, first.x, first.y), area);
    assert_eq!(
        state.editor.chip_count(),
        1,
        "modal paint removed the prompt map"
    );
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    state.handle_key(KeyAction::Left).await;
    state.handle_mouse(mouse(down, first.x, first.y), area);
    assert_eq!(
        state.editor.chip_count(),
        1,
        "caret/viewport identity is stale"
    );
    state.handle_key(KeyAction::Right).await;
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    state.handle_key(KeyAction::Char('!')).await;
    state.handle_mouse(mouse(down, first.x, first.y), area);
    assert_eq!(state.editor.chip_count(), 1, "edited draft is stale");
    state.handle_key(KeyAction::Undo).await;
    state.push_note(&format!("{}\n", "toast ".repeat(8)).repeat(30));
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let toast = crate::shell::toast_rect(&state, area).unwrap();
    assert!(toast.contains((first.x, first.y).into()));
    state.handle_mouse(mouse(down, first.x, first.y), area);
    state.note = None;
    state.handle_mouse(mouse(down, first.x, first.y), area);
    assert_eq!(
        state.editor.chip_count(),
        1,
        "toast-covered cells were not painted prompt"
    );
    state.handle_paste(" suffix");
    actual.push_str(" suffix");
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    state.handle_mouse(mouse(down, second.x + 2, second.y), area);
    assert_eq!(
        state.editor.chip_count(),
        0,
        "left-down expands the wrapped continuation"
    );
    assert_eq!(state.input(), actual);
    assert_eq!(state.editor.cursor, chip_end);
    assert!(state.editor.selected().is_none());
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.editor.chip_count(), 1);
    state.handle_key(KeyAction::Redo).await;
    assert_eq!(state.editor.chip_count(), 0);
    assert_eq!(state.input(), actual);
    // Repeat paste follows the same extmark expansion and preserves suffix.
    state.handle_key(KeyAction::Undo).await;
    state.editor.move_to(chip_end, false);
    assert!(state.handle_paste(pasted).note.is_none());
    assert_eq!(state.editor.chip_count(), 0);
    assert_eq!(state.input(), actual);
}

#[tokio::test]
async fn vis07_up_down_use_painted_width_before_history_and_select_raw_wrap() {
    use ratatui::{Terminal, backend::TestBackend};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    state.attach_page(&page(
        vec![msg(1, Role::User, "previous prompt")],
        1,
        false,
        false,
    ));
    // Home is the 70-cell textarea at 120 columns, independent of session width.
    state.home = true;
    let text =
        "VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord";
    state.restore_prompt(text.into());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    assert_eq!(state.prompt_layout(70).1, (1, 17));
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        text,
        "first Up moves within the visual draft"
    );
    assert_eq!(state.prompt_layout(70).1, (0, 17));
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.prompt_layout(70).1, (1, 17));
    state.handle_key(KeyAction::SelectUp).await;
    assert_eq!(state.editor.selected(), Some((17, text.len())));
    assert!(state.prompt_layout(70).0[1].spans.iter().all(|s| s.1));
    state.handle_key(KeyAction::Right).await;
    state.handle_key(KeyAction::Up).await;
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        text,
        "visual-edge Up first moves to raw start"
    );
    assert_eq!(state.editor.cursor, 0);
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        "previous prompt",
        "only a subsequent Up at raw start recalls history"
    );
    assert_eq!(
        state.editor.cursor, 0,
        "previous history starts at raw start"
    );
    state.handle_key(KeyAction::Down).await;
    assert_eq!(
        state.input(),
        "previous prompt",
        "last-row Down first moves to raw end"
    );
    assert_eq!(state.editor.cursor, "previous prompt".len());
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.input(), text);
    assert_eq!(state.prompt_layout(70).1, (1, 17));
    let last_row_middle = text.find(" actual").unwrap() + 1 + 5;
    state.editor.move_to(last_row_middle, false);
    assert_eq!(state.prompt_layout(70).1, (1, 5));
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.input(), text);
    assert_eq!(state.editor.cursor, text.len());
}

#[tokio::test]
async fn vis07_repeat_paste_after_real_suffix_space_inserts_a_new_chip() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    state.handle_paste("a\nb\nc");
    type_text(&mut state, " suffix").await;
    for _ in 0.."suffix".len() {
        state.handle_key(KeyAction::Left).await;
    }
    assert_eq!(state.editor.cursor, "a\nb\nc ".len());
    assert!(state.handle_paste("a\nb\nc").note.is_none());
    assert_eq!(state.input(), "a\nb\nc a\nb\ncsuffix");
    assert_eq!(state.editor.chip_count(), 2);
    assert_eq!(state.editor.cursor, "a\nb\nc a\nb\nc".len());
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.input(), "a\nb\nc suffix");
    assert_eq!(state.editor.chip_count(), 1);
    state.handle_key(KeyAction::Redo).await;
    assert_eq!(state.input(), "a\nb\nc a\nb\ncsuffix");
    assert_eq!(state.editor.chip_count(), 2);
}

#[tokio::test]
async fn v05_review_trimmed_chip_edit_during_pending_keeps_receipt_immutable() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("trimmed-pending"));
    assert!(state.handle_paste("a\nb\nc\n").note.is_some());
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    assert_eq!(text, "a\nb\nc");
    state.handle_key(KeyAction::Char('Z')).await;
    state.handle_key(KeyAction::Enter).await; // pending cannot duplicate
    assert!(inbox.try_recv().is_err());
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.input(), "a\nb\ncZ");
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~3 lines] Z");
    assert_eq!(state.history().rows()[0].text, text);
}

#[tokio::test]
async fn vis11_pending_interrupt_consumed_then_normal_exit_remains_available() {
    let mut state = fresh_state("leader-exit").await;
    state.handle_key(KeyAction::Leader).await;
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert!(!state.leader_pending());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Quit);
}

#[tokio::test]
async fn vis11_leader_pending_lifecycle_preserves_editor_and_expires_without_input() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("leader-lifecycle"));
    state.chrome.animations = Some(false);
    state.chrome.leader_timeout_ms = Some(321);
    state.handle_paste("first\nsecond\nthird\n");
    type_text(&mut state, "🦊draft").await;
    state.handle_key(KeyAction::Left).await;
    let draft = state.input.clone();
    let caret = state.editor.cursor;
    let layout: Vec<_> = state
        .prompt_layout(80)
        .0
        .into_iter()
        .map(|row| row.spans)
        .collect();
    let key = |code, modifiers| KeyEvent::new(code, modifiers);
    for ending in [
        KeyCode::Esc,
        KeyCode::Backspace,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char('c'),
        KeyCode::Char('x'),
    ] {
        let leader = state
            .terminal_key(key(KeyCode::Char('x'), KeyModifiers::CONTROL))
            .unwrap();
        state.handle_key(leader).await;
        assert!(state.leader_pending());
        let deadline = state.next_ui_deadline().unwrap();
        assert_eq!(deadline, state.leader.unwrap() + Duration::from_millis(321));
        let next = state
            .terminal_key(key(
                ending,
                if matches!(ending, KeyCode::Char('x' | 'c')) {
                    KeyModifiers::CONTROL
                } else {
                    KeyModifiers::NONE
                },
            ))
            .unwrap();
        assert!(state.handle_key(next).await.intent.is_none());
        assert!(!state.leader_pending());
        assert_eq!(state.input, draft);
        assert_eq!(state.editor.cursor, caret);
        assert_eq!(
            state
                .prompt_layout(80)
                .0
                .into_iter()
                .map(|row| row.spans)
                .collect::<Vec<_>>(),
            layout
        );
        assert!(inbox.try_recv().is_err());
    }
    state.handle_key(KeyAction::Leader).await;
    let until = state.next_ui_deadline().unwrap();
    assert!(!state.tick_ui(until - Duration::from_millis(1)));
    assert!(state.tick_ui(until));
    assert!(!state.leader_pending());
    assert_eq!(state.next_ui_deadline(), None);
    assert!(!state.tick_ui(until + Duration::from_secs(60)));
    assert_eq!(state.input, draft);
    assert_eq!(state.editor.cursor, caret);

    state.handle_key(KeyAction::Leader).await;
    let next = state
        .terminal_key(key(KeyCode::Char('z'), KeyModifiers::NONE))
        .unwrap();
    state.handle_key(next).await;
    let mut edited = draft.clone();
    edited.insert(caret, 'z');
    assert_eq!(state.input, edited);
    assert_eq!(state.editor.cursor, caret + 1);
    assert!(!state.leader_pending());
    assert_eq!(state.editor.chip_count(), 1);
    assert!(inbox.try_recv().is_err());
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.input, draft);

    let mut catalog = snapshot();
    catalog.chrome.conversation_shortcuts.leader = "alt+x".into();
    state.apply_catalog(catalog);
    let leader = state
        .terminal_key(key(KeyCode::Char('x'), KeyModifiers::ALT))
        .unwrap();
    state.handle_key(leader).await;
    let next = state
        .terminal_key(key(KeyCode::Char('m'), KeyModifiers::NONE))
        .unwrap();
    state.handle_key(next).await;
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert!(!state.leader_pending());
    assert_eq!(state.input, draft);
    assert!(inbox.try_recv().is_err());
    state.close_panel();
    let leader = state
        .terminal_key(key(KeyCode::Char('x'), KeyModifiers::ALT))
        .unwrap();
    state.handle_key(leader).await;
    let next = state
        .terminal_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    state.handle_key(next).await;
    assert!(!state.leader_pending());
    let Some(oc_core::core_app::InboxMsg::Submit { text, .. }) = inbox.try_recv().ok() else {
        panic!("unmatched Enter must use normal submission")
    };
    assert_eq!(text, draft);
}

#[tokio::test]
async fn vis11_configured_palette_chord_and_modal_printable_keep_focus() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut state = fresh_state("leader-palette").await;
    type_text(&mut state, "complete composer draft").await;
    state.handle_key(KeyAction::Left).await;
    let (draft, caret) = (state.input.clone(), state.editor.cursor);
    let mut catalog = snapshot();
    catalog.chrome.conversation_shortcuts.leader = "ctrl+g".into();
    catalog.chrome.command_palette_shortcut = Some("ctrl+g p".into());
    state.apply_catalog(catalog);
    let key = |value, modifiers| KeyEvent::new(KeyCode::Char(value), modifiers);
    assert_eq!(
        state.terminal_key(key('p', KeyModifiers::CONTROL)),
        None,
        "override removes old direct root shortcut"
    );
    let leader = state.terminal_key(key('g', KeyModifiers::CONTROL)).unwrap();
    state.handle_key(leader).await;
    assert!(state.leader_pending());
    let next = state.terminal_key(key('p', KeyModifiers::NONE)).unwrap();
    state.handle_key(next).await;
    assert_eq!(state.panel(), &TuiPanel::Commands);
    assert!(!state.leader_pending());
    let leader = state.terminal_key(key('g', KeyModifiers::CONTROL)).unwrap();
    state.handle_key(leader).await;
    assert!(state.leader_pending());
    let deadline = state.next_ui_deadline().unwrap();
    assert!(state.tick_ui(deadline));
    assert_eq!(state.select.query, "");
    assert_eq!(state.panel(), &TuiPanel::Commands);
    let leader = state.terminal_key(key('g', KeyModifiers::CONTROL)).unwrap();
    state.handle_key(leader).await;
    let next = state.terminal_key(key('z', KeyModifiers::NONE)).unwrap();
    state.handle_key(next).await;
    assert!(!state.leader_pending());
    assert_eq!(state.select.query, "z");
    assert_eq!(state.input, draft);
    assert_eq!(state.editor.cursor, caret);
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(state.input, draft);
    assert_eq!(state.editor.cursor, caret);
}

#[tokio::test]
async fn vis11_commands_hints_project_effective_leaders_and_literal_overrides() {
    use crate::commands::CommandAction;
    let mut state = fresh_state("leader-hints").await;
    state.chrome.conversation_shortcuts.leader = "ctrl+g,alt+x".into();
    state.chrome.command_palette_shortcut = Some("ctrl+g p,alt+p".into());
    let spec = crate::commands::spec;
    assert_eq!(
        state.command_footer(spec(&CommandAction::NewSession)),
        "ctrl+g n alt+x n"
    );
    assert_eq!(
        state.command_footer(spec(&CommandAction::OpenModelPicker)),
        "ctrl+g m alt+x m"
    );
    assert_eq!(
        state.command_footer(spec(&CommandAction::OpenAgents)),
        "ctrl+g a alt+x a shift+tab"
    );
    assert_eq!(
        state.command_footer(spec(&CommandAction::OpenCommands)),
        "ctrl+g p alt+p"
    );
    for disabled in ["", "none"] {
        state.chrome.conversation_shortcuts.leader = disabled.into();
        assert_eq!(state.command_footer(spec(&CommandAction::NewSession)), "");
        assert_eq!(
            state.command_footer(spec(&CommandAction::OpenModelPicker)),
            ""
        );
        assert_eq!(
            state.command_footer(spec(&CommandAction::OpenAgents)),
            "shift+tab"
        );
        assert_eq!(
            state.command_footer(spec(&CommandAction::OpenCommands)),
            "ctrl+g p alt+p"
        );
    }
}

#[tokio::test]
async fn ctrl_c_clears_slash_draft_and_editor_history_before_empty_exit() {
    let mut state = fresh_state("interrupt-slash").await;
    state.attach_page(&page(
        vec![msg(1, Role::User, "stored prompt")],
        1,
        false,
        false,
    ));
    type_text(&mut state, "/side").await;
    assert!(state.slash_options().is_some());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "");
    assert!(state.slash_options().is_none());
    assert_eq!(state.editor.cursor, 0);
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Quit);

    let mut state = fresh_state("interrupt-draft").await;
    state.attach_page(&page(
        vec![msg(1, Role::User, "stored prompt")],
        1,
        false,
        false,
    ));
    type_text(&mut state, "plain draft").await;
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.input(), "");
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.input(), "", "root clear must not be undoable");
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        "stored prompt",
        "durable history remains available"
    );
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.input(), "");
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Quit);
}

#[tokio::test]
async fn ctrl_c_dismisses_slash_token_but_preserves_text_after_caret() {
    let mut state = fresh_state("interrupt-slash-suffix").await;
    state.handle_paste("/side suffix");
    for _ in 0..7 {
        state.handle_key(KeyAction::Left).await;
    }
    assert_eq!(state.editor.cursor, 5);
    assert!(state.slash_options().is_some());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), " suffix");
    assert_eq!(state.editor.cursor, 0);
    assert!(state.slash_options().is_none());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.input(), "");
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Quit);
}

#[tokio::test]
async fn ctrl_c_clears_mention_and_multiline_chip_without_reusing_stale_suggestions() {
    let mut state = fresh_state("interrupt-mention").await;
    state.chrome.location = Some("/A".into());
    state.handle_paste("@sr");
    let stale = state.mention_request().expect("mention query");
    assert!(state.apply_file_suggestions(stale.clone(), file_result("/A", 1, &["src/main.rs"])));
    assert!(state.mention_options().is_some());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "@sr", "first Ctrl+C only hides references");
    assert!(state.mention_options().is_none());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "");
    state.handle_paste("@sr");
    assert!(!state.apply_file_suggestions(stale, file_result("/A", 1, &["stale.rs"])));
    assert!(state.mention_options().is_none());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(
        state.input(),
        "@sr",
        "unloaded mention still owns dismissal"
    );
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.input(), "");

    state.handle_paste("one\ntwo\nthree");
    assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~3 lines] ");
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::Idle);
    assert_eq!(state.input(), "");
    assert_eq!(state.editor.cursor, 0);
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(
        state.input(),
        "",
        "paste undo must not restore cleared text"
    );
    state.handle_key(KeyAction::Char('x')).await;
    assert_eq!(state.prompt_layout(80).0[0].text, "x");
}

#[tokio::test]
async fn ctrl_c_clears_pending_draft_without_cancelling_accepted_turn() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("interrupt-pending"));
    type_text(&mut state, "submitted").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submission")
    };
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(state.status(), &TuiStatus::PendingSubmission);
    assert_eq!(state.input(), "");
    assert!(
        inbox.try_recv().is_err(),
        "interrupt must not enqueue cancellation"
    );
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    state.poll_submission();
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("accepted".into())));
    assert_eq!(state.history().rows()[0].text, "submitted");
    assert_eq!(state.input(), "");
    assert_eq!(state.status(), &TuiStatus::Streaming);
}

#[tokio::test]
async fn v05_review_shift_edges_select_entire_multiline_buffer() {
    let mut state = fresh_state("buffer-edges").await;
    state.handle_paste("один\nдва");
    state.handle_key(KeyAction::SelectHome).await;
    state.handle_key(KeyAction::Char('X')).await;
    assert_eq!(state.input(), "X");
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::SelectEnd).await;
    state.handle_key(KeyAction::Char('Y')).await;
    assert_eq!(state.input(), "Y");
}

#[tokio::test]
async fn v05_review_empty_draft_up_recalls_durable_history() {
    let mut state = fresh_state("empty-recall").await;
    state.attach_page(&page(
        vec![msg(1, Role::User, "stored prompt")],
        1,
        false,
        false,
    ));
    assert_eq!(state.input(), "");
    state.handle_key(KeyAction::Up).await;
    assert_eq!(state.input(), "stored prompt");
    assert_eq!(state.editor.cursor, 0);
    state.handle_key(KeyAction::Down).await;
    assert_eq!(
        state.input(),
        "stored prompt",
        "first Down moves to raw end"
    );
    assert_eq!(state.editor.cursor, "stored prompt".len());
    state.handle_key(KeyAction::Down).await;
    assert_eq!(state.input(), "");
}

#[tokio::test]
async fn vis27_owner_catalog_mode_and_absence_replace_prior_selection() {
    use crossterm::event::{MouseButton, MouseEventKind};
    use oc_core::queries::TerminalCopyMode;

    let mut state = fresh_state("owner-terminal-copy").await;
    let frame = Rect::new(0, 0, 100, 28);
    state.live_text = "owner copy text".into();
    let (x, y) = paint_selection_fixture(&mut state, frame, "owner");
    let mut configured = snapshot();
    configured.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    state.apply_catalog(configured.clone());
    assert_eq!(state.clipboard_mode(), super::ClipboardMode::Manual);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Left), x, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Drag(MouseButton::Left), x + 5, y),
        frame,
    );
    state.handle_mouse(
        selection_mouse(MouseEventKind::Up(MouseButton::Left), x + 5, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request().as_deref(), Some("owner"));

    configured.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    state.refresh_configuration(configured.clone());
    assert_eq!(state.clipboard_mode(), super::ClipboardMode::Select);
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(state.take_copy_request(), None);

    configured.chrome.terminal_copy = None;
    state.apply_catalog(configured);
    assert_eq!(state.clipboard_mode(), super::ClipboardMode::default());
    state.handle_mouse(
        selection_mouse(MouseEventKind::Down(MouseButton::Right), x, y),
        frame,
    );
    assert_eq!(
        state.take_copy_request().as_deref(),
        if cfg!(windows) { Some("owner") } else { None },
        "unconfigured right-click follows the platform default"
    );
    let other = TuiState::new_home(state.app.clone());
    state.sync_clipboard_mode_from(&other);
    assert_eq!(state.clipboard_mode(), super::ClipboardMode::default());
}

#[tokio::test]
async fn slash_overlay_commands_consume_alias_but_compress_keeps_input() {
    let mut state = fresh_state("s-slash").await;
    for (command, intent) in [
        ("/model", PanelIntent::LoadCatalog),
        ("/agents", PanelIntent::LoadCatalog),
        ("/sessions", PanelIntent::LoadSessions),
        ("/skills", PanelIntent::LoadSkills),
    ] {
        type_text(&mut state, command).await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert_eq!(outcome.intent, Some(intent), "{command}");
        assert!(!outcome.consumed_input, "snapshot still pending");
        assert_eq!(
            state.input(),
            "",
            "opening an overlay consumes the alias, not its search query"
        );
        assert_eq!(outcome.note, None);
        state.accept_intent();
        state.close_panel();
    }

    type_text(&mut state, "/dcp-compress draft span").await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::Compress {
            focus: "draft span".to_string()
        })
    );
    assert!(!outcome.consumed_input);
    assert_eq!(state.input(), "/dcp-compress draft span");
    assert_eq!(state.panel(), &TuiPanel::Dcp);

    // Once the snapshots have arrived, opening a panel consumes input.
    let mut state = fresh_state("s-slash2").await;
    state.apply_catalog(snapshot());
    state.apply_sessions(vec!["s-slash2".to_string()]);
    state.apply_skills(vec![SkillCard {
        id: "sk".to_string(),
        name: "Skill".to_string(),
        description: "does things".to_string(),
    }]);
    for command in ["/model", "/agents", "/sessions", "/skills"] {
        type_text(&mut state, command).await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert_eq!(
            outcome.intent,
            if command == "/sessions" {
                Some(PanelIntent::LoadSessions)
            } else {
                None
            },
            "{command}"
        );
        assert!(outcome.consumed_input, "{command}");
        assert!(state.input().is_empty(), "{command}");
        state.close_panel();
    }
    type_text(&mut state, "/quit").await;
    let outcome = state.handle_key(KeyAction::Enter).await;
    assert!(outcome.consumed_input);
    assert_eq!(state.status(), &TuiStatus::Quit);
}

#[tokio::test]
async fn v04_pending_command_availability_preserves_receipt_focus_and_draft() {
    use crate::commands::CommandAction;
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("v04-pending"));
    state.apply_catalog(snapshot());
    type_text(&mut state, "pending prompt").await;
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submit")
    };
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("New session");
    assert!(state.modal_options()[0].footer.contains("turn active"));
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(outcome.intent, None);
    assert_eq!(
        outcome.note.as_deref(),
        Some("turn active; action unavailable")
    );
    assert_eq!(state.panel(), &TuiPanel::Commands);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "pending prompt");
    assert_eq!(state.status(), &TuiStatus::PendingSubmission);
    for action in [
        CommandAction::NewSession,
        CommandAction::OpenSessions,
        CommandAction::OpenModelPicker,
        CommandAction::OpenVariants,
        CommandAction::OpenAgents,
    ] {
        let outcome = state.run_command(action);
        assert_eq!(outcome.intent, None);
        assert!(outcome.note.unwrap().contains("turn active"));
        assert_eq!(state.panel(), &TuiPanel::None);
    }
    assert!(inbox.try_recv().is_err());
    ack.send(Ok(WorkerTurnId("receipt-intact".into()))).unwrap();
    state.poll_submission();
    assert_eq!(
        state.active_turn(),
        Some(&WorkerTurnId("receipt-intact".into()))
    );
    assert_eq!(state.history().rows()[0].text, "pending prompt");
    assert!(state.input().is_empty());
}

#[tokio::test]
async fn v04_variant_current_focus_restores_after_clearing_search() {
    use crate::commands::CommandAction;
    let mut state = fresh_state("v04-variant").await;
    let mut catalog = snapshot();
    catalog.variant = Some("none".into());
    catalog.models[0].variants.push(VariantEntry {
        name: "none".into(),
        disabled: false,
        reasoning_effort: Some("low".into()),
    });
    state.apply_catalog(catalog.clone());
    state.run_command(CommandAction::OpenVariants);
    assert!(state.modal_options()[state.select.cursor].current);
    state.handle_paste("Default");
    assert!(!state.modal_options()[state.select.cursor].current);
    state.handle_panel_key(KeyAction::Interrupt);
    assert!(state.modal_options()[state.select.cursor].current);
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::ChooseModel {
            id: "a".into(),
            variant: Some("none".into())
        })
    );
    state.model_choice_applied(catalog);
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn vis29_variant_current_focus_centers_on_open_and_clear_in_narrow_view() {
    use crate::commands::CommandAction;
    let mut state = fresh_state("vis29-narrow-current").await;
    let mut catalog = snapshot();
    catalog.variant = Some("max".into());
    catalog.models[0].variants = [
        "none", "minimal", "low", "medium", "high", "xhigh", "max", "zeta", "alpha",
    ]
    .into_iter()
    .map(|name| VariantEntry {
        name: name.into(),
        disabled: false,
        reasoning_effort: None,
    })
    .collect();
    state.apply_catalog(catalog);
    type_text(&mut state, "draft retained").await;
    state.run_command(CommandAction::OpenVariants);
    let rows = crate::views::render_test(&state, 80, 24);
    assert!(rows[11].contains("medium"));
    assert!(rows[14].contains("● max"));
    assert!(rows[16].contains("alpha"));
    state.handle_paste("Default");
    crate::views::render_test(&state, 80, 24);
    state.handle_panel_key(KeyAction::Interrupt);
    let restored = crate::views::render_test(&state, 80, 24);
    assert_eq!(restored, rows);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "draft retained");
}

#[tokio::test]
async fn variant_cycle_keeps_draft_and_modal_focus_and_refuses_pending_submit() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("cycle-focus"));
    state.apply_catalog(snapshot());
    type_text(&mut state, "unchanged draft").await;
    assert_eq!(
        state.handle_key(KeyAction::CycleVariant).await.intent,
        Some(PanelIntent::CycleVariant)
    );
    assert_eq!(state.input(), "unchanged draft");
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(inbox.try_recv().is_err());
    state.handle_key(KeyAction::Agents).await;
    assert_eq!(state.handle_key(KeyAction::CycleVariant).await.intent, None);
    assert_eq!(state.panel(), &TuiPanel::Agents);
    state.handle_key(KeyAction::Cancel).await;
    state.handle_key(KeyAction::Enter).await;
    let Some(oc_core::core_app::InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submit")
    };
    let result = state.handle_key(KeyAction::CycleVariant).await;
    assert_eq!(result.intent, None);
    assert_eq!(
        result.note.as_deref(),
        Some("turn active; action unavailable")
    );
    assert_eq!(state.input(), "unchanged draft");
    assert!(inbox.try_recv().is_err());
    drop(ack);
}

#[tokio::test]
async fn v04_modal_search_scroll_focus_and_draft() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = fresh_state("v04-draft").await;
    let mut catalog = snapshot();
    let model = catalog.models[0].clone();
    catalog.models = (0..30)
        .map(|i| {
            let mut m = model.clone();
            m.id = format!("m{i:02}");
            m.display_name = format!("Display {i:02}");
            m.provider_name = "Real provider".into();
            m
        })
        .collect();
    catalog.model_id = "m04".into();
    state.apply_catalog(catalog);
    type_text(&mut state, "kept draft 🌍").await;
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|f| crate::views::render_frame(f, &state))
        .unwrap();
    let base = terminal.backend().buffer().clone();
    let key =
        crate::events::map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)).unwrap();
    state.handle_key(key).await;
    assert_eq!(state.panel(), &TuiPanel::Commands);
    state.handle_paste("model");
    assert_eq!(
        state
            .modal_options()
            .iter()
            .map(|o| o.title.as_str())
            .collect::<Vec<_>>(),
        ["Switch model"]
    );
    state.handle_panel_key(KeyAction::Enter);
    assert_eq!(state.panel(), &TuiPanel::Model);
    terminal
        .draw(|f| crate::views::render_frame(f, &state))
        .unwrap();
    let modal = terminal.backend().buffer();
    for y in [0, 1, 35, 39] {
        for x in 0..120 {
            assert_eq!(
                base[(x, y)].symbol(),
                modal[(x, y)].symbol(),
                "underlay reflow at {x},{y}"
            );
            assert_eq!(
                modal[(x, y)].fg,
                if base[(x, y)].symbol() == " " {
                    ratatui::style::Color::Rgb(255, 255, 255)
                } else {
                    crate::dialog::backdrop(base[(x, y)].fg, crate::theme::Theme::dark().text())
                }
            );
            assert_eq!(
                modal[(x, y)].modifier,
                if base[(x, y)].symbol() == " " {
                    ratatui::style::Modifier::empty()
                } else {
                    base[(x, y)].modifier
                }
            );
            assert_eq!(
                modal[(x, y)].bg,
                crate::dialog::backdrop(base[(x, y)].bg, crate::theme::Theme::dark().background())
            );
        }
    }
    for _ in 0..25 {
        state.handle_panel_key(KeyAction::Down);
    }
    assert_eq!(state.picker_selection().unwrap().0, "m25");
    let text = crate::views::render_test(&state, 120, 40).join("\n");
    assert!(text.contains("Display 25") && !text.contains("Display 00"));
    state.handle_paste("missing-no-results");
    assert!(state.modal_options().is_empty());
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    assert!(
        crate::views::render_test(&state, 120, 40)
            .join("\n")
            .contains("No results found")
    );
    state.handle_panel_key(KeyAction::Interrupt); // clears only the filter
    state.handle_paste("Display 29");
    assert_eq!(state.modal_options().len(), 1);
    assert_eq!(state.picker_selection().unwrap().0, "m29");
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "kept draft 🌍");
    assert_eq!(state.active_model_label().unwrap().0, "Display 04");
    assert_eq!(state.status(), &TuiStatus::Idle);
    state.handle_key(KeyAction::Leader).await;
    state.handle_key(KeyAction::Char('m')).await;
    assert_eq!(state.panel(), &TuiPanel::Model);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "kept draft 🌍");
}

#[tokio::test]
async fn vis09_keyboard_model_focus_centers_clamped_viewport_without_moving_current() {
    use crate::commands::CommandAction;
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let mut state = fresh_state("vis09-model-scroll").await;
    let mut catalog = snapshot();
    let model = catalog.models[0].clone();
    catalog.models = (0..20)
        .map(|i| {
            let mut entry = model.clone();
            entry.id = format!("m{i:02}");
            entry.display_name = format!("Model {i:02}");
            entry
        })
        .collect();
    catalog.model_id = "m03".into();
    state.apply_catalog(catalog);
    state.run_command(CommandAction::OpenModelPicker);
    let mut cursor_output = Vec::new();
    crate::terminal::set_cursor_color(&mut cursor_output, crate::views::cursor_color(&state))
        .unwrap();
    assert_eq!(cursor_output, b"\x1b]12;#fab283\x07");
    let area = Rect::new(0, 0, 120, 40);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let row = |terminal: &Terminal<TestBackend>, y| -> String {
        (34..42)
            .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_string())
            .collect()
    };
    terminal
        .draw(|frame| crate::dialog::render(frame, &state))
        .unwrap();
    assert_eq!(state.select.cursor, 0);
    assert_eq!(row(&terminal, 15), "Model 00");
    assert_eq!(row(&terminal, 18), "Model 03");
    assert_eq!(terminal.backend().buffer()[(32, 18)].symbol(), "●");

    for _ in 0..16 {
        state.handle_panel_key(KeyAction::Down);
        terminal
            .draw(|frame| crate::dialog::render(frame, &state))
            .unwrap();
    }
    assert_eq!(state.select.cursor, 16);
    assert_eq!(state.picker_selection().unwrap().0, "m16");
    assert_eq!(row(&terminal, 15), "Model 06");
    assert_eq!(row(&terminal, 25), "Model 16");
    assert_eq!(terminal.backend().buffer()[(32, 25)].symbol(), " ");
    assert_eq!(
        terminal.backend().buffer()[(32, 25)].bg,
        crate::theme::Theme::dark()
            .color("background.action.primary.$focused")
            .unwrap()
    );
    assert!(
        (15..29).all(|y| terminal.backend().buffer()[(32, y)].symbol() != "●"),
        "the current model m03 is above the keyboard-centered viewport"
    );

    state.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 35,
            row: 15,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    terminal
        .draw(|frame| crate::dialog::render(frame, &state))
        .unwrap();
    assert_eq!(row(&terminal, 15), "Model 03");
    assert_eq!(state.select.cursor, 16);

    state.handle_paste("Model 00");
    terminal
        .draw(|frame| crate::dialog::render(frame, &state))
        .unwrap();
    assert_eq!(state.modal_options().len(), 1);
    assert_eq!(state.select.cursor, 0);
    assert_eq!(row(&terminal, 15), "Model 00");
    state.handle_panel_key(KeyAction::Cancel);
    crate::terminal::set_cursor_color(&mut cursor_output, crate::views::cursor_color(&state))
        .unwrap();
    assert!(cursor_output.ends_with(b"\x1b]112\x07"));
}

#[tokio::test]
async fn v04_mouse_scroll_hover_and_drag_release_keep_modal_owner() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let mut state = fresh_state("v04-pointer").await;
    let mut catalog = snapshot();
    let model = catalog.models[0].clone();
    catalog.models = (0..20)
        .map(|i| {
            let mut entry = model.clone();
            entry.id = format!("m{i:02}");
            entry.display_name = format!("Mouse {i:02}");
            entry
        })
        .collect();
    catalog.model_id = "m00".into();
    state.apply_catalog(catalog);
    type_text(&mut state, "draft stays").await;
    state.handle_key(KeyAction::Leader).await;
    state.handle_key(KeyAction::Char('m')).await;
    assert_eq!(state.panel(), &TuiPanel::Model);
    let area = Rect::new(0, 0, 80, 24);
    let event = |kind, column, row| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    let pointer = |state: &mut TuiState, kind, x, y| state.handle_mouse(event(kind, x, y), area);
    // Second ungrouped option is at y=12 (the list starts at y=11).
    pointer(&mut state, MouseEventKind::Moved, 20, 12);
    assert_eq!(state.picker_selection().unwrap().0, "m01");
    pointer(&mut state, MouseEventKind::ScrollDown, 20, 12);
    assert_eq!(
        state.picker_selection().unwrap().0,
        "m01",
        "wheel scrolls without selecting"
    );
    let options = state.modal_options();
    assert_eq!(
        state
            .select
            .hit(area, crate::dialog::DialogSize::Medium, &options, 20, 12),
        crate::dialog::DialogHit::Option(4)
    );
    pointer(&mut state, MouseEventKind::Moved, 20, 12);
    assert_eq!(state.picker_selection().unwrap().0, "m04");
    // Releasing over the backdrop after starting a text selection inside
    // the dialog must not dismiss it or submit an option.
    pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
    pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 0, 0);
    assert_eq!(state.panel(), &TuiPanel::Model);
    assert_eq!(state.input(), "draft stays");
    assert_eq!(
        pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
        None
    );
    pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
    assert_eq!(
        pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
        Some(PanelIntent::SelectModel { id: "m04".into() })
    );
    assert_eq!(
        pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
        None,
        "release cannot submit the same option twice"
    );
    // A held press cannot cross a keyboard-driven panel replacement.
    pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
    state.open_variants();
    assert_eq!(
        pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
        None,
        "release in replacement dialog cannot select a variant"
    );
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "draft stays");
    state.handle_key(KeyAction::Char('!')).await;
    assert_eq!(state.input(), "draft stays!");
}

#[tokio::test]
async fn v04_mouse_status_rows_do_not_start_compression() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let mut state = fresh_state("v04-dcp-mouse").await;
    state.apply_catalog(snapshot());
    state.panel = TuiPanel::Dcp;
    let area = Rect::new(0, 0, 80, 24);
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let outcome = state.handle_mouse(
            MouseEvent {
                kind,
                column: 20,
                row: 11,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        assert_eq!(outcome.intent, None);
    }
    assert_eq!(state.panel(), &TuiPanel::Dcp);
}

#[tokio::test]
async fn painted_toast_slash_and_mentions_block_reasoning_press_and_release() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use oc_core::queries::{HistoryTurn, TranscriptPart};
    let area = Rect::new(0, 0, 80, 24);
    let mouse = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let click = |state: &mut TuiState, x, y| {
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), area);
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    };
    let mut toast = fresh_state("reasoning-toast").await;
    let mut message = msg(9, Role::Assistant, "aggregate");
    message.turn = Some(HistoryTurn {
        parts: vec![TranscriptPart::Reasoning {
            text: format!("**{}**\n\nbody", "title".repeat(20)),
            duration_ms: None,
        }],
        status: "completed".into(),
        ..Default::default()
    });
    toast.attach_page(&page(vec![message], 1, false, false));
    let rect = crate::shell::transcript_area(&toast, area);
    let y = rect.y + 2;
    toast.push_note("Overpaint");
    let surface = crate::shell::toast_rect(&toast, area).unwrap();
    let x = surface.x + 2;
    assert!(surface.contains((x, y).into()));
    toast.note = None;
    assert!(
        toast.reasoning_hit(area, x, y).is_some(),
        "underlying long header"
    );
    let (rows, total, scroll) =
        toast.visible_transcript_at_viewport(rect.width, area.width, rect.height);
    toast.paint_transcript(rect, &rows, total, scroll);
    toast.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), area);
    toast.push_note("Overpaint");
    toast.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    toast.handle_mouse(mouse(MouseEventKind::Moved, x, y), area);
    let (rows, total, scroll) =
        toast.visible_transcript_at_viewport(rect.width, area.width, rect.height);
    let painted = toast.paint_transcript_at(rect, &rows, total, scroll, Some(area), &[]);
    assert_eq!(
        painted[(y - rect.y) as usize].spans()[1].style().fg,
        Some(ratatui::style::Color::Rgb(0x97, 0x68, 0x2c)),
        "toast-owned header does not acquire hover"
    );
    toast.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    click(&mut toast, x, y);
    assert!(
        toast.reasoning_expanded.is_empty(),
        "entire toast surface owns input"
    );
    toast.note = None;
    click(&mut toast, x, y);
    assert_eq!(toast.reasoning_expanded.len(), 1);

    let mut state = fresh_state("reasoning-autocomplete").await;
    state.chrome.location = Some("/A".into());
    for i in 0..20 {
        state
            .window
            .push_synthetic("assistant", &format!("earlier {i}"), None, None);
    }
    state.live_reasoning = "**Latest**\n\nbody".into();
    state.active_turn = Some(WorkerTurnId("overlay-turn".into()));
    let rect = crate::shell::transcript_area(&state, area);
    let (lines, _) = state.visible_transcript(rect.width, area.width, rect.height);
    let y = rect.y
        + lines
            .iter()
            .position(|line| line.plain_text().contains("Thinking"))
            .unwrap() as u16;
    let x = rect.x + 5;
    assert!(state.reasoning_hit(area, x, y).is_some());
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(rect.width, area.width, rect.height);
    state.paint_transcript(rect, &rows, total, scroll);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), area);
    type_text(&mut state, "/").await;
    assert!(state.slash_options().is_some());
    assert!(
        state.transcript_overpainted(area, x, y),
        "slash rect covers painted header"
    );
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    click(&mut state, x, y);
    assert!(state.reasoning_expanded.is_empty());
    state.input.clear();
    state.editor.clear();
    type_text(&mut state, "@").await;
    let request = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(
        request,
        file_result("/A", 1, &["a", "b", "c", "d", "e", "f", "g", "h"])
    ));
    assert!(
        state.transcript_overpainted(area, x, y),
        "mention rect covers painted header"
    );
    click(&mut state, x, y);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), area);
    assert!(state.reasoning_expanded.is_empty());
    state.input.clear();
    state.editor.clear();
    click(&mut state, x, y);
    assert_eq!(
        state.reasoning_expanded.len(),
        1,
        "uncovered running header works"
    );
}

#[tokio::test]
async fn vis07_submit_pointer_in_unpainted_transcript_space_is_inert() {
    use crate::styled::Line;
    let mut state = fresh_state("paste-pointer").await;
    let frame = Rect::new(0, 0, 120, 40);
    let area = crate::shell::transcript_area(&state, frame);
    let rows = vec![Line::plain("accepted user"), Line::plain("assistant")];
    assert!(area.height > 21);
    // The suffix-space paste probe leaves the mouse at the old chip row.
    // After the composer clears, that coordinate belongs to blank space
    // below the eight (or fewer) newly painted transcript rows.
    for row in [2, 21, area.height - 1] {
        state.last_mouse = Some((area.x + 5, area.y + row, frame));
        assert_eq!(
            state.paint_transcript_at(area, &rows, rows.len(), 0, Some(frame), &[]),
            rows
        );
        assert!(state.exploration_expanded.is_empty());
    }
}

#[tokio::test]
async fn panel_enter_returns_selection_intents() {
    let mut state = fresh_state("s-panels").await;
    state.apply_catalog(snapshot());

    type_text(&mut state, "/model").await;
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(state.panel(), &TuiPanel::Model);
    state.handle_panel_key(KeyAction::Down);
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::SelectModel {
            id: "b".to_string(),
        })
    );
    assert_eq!(state.picker_selection(), Some(("b".to_string(), None)));
    state.accept_intent();
    state.close_panel();

    type_text(&mut state, "/agents").await;
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(state.panel(), &TuiPanel::Agents);
    state.handle_panel_key(KeyAction::Down);
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::SelectAgent {
            id: "y".to_string()
        })
    );
    state.accept_intent();

    state.apply_sessions(vec!["s1".to_string(), "s2".to_string()]);
    state.close_panel();
    type_text(&mut state, "/sessions").await;
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(state.panel(), &TuiPanel::Sessions);
    assert_eq!(state.sessions_cursor(), 0);
    state.handle_panel_key(KeyAction::Down);
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::SwitchSession {
            id: "s2".to_string()
        })
    );

    // Esc closes without an intent; DCP Enter asks for a compress.
    let outcome = state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(outcome, KeyOutcome::default());
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn sessions_metadata_keeps_owner_order_routes_ids_and_requeries_search_scope() {
    let mut state = fresh_state("current-root").await;
    state.chrome.location = Some("/work/project".into());
    state.session_project_name = Some("project".into());
    state.handle_paste("kept draft");
    state.run_command(crate::commands::CommandAction::OpenSessions);
    state.apply_session_entries(vec![
        oc_core::queries::SessionListEntry {
            id: SessionId("other-root".into()),
            title: "Newest title".into(),
            directory: Some("/work/project".into()),
            created_at: "0".into(),
            updated_at: Some("20".into()),
            date_group: "Today".into(),
            running: false,
            worktree: None,
        },
        oc_core::queries::SessionListEntry {
            id: SessionId("current-root".into()),
            title: "Older title".into(),
            directory: Some("/work/project".into()),
            created_at: "0".into(),
            updated_at: Some("10".into()),
            date_group: "Thu Jan 01 1970".into(),
            running: false,
            worktree: None,
        },
    ]);
    let options = state.modal_options();
    assert_eq!(options[0].title, "Newest title");
    assert!(options[1].current);
    assert_eq!(state.select.cursor, 1);
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        Some(PanelIntent::SwitchSession {
            id: "current-root".into()
        })
    );
    assert_eq!(
        state.handle_panel_key(KeyAction::Char('o')).intent,
        Some(PanelIntent::LoadSessions)
    );
    assert_eq!(state.session_search(), "o");
    assert_eq!(
        state.handle_panel_key(KeyAction::CtrlA).intent,
        Some(PanelIntent::LoadSessions)
    );
    assert!(!state.sessions_all_projects());
    assert_eq!(state.sessions_title(), "Sessions for project");
    assert_eq!(
        state.handle_panel_key(KeyAction::Interrupt).intent,
        Some(PanelIntent::LoadSessions)
    );
    assert_eq!(state.session_search(), "");
    state.apply_session_entries(Vec::new());
    assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.input(), "kept draft");
}

#[tokio::test]
async fn sessions_selected_actions_confirm_exact_row_keep_draft_and_clear_on_move() {
    use crate::commands::CommandAction;
    let mut state = fresh_state("current").await;
    state.handle_paste("draft survives");
    state.run_command(CommandAction::OpenSessions);
    state.apply_session_entries(vec![oc_core::queries::SessionListEntry {
        id: SessionId("selected-other".into()),
        title: "Actual title".into(),
        directory: None,
        created_at: "1".into(),
        updated_at: Some("2".into()),
        date_group: "Today".into(),
        running: false,
        worktree: Some("feature-checkout".into()),
    }]);
    assert_eq!(state.modal_options()[0].footer, "feature-checkout");
    assert_eq!(state.handle_panel_key(KeyAction::DeleteOrQuit).intent, None);
    assert!(state.modal_options()[0].destructive);
    assert_eq!(
        state.modal_options()[0].title,
        "Press ctrl+d again to confirm"
    );
    state.handle_panel_key(KeyAction::Down);
    assert!(!state.modal_options()[0].destructive);
    assert_eq!(state.handle_panel_key(KeyAction::DeleteOrQuit).intent, None);
    assert_eq!(
        state.handle_panel_key(KeyAction::DeleteOrQuit).intent,
        Some(PanelIntent::DeleteSelectedSession {
            id: "selected-other".into()
        })
    );
    state.session_delete_rejected("owner refused".into());
    assert!(!state.modal_options()[0].destructive);
    state.handle_panel_key(KeyAction::Rename);
    assert_eq!(state.rename_title(), Some("Actual title"));
    assert_eq!(state.selected_session_rename(), Some("selected-other"));
    state.handle_panel_key(KeyAction::Interrupt);
    state.handle_paste("Renamed other");
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        Some(PanelIntent::RenameSelectedSession {
            id: "selected-other".into(),
            title: "Renamed other".into()
        })
    );
    state.rename_session_rejected("owner refused".into());
    assert_eq!(state.panel(), &TuiPanel::Rename);
    assert_eq!(state.rename_title(), Some("Renamed other"));
    assert_eq!(state.input(), "draft survives");
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.selected_session_rename(), None);
    assert_eq!(state.input(), "draft survives");
    state.run_command(CommandAction::OpenSessions);
    state.apply_session_entries(Vec::new());
    assert_eq!(state.handle_panel_key(KeyAction::Rename).intent, None);
    assert_eq!(state.handle_panel_key(KeyAction::DeleteOrQuit).intent, None);
    assert_eq!(state.panel(), &TuiPanel::Sessions);
}
