use super::*;

#[test]
fn conn03_snapshot_reconstruction_and_picker_keep_unknown_empty_false_and_exact_efforts() {
    use oc_core::queries::{ModelCapabilities, ModelEntry, PartialModelCapabilities, VariantEntry};
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("metadata".into()));
    let catalog = CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: Default::default(),
        provider: "custom".into(),
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
        models: vec![
            ModelEntry {
                id: "unknown".into(),
                ..Default::default()
            },
            ModelEntry {
                id: "empty".into(),
                variants_known: true,
                capabilities: ModelCapabilities {
                    input: Some(Vec::new()),
                    output: Some(Vec::new()),
                    reasoning: Some(false),
                    tools: Some(false),
                    ..Default::default()
                },
                ..Default::default()
            },
            ModelEntry {
                id: "partial".into(),
                variants_known: true,
                variants: vec![
                    VariantEntry {
                        name: "precise".into(),
                        reasoning_effort: Some("supplied-exact".into()),
                        disabled: false,
                    },
                    VariantEntry {
                        name: "max".into(),
                        reasoning_effort: None,
                        disabled: true,
                    },
                ],
                capabilities: ModelCapabilities {
                    partial: PartialModelCapabilities {
                        image_input: Some(true),
                        pdf_input: Some(false),
                        reasoning: Some(true),
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        ],
    };
    state.apply_catalog(catalog);
    let picker = state.picker.as_mut().unwrap();
    let caps = |picker: &mut crate::picker::ModelPicker, id| {
        picker.choose_id(id, None).unwrap();
        oc_adapters::models::capabilities(&picker.selection().unwrap().entry)
    };
    let unknown = caps(picker, "unknown");
    assert_eq!(
        (unknown.input, unknown.output, unknown.tools),
        (None, None, None)
    );
    let empty = caps(picker, "empty");
    assert_eq!((empty.input, empty.output), (Some(vec![]), Some(vec![])));
    assert_eq!((empty.reasoning, empty.tools), (Some(false), Some(false)));
    let partial = caps(picker, "partial");
    assert_eq!(
        (partial.partial.image_input, partial.partial.pdf_input),
        (Some(true), Some(false))
    );
    assert_eq!(
        oc_adapters::models::available_variants(&picker.selection().unwrap().entry)
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["precise"]
    );
    picker.choose_id("unknown", None).unwrap();
    assert!(picker.selection().unwrap().entry.get("variants").is_none());
    picker.choose_id("partial", Some("precise")).unwrap();
    let selected = picker.selection().unwrap();
    assert_eq!(
        selected
            .variant
            .as_ref()
            .unwrap()
            .reasoning_effort
            .as_deref(),
        Some("supplied-exact")
    );
    assert_eq!(
        oc_adapters::models::capabilities(&selected.entry).input,
        None
    );
    assert!(oc_adapters::models::capabilities(&selected.entry).supports_image());
    assert!(state.model_selection.committed.is_none());
    state.panel = TuiPanel::Model;
    state.select.query = "partial".into();
    state.sync_modal_cursor();
    // Pinned DialogModel has no metadata panel: capabilities stay owner data.
    for (width, height) in [(100, 36), (64, 24)] {
        let frame = crate::views::render_test(&state, width, height).join("\n");
        assert!(frame.contains("partial"), "{width}x{height}: {frame}");
        for debug in [
            "In: ",
            "Reasoning:",
            "Effort:",
            "Limits:",
            "Client media",
            "Partial ",
        ] {
            assert!(!frame.contains(debug), "{width}x{height}: {frame}");
        }
    }
}

#[test]
fn go05_unchosen_catalog_has_no_committed_or_recent_fake_model() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("local".into()));
    let catalog = CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
        provider: "opencode-go".into(),
        models: Vec::new(),
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
    };
    state.apply_catalog(catalog);
    assert!(state.model_selection.committed.is_none());
    assert!(state.model_selection.committed_recent.is_empty());
    assert!(state.model_selection.recent.is_empty());
    assert!(state.captured_model_commit().is_none());
}

