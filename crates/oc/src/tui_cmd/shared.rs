//! Prepared workspace adoption. Retained editors and receipts never cross a
//! task boundary; one read slot coalesces publications during an owner read.
use super::*;
use oc_core::queries::{ChildJob, HistoryPage, WorkspaceSnapshot};

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

#[derive(Clone)]
struct Target {
    route: Option<read_jobs::Route>,
    identity: (u64, u64),
    id: SessionId,
    location: Option<String>,
    floor: u64,
    child: Option<ChildJob>,
    summaries: Vec<(String, usize)>,
}

impl Target {
    fn capture(view: &TuiState) -> Option<Self> {
        Some(Self {
            route: read_jobs::Route::capture(view),
            identity: view.view_identity(),
            id: view.attached_session()?.clone(),
            location: view.chrome.location.clone(),
            floor: view.projection_floor(),
            child: view.linked_child().cloned(),
            summaries: view.dcp_summary_requests(),
        })
    }
}

struct Projection {
    revision: u64,
    page: HistoryPage,
    live: Option<oc_core::queries::LiveTurnSnapshot>,
    jobs: Vec<ChildJob>,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    mcp: oc_core::queries::McpSnapshot,
    shells: Option<Vec<oc_core::queries::ShellJob>>,
    summaries: Vec<(String, oc_core::dcp_view::DcpSummaryPage)>,
}

pub(super) struct Attached {
    projection: Option<Projection>,
    inbox: Vec<oc_core::queries::InboxItem>,
}
pub(super) async fn prepare_attached(
    app: &CoreApp,
    id: SessionId,
    location: Option<String>,
    child: Option<ChildJob>,
    floor: u64,
) -> Result<Option<Attached>, String> {
    if !app.shares_workspace() {
        return Ok(None);
    }
    let snapshot = app
        .workspace(Some(id.clone()))
        .await
        .map_err(|error| error.to_string())?;
    let target = Target {
        route: None,
        identity: (0, 0),
        id,
        location,
        floor,
        child,
        summaries: Vec::new(),
    };
    Ok(Some(Attached {
        projection: prepare_projection(app, &target, &snapshot).await?,
        inbox: snapshot.inbox,
    }))
}
pub(super) async fn apply_attached(app: &CoreApp, view: &mut TuiState, attached: Option<Attached>) {
    if let Some(attached) = attached {
        if let Some(projection) = attached.projection {
            apply_projection(app, view, projection).await;
        }
        if let Some(session) = view.attached_session().cloned() {
            view.apply_inbox(&session, attached.inbox);
        }
    }
}

async fn prepare_projection(
    app: &CoreApp,
    target: &Target,
    snapshot: &WorkspaceSnapshot,
) -> Result<Option<Projection>, String> {
    if snapshot.revision < target.floor {
        return Ok(None);
    }
    if target.location.as_deref() != Some(snapshot.deck.location.as_str()) {
        if target.child.is_none() {
            return Err(CoreError::LocationChanged.to_string());
        }
        if !snapshot.turns.iter().any(|live| live.session == target.id) {
            return Ok(None);
        }
    }
    let page = if let Some(job) = &target.child {
        app.read_child_transcript(job.parent.clone(), job.clone())
            .await
    } else {
        app.transcript_page(target.id.clone(), None, None, HISTORY_PAGE_LIMIT)
            .await
    }
    .map_err(|error| error.to_string())?;
    let compactions = app
        .compaction_history(target.id.clone())
        .await
        .map_err(|error| error.to_string())?;
    let mcp = app.mcp_status().await.map_err(|error| error.to_string())?;
    let shells = app.shell_jobs(target.id.clone()).await.ok();
    let mut jobs = app
        .child_jobs(target.id.clone())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(job) = &target.child {
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
    let mut summaries = Vec::new();
    for (op, index) in &target.summaries {
        if let Ok(Some(page)) = app
            .dcp_summary_page(
                target.id.clone(),
                op.clone(),
                *index,
                0,
                oc_tui::dcp_view::SUMMARY_PAGE_BYTES,
            )
            .await
        {
            summaries.push((op.clone(), page));
        }
    }
    Ok(Some(Projection {
        revision: snapshot.revision,
        page,
        live: snapshot
            .turns
            .iter()
            .find(|live| live.session == target.id)
            .cloned(),
        jobs,
        compactions,
        mcp,
        shells,
        summaries,
    }))
}

// This async signature is shared with replay routing, but the replay path is
// deliberately pure: it must never query the owner or execute effects.
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
            .expect("prepared target projection is infallible");
        }
    }
    view.apply_child_jobs(projection.jobs);
    view.apply_compaction_history(projection.compactions);
    view.apply_mcp_snapshot(projection.mcp);
    if let Some(shells) = projection.shells {
        view.apply_shell_jobs(shells);
    }
    if let Some(session) = view.attached_session().cloned() {
        for (op, page) in projection.summaries {
            view.apply_dcp_summary(&session, &op, page);
        }
    }
    view.cover_projection(projection.revision, truncated);
}

