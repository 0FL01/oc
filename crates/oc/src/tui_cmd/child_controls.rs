//! Linked read-only navigation over the existing owner; root deck never changes.
use super::*;
use oc_core::core_app::WorkerTurnId;
use oc_core::queries::ChildJob;

#[cfg(test)]
mod tests;

fn park(deck: &mut LoopState, view: TuiState) {
    if deck.child_views.len() >= 16 && !deck.child_views.contains_key(view.session()) {
        let retired = deck
            .child_views
            .iter()
            .find(|(id, v)| !v.is_busy() && view.linked_child().is_none_or(|j| &j.parent != *id))
            .map(|(id, _)| id.clone());
        if let Some(retired) = retired {
            deck.child_views.remove(&retired);
        }
    }
    if deck.child_views.len() < 16 || deck.child_views.contains_key(view.session()) {
        deck.child_views.insert(view.session().clone(), view);
    }
}

pub(super) async fn refresh(app: &CoreApp, state: &mut TuiState) -> Result<(), String> {
    let parent = state
        .linked_child()
        .map(|j| j.parent.clone())
        .or_else(|| state.attached_session().cloned());
    if let Some(parent) = parent {
        let mut rows = app.child_jobs(parent).await.map_err(|e| e.to_string())?;
        if let Some(linked) = state.linked_child().cloned() {
            if let Some(current) = rows.iter().find(|j| j.operation == linked.operation) {
                state.attach_linked_child(current.clone());
            }
            if let Some(current) = state.linked_child().cloned()
                && !matches!(
                    current.state,
                    oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
                )
                && state.active_turn().is_some()
            {
                let page = app
                    .read_child(current.parent.clone(), current.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                state.reconcile_linked_terminal(&current, &page);
            }
            rows.extend(
                app.child_jobs(linked.child)
                    .await
                    .map_err(|e| e.to_string())?,
            );
        }
        rows.sort_by_key(|j| {
            !matches!(
                j.state,
                oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
            )
        });
        rows.truncate(16);
        state.apply_child_jobs(rows);
    }
    Ok(())
}

async fn view(app: &CoreApp, selected: ChildJob) -> Result<TuiState, String> {
    let page = app
        .read_child(selected.parent.clone(), selected.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut catalog = app.catalog().await.map_err(|e| e.to_string())?;
    if let Some((provider, model)) = selected.model.split_once('/') {
        catalog.provider = provider.into();
        let (model, variant) = model
            .split_once('#')
            .map_or((model, None), |(m, v)| (m, Some(v.to_owned())));
        catalog.model_id = model.into();
        catalog.variant = variant;
    }
    catalog.agent_id = Some(selected.agent.clone());
    catalog.chrome.location = Some(selected.location.clone());
    catalog.chrome.selection_generation = selected.generation;
    let mut view = TuiState::new(app.clone(), selected.child.clone());
    view.attach_page(&page);
    view.apply_catalog(catalog);
    view.attach_linked_child(selected);
    if let Some(job) = view.linked_child()
        && job.state == oc_core::queries::ChildState::Running
        && let Some(turn) = job.turn.clone()
    {
        view.begin_linked_turn(WorkerTurnId(turn));
    }
    Ok(view)
}

pub(super) async fn open(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    selected: ChildJob,
) -> Result<(), String> {
    let parent = deck
        .child_parent
        .as_ref()
        .and_then(|v| v.attached_session())
        .or_else(|| state.attached_session());
    let sibling_parent = state.linked_child().map(|j| &j.parent);
    if parent != Some(&selected.parent)
        && state.attached_session() != Some(&selected.parent)
        && sibling_parent != Some(&selected.parent)
    {
        return Err("foreign child selection".into());
    }
    // Always validate the exact current fence even when reusing a live view.
    let current = app
        .child_jobs(selected.parent.clone())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|j| j.operation == selected.operation)
        .ok_or_else(|| "child generation unavailable".to_owned())?;
    let page = app
        .read_child(selected.parent.clone(), selected)
        .await
        .map_err(|e| e.to_string())?;
    let selected = current;
    let mut next = if let Some(view) = deck.child_views.remove(&selected.child) {
        view
    } else {
        view(app, selected.clone()).await?
    };
    next.attach_linked_child(selected);
    if let Some(current) = next.linked_child().cloned()
        && !matches!(
            current.state,
            oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
        )
    {
        next.reconcile_linked_terminal(&current, &page);
    }
    state.hide_children();
    if deck.child_parent.is_some() {
        let old = std::mem::replace(state, next);
        park(deck, old);
    } else {
        deck.child_parent = Some(Box::new(std::mem::replace(state, next)));
    }
    deck.read_only = true;
    refresh(app, state).await?;
    refresh_approvals(app, state).await?;
    Ok(())
}

pub(super) fn return_parent(state: &mut TuiState, deck: &mut LoopState) {
    if let Some(parent_id) = state.linked_child().map(|j| j.parent.clone())
        && deck
            .child_parent
            .as_ref()
            .and_then(|v| v.attached_session())
            != Some(&parent_id)
        && let Some(parent) = deck.child_views.remove(&parent_id)
    {
        let child = std::mem::replace(state, parent);
        park(deck, child);
        return;
    }
    if let Some(parent) = deck.child_parent.take() {
        let child = std::mem::replace(state, *parent);
        park(deck, child);
        deck.read_only = false;
    }
}

fn event_owner(event: &CoreEvent) -> Option<&SessionId> {
    match event {
        CoreEvent::TurnStarted { session, .. }
        | CoreEvent::TurnPresentation { session, .. }
        | CoreEvent::TextDelta { session, .. }
        | CoreEvent::ReasoningDelta { session, .. }
        | CoreEvent::ReasoningItemEnded { session, .. }
        | CoreEvent::ToolCallStarted { session, .. }
        | CoreEvent::ToolArgumentStream { session, .. }
        | CoreEvent::ToolCallFinished { session, .. }
        | CoreEvent::TurnUsage { session, .. }
        | CoreEvent::TurnFinished { session, .. }
        | CoreEvent::TurnInterrupted { session, .. }
        | CoreEvent::TurnFailed { session, .. }
        | CoreEvent::SessionMoved { session, .. }
        | CoreEvent::ShellChanged { session }
        | CoreEvent::RetryScheduled { session, .. } => Some(session),
        CoreEvent::ChildNotice(n) => Some(&n.job.parent),
        CoreEvent::ShellNotice(n) => Some(&n.session),
        _ => None,
    }
}

pub(super) async fn route_event(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    event: &CoreEvent,
) -> Result<bool, String> {
    let Some(owner) = event_owner(event).cloned() else {
        return Ok(false);
    };
    if state.attached_session() == Some(&owner) {
        return Ok(false);
    }
    if deck
        .child_parent
        .as_ref()
        .is_some_and(|v| v.attached_session() == Some(&owner))
    {
        let mut parent = deck.child_parent.take().expect("linked parent");
        Box::pin(handle_worker_event(
            app,
            &mut parent,
            deck,
            &owner,
            event.clone(),
        ))
        .await?;
        deck.child_parent = Some(parent);
        deck.read_only = true;
        return Ok(true);
    }
    if !deck.child_views.contains_key(&owner) && matches!(event, CoreEvent::TurnStarted { .. }) {
        let root = deck
            .child_parent
            .as_ref()
            .and_then(|v| v.attached_session())
            .or_else(|| state.attached_session());
        if let Some(root) = root {
            let mut parents = vec![root.clone()];
            if state.linked_child().is_some() {
                parents.push(state.session().clone());
            }
            parents.extend(deck.child_views.keys().cloned());
            parents.sort_by(|a, b| a.0.cmp(&b.0));
            parents.dedup();
            for parent in parents {
                let jobs = app.child_jobs(parent).await.map_err(|e| e.to_string())?;
                if let Some(selected) = jobs.iter().find(|j| j.child == owner) {
                    if deck.child_views.len() >= 16 {
                        let retired = deck
                            .child_views
                            .iter()
                            .find(|(_, v)| !v.is_busy())
                            .map(|(id, _)| id.clone());
                        if let Some(retired) = retired {
                            deck.child_views.remove(&retired);
                        }
                    }
                    if deck.child_views.len() < 16 {
                        deck.child_views
                            .insert(owner.clone(), view(app, selected.clone()).await?);
                    }
                    if state.children_open() {
                        refresh(app, state).await?;
                    }
                    break;
                }
            }
        }
    }
    if let Some(mut child) = deck.child_views.remove(&owner) {
        Box::pin(handle_worker_event(
            app,
            &mut child,
            deck,
            &owner,
            event.clone(),
        ))
        .await?;
        park(deck, child);
        if state.children_open() || state.linked_child().is_some() {
            refresh(app, state).await?;
        }
        return Ok(true);
    }
    Ok(false)
}
