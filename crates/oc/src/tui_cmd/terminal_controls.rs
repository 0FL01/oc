//! Captured session controls over CoreApp; navigation never retargets a PTY.
use super::*;
use oc_core::queries::{TerminalAction, TerminalReceipt, TerminalRef};
use oc_tui::terminal_view::TerminalIntent;

pub(super) async fn refresh(app: &CoreApp, state: &mut TuiState) -> Result<bool, CoreError> {
    let Some(session) = state.attached_session().cloned() else {
        return Ok(false);
    };
    let TerminalReceipt::Inventory(inventory) =
        app.terminal(session.clone(), TerminalAction::List).await?
    else {
        return Err(CoreError::Shutdown);
    };
    let before = state.terminal_visible().cloned();
    let changed = state.terminal_inventory() != inventory.entries || before != inventory.selected;
    state.apply_terminal_inventory(&session, inventory);
    let Some(target) = state.terminal_visible().cloned() else {
        return Ok(changed);
    };
    if state.terminal_cursor().is_none() {
        let TerminalReceipt::Snapshot(snapshot) = app
            .terminal(session.clone(), TerminalAction::Snapshot(target.clone()))
            .await?
        else {
            return Err(CoreError::Shutdown);
        };
        state.apply_terminal_snapshot(snapshot);
    }
    let cursor = state.terminal_cursor().ok_or(CoreError::Shutdown)?;
    let TerminalReceipt::Replay(replay) = app
        .terminal(session, TerminalAction::Replay { target, cursor })
        .await?
    else {
        return Err(CoreError::Shutdown);
    };
    Ok(state.apply_terminal_replay(replay) || changed)
}
pub(super) async fn refresh_or_report(app: &CoreApp, state: &mut TuiState) {
    if !state.chrome.session_terminal || state.attached_session().is_none() {
        return;
    }
    if let Err(error) = refresh(app, state).await {
        let composer = state.terminals_open();
        state.terminal_failure(composer);
        if !composer {
            state.push_note(&error.to_string());
        }
    }
}

async fn select(
    app: &CoreApp,
    session: &SessionId,
    target: Option<TerminalRef>,
) -> Result<(), CoreError> {
    app.terminal(session.clone(), TerminalAction::Select(target))
        .await?;
    Ok(())
}
async fn create(
    app: &CoreApp,
    session: &SessionId,
    action: TerminalAction,
) -> Result<(), CoreError> {
    let TerminalReceipt::Created(entry) = app.terminal(session.clone(), action).await? else {
        return Err(CoreError::Shutdown);
    };
    select(app, session, Some(entry.target)).await
}

pub(super) async fn apply(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    session: SessionId,
    action: TerminalIntent,
) {
    let load_feedback = matches!(
        action,
        TerminalIntent::Refresh
            | TerminalIntent::Create(_)
            | TerminalIntent::Toggle(_)
            | TerminalIntent::Select(_)
    );
    let focus = matches!(
        action,
        TerminalIntent::Create(_) | TerminalIntent::Toggle(_) | TerminalIntent::Select(_)
    );
    let control = matches!(action, TerminalIntent::Control(_));
    let frame = state.detail_area();
    // One invocation owns its immutable source, including after child close.
    let result = async {
        match action {
            TerminalIntent::Refresh | TerminalIntent::CloseComposer => {}
            TerminalIntent::Create(action) => create(app, &session, action).await?,
            TerminalIntent::Select(target) => select(app, &session, Some(target)).await?,
            TerminalIntent::Hide => select(app, &session, None).await?,
            TerminalIntent::Remove(target) => {
                app.terminal(session.clone(), TerminalAction::Remove(target))
                    .await?;
            }
            TerminalIntent::Control(control) => {
                app.terminal(session.clone(), control).await?;
            }
            TerminalIntent::Toggle(creation) => {
                let TerminalReceipt::Inventory(inventory) =
                    app.terminal(session.clone(), TerminalAction::List).await?
                else {
                    return Err(CoreError::Shutdown);
                };
                if inventory.selected.is_some() {
                    select(app, &session, None).await?;
                } else if let Some(last) = inventory.entries.last() {
                    select(app, &session, Some(last.target.clone())).await?;
                } else {
                    create(app, &session, creation).await?;
                }
            }
        }
        Ok::<_, CoreError>(())
    }
    .await;
    if let Err(error) = result {
        if load_feedback {
            state.terminal_failure(true);
        } else {
            state.focus_terminal(false);
            state.push_note(&error.to_string());
        }
        return;
    }
    // Selection belongs to the captured session, NOT a newly focused parent.
    let destination = if state.attached_session() == Some(&session) {
        Some(state)
    } else {
        deck.child_views.get_mut(&session).or_else(|| {
            deck.tabs
                .iter_mut()
                .flatten()
                .find(|v| v.attached_session() == Some(&session))
        })
    };
    if let Some(view) = destination {
        if control {
            return;
        }
        refresh_or_report(app, view).await;
        if focus {
            view.focus_terminal(true);
        }
        resize(app, view, frame).await;
    }
}

pub(super) async fn resize(app: &CoreApp, state: &mut TuiState, frame: ratatui::layout::Rect) {
    let Some(target) = state.terminal_visible().cloned() else {
        return;
    };
    let size = state.terminal_size_for_frame(frame);
    if state.terminal_snapshot().is_some_and(|s| s.size == size) {
        return;
    }
    if let Err(error) = app
        .terminal(
            target.session.clone(),
            TerminalAction::Resize { target, size },
        )
        .await
    {
        state.terminal_failure(false);
        state.push_note(&error.to_string());
    }
}

#[cfg(test)]
mod tests;