/// Startup/explicit exact-child attach, before a mounted input loop exists.
pub(super) async fn attach(app: &CoreApp, view: &mut TuiState) -> Result<(), String> {
    if !app.shares_workspace() {
        return Ok(());
    }
    let snapshot = app
        .workspace(view.attached_session().cloned())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(target) = Target::capture(view)
        && let Some(projection) = prepare_projection(app, &target, &snapshot).await?
    {
        apply_projection(app, view, projection).await;
    }
    if let Some(session) = view.attached_session().cloned() {
        view.apply_inbox(&session, snapshot.inbox);
    }
    Ok(())
}

struct Root {
    id: SessionId,
    title: bool,
    catalog: Option<CatalogSnapshot>,
    projection: Option<Projection>,
}

struct Request {
    selected: Option<SessionId>,
    navigation: u64,
    location: Option<String>,
    deck_revision: Option<String>,
    targets: Vec<Target>,
    roots: Vec<(SessionId, (u64, u64))>,
    gap: bool,
    read_only: bool,
    pending: std::collections::HashSet<SessionId>,
    provisional: std::collections::HashSet<SessionId>,
}

struct Prepared {
    snapshot: WorkspaceSnapshot,
    roots: Vec<Root>,
    repairs: Vec<(Target, Projection)>,
    home: Option<CatalogSnapshot>,
    unavailable: Vec<oc_core::queries::ServiceDiagnostic>,
    save_disabled: bool,
}

pub(super) struct Job {
    request: Request,
    job: tokio::task::JoinHandle<Result<Prepared, String>>,
}
impl Job {
    pub(super) fn ready(&self) -> bool {
        self.job.is_finished()
    }
}

