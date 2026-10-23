//! Accepted picker/Location routes, prepared without moving a retained editor.
use super::*;

pub(super) enum Request {
    Open(SessionId),
    Location(String),
    Moved(Box<oc_core::queries::LocationSnapshot>),
    #[cfg(test)]
    Published {
        catalog: Box<CatalogSnapshot>,
        location: String,
    },
}
pub(super) struct Job {
    location: Option<String>,
    navigation: u64,
    target: Option<read_jobs::Route>,
    origin: u64,
    command: Option<(u64, String)>,
    explicit_location: bool,
    returning: bool,
    pointer: Option<(u64, (u16, u16, ratatui::layout::Rect))>,
    task: Option<tokio::task::JoinHandle<Result<Prepared, String>>>,
    prepared: Option<Result<Prepared, String>>,
}
pub(super) struct Moved {
    identity: u64,
    session: SessionId,
    location: Option<String>,
    navigation: u64,
    snapshot: Box<oc_core::queries::LocationSnapshot>,
}
pub(super) fn moved(
    state: &TuiState,
    deck: &mut LoopState,
    session: &SessionId,
    snapshot: Box<oc_core::queries::LocationSnapshot>,
) {
    let root = deck.child_parent.as_deref().unwrap_or(state);
    if root.tab_route_session() == Some(session) {
        deck.moved_pending = Some(Moved {
            identity: root.view_identity().0,
            session: session.clone(),
            location: deck.location.clone(),
            navigation: deck.navigation_revision,
            snapshot,
        });
    }
}
fn scope_ready(state: &TuiState, deck: &LoopState) -> bool {
    !std::iter::once(state)
        .chain(deck.tabs.iter().flatten())
        .chain(deck.home.iter())
        .chain(deck.child_parent.as_deref())
        .chain(deck.child_views.values())
        .any(|view| {
            view.has_occupied_submission()
                || view.attached_session().is_none() && view.tab_route_session().is_some()
        })
}
struct View {
    id: SessionId,
    page: oc_core::queries::HistoryPage,
    catalog: CatalogSnapshot,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    attached: Option<shared::Attached>,
    mcp: Option<oc_core::queries::McpSnapshot>,
    shells: Vec<oc_core::queries::ShellJob>,
    refresh_failed: bool,
}
struct Prepared {
    location: String,
    catalog: CatalogSnapshot,
    deck: Result<TabDeckSnapshot, CoreError>,
    views: Vec<View>,
    selected: Option<SessionId>,
    cross: bool,
}
impl Job {
    pub(super) fn retain_pointer(
        &mut self,
        revision: u64,
        pointer: (u16, u16, ratatui::layout::Rect),
    ) {
        self.pointer = Some((revision, pointer));
    }
    pub(super) fn ready(&self) -> bool {
        self.task.as_ref().is_some_and(|task| task.is_finished())
    }
}
async fn view(
    app: &CoreApp,
    id: SessionId,
    page: Option<oc_core::queries::HistoryPage>,
    catalog: Option<CatalogSnapshot>,
) -> Result<View, String> {
    let initial = page.is_none();
    let page = match page {
        Some(page) => Ok(page),
        None => {
            app.transcript_page(id.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
        }
    };
    let catalog = match catalog {
        Some(catalog) => Ok(catalog),
        None => {
            app.session_selection(id.clone(), false, SelectionAction::Current)
                .await
        }
    };
    let page = page.map_err(|e| e.to_string())?;
    let catalog = catalog.map_err(|e| e.to_string())?;
    let compactions = app.compaction_history(id.clone()).await;
    let mut refresh_failed = compactions.is_err();
    let mcp = if initial {
        app.mcp_status().await.ok()
    } else {
        None
    };
    let shells = if initial {
        app.shell_jobs(id.clone()).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let attached =
        shared::prepare_attached(app, id.clone(), catalog.chrome.location.clone(), None, 0).await;
    refresh_failed |= attached.is_err();
    Ok(View {
        id,
        page,
        catalog,
        compactions: compactions.unwrap_or_default(),
        attached: attached.ok().flatten(),
        mcp,
        shells,
        refresh_failed,
    })
}
pub(super) fn start(
    app: &CoreApp,
    state: &TuiState,
    deck: &mut LoopState,
    request: Request,
) -> Result<(), String> {
    if deck.navigation_job.is_some() {
        return Err("session navigation pending; original request retained".into());
    }
    if deck.deck_job.is_some() || deck.deck_read.is_some() {
        return Err("tab save pending; retry session navigation after acknowledgement".into());
    }
    if deck.home_job.is_some() || deck.home_pending.is_some() && !deck.scope_uncertain {
        return Err(
            "Home selection preparing; retry session navigation after acknowledgement".into(),
        );
    }
    deck.departure_ready(state)?;
    if !scope_ready(state, deck) {
        return Err(
            "recover or close the retained provisional request before changing the session scope"
                .into(),
        );
    }
    let search = state.session_search();
    let all = state.sessions_all_projects();
    let saved = deck.structural_snapshot(state);
    let location = deck.location.clone();
    let old = location.clone();
    let owner = app.clone();
    let navigation = deck.navigation_revision;
    let explicit_location = matches!(&request, Request::Location(_));
    let command = match (&request, dispatch(state.input().trim())) {
        (Request::Location(target), Some(CommandAction::SwitchLocation { path }))
            if *target == path =>
        {
            Some((state.input_revision(), state.input().to_owned()))
        }
        _ => None,
    };
    let target = if let Request::Open(id) = &request {
        deck_jobs::route_index(state, deck, id).and_then(|index| {
            let view = if deck.active_tab == Some(index) {
                deck.child_parent.as_deref().unwrap_or(state)
            } else {
                deck.tabs[index].as_ref()?
            };
            read_jobs::Route::capture(view)
        })
    } else {
        None
    };
    deck.navigation_job = Some(Job {
        location,
        navigation,
        target,
        origin: state.view_identity().0,
        command,
        explicit_location,
        returning: false,
        pointer: None,
        prepared: None,
        task: Some(oc_core::wake::spawn(async move {
            let (location, catalog, saved, selected, first) = match request {
                Request::Open(target) => {
                    let receipt = owner
                        .open_picker_session(target.clone(), search, all, saved)
                        .await
                        .map_err(|e| e.to_string())?;
                    if receipt.session != target
                        || receipt.deck.location != receipt.location
                        || receipt.catalog.chrome.location.as_deref().unwrap_or("")
                            != receipt.location
                    {
                        return Err("Session open returned a mismatched Location binding".into());
                    }
                    let first = view(
                        &owner,
                        target.clone(),
                        Some(receipt.page),
                        Some(receipt.catalog.clone()),
                    )
                    .await?;
                    (
                        receipt.location,
                        receipt.catalog,
                        Ok(receipt.deck),
                        Some(target),
                        Some(first),
                    )
                }
                Request::Location(path) => {
                    let snapshot = owner
                        .switch_location_home(path)
                        .await
                        .map_err(switch_error)?;
                    if snapshot.catalog.chrome.location.as_deref()
                        != Some(snapshot.location.as_str())
                    {
                        return Err("Location switch returned a mismatched owner catalog".into());
                    }
                    (
                        snapshot.location,
                        snapshot.catalog,
                        owner.tab_deck().await,
                        None,
                        None,
                    )
                }
                Request::Moved(snapshot) => {
                    if owner.shares_workspace() {
                        owner
                            .switch_location_home(snapshot.location.clone())
                            .await
                            .map_err(switch_error)?;
                    }
                    if owner
                        .catalog()
                        .await
                        .map_err(|e| e.to_string())?
                        .chrome
                        .location
                        .as_deref()
                        != Some(snapshot.location.as_str())
                    {
                        return Err(
                            "Session moved; visible Location changed before route adoption".into(),
                        );
                    }
                    (
                        snapshot.location,
                        snapshot.catalog,
                        owner.tab_deck().await,
                        None,
                        None,
                    )
                }
                #[cfg(test)]
                Request::Published { catalog, location } => {
                    (location, *catalog, owner.tab_deck().await, None, None)
                }
            };
            let cross = old.as_deref() != Some(location.as_str());
            let mut views = first.into_iter().collect::<Vec<_>>();
            if (cross || selected.is_none())
                && let Ok(saved) = &saved
            {
                for id in &saved.sessions {
                    if views.iter().any(|item| &item.id == id) {
                        continue;
                    }
                    if let Ok(prepared) = view(&owner, id.clone(), None, None).await {
                        views.push(prepared);
                    }
                }
            }
            Ok(Prepared {
                location,
                catalog,
                deck: saved,
                views,
                selected,
                cross,
            })
        })),
    });
    Ok(())
}
fn remember_drafts(state: &TuiState, deck: &mut LoopState) {
    let state = deck.child_parent.as_deref().unwrap_or(state);
    if let Some(location) = &deck.location {
        let mut drafts = vec![(state.attached_session().cloned(), state.composer_draft())];
        drafts.extend(
            deck.tabs
                .iter()
                .flatten()
                .map(|view| (view.attached_session().cloned(), view.composer_draft())),
        );
        if let Some(home) = &deck.home {
            drafts.push((None, home.composer_draft()));
        }
        if deck.picker_drafts.len() >= 4
            && let Some(key) = deck.picker_drafts.keys().next().cloned()
        {
            deck.picker_drafts.remove(&key);
        }
        deck.picker_drafts.insert(location.clone(), drafts);
    }
}
async fn build(app: &CoreApp, prepared: View) -> TuiState {
    let mut view = TuiState::new(app.clone(), prepared.id);
    view.attach_page(&prepared.page);
    view.apply_catalog(prepared.catalog);
    view.apply_compaction_history(prepared.compactions);
    if let Some(mcp) = prepared.mcp {
        view.apply_mcp_snapshot(mcp);
    }
    view.apply_shell_jobs(prepared.shells);
    if prepared.refresh_failed {
        view.push_note("Session opened; resource refresh pending");
    }
    shared::apply_attached(app, &mut view, prepared.attached).await;
    view
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    if deck.navigation_job.is_none()
        && let Some(moved) = deck.moved_pending.take()
    {
        let root = deck.child_parent.as_deref().unwrap_or(state);
        if moved.identity == root.view_identity().0
            && root.tab_route_session() == Some(&moved.session)
            && deck.location == moved.location
            && deck.navigation_revision == moved.navigation
        {
            if deck.departure_ready(state).is_ok()
                && deck.deck_job.is_none()
                && deck.deck_read.is_none()
                && scope_ready(state, deck)
                && root.attached_session().is_some()
            {
                let _ = start(app, state, deck, Request::Moved(moved.snapshot));
            } else {
                deck.moved_pending = Some(moved);
            }
        } else {
            deck.shared_refresh |= app.shares_workspace();
        }
    }
    if !deck
        .navigation_job
        .as_ref()
        .is_some_and(|job| job.ready() || job.prepared.is_some())
    {
        return false;
    }
    let mut job = deck.navigation_job.take().expect("finished navigation");
    if let Some(task) = job.task.take() {
        job.prepared = Some(task.await.unwrap_or_else(|_| {
            Err("session navigation outcome unknown; refresh before retrying".into())
        }));
    }
    if job.prepared.as_ref().is_some_and(Result::is_ok)
        && read_jobs::views(state, deck).any(|view| deck.owns_receipt(view))
    {
        deck.navigation_job = Some(job);
        return false;
    }
    // The foreign open is already committed. A newer local Root/New choice
    // wins presentation: restore that known owner context, not the old effect.
    if !job.returning
        && deck.navigation_revision != job.navigation
        && job
            .prepared
            .as_ref()
            .is_some_and(|result| result.as_ref().is_ok_and(|prepared| prepared.cross))
        && let Some(original) = job.location.clone()
    {
        for view in read_jobs::views(state, deck) {
            view.resume_transcript_projection();
        }
        let owner = app.clone();
        job.returning = true;
        job.prepared = None;
        job.task = Some(oc_core::wake::spawn(async move {
            let snapshot = owner
                .switch_location_home(original.clone())
                .await
                .map_err(switch_error)?;
            if snapshot.location != original
                || snapshot.catalog.chrome.location.as_deref() != Some(original.as_str())
            {
                return Err("retained Location restore returned a mismatched binding".into());
            }
            Ok(Prepared {
                location: original,
                catalog: snapshot.catalog,
                deck: owner.tab_deck().await,
                views: Vec::new(),
                selected: None,
                cross: false,
            })
        }));
        deck.navigation_job = Some(job);
        return true;
    }
    if !job.returning
        && deck.location == job.location
        && job.prepared.as_ref().is_some_and(|result| {
            result.as_ref().is_ok_and(|prepared| {
                prepared.cross || prepared.selected.is_none() || prepared.deck.is_err()
            })
        })
    {
        let mut pending = false;
        for view in read_jobs::views(state, deck) {
            view.cancel_transcript_projection();
            view.poll_transcript_projection();
            pending |= view.has_pending_transcript_projection();
        }
        if pending {
            deck.navigation_job = Some(job);
            return false;
        }
    }
    for view in read_jobs::views(state, deck) {
        view.resume_transcript_projection();
    }
    let Job {
        location,
        navigation,
        target: target_route,
        origin,
        pointer,
        returning,
        command,
        explicit_location,
        prepared,
        ..
    } = job;
    let prepared = match prepared.expect("prepared navigation") {
        Ok(prepared) => prepared,
        Err(error) => {
            if let Some(view) =
                read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == origin)
            {
                view.push_note(&error);
            }
            deck.shared_refresh |= app.shares_workspace();
            deck.scope_uncertain |= returning;
            return true;
        }
    };
    if deck.location != location {
        deck.shared_refresh |= app.shares_workspace();
        return true;
    }
    let Prepared {
        location,
        catalog,
        deck: saved,
        views,
        selected,
        cross,
    } = prepared;
    let navigation_changed = deck.navigation_revision != navigation;
    let local = location.is_empty() && deck.location.is_none();
    let saved = match saved {
        Ok(saved)
            if saved.location == location
                && saved.sessions.len() <= MAX_TABS
                && (saved.active.is_some() || saved.sessions.len() < MAX_TABS)
                && (saved.new_session_titles.is_empty()
                    || saved.new_session_titles.len() == saved.sessions.len())
                && saved.sessions.iter().all(|id| valid_tab_id(&id.0))
                && saved
                    .sessions
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == saved.sessions.len()
                && saved
                    .active
                    .as_ref()
                    .is_none_or(|id| saved.sessions.contains(id))
                && saved.rail_width.is_none_or(|width| {
                    (oc_core::queries::TAB_RAIL_MIN_WIDTH..=oc_core::queries::TAB_RAIL_MAX_WIDTH)
                        .contains(&width)
                }) =>
        {
            saved
        }
        _ => {
            if returning {
                deck.scope_uncertain = true;
                state.push_note("Retained Location verification failed; switch Location explicitly before sending");
                return true;
            }
            remember_drafts(state, deck);
            deck.reset_deck();
            deck.location = Some(location);
            deck.revision = None;
            deck.save_disabled = true;
            deck.read_only = false;
            *state = TuiState::new_home(app.clone());
            state.apply_catalog(catalog);
            state.push_note("Location tabs unavailable; check saved tabs");
            deck.sync_tabs(state);
            return true;
        }
    };
    if !navigation_changed
        && let Some((revision, text)) = &command
        && let Some(view) =
            read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == origin)
        && view.input_revision() == *revision
        && view.input() == text
    {
        view.accept_intent();
    }
    if returning {
        deck.revision = saved.revision.clone();
        deck.authoritative_deck = Some(saved.clone());
        deck.save_disabled = saved.projected();
        deck.selection_blocked = false;
        deck.scope_uncertain = false;
        deck.selection_dirty = true;
        deck.shared_refresh |= app.shares_workspace();
        refresh_provider_views(app, state, deck).await;
        return true;
    }
    let unreadable = (cross || selected.is_none())
        && saved
            .sessions
            .iter()
            .any(|id| !views.iter().any(|view| &view.id == id));
    if cross || selected.is_none() {
        remember_drafts(state, deck);
        let drafts = deck.picker_drafts.remove(&location).unwrap_or_default();
        deck.reset_deck();
        deck.read_only = false;
        let mut home = TuiState::new_home(app.clone());
        home.apply_catalog(catalog.clone());
        if let Some((_, draft)) = drafts.iter().find(|(id, _)| id.is_none()) {
            home.restore_composer_draft(draft.clone());
        }
        *state = home;
        let active = (!navigation_changed)
            .then_some(selected.as_ref().or(saved.active.as_ref()))
            .flatten();
        let mut prepared = views;
        for (index, id) in saved.sessions.iter().enumerate() {
            let Some(position) = prepared.iter().position(|view| &view.id == id) else {
                // A failed Session catalog is not a usable tab with Home's
                // binding. Keep the complete owner deck, but omit this view.
                continue;
            };
            let mut view = build(app, prepared.remove(position)).await;
            view.new_session_tab = saved
                .new_session_titles
                .get(index)
                .copied()
                .unwrap_or(false);
            if let Some((_, draft)) = drafts
                .iter()
                .find(|(session, _)| session.as_ref() == Some(id))
            {
                view.restore_composer_draft(draft.clone());
            } else if let Some(draft) = deck.picker_drafts.values_mut().find_map(|drafts| {
                let index = drafts
                    .iter()
                    .position(|(session, _)| session.as_ref() == Some(id))?;
                Some(drafts.remove(index).1)
            }) {
                // Session-bound editors follow a moved global SessionId. Home
                // editors remain strictly Location-local.
                view.restore_composer_draft(draft);
            }
            if active == Some(id) {
                deck.home = Some(std::mem::replace(state, view));
                deck.active_tab = Some(deck.tabs.len());
                deck.tabs.push(None);
            } else {
                deck.tabs.push(Some(view));
            }
            deck.tab_cards_before.push(None);
        }
    } else if let Some(target) = selected {
        let mut views = views;
        if let Some(position) = views.iter().position(|view| view.id == target) {
            let prepared = views.remove(position);
            if let Some(index) = deck_jobs::route_index(state, deck, &target) {
                let metadata_current = read_jobs::views_mut(state, deck).any(|view| {
                    target_route
                        .as_ref()
                        .is_some_and(|route| route.matches(view))
                });
                // Healthy retained history/drafts are authoritative locally.
                if deck.navigation_revision == navigation {
                    state.close_panel();
                    let _ = deck.activate(state, index);
                }
                let mut jobs = std::mem::take(&mut deck.reads);
                if let Some(view) = read_jobs::views_mut(state, deck)
                    .find(|view| view.attached_session() == Some(&target))
                {
                    if !navigation_changed {
                        view.close_panel();
                    }
                    if metadata_current && let Some(title) = prepared.page.title {
                        view.session_title = Some(title);
                    }
                    jobs.request(view, false);
                }
                deck.reads = jobs;
            } else {
                let next = build(app, prepared).await;
                if deck.navigation_revision == navigation {
                    state.close_panel();
                    if let Some(old) = deck.active_tab.take() {
                        deck.tabs[old] = Some(std::mem::replace(state, next));
                        deck.tab_cards_before[old] = deck.cards_before;
                    } else {
                        deck.home = Some(std::mem::replace(state, next));
                    }
                    deck.active_tab = Some(deck.tabs.len());
                    deck.tabs.push(None);
                } else {
                    deck.tabs.push(Some(next));
                }
                deck.tab_cards_before.push(None);
            }
        }
    }
    deck.location = (!local).then_some(location);
    deck.scope_uncertain = false;
    deck.revision = saved.revision.clone();
    deck.authoritative_deck = Some(saved.clone());
    deck.save_disabled = saved.projected() || unreadable;
    let _ = state.apply_tab_rail_width(saved.rail_width);
    deck.selection_dirty = navigation_changed || deck.snapshot(state).active != saved.active;
    deck.controls.request();
    deck.sync_tabs(state);
    if explicit_location && !navigation_changed {
        if deck.save_disabled {
            state.push_note("Location tabs unavailable; check saved tabs");
        } else {
            state.push_note(&format!(
                "location: {}",
                deck.location.as_deref().unwrap_or_default()
            ));
        }
    }
    if !cross
        && !navigation_changed
        && let Some((revision, pointer)) = pointer
        && revision == deck.pointer_revision
    {
        state.restore_mouse_hover(pointer);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unreadable_location_tab_never_borrows_home_binding_or_rewrites_saved_membership() {
        let (app, mut inbox, _, _, _admissions) = CoreApp::native_channel(8, 77);
        let mut state = TuiState::new_home(app.clone());
        state.chrome.location = Some("/A".into());
        let saved = TabDeckSnapshot {
            location: "/B".into(),
            revision: Some("original-unreadable".into()),
            sessions: vec![SessionId("unreadable".into())],
            active: Some(SessionId("unreadable".into())),
            ..Default::default()
        };
        let mut catalog = CatalogSnapshot {
            chrome: Default::default(),
            auto_accept: Default::default(),
            provider: "fixture".into(),
            models: Vec::new(),
            model_id: String::new(),
            variant: None,
            agents: Vec::new(),
            agent_id: None,
            commands: Vec::new(),
            command_descriptions: Default::default(),
        };
        catalog.chrome.location = Some("/B".into());
        let mut deck = LoopState {
            location: Some("/A".into()),
            ..Default::default()
        };
        deck.navigation_job = Some(Job {
            location: deck.location.clone(),
            navigation: deck.navigation_revision,
            target: None,
            origin: state.view_identity().0,
            command: None,
            explicit_location: true,
            returning: false,
            pointer: None,
            task: None,
            prepared: Some(Ok(Prepared {
                location: "/B".into(),
                catalog,
                deck: Ok(saved.clone()),
                views: Vec::new(),
                selected: None,
                cross: true,
            })),
        });
        assert!(sync(&app, &mut state, &mut deck).await);
        assert!(deck.navigation_job.is_none());
        assert_eq!(deck.location.as_deref(), Some("/B"));
        assert!(state.attached_session().is_none());
        assert!(deck.tabs.is_empty(), "no guessed Session catalog");
        assert!(deck.save_disabled);
        assert_eq!(deck.authoritative_deck.as_ref(), Some(&saved));
        assert_eq!(
            state.note(),
            Some("Location tabs unavailable; check saved tabs")
        );
        deck_jobs::sync(&app, &mut state, &mut deck).await;
        assert!(
            inbox.try_recv().is_err(),
            "no pruned deck write or admission"
        );
    }
}