#[test]
fn go05_filtered_picker_keeps_composer_commit_and_captures_target_scope() {
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("local".into()));
    let current = CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: Default::default(),
        provider: "custom".into(),
        models: Vec::new(),
        model_id: "same/slash".into(),
        variant: Some("custom-variant".into()),
        agents: Vec::new(),
        agent_id: Some("build".into()),
        commands: Vec::new(),
        command_descriptions: Default::default(),
    };
    state.apply_catalog(current.clone());
    let committed = state.model_selection.committed.clone();
    let chrome = state.chrome.clone();
    state.panel = TuiPanel::Model;
    let mut public = current.clone();
    public.provider = "opencode-go".into();
    public.model_id.clear();
    public.variant = None;
    state.apply_picker_catalog(public.clone());
    assert_eq!(state.model_selection.committed, committed);
    assert_eq!(state.chrome, chrome);
    assert_eq!(state.picker_provider_filter(), Some("opencode-go"));
    assert!(state.captured_model_commit().is_none());
    // Same slash ID under another provider cannot inherit the custom variant.
    state.picker.as_mut().unwrap().refresh(ModelCatalog {
        provider: "opencode-go".into(),
        models: BTreeMap::from([("same/slash".into(), serde_json::json!({"name":"Go row"}))]),
    });
    state.draft_model("same/slash").unwrap();
    let commit = state.captured_model_commit().unwrap();
    assert_eq!(commit.binding.provider, "opencode-go");
    assert_eq!(commit.binding.agent_id.as_deref(), Some("build"));
    assert_eq!(commit.model_id, "same/slash");
    assert!(commit.variant.is_none());
    assert_eq!(state.model_selection.committed, committed);
    assert_eq!(state.model_selection.recent.len(), 2);
    assert!(
        state
            .model_selection
            .recent
            .iter()
            .any(|(_, model)| model.provider == "custom"
                && model.variant.as_deref() == Some("custom-variant"))
    );
    // A snapshot from a different Location/generation cannot replace this draft.
    public.chrome.selection_generation = 99;
    state.apply_picker_catalog(public);
    assert_eq!(state.captured_model_commit().unwrap(), commit);
}

#[test]
fn all_provider_dialog_drafts_other_provider_records_recent_and_toggles_favorite() {
    use crate::picker::directory_tests::{fixture_parts, m};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (app, _, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("dialog".into()));
    let (catalogs, preference) = fixture_parts(true);
    state.apply_catalog(catalogs.catalogs[0].clone());
    let outcome = state.run_command(CommandAction::OpenModelPicker);
    assert_eq!(outcome.intent, Some(PanelIntent::LoadModelDirectory));
    state.apply_model_directory(catalogs, preference.clone());

    let frame = crate::views::render_test(&state, 100, 44).join("\n");
    for text in [
        "Favorites",
        "DeepSeek V4.1 Flash · opencode-go ludka2",
        "Recent",
        "OpenCode Go",
        "Connect an integration ctrl+a  Favorite ctrl+f",
    ] {
        assert!(frame.contains(text), "{text}: {frame}");
    }

    // Ctrl+F is the effective model.dialog.favorite binding, not search input.
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(
        state.handle_panel_key(action).intent,
        Some(PanelIntent::FavoriteModel {
            model: m("ludka2", "DeepSeek V4.1 Flash · opencode-go"),
            enabled: false,
        })
    );
    let mut stored = preference;
    stored.favorite.remove(0);
    state.apply_model_preference(stored);
    assert_eq!(state.modal_options()[0].title, "GLM-5.3");
    assert_eq!(
        state.select.cursor, 0,
        "DialogSelect keeps the focused index"
    );

    // Enter on another provider's row binds that view, then drafts exactly.
    state.select.cursor = state
        .modal_options()
        .iter()
        .position(|o| o.title == "Cat Jam Free")
        .unwrap();
    let outcome = state.handle_panel_key(KeyAction::Enter);
    assert_eq!(
        outcome.intent,
        Some(PanelIntent::SelectModel {
            id: "Cat Jam Free".into(),
            recent: Some(m("opencode-go", "Cat Jam Free")),
        })
    );
    state.draft_model("Cat Jam Free").unwrap();
    let commit = state.captured_model_commit().unwrap();
    assert_eq!(
        (commit.binding.provider.as_str(), commit.model_id.as_str()),
        ("opencode-go", "Cat Jam Free")
    );
    assert!(state.model_selection.committed.is_none());

    // Ctrl+A replaces the dialog with the integration list.
    state.run_command(CommandAction::OpenModelPicker);
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(matches!(
        state.handle_panel_key(action).intent,
        Some(PanelIntent::LoadProviderConnections { .. })
    ));
    assert_eq!(state.panel(), &TuiPanel::Accounts);
}
