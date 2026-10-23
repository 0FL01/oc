//! Linked read-only routing; durable launch reads belong to child_jobs.
use super::*;
use oc_core::queries::ChildJob;
#[cfg(test)]
mod tests;

pub(super) fn park(deck: &mut LoopState, view: TuiState) {
    if deck.child_views.len() >= 16 && !deck.child_views.contains_key(view.session()) {
        let retired = deck
            .child_views
            .iter()
            .find(|(id, v)| {
                retirable(deck, v) && view.linked_child().is_none_or(|j| &j.parent != *id)
            })
            .map(|(id, _)| id.clone());
        if let Some(retired) = retired {
            deck.child_views.remove(&retired);
        }
    }
    // The current child has one reserved return slot. New opens cannot grow
    // this reserve, and an unresolved surface is never an eviction candidate.
    if deck.child_views.len() < 17 || deck.child_views.contains_key(view.session()) {
        deck.child_views.insert(view.session().clone(), view);
    }
}
fn retirable(deck: &LoopState, view: &TuiState) -> bool {
    !view.is_busy()
        && !deck.owns_receipt(view)
        && !view.has_occupied_submission()
        && view.input().is_empty()
        && !deck.children.opening(view.session())
        && if view.has_pending_transcript_projection() {
            view.cancel_transcript_projection();
            false
        } else {
            true
        }
}
pub(super) fn room_for(deck: &LoopState, session: &SessionId) -> bool {
    deck.child_views.contains_key(session)
        || deck.child_views.len() < 16
        || deck.child_views.values().any(|view| retirable(deck, view))
}
pub(super) fn same_launch(a: &ChildJob, b: &ChildJob) -> bool {
    a.parent == b.parent
        && a.child == b.child
        && a.operation == b.operation
        && a.generation == b.generation
        && a.location == b.location
        && a.delivery_id == b.delivery_id
}
pub(super) async fn open(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    selected: ChildJob,
) -> Result<(), String> {
    child_jobs::open(app, state, deck, selected)
}
pub(super) fn return_parent(state: &mut TuiState, deck: &mut LoopState) {
    deck.navigation_revision = deck.navigation_revision.wrapping_add(1);
    if let Some(parent_id) = state.linked_child().map(|j| j.parent.clone())
        && deck
            .child_parent
            .as_ref()
            .and_then(|v| v.attached_session())
            != Some(&parent_id)
        && let Some(parent) = deck.child_views.remove(&parent_id)
    {
        let child = std::mem::replace(state, parent);
        state.resume_transcript_projection();
        park(deck, child);
        return;
    }
    return_root(state, deck);
}
pub(super) fn return_root(state: &mut TuiState, deck: &mut LoopState) {
    if let Some(parent) = deck.child_parent.take() {
        let child = std::mem::replace(state, *parent);
        state.resume_transcript_projection();
        park(deck, child);
        deck.read_only = deck.child_parent_read_only;
        deck.navigation_revision = deck.navigation_revision.wrapping_add(1);
    }
}
pub(super) fn refresh_parent(state: &mut TuiState, deck: &mut LoopState, parent: &SessionId) {
    let mut reads = std::mem::take(&mut deck.reads);
    for view in
        read_jobs::views_mut(state, deck).filter(|view| view.attached_session() == Some(parent))
    {
        reads.request(view, false);
    }
    deck.reads = reads;
}
pub(super) fn event_owner(event: &CoreEvent) -> Option<&SessionId> {
    match event {
        CoreEvent::TurnStarted { session, .. }
        | CoreEvent::TurnPresentation { session, .. }
        | CoreEvent::TextDelta { session, .. }
        | CoreEvent::ReasoningDelta { session, .. }
        | CoreEvent::ReasoningItemEnded { session, .. }
        | CoreEvent::ToolCallStarted { session, .. }
        | CoreEvent::ToolArgumentStream { session, .. }
        | CoreEvent::ToolCallProgress { session, .. }
        | CoreEvent::ToolCallFinished { session, .. }
        | CoreEvent::TurnUsage { session, .. }
        | CoreEvent::TurnContextUsage { session, .. }
        | CoreEvent::TurnFinished { session, .. }
        | CoreEvent::TurnInterrupted { session, .. }
        | CoreEvent::TurnFailed { session, .. }
        | CoreEvent::SessionMoved { session, .. }
        | CoreEvent::SessionModelSelected { session, .. }
        | CoreEvent::SelectionNoticeCommitted { session, .. }
        | CoreEvent::SessionTitleUpdated { session, .. }
        | CoreEvent::ShellChanged { session }
        | CoreEvent::TerminalChanged { session }
        | CoreEvent::InboxChanged { session, .. }
        | CoreEvent::RetryScheduled { session, .. }
        | CoreEvent::ParentContinuationRefused { session, .. } => Some(session),
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
    revision: Option<u64>,
    replay: bool,
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
        let result = Box::pin(handle_worker_event_at(
            app,
            &mut parent,
            deck,
            &owner,
            event.clone(),
            revision,
            replay,
        ))
        .await;
        deck.child_parent = Some(parent);
        deck.read_only = true;
        result?;
        return Ok(true);
    }
    if !replay
        && !deck.child_views.contains_key(&owner)
        && let CoreEvent::TurnStarted { turn, .. } = event
    {
        let mut parents: Vec<_> = std::iter::once(&*state)
            .chain(deck.child_parent.as_deref())
            .chain(deck.tabs.iter().flatten())
            .chain(deck.child_views.values())
            .filter_map(read_jobs::Route::capture)
            .collect();
        parents.sort_by(|a, b| a.session.0.cmp(&b.session.0));
        parents.dedup_by(|a, b| a.session == b.session);
        deck.children
            .discover(app, owner.clone(), turn.clone(), parents);
        return Ok(true);
    }
    if let Some(mut child) = deck.child_views.remove(&owner) {
        let parent = child.linked_child().map(|job| job.parent.clone());
        let result = Box::pin(handle_worker_event_at(
            app,
            &mut child,
            deck,
            &owner,
            event.clone(),
            revision,
            replay,
        ))
        .await;
        park(deck, child);
        result?;
        if !replay {
            if let Some(parent) = &parent
                && matches!(
                    event,
                    CoreEvent::TurnStarted { .. }
                        | CoreEvent::TurnFinished { .. }
                        | CoreEvent::TurnFailed { .. }
                        | CoreEvent::TurnInterrupted { .. }
                )
            {
                refresh_parent(state, deck, parent);
            }
            if parent.as_ref() != state.attached_session()
                && (state.children_open()
                    || state.linked_child().is_some()
                    || state.has_subagent_cards())
            {
                deck.reads.request(state, false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}
