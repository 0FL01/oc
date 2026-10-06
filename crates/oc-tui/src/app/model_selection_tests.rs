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