async fn prepare(app: &CoreApp, request: &Request) -> Result<Prepared, String> {
    let snapshot = app
        .workspace(request.selected.clone())
        .await
        .map_err(|error| error.to_string())?;
    let mut repairs = Vec::new();
    if request.gap {
        for target in &request.targets {
            let selected;
            let covered = if snapshot.deck.sessions.contains(&target.id)
                || request.selected.as_ref() == Some(&target.id)
            {
                &snapshot
            } else {
                selected = app
                    .workspace(Some(target.id.clone()))
                    .await
                    .map_err(|error| error.to_string())?;
                &selected
            };
            if let Some(projection) = prepare_projection(app, target, covered).await? {
                repairs.push((target.clone(), projection));
            }
        }
    }
    let mut roots = Vec::new();
    let mut unavailable = Vec::new();
    let mut save_disabled = snapshot.deck.projected();
    if !request.read_only && !request.gap {
        for (index, id) in snapshot.deck.sessions.iter().enumerate() {
            if request.provisional.contains(id) {
                roots.push(Root {
                    id: id.clone(),
                    title: snapshot
                        .deck
                        .new_session_titles
                        .get(index)
                        .copied()
                        .unwrap_or(false),
                    catalog: None,
                    projection: None,
                });
                continue;
            }
            let retained = request.targets.iter().find(|target| {
                &target.id == id
                    && target.location.as_deref() == Some(snapshot.deck.location.as_str())
                    && !request.pending.contains(id)
            });
            let result = async {
                let catalog = app
                    .session_selection(id.clone(), false, SelectionAction::Current)
                    .await?;
                let projection = if retained.is_none() {
                    let target = Target {
                        route: None,
                        identity: (0, 0),
                        id: id.clone(),
                        location: Some(snapshot.deck.location.clone()),
                        floor: 0,
                        child: None,
                        summaries: Vec::new(),
                    };
                    prepare_projection(app, &target, &snapshot)
                        .await
                        .map_err(CoreError::Application)?
                } else {
                    None
                };
                Ok::<_, CoreError>((catalog, projection))
            }
            .await;
            match result {
                Ok((catalog, projection)) => roots.push(Root {
                    id: id.clone(),
                    title: snapshot
                        .deck
                        .new_session_titles
                        .get(index)
                        .copied()
                        .unwrap_or(false),
                    catalog: Some(catalog),
                    projection,
                }),
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
                }
            }
        }
    }
    let home = if !request.read_only && !request.gap {
        Some(
            app.home_selection(SelectionAction::Current)
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(Prepared {
        snapshot,
        roots,
        repairs,
        home,
        unavailable,
        save_disabled,
    })
}

pub(super) async fn sync(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
) -> Result<bool, String> {
    let mut changed = false;
    if deck.workspace_job.as_ref().is_some_and(Job::ready) {
        let Job { request, job } = deck.workspace_job.take().expect("finished workspace read");
        let prepared = match job
            .await
            .unwrap_or_else(|_| Err("workspace read interrupted".into()))
        {
            Ok(prepared) => prepared,
            Err(error) => {
                deck.shared_gap |= request.gap;
                // A failed read is never an excuse to replace retained owners.
                if deck.save_disabled && deck.authoritative_deck.is_none() {
                    // Startup already refused a malformed preference. A failed
                    // background read must not erase that actionable contract,
                    // nor replace a newer explicit save-refusal message.
                    if !state
                        .note()
                        .is_some_and(|note| note.starts_with("tab deck could not be saved"))
                    {
                        state.push_note(&format!(
                            "saved tab deck unavailable; workspace refresh failed: {error}"
                        ));
                    }
                } else {
                    state.push_note(&format!("Workspace refresh failed: {error}"));
                }
                return Ok(true);
            }
        };
        if deck.location != request.location {
            return Ok(false);
        }
        if deck.navigation_revision == request.navigation {
            deck.activity
                .apply(&prepared.snapshot, request.selected.clone());
        } else {
            deck.shared_refresh = true;
        }
        for (target, projection) in prepared.repairs {
            let mut applied = false;
            let mut stale = false;
            if let Some(view) = read_jobs::views_mut(state, deck)
                .find(|view| view.view_identity() == target.identity)
            {
                if target
                    .route
                    .as_ref()
                    .is_some_and(|route| route.matches(view))
                {
                    apply_projection(app, view, projection).await;
                    applied = true;
                } else {
                    stale = true;
                }
            }
            if applied {
                deck.picker_pending_tabs.remove(&target.id);
            }
            if stale {
                deck.shared_gap = true;
                deck.shared_refresh = true;
            }
        }
        if request.gap {
            deck.shared_refresh |= !deck.read_only;
            deck.controls.request();
            changed = true;
        } else if !deck.read_only
            && deck.child_parent.is_none()
            && deck.deck_job.is_none()
            && deck.navigation_job.is_none()
            && deck.moved_pending.is_none()
            && !deck.scope_uncertain
            && deck.revision == request.deck_revision
        {
            adopt(
                app,
                state,
                deck,
                request,
                prepared.snapshot,
                prepared.roots,
                prepared.home,
                prepared.save_disabled,
                prepared.unavailable,
            )
            .await?;
            changed = true;
        } else if !deck.read_only {
            deck.shared_refresh = true;
        }
    }
    if deck.workspace_job.is_none() && deck.shared_refresh {
        if (deck.navigation_job.is_some() || deck.moved_pending.is_some() || deck.scope_uncertain)
            && !deck.shared_gap
        {
            return Ok(changed);
        }
        if deck.read_only && !deck.shared_gap {
            deck.shared_refresh = false;
            return Ok(changed);
        }
        let request = Request {
            selected: state.attached_session().cloned(),
            navigation: deck.navigation_revision,
            location: deck.location.clone(),
            deck_revision: deck.revision.clone(),
            gap: deck.shared_gap,
            read_only: deck.read_only,
            pending: deck.picker_pending_tabs.clone(),
            targets: std::iter::once(&*state)
                .chain(deck.tabs.iter().flatten())
                .chain(deck.home.iter())
                .chain(deck.child_parent.as_deref())
                .chain(deck.child_views.values())
                .filter_map(Target::capture)
                .collect(),
            roots: std::iter::once(&*state)
                .chain(deck.tabs.iter().flatten())
                .filter_map(|view| {
                    view.attached_session()
                        .cloned()
                        .map(|id| (id, view.view_identity()))
                })
                .collect(),
            provisional: std::iter::once(&*state)
                .chain(deck.tabs.iter().flatten())
                .filter(|view| view.attached_session().is_none())
                .filter_map(|view| tab_route_session(view).cloned())
                .collect(),
        };
        deck.shared_refresh = false;
        deck.shared_gap = false;
        let captured = Request {
            selected: request.selected.clone(),
            navigation: request.navigation,
            location: request.location.clone(),
            targets: request.targets.clone(),
            roots: request.roots.clone(),
            deck_revision: request.deck_revision.clone(),
            gap: request.gap,
            read_only: request.read_only,
            pending: request.pending.clone(),
            provisional: request.provisional.clone(),
        };
        let owner = app.clone();
        deck.workspace_job = Some(Job {
            request,
            job: oc_core::wake::spawn(async move { prepare(&owner, &captured).await }),
        });
    }
    Ok(changed)
}

#[allow(clippy::too_many_arguments)]
async fn adopt(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    request: Request,
    snapshot: WorkspaceSnapshot,
    roots: Vec<Root>,
    home_catalog: Option<CatalogSnapshot>,
    save_disabled: bool,
    unavailable: Vec<oc_core::queries::ServiceDiagnostic>,
) -> Result<(), String> {
    TuiState::new_home(app.clone())
        .apply_tab_rail_width(snapshot.deck.rail_width)
        .map_err(|error| error.to_string())?;
    let local_focus = tab_route_session(state).cloned();
    let local_identity = state.view_identity();
    // A read may observe an external focus edit before the deferred local CAS
    // starts. Adopt those facts, but do not silently erase its conflict feedback
    // merely because the external focus happens to equal the local destination.
    let selection_superseded = deck.selection_dirty
        && deck.location.as_deref() == Some(snapshot.deck.location.as_str())
        && deck.revision != snapshot.deck.revision
        && deck.authoritative_deck.as_ref().is_some_and(|saved| {
            saved.sessions == snapshot.deck.sessions
                && saved.new_session_titles == snapshot.deck.new_session_titles
        });
    let preserve_focus = deck.navigation_revision != request.navigation
        || deck.selection_dirty
        || deck.deck_job.is_some()
        // A retained fresh candidate has no durable deck membership yet. An
        // empty owner deck cannot choose a replacement Home behind its dialog.
        || state.tab_route_session().is_some() && state.attached_session().is_none()
        || state.has_occupied_submission()
        || deck.owns_receipt(state);
    let mut local = Vec::new();
    let mut home = deck.home.take();
    for (index, view) in deck
        .tabs
        .iter_mut()
        .enumerate()
        .filter_map(|(index, view)| view.take().map(|view| (index, view)))
    {
        local.push((view, deck.tab_cards_before[index]));
    }
    let old = std::mem::replace(state, TuiState::new_home(app.clone()));
    if deck.active_tab.is_some() || tab_route_session(&old).is_some() {
        local.push((old, deck.cards_before));
    } else {
        home = Some(old);
    }
    let mut tabs = Vec::new();
    let mut cursors = Vec::new();
    let retained = |view: &TuiState, deck: &LoopState| {
        let captured = request
            .roots
            .iter()
            .any(|(_, identity)| *identity == view.view_identity());
        view.attached_session().is_none()
            || view.has_occupied_submission()
            || deck.owns_receipt(view)
            || tab_route_session(view).is_some_and(|id| {
                snapshot.deck.sessions.contains(id) || deck.closed_owners.contains_key(id)
            })
            || !captured
    };
    let mut spare = MAX_TABS.saturating_sub(
        local
            .iter()
            .filter(|(view, _)| retained(view, deck) || view.has_pending_transcript_projection())
            .count(),
    );
    for root in roots {
        let previous = local
            .iter()
            .position(|(view, _)| tab_route_session(view) == Some(&root.id))
            .map(|index| local.remove(index));
        if previous.is_none() && root.catalog.is_none() {
            deck.shared_refresh = true;
            continue;
        }
        if previous.is_none() {
            if spare == 0 {
                deck.picker_pending_tabs.insert(root.id);
                continue;
            }
            spare -= 1;
        }
        let (mut view, cursor) = if let Some(previous) = previous {
            previous
        } else {
            (TuiState::new(app.clone(), root.id.clone()), None)
        };
        view.resume_transcript_projection();
        // A fresh provisional's receipt is the sole authority to attach it.
        // Workspace membership may become durable before that ACK arrives.
        if view.attached_session().is_some() {
            if let Some(projection) = root.projection
                && request
                    .targets
                    .iter()
                    .find(|target| target.identity == view.view_identity())
                    .is_none_or(|target| {
                        target
                            .route
                            .as_ref()
                            .is_some_and(|route| route.matches(&view))
                    })
            {
                apply_projection(app, &mut view, projection).await;
            }
            if !view.has_pending_model_commit()
                && !view.has_pending_submission()
                && let Some(catalog) = root.catalog
            {
                let generation = view.chrome.selection_generation;
                view.apply_catalog_refresh(catalog);
                if generation != view.chrome.selection_generation
                    && view.picker_provider_filter().is_some()
                {
                    deck.catalogs
                        .request(app, &mut view, catalog_jobs::Request::Provider);
                }
            }
        }
        view.new_session_tab = root.title;
        deck.picker_pending_tabs.remove(&root.id);
        tabs.push(Some(view));
        cursors.push(cursor);
    }
    // No authoritative read may retire an occupied receipt, a fresh candidate
    // (including Failed/unknown), or a route created after this read began.
    for (view, cursor) in local {
        let keep = retained(&view, deck);
        if !keep && view.has_pending_transcript_projection() {
            view.cancel_transcript_projection();
            view.poll_transcript_projection();
            deck.shared_refresh = true;
        }
        if keep || view.has_pending_transcript_projection() {
            tabs.push(Some(view));
            cursors.push(cursor);
        }
    }
    let active = if preserve_focus {
        tabs.iter().position(|view| {
            view.as_ref().is_some_and(|view| {
                view.view_identity() == local_identity
                    || local_focus
                        .as_ref()
                        .is_some_and(|id| tab_route_session(view) == Some(id))
            })
        })
    } else {
        snapshot.deck.active.as_ref().and_then(|id| {
            tabs.iter().position(|view| {
                view.as_ref()
                    .is_some_and(|view| tab_route_session(view) == Some(id))
            })
        })
    };
    let mut selected = if let Some(index) = active {
        tabs[index].take().expect("selected retained root")
    } else if let Some(home) = home.take() {
        home
    } else {
        let mut home = TuiState::new_home(app.clone());
        if let Some(catalog) = home_catalog {
            home.apply_catalog(catalog);
        }
        home
    };
    if selected.tab_rail_preference() != snapshot.deck.rail_width {
        selected
            .apply_tab_rail_width(snapshot.deck.rail_width)
            .expect("validated geometry");
    }
    if save_disabled {
        for diagnostic in unavailable {
            if !selected.chrome.service_diagnostics.contains(&diagnostic) {
                selected.chrome.service_diagnostics.push(diagnostic);
            }
        }
        // Repeated background publication must not replace the explicit
        // feedback for a user's refused save/activation on this same deck.
        if !deck.save_disabled || deck.location.as_deref() != Some(snapshot.deck.location.as_str())
        {
            selected.push_note("saved tabs partially unavailable; review saved tabs");
        }
    } else if selection_superseded {
        selected.push_note(
            "tab deck could not be saved; saved tabs changed before local selection; refresh saved tabs before retrying",
        );
    }
    *state = selected;
    deck.home = if active.is_some() { home } else { None };
    deck.tabs = tabs;
    deck.tab_cards_before = cursors;
    deck.active_tab = active;
    deck.cards_before = active.and_then(|index| deck.tab_cards_before[index]);
    deck.authoritative_deck = Some(snapshot.deck.clone());
    deck.selection_blocked = false;
    deck.save_disabled = save_disabled;
    deck.location = Some(snapshot.deck.location);
    // Never race a read's older token over an in-flight CAS acknowledgement.
    if deck.deck_job.is_none() {
        deck.revision = snapshot.deck.revision;
    }
    deck.controls.request();
    deck.sync_tabs(state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_workspace_read_keeps_malformed_deck_contract_and_newer_save_refusal() {
        let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(8, 78);
        let mut state = TuiState::new_home(app.clone());
        state.chrome.location = Some("/fixture".into());
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            save_disabled: true,
            ..Default::default()
        };
        for explicit in [false, true] {
            state.push_note(if explicit {
                "tab deck could not be saved; review saved tabs"
            } else {
                "saved tab deck unavailable; review saved tabs"
            });
            deck.workspace_job = Some(Job {
                request: Request {
                    selected: None,
                    navigation: deck.navigation_revision,
                    location: deck.location.clone(),
                    deck_revision: None,
                    targets: Vec::new(),
                    roots: Vec::new(),
                    gap: false,
                    read_only: false,
                    pending: Default::default(),
                    provisional: Default::default(),
                },
                job: oc_core::wake::spawn(async { Err("invalid stored tab deck".into()) }),
            });
            tokio::time::timeout(Duration::from_secs(1), async {
                while !deck.workspace_job.as_ref().unwrap().ready() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(sync(&app, &mut state, &mut deck).await.unwrap());
            if explicit {
                assert_eq!(
                    state.note(),
                    Some("tab deck could not be saved; review saved tabs")
                );
            } else {
                assert!(
                    state
                        .note()
                        .unwrap()
                        .starts_with("saved tab deck unavailable")
                );
                assert!(state.note().unwrap().contains("invalid stored tab deck"));
            }
            assert!(deck.save_disabled);
            assert!(deck.revision.is_none());
            assert!(deck.authoritative_deck.is_none());
            assert!(inbox.try_recv().is_err(), "no repair write or admission");
        }
    }

    #[tokio::test]
    async fn repeated_partial_deck_publication_keeps_explicit_save_failure_and_original_membership()
    {
        let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(8, 75);
        let id = SessionId("good".into());
        let mut state = TuiState::new(app.clone(), id.clone());
        state.chrome.location = Some("/fixture".into());
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            revision: Some("original".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        let saved = TabDeckSnapshot {
            location: "/fixture".into(),
            revision: Some("original".into()),
            sessions: vec![id.clone(), SessionId("unreadable".into())],
            active: Some(id.clone()),
            ..Default::default()
        };
        for repeated in [false, true] {
            let request = Request {
                selected: Some(id.clone()),
                navigation: deck.navigation_revision,
                location: deck.location.clone(),
                deck_revision: deck.revision.clone(),
                targets: Vec::new(),
                roots: vec![(id.clone(), state.view_identity())],
                gap: false,
                read_only: false,
                pending: Default::default(),
                provisional: Default::default(),
            };
            adopt(
                &app,
                &mut state,
                &mut deck,
                request,
                WorkspaceSnapshot {
                    revision: 1,
                    deck: saved.clone(),
                    turns: Vec::new(),
                    inbox: Vec::new(),
                    activity: Vec::new(),
                    selected_root: Some(id.clone()),
                },
                vec![Root {
                    id: id.clone(),
                    title: false,
                    catalog: None,
                    projection: None,
                }],
                None,
                true,
                Vec::new(),
            )
            .await
            .unwrap();
            if !repeated {
                assert_eq!(
                    state.note(),
                    Some("saved tabs partially unavailable; review saved tabs")
                );
                apply_intent(
                    &app,
                    &mut state,
                    &mut deck,
                    PanelIntent::ActivateTab { index: 0 },
                )
                .await
                .unwrap();
            }
            assert_eq!(
                state.note(),
                Some("tab deck could not be saved; review saved tabs")
            );
            assert!(deck.save_disabled);
            assert_eq!(deck.authoritative_deck.as_ref(), Some(&saved));
            deck_jobs::sync(&app, &mut state, &mut deck).await;
            assert!(inbox.try_recv().is_err(), "no pruned membership save");
        }
        for _ in 0..2 {
            apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
                .await
                .unwrap();
            assert_eq!(
                state.note(),
                Some("tab deck could not be saved; review saved tabs")
            );
            assert!(deck.save_disabled);
            assert_eq!(deck.authoritative_deck.as_ref(), Some(&saved));
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::ActivateTab { index: 0 },
            )
            .await
            .unwrap();
        }
        assert!(
            inbox.try_recv().is_err(),
            "new and restored Home do not save a pruned deck"
        );
    }

    #[tokio::test]
    async fn external_focus_read_reports_superseded_local_selection_without_replaying_write() {
        let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(8, 76);
        let id = SessionId("root".into());
        let mut state = TuiState::new(app.clone(), id.clone());
        state.chrome.location = Some("/fixture".into());
        let mut deck = LoopState {
            location: Some("/fixture".into()),
            revision: Some("original".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        deck.authoritative_deck = Some(deck.snapshot(&state));
        apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
            .await
            .unwrap();
        let external = TabDeckSnapshot {
            revision: Some("external".into()),
            active: None,
            ..deck.authoritative_deck.clone().unwrap()
        };
        let request = Request {
            selected: None,
            navigation: deck.navigation_revision,
            location: deck.location.clone(),
            deck_revision: deck.revision.clone(),
            targets: Vec::new(),
            roots: vec![(id.clone(), deck.tabs[0].as_ref().unwrap().view_identity())],
            gap: false,
            read_only: false,
            pending: Default::default(),
            provisional: Default::default(),
        };
        adopt(
            &app,
            &mut state,
            &mut deck,
            request,
            WorkspaceSnapshot {
                revision: 1,
                deck: external.clone(),
                turns: Vec::new(),
                inbox: Vec::new(),
                activity: Vec::new(),
                selected_root: None,
            },
            vec![Root {
                id,
                title: false,
                catalog: None,
                projection: None,
            }],
            None,
            false,
            Vec::new(),
        )
        .await
        .unwrap();
        assert!(
            state
                .note()
                .unwrap()
                .starts_with("tab deck could not be saved")
        );
        assert_eq!(deck.authoritative_deck.as_ref(), Some(&external));
        deck_jobs::sync(&app, &mut state, &mut deck).await;
        assert!(
            inbox.try_recv().is_err(),
            "read alignment never replays the old CAS"
        );
    }
}
