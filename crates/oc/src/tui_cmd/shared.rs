//! Shared workspace adoption. Editor objects stay local; an authoritative deck
//! never replaces another chat's unsent text, image bytes, caret or undo stack.
use super::*;

pub(super) async fn sync(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
) -> Result<bool, String> {
    if state.has_pending_submission()
        || state.has_pending_model_commit()
        || deck
            .home
            .as_ref()
            .is_some_and(TuiState::has_pending_model_commit)
        || deck
            .tabs
            .iter()
            .flatten()
            .any(TuiState::has_pending_submission)
        || deck
            .tabs
            .iter()
            .flatten()
            .any(TuiState::has_pending_model_commit)
        || deck.conversation_job.is_some()
        || deck.reload_job.is_some()
    {
        return Ok(false);
    }
    let snapshot = match app.workspace(state.attached_session().cloned()).await {
        Ok(snapshot) => snapshot,
        Err(CoreError::StoredTabDeck | CoreError::InvalidTabDeck) => {
            // Malformed preferences are not repaired on read. Keep the safe
            // initial Home (or the prior projection), with saving disabled.
            deck.save_disabled = true;
            deck.shared_refresh = false;
            deck.shared_gap = false;
            state.push_note("saved tab deck unavailable; review saved tabs");
            return Ok(true);
        }
        Err(error) => return Err(error.to_string()),
    };
    if deck.read_only {
        deck.shared_refresh = false;
        return Ok(false);
    }

    // Prepare every fallible read and live adoption before consuming any local
    // editor/receipt. A failed query must leave the complete old view intact.
    let mut next = LoopState::default();
    let mut unavailable = Vec::new();
    for (index, id) in snapshot.deck.sessions.iter().enumerate() {
        let mut view = match load_tab(app, id.clone()).await {
            Ok(view) => view,
            Err(error)
                if restore_error_is_fatal(
                    &error,
                    false,
                    snapshot.deck.active.as_ref() == Some(id),
                ) =>
            {
                return Err(error.to_string());
            }
            Err(error) => {
                if let CoreError::Diagnostic(diagnostic) = error {
                    unavailable.push(diagnostic);
                }
                next.save_disabled = true;
                continue;
            }
        };
        view.new_session_tab = snapshot
            .deck
            .new_session_titles
            .get(index)
            .copied()
            .unwrap_or(false);
        next.tabs.push(Some(view));
    }
    next.active_tab = snapshot.deck.active.as_ref().and_then(|id| {
        next.tabs
            .iter()
            .position(|view| view.as_ref().and_then(TuiState::attached_session) == Some(id))
    });
    let mut selected = if let Some(index) = next.active_tab {
        next.tabs[index].take().expect("selected shared tab")
    } else {
        let mut home = TuiState::new_home(app.clone());
        home.apply_catalog(
            app.home_selection(SelectionAction::Current)
                .await
                .map_err(|error| error.to_string())?,
        );
        home
    };
    next.save_disabled |= snapshot.deck.projected();
    next.location = Some(snapshot.deck.location);
    next.revision = snapshot.deck.revision;
    selected
        .apply_tab_rail_width(snapshot.deck.rail_width)
        .map_err(|error| error.to_string())?;
    for live in snapshot.turns {
        for mut event in live.events {
            if let CoreEvent::TurnStarted {
                model_switch: Some(notice),
                ..
            } = &mut event
            {
                notice.display_name = live.display_name.clone();
            }
            handle_worker_event(app, &mut selected, &mut next, &live.session, event).await?;
        }
    }
    refresh_control_views(app, &mut selected, &mut next).await?;
    if next.save_disabled {
        selected.chrome.service_diagnostics.extend(unavailable);
        selected.push_note("saved tabs partially unavailable; review saved tabs");
    }

    let mut local = std::collections::HashMap::new();
    let mut home = deck.home.take();
    for view in deck.tabs.iter_mut().filter_map(Option::take) {
        if let Some(id) = view.attached_session().cloned() {
            local.insert(id, view);
        }
    }
    let old = std::mem::replace(state, TuiState::new_home(app.clone()));
    if let Some(id) = old.attached_session().cloned() {
        local.insert(id, old);
    } else {
        home = Some(old);
    }
    for view in next
        .tabs
        .iter_mut()
        .flatten()
        .chain(std::iter::once(&mut selected))
    {
        if let Some(previous) = view.attached_session().and_then(|id| local.remove(id)) {
            view.preserve_shared_composer(previous);
        }
    }
    if next.active_tab.is_some() {
        deck.home = home;
    } else {
        if let Some(home) = home {
            selected.preserve_shared_composer(home);
        }
        deck.home = None;
    }
    *state = selected;
    deck.tabs = next.tabs;
    deck.active_tab = next.active_tab;
    deck.tab_cards_before = vec![None; deck.tabs.len()];
    deck.save_disabled = next.save_disabled;
    deck.location = next.location;
    deck.revision = next.revision;
    deck.shared_refresh = false;
    deck.shared_gap = false;
    deck.projection_revision = snapshot.revision;
    deck.sync_tabs(state);
    Ok(true)
}
