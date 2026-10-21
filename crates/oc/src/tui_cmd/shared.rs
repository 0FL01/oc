//! Shared workspace adoption. Editor objects stay local; an authoritative deck
//! never replaces another chat's unsent text, image bytes, caret or undo stack.
use super::*;

pub(super) fn turn_event(event: &CoreEvent) -> bool {
    matches!(
        event,
        CoreEvent::TurnStarted { .. }
            | CoreEvent::TurnPresentation { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ReasoningDelta { .. }
            | CoreEvent::ReasoningItemEnded { .. }
            | CoreEvent::ToolArgumentStream { .. }
            | CoreEvent::ToolCallStarted { .. }
            | CoreEvent::ToolCallProgress { .. }
            | CoreEvent::ToolCallFinished { .. }
            | CoreEvent::TurnUsage { .. }
            | CoreEvent::TurnContextUsage { .. }
            | CoreEvent::RetryScheduled { .. }
            | CoreEvent::TurnFinished { .. }
            | CoreEvent::TurnInterrupted { .. }
            | CoreEvent::TurnFailed { .. }
    )
}

struct Projection {
    revision: u64,
    page: oc_core::queries::HistoryPage,
    live: Option<oc_core::queries::LiveTurnSnapshot>,
    jobs: Vec<oc_core::queries::ChildJob>,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    mcp: oc_core::queries::McpSnapshot,
    shells: Option<Vec<oc_core::queries::ShellJob>>,
}

async fn prepare_projection(
    app: &CoreApp,
    view: &TuiState,
    snapshot: &oc_core::queries::WorkspaceSnapshot,
) -> Result<Option<Projection>, String> {
    let Some(id) = view.attached_session() else {
        return Ok(None);
    };
    if snapshot.revision < view.projection_floor() {
        return Ok(None);
    }
    if view.chrome.location.as_deref() != Some(snapshot.deck.location.as_str()) {
        if view.linked_child().is_none() {
            return Err(CoreError::LocationChanged.to_string());
        }
        if !snapshot.turns.iter().any(|live| &live.session == id) {
            // An old launch can still be read through its exact fence after
            // the parent moves. Its historical Source is not workspace
            // coverage: retain that view, never infer idle from this absence.
            return Ok(None);
        }
    }
    // Always read canonical state AFTER the snapshot, even with a running T2:
    // its terminal predecessor T1 is no longer in the live materialization.
    let page = if let Some(job) = view.linked_child().cloned() {
        app.read_child_transcript(job.parent.clone(), job).await
    } else {
        app.transcript_page(id.clone(), None, None, HISTORY_PAGE_LIMIT)
            .await
    }
    .map_err(|error| error.to_string())?;
    // Preserve the existing shared-refresh resource reconciliation when a wire
    // gap also loses a compaction, Shell or MCP publication.
    let compactions = app
        .compaction_history(id.clone())
        .await
        .map_err(|error| error.to_string())?;
    let mcp = app.mcp_status().await.map_err(|error| error.to_string())?;
    let shells = app.shell_jobs(id.clone()).await.ok();
    let mut jobs = app
        .child_jobs(id.clone())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(job) = view.linked_child() {
        jobs.extend(
            app.child_jobs(job.parent.clone())
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    jobs.sort_by_key(|job| {
        !matches!(
            job.state,
            oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
        )
    });
    jobs.truncate(16);
    Ok(Some(Projection {
        revision: snapshot.revision,
        page,
        live: snapshot
            .turns
            .iter()
            .find(|live| &live.session == id)
            .cloned(),
        jobs,
        compactions,
        mcp,
        shells,
    }))
}

async fn apply_projection(app: &CoreApp, view: &mut TuiState, projection: Projection) {
    if projection.revision < view.projection_floor() {
        return;
    }
    view.refresh_completed_page(&projection.page);
    if let Some(job) = projection.page.child_job.as_deref() {
        view.refresh_linked_child(job.clone());
    }
    view.reset_live_projection();
    let mut truncated = false;
    if let Some(live) = projection.live {
        truncated = live.truncated;
        let mut isolated = LoopState::default();
        for mut event in live.events {
            if let CoreEvent::TurnStarted {
                model_switch: Some(notice),
                ..
            } = &mut event
            {
                notice.display_name = live.display_name.clone();
            }
            // Prepared snapshot replay contains only target turn presentation.
            // No owner reads, route changes or execution effects occur here.
            Box::pin(handle_worker_event_at(
                app,
                view,
                &mut isolated,
                &live.session,
                event,
                None,
                true,
            ))
            .await
            .expect("prepared target turn projection is infallible");
        }
    }
    view.apply_child_jobs(projection.jobs);
    view.apply_compaction_history(projection.compactions);
    view.apply_mcp_snapshot(projection.mcp);
    if let Some(shells) = projection.shells {
        view.apply_shell_jobs(shells);
    }
    view.cover_projection(projection.revision, truncated);
    refresh_dcp_summaries(app, view).await;
}

pub(super) async fn attach(app: &CoreApp, view: &mut TuiState) -> Result<(), String> {
    if !app.shares_workspace() {
        return Ok(());
    }
    let snapshot = app
        .workspace(view.attached_session().cloned())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(projection) = prepare_projection(app, view, &snapshot).await? {
        apply_projection(app, view, projection).await;
    }
    // OC2 steer/queue input pending for this session survives a reattach.
    if let Some(session) = view.attached_session().cloned() {
        view.apply_inbox(&session, snapshot.inbox);
    }
    Ok(())
}

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
            .is_some_and(|view| view.has_pending_submission() || view.has_pending_model_commit())
        || deck
            .child_parent
            .as_deref()
            .is_some_and(|view| view.has_pending_submission() || view.has_pending_model_commit())
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
    if deck.read_only && !deck.shared_gap {
        deck.shared_refresh = false;
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
    if deck.shared_gap {
        // Stage every bounded repair before touching an existing owner. Child
        // snapshots also contain roots; apply each response only to its target.
        let mut repairs = Vec::new();
        for view in std::iter::once(&*state)
            .chain(deck.tabs.iter().flatten())
            .chain(deck.child_parent.as_deref())
            .chain(deck.child_views.values())
        {
            let Some(id) = view.attached_session() else {
                continue;
            };
            let selected;
            let covered =
                if snapshot.deck.sessions.contains(id) || state.attached_session() == Some(id) {
                    &snapshot
                } else {
                    selected = app
                        .workspace(Some(id.clone()))
                        .await
                        .map_err(|error| error.to_string())?;
                    &selected
                };
            if let Some(projection) = prepare_projection(app, view, covered).await? {
                repairs.push((id.clone(), projection));
            }
        }
        for (id, projection) in repairs {
            if let Some(view) = std::iter::once(&mut *state)
                .chain(deck.tabs.iter_mut().flatten())
                .chain(deck.child_parent.as_deref_mut())
                .chain(deck.child_views.values_mut())
                .find(|view| view.attached_session() == Some(&id))
            {
                apply_projection(app, view, projection).await;
                deck.picker_pending_tabs.remove(&id);
            }
        }
        refresh_control_views(app, state, deck).await?;
        deck.shared_gap = false;
        // Reconcile deck/catalog separately, without throwing repaired windows
        // away. A linked route does not adopt another root selection.
        deck.shared_refresh = !deck.read_only;
        deck.sync_tabs(state);
        return Ok(true);
    }

    // Ordinary WorkspaceChanged must not reload healthy retained history. Stage
    // only missing views and catalogs, then move the existing complete owners.
    let mut prepared = Vec::new();
    let mut unavailable = Vec::new();
    let mut save_disabled = snapshot.deck.projected();
    for (index, id) in snapshot.deck.sessions.iter().enumerate() {
        let retained = std::iter::once(&*state)
            .chain(deck.tabs.iter().flatten())
            .find(|view| {
                view.attached_session() == Some(id)
                    && view.chrome.location.as_deref() == Some(snapshot.deck.location.as_str())
                    && !deck.picker_pending_tabs.contains(id)
            });
        let loaded = if retained.is_some() {
            app.session_selection(id.clone(), false, SelectionAction::Current)
                .await
                .map(|catalog| (None, Some(catalog)))
        } else {
            load_tab(app, id.clone())
                .await
                .map(|view| (Some(view), None))
        };
        let (mut view, catalog) = match loaded {
            Ok(loaded) => loaded,
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
                save_disabled = true;
                continue;
            }
        };
        if let Some(view) = view.as_mut()
            && let Some(projection) = prepare_projection(app, view, &snapshot).await?
        {
            apply_projection(app, view, projection).await;
        }
        let new_title = snapshot
            .deck
            .new_session_titles
            .get(index)
            .copied()
            .unwrap_or(false);
        prepared.push((id.clone(), new_title, view, catalog));
    }
    let active_tab = snapshot
        .deck
        .active
        .as_ref()
        .and_then(|id| prepared.iter().position(|(target, ..)| target == id));
    let mut next_home = if active_tab.is_none() {
        let mut home = TuiState::new_home(app.clone());
        home.apply_catalog(
            app.home_selection(SelectionAction::Current)
                .await
                .map_err(|error| error.to_string())?,
        );
        Some(home)
    } else {
        None
    };
    // Validate geometry before taking any retained view.
    TuiState::new_home(app.clone())
        .apply_tab_rail_width(snapshot.deck.rail_width)
        .map_err(|error| error.to_string())?;
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
    let mut tabs = Vec::new();
    for (id, new_title, fresh, catalog) in prepared {
        deck.picker_pending_tabs.remove(&id);
        let mut view = if let Some(mut fresh) = fresh {
            if let Some(previous) = local.remove(&id) {
                fresh.preserve_shared_composer(previous);
            }
            fresh
        } else {
            local.remove(&id).expect("prepared retained root")
        };
        if let Some(catalog) = catalog {
            view.apply_catalog(catalog);
        }
        view.new_session_tab = new_title;
        tabs.push(Some(view));
    }
    let mut selected = if let Some(index) = active_tab {
        deck.home = home;
        tabs[index].take().expect("selected shared root")
    } else {
        let mut selected = next_home.take().expect("prepared Home");
        if let Some(home) = home {
            selected.preserve_shared_composer(home);
        }
        deck.home = None;
        selected
    };
    selected
        .apply_tab_rail_width(snapshot.deck.rail_width)
        .expect("validated tab geometry");
    if save_disabled {
        selected.chrome.service_diagnostics.extend(unavailable);
        selected.push_note("saved tabs partially unavailable; review saved tabs");
    }
    *state = selected;
    deck.tabs = tabs;
    deck.active_tab = active_tab;
    deck.tab_cards_before = vec![None; deck.tabs.len()];
    deck.save_disabled = save_disabled;
    deck.location = Some(snapshot.deck.location);
    deck.revision = snapshot.deck.revision;
    deck.shared_refresh = false;
    deck.shared_gap = false;
    deck.sync_tabs(state);
    refresh_control_views(app, state, deck).await?;
    Ok(true)
}
