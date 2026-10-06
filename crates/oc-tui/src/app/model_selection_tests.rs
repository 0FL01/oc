use super::*;

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
