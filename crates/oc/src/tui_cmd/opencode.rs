//! Source-only bootstrap helpers. Normal native startup is unchanged.
use super::*;

pub(super) async fn load(app: &CoreApp, session: Option<SessionId>) -> Result<TuiState, CoreError> {
    let mut state = match session.clone() {
        Some(session) => TuiState::new(app.clone(), session),
        None => TuiState::new_home(app.clone()),
    };
    state.apply_catalog(app.catalog().await?);
    if let Some(session) = session {
        let oc_core::opencode::Reply::View { view } = app
            .opencode(oc_core::opencode::Request::Open { session })
            .await?
        else {
            return Err(CoreError::Application(
                "Invalid OpenCode session projection".into(),
            ));
        };
        state.apply_opencode_view(*view);
    }
    state.flush_opencode();
    state.push_note("OpenCode backend · local tabs. Native-only config, MCP, DCP, shell and attachments are unavailable; /exit returns to stock TUI.");
    Ok(state)
}

pub(super) fn entries(
    sessions: Vec<oc_core::opencode::Session>,
) -> Vec<oc_core::queries::SessionListEntry> {
    sessions
        .into_iter()
        .map(|session| oc_core::queries::SessionListEntry {
            id: session.id,
            title: session.title.unwrap_or_else(|| "Untitled session".into()),
            created_at: session.created,
            updated_at: Some(session.updated),
            directory: Some(session.location),
            date_group: "OpenCode".into(),
            running: session.running,
            worktree: None,
        })
        .collect()
}
