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
    let details = picker.model_details("unknown").join("\n");
    assert!(details.contains("In: unknown · Out: unknown") && details.contains("Effort: unknown"));
    let details = picker.model_details("empty").join("\n");
    assert!(details.contains("In: [] · Out: []") && details.contains("Effort: none declared"));
    assert!(details.contains("Reasoning: unsupported · Tools: unsupported"));
    let details = picker.model_details("partial").join("\n");
    assert!(
        details.contains("Partial image input: supported")
            && details.contains("Partial PDF input: unsupported")
    );
    assert!(details.contains("precise=supplied-exact") && !details.contains("max"));
    assert!(details.contains("Client media: image input only"));
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
    for (width, height) in [(100, 36), (64, 24)] {
        let frame = crate::views::render_test(&state, width, height).join("\n");
        assert!(
            frame.contains("precise=supplied-exact"),
            "{width}x{height}: {frame}"
        );
        assert!(
            frame.contains("Partial image input: supported"),
            "{width}x{height}: {frame}"
        );
        assert!(
            frame.contains("Partial PDF input: unsupported"),
            "{width}x{height}: {frame}"
        );
        assert!(
            frame.contains("Client media: image input only"),
            "{width}x{height}: {frame}"
        );
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
