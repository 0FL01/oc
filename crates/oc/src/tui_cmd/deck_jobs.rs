//! One serialized CAS lane. Navigation is local; structural changes are not.
use super::*;

pub(super) enum Effect {
    Selection,
    Adopt,
    Move {
        session: SessionId,
        order: Vec<SessionId>,
        navigation: u64,
    },
    Close {
        session: SessionId,
    },
    Delete {
        session: SessionId,
    },
    Width(u16),
    CloseHome {
        identity: (u64, u64),
    },
}

pub(super) struct Job {
    requested: TabDeckSnapshot,
    effect: Effect,
    staged: Option<Structural>,
    close_pointer: Option<(u64, oc_tui::app::TabCloseSnapshot)>,
    job: tokio::task::JoinHandle<Outcome>,
}

enum Structural {
    Delete(Deletion),
    Write {
        base: TabDeckSnapshot,
        requested: TabDeckSnapshot,
        effect: Effect,
    },
    Close {
        before: TabDeckSnapshot,
        requested: TabDeckSnapshot,
        session: SessionId,
        home: bool,
        title: bool,
    },
}
struct Deletion {
    before: TabDeckSnapshot,
    requested: TabDeckSnapshot,
    session: SessionId,
    home: bool,
    search: String,
    all: bool,
}

struct Outcome {
    result: Result<TabDeckSnapshot, CoreError>,
    preflight: Option<TabDeckSnapshot>,
    home: Option<CatalogSnapshot>,
}

async fn save(app: &CoreApp, requested: TabDeckSnapshot) -> Result<TabDeckSnapshot, CoreError> {
    let saved = app.save_tab_deck(requested.clone()).await?;
    if saved.location != requested.location
        || saved.sessions != requested.sessions
        || saved.new_session_titles != requested.new_session_titles
        || saved.active != requested.active
        || saved.rail_width != requested.rail_width
        || saved.revision.is_none()
    {
        return Err(CoreError::TabDeckStorage);
    }
    Ok(saved)
}

impl Job {
    pub(super) fn retain_close_pointer(
        &mut self,
        navigation: u64,
        snapshot: oc_tui::app::TabCloseSnapshot,
    ) {
        self.close_pointer = Some((navigation, snapshot));
    }
    pub(super) fn ready(&self) -> bool {
        self.job.is_finished()
    }

    /// A pointer release following a drag belongs to that structural command's
    /// destination geometry, even when both arrive in the same input batch.
    /// Resolve it to a stable ID; the displayed order still waits for CAS.
    pub(super) fn pointer_target(&self, index: usize) -> Option<&SessionId> {
        let effect = match &self.staged {
            Some(Structural::Write { effect, .. }) => effect,
            _ => &self.effect,
        };
        match effect {
            Effect::Move { order, .. } => order.get(index),
            _ => None,
        }
    }

    pub(super) fn closing(&self, view: &TuiState) -> bool {
        let matches = |effect: &Effect| match effect {
            Effect::Close { session } | Effect::Delete { session } => {
                tab_route_session(view) == Some(session)
            }
            Effect::CloseHome { identity } => view.view_identity().0 == identity.0,
            _ => false,
        };
        matches(&self.effect)
            || self.staged.as_ref().is_some_and(|staged| match staged {
                Structural::Write { effect, .. } => matches(effect),
                Structural::Close { session, .. } => tab_route_session(view) == Some(session),
                Structural::Delete(delete) => tab_route_session(view) == Some(&delete.session),
            })
    }

    fn stage(&mut self, staged: Structural) -> Result<(), String> {
        let base = match &staged {
            Structural::Write { base, .. } => base,
            Structural::Close { before, .. } => before,
            Structural::Delete(delete) => &delete.before,
        };
        if !matches!(self.effect, Effect::Selection) || self.staged.is_some() {
            return Err("structural tab action pending; original request retained".into());
        }
        if base.location != self.requested.location
            || base.revision != self.requested.revision
            || base.sessions != self.requested.sessions
            || base.new_session_titles != self.requested.new_session_titles
        {
            return Err("tab membership changed; refresh before structural action".into());
        }
        self.staged = Some(staged);
        Ok(())
    }
}

pub(super) fn start(
    app: &CoreApp,
    deck: &mut LoopState,
    requested: TabDeckSnapshot,
    effect: Effect,
) -> Result<(), String> {
    if deck.deck_read.is_some() {
        return Err(
            "tab deck refresh pending; retry structural action after acknowledgement".into(),
        );
    }
    let base = deck
        .authoritative_deck
        .clone()
        .unwrap_or_else(|| requested.clone());
    if let Some(job) = deck.deck_job.as_mut() {
        return job.stage(Structural::Write {
            base,
            requested,
            effect,
        });
    }
    if deck.save_disabled || requested.location.is_empty() && deck.location.is_some() {
        return Err("tab deck unavailable; review saved tabs".into());
    }
    let owner = app.clone();
    let captured = requested.clone();
    let local = deck.location.is_none();
    deck.deck_job = Some(Job {
        requested,
        effect,
        staged: None,
        close_pointer: None,
        job: tokio::spawn(async move {
            Outcome {
                result: if local {
                    Ok(captured)
                } else {
                    save(&owner, captured).await
                },
                preflight: None,
                home: None,
            }
        }),
    });
    Ok(())
}

/// Retire a newly accepted root marker before the removal write, just as the
/// former synchronous close did. Both CAS operations occupy this same lane.
pub(super) fn start_close(
    app: &CoreApp,
    deck: &mut LoopState,
    before: TabDeckSnapshot,
    requested: TabDeckSnapshot,
    session: SessionId,
) -> Result<(), String> {
    if deck.deck_read.is_some() {
        return Err("tab deck refresh pending; retry close after acknowledgement".into());
    }
    if deck.save_disabled {
        return Err("tab close refused; saved tabs unavailable".into());
    }
    if deck.location.as_deref() == Some("") {
        return Err(CoreError::InvalidTabDeck.to_string());
    }
    let home = before.sessions.len() == 1 && deck.home.is_none() && deck.active_tab.is_some();
    let title = deck
        .title_job
        .as_ref()
        .is_some_and(|(target, _)| target == &session);
    if let Some(job) = deck.deck_job.as_mut() {
        return job.stage(Structural::Close {
            before,
            requested,
            session,
            home,
            title,
        });
    }
    launch_close(app, deck, before, requested, session, home, title);
    Ok(())
}

pub(super) fn start_delete(
    app: &CoreApp,
    state: &TuiState,
    deck: &mut LoopState,
    session: SessionId,
) -> Result<(), String> {
    if deck.deck_read.is_some() {
        return Err("tab deck refresh pending; retry delete after acknowledgement".into());
    }
    if deck.save_disabled {
        return Err("tab delete refused; saved tabs unavailable".into());
    }
    let before = deck.structural_snapshot(state);
    let mut requested = before.clone();
    if let Some(index) = requested.sessions.iter().position(|id| id == &session) {
        requested.sessions.remove(index);
        if !requested.new_session_titles.is_empty() {
            requested.new_session_titles.remove(index);
        }
        if requested.active.as_ref() == Some(&session) {
            requested.active = requested.sessions.get(index.saturating_sub(1)).cloned();
        }
    }
    let home = requested.sessions.is_empty() && deck.home.is_none();
    let search = state.session_search();
    let all = state.sessions_all_projects();
    let delete = Deletion {
        before,
        requested,
        session,
        home,
        search,
        all,
    };
    if let Some(job) = deck.deck_job.as_mut() {
        return job.stage(Structural::Delete(delete));
    }
    launch_delete(app, deck, delete);
    Ok(())
}

fn launch_delete(app: &CoreApp, deck: &mut LoopState, delete: Deletion) {
    let Deletion {
        before,
        requested,
        session,
        home,
        search,
        all,
    } = delete;
    let owner = app.clone();
    let local = deck.location.is_none();
    deck.deck_job = Some(Job {
        requested,
        effect: Effect::Delete {
            session: session.clone(),
        },
        staged: None,
        close_pointer: None,
        job: tokio::spawn(async move {
            let home = if home {
                match owner.home_selection(SelectionAction::Current).await {
                    Ok(catalog) => Some(catalog),
                    Err(error) => {
                        return Outcome {
                            result: Err(error),
                            preflight: None,
                            home: None,
                        };
                    }
                }
            } else {
                None
            };
            let preflight = if local {
                None
            } else {
                match save(&owner, before).await {
                    Ok(saved) => Some(saved),
                    Err(error) => {
                        return Outcome {
                            result: Err(error),
                            preflight: None,
                            home,
                        };
                    }
                }
            };
            let result = match owner
                .picker_session_action(
                    session,
                    search,
                    all,
                    oc_core::queries::SessionPickerAction::Delete,
                )
                .await
            {
                Ok(oc_core::queries::SessionPickerResult::Deleted(snapshot)) => Ok(snapshot),
                Ok(_) => Err(CoreError::TabDeckStorage),
                Err(error) => Err(error),
            };
            Outcome {
                result,
                preflight,
                home,
            }
        }),
    });
}

fn launch_close(
    app: &CoreApp,
    deck: &mut LoopState,
    before: TabDeckSnapshot,
    requested: TabDeckSnapshot,
    session: SessionId,
    home: bool,
    title: bool,
) {
    let owner = app.clone();
    let captured = requested.clone();
    let local = deck.location.is_none();
    deck.deck_job = Some(Job {
        requested,
        effect: Effect::Close {
            session: session.clone(),
        },
        staged: None,
        close_pointer: None,
        job: tokio::spawn(async move {
            let home = if home {
                match owner.home_selection(SelectionAction::Current).await {
                    Ok(catalog) => Some(catalog),
                    Err(error) => {
                        return Outcome {
                            result: Err(error),
                            preflight: None,
                            home: None,
                        };
                    }
                }
            } else {
                None
            };
            let preflight = if local {
                None
            } else {
                match save(&owner, before).await {
                    Ok(saved) => Some(saved),
                    Err(error) => {
                        return Outcome {
                            result: Err(error),
                            preflight: None,
                            home,
                        };
                    }
                }
            };
            let mut requested = captured;
            if let Some(saved) = &preflight {
                requested.revision = saved.revision.clone();
            }
            let result = if local {
                Ok(requested)
            } else {
                save(&owner, requested).await
            };
            if result.is_ok() && title {
                let _ = owner.cancel_title(session).await;
            }
            Outcome {
                result,
                preflight,
                home,
            }
        }),
    });
}

pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut changed = false;
    if let Some(job) = deck.deck_job.as_ref().filter(|job| job.ready()) {
        let mut pending = false;
        for view in read_jobs::views(state, deck).filter(|view| job.closing(view)) {
            view.cancel_transcript_projection();
            view.poll_transcript_projection();
            pending |= view.has_pending_transcript_projection();
        }
        if pending {
            return false;
        }
    }
    if deck.deck_job.as_ref().is_some_and(Job::ready) && !state.tab_gesture_active() {
        let Job {
            requested,
            effect,
            staged,
            mut close_pointer,
            job,
        } = deck.deck_job.take().expect("finished deck CAS");
        let Outcome {
            result,
            preflight,
            home,
        } = job.await.unwrap_or(Outcome {
            result: Err(CoreError::Shutdown),
            preflight: None,
            home: None,
        });
        if deck.location.as_deref().unwrap_or_default() != requested.location
            || deck.revision != requested.revision
        {
            deck.shared_refresh |= app.shares_workspace();
            deck.selection_blocked = true;
            state.push_note(
                "tab save completed after its binding changed; refresh saved tabs before retrying",
            );
            return true;
        }
        if let Some(saved) = preflight {
            deck.revision = saved.revision.clone();
            deck.selection_blocked = false;
            deck.authoritative_deck = Some(saved);
        }
        match result {
            Ok(saved)
                if saved.location == requested.location
                    && saved.sessions == requested.sessions
                    && (saved.active == requested.active
                        || matches!(effect, Effect::Delete { .. }))
                    && saved.rail_width == requested.rail_width
                    && (deck.location.is_none() || saved.revision.is_some())
                    && deck.location.as_deref().unwrap_or_default() == requested.location =>
            {
                deck.revision = saved.revision;
                deck.selection_blocked = false;
                deck.authoritative_deck = Some(TabDeckSnapshot {
                    revision: deck.revision.clone(),
                    ..saved
                });
                let deleting = matches!(effect, Effect::Delete { .. });
                let pointer_current = close_pointer
                    .as_ref()
                    .is_some_and(|(navigation, _)| *navigation == deck.navigation_revision);
                match effect {
                    Effect::Selection | Effect::Adopt => {}
                    Effect::Width(width) => {
                        let _ = state.apply_tab_rail_width(Some(width));
                    }
                    Effect::Move {
                        session,
                        order,
                        navigation,
                    } => {
                        let focused =
                            tab_route_session(deck.child_parent.as_deref().unwrap_or(state))
                                .cloned();
                        // Stable IDs, never the indices captured before a later
                        // activation. Provisional owners stay owned throughout.
                        for (index, id) in order.iter().enumerate() {
                            if let Some(from) = route_index(state, deck, id) {
                                let tab = deck.tabs.remove(from);
                                deck.tabs.insert(index, tab);
                                let cursor = deck.tab_cards_before.remove(from);
                                deck.tab_cards_before.insert(index, cursor);
                                if let Some(active) = deck.active_tab {
                                    deck.active_tab = Some(if active == from {
                                        index
                                    } else if from < active && active <= index {
                                        active - 1
                                    } else if index <= active && active < from {
                                        active + 1
                                    } else {
                                        active
                                    });
                                }
                            }
                        }
                        if deck.navigation_revision == navigation {
                            if let Some(index) = route_index(state, deck, &session) {
                                let _ = deck.activate(state, index);
                            }
                        } else if let Some(focused) = focused {
                            debug_assert_eq!(
                                tab_route_session(deck.child_parent.as_deref().unwrap_or(state)),
                                Some(&focused)
                            );
                        }
                    }
                    Effect::Close { session } | Effect::Delete { session } => {
                        if let Some(index) = route_index(state, deck, &session) {
                            if deck.target_ready(state, index).is_ok() {
                                close_local_prepared(app, state, deck, index, home);
                            } else {
                                deck.closed_owners.insert(session.clone(), home);
                                state.push_note("tab closed in saved deck; retaining its pending receipt until acknowledgement");
                            }
                        }
                        if deleting && let Some((target, job)) = deck.title_job.take() {
                            if target == session {
                                job.abort();
                            } else {
                                deck.title_job = Some((target, job));
                            }
                        }
                        if deleting
                            && let Err(error) =
                                deck.picker.request(app, state, picker_jobs::Request::List)
                        {
                            state.push_note(&error);
                        }
                        deck.shared_refresh |= app.shares_workspace();
                    }
                    Effect::CloseHome { identity } => {
                        let pointer = state.mouse_position();
                        if state.view_identity() == identity
                            && deck.active_tab.is_none()
                            && !deck.tabs.is_empty()
                        {
                            let _ = deck.activate(state, deck.tabs.len() - 1);
                        }
                        if deck
                            .home
                            .as_ref()
                            .is_some_and(|home| home.view_identity() == identity)
                        {
                            deck.home = None;
                        }
                        deck.sync_tabs(state);
                        if let Some(pointer) = pointer {
                            state.restore_mouse_hover(pointer);
                        }
                    }
                }
                let visible = deck.snapshot(state);
                if visible.active == requested.active && visible.rail_width == requested.rail_width
                {
                    deck.selection_dirty = false;
                }
                changed = true;
                if let Some(staged) = staged {
                    match staged {
                        Structural::Delete(mut delete) => {
                            delete.before.revision = deck.revision.clone();
                            delete.requested.revision = deck.revision.clone();
                            launch_delete(app, deck, delete);
                        }
                        Structural::Write {
                            mut requested,
                            effect,
                            ..
                        } => {
                            requested.revision = deck.revision.clone();
                            if let Err(error) = start(app, deck, requested, effect) {
                                state.push_note(&error);
                            }
                        }
                        Structural::Close {
                            mut before,
                            mut requested,
                            session,
                            home,
                            title,
                        } => {
                            before.revision = deck.revision.clone();
                            requested.revision = deck.revision.clone();
                            launch_close(app, deck, before, requested, session, home, title);
                        }
                    }
                    if let Some(job) = deck.deck_job.as_mut() {
                        job.close_pointer = close_pointer.take();
                    }
                } else if pointer_current && let Some((_, snapshot)) = close_pointer {
                    deck.sync_tabs(state);
                    state.restore_mouse_close(snapshot);
                }
            }
            _ => {
                for view in read_jobs::views(state, deck) {
                    view.resume_transcript_projection();
                }
                // Do not retry an unknown write, or adopt an unrelated token.
                // Retain the latest explicit local choice, but do not reuse an
                // unknown/conflicting token. An authoritative read unblocks it.
                deck.selection_dirty = matches!(effect, Effect::Selection) || deck.selection_dirty;
                deck.selection_blocked = true;
                deck.shared_refresh |= app.shares_workspace();
                state.push_note(
                    "tab deck could not be saved; refused or outcome unknown; refresh saved tabs before retrying",
                );
                return true;
            }
        }
    }
    if deck
        .deck_read
        .as_ref()
        .is_some_and(|(_, job)| job.is_finished())
    {
        let (location, job) = deck
            .deck_read
            .take()
            .expect("finished authoritative deck read");
        if let Ok(Ok(saved)) = job.await
            && deck.location.as_deref() == Some(location.as_str())
            && saved.location == location
            && saved.revision.is_some()
            && saved.sessions.len() <= MAX_TABS
            && saved
                .active
                .as_ref()
                .is_none_or(|id| saved.sessions.contains(id))
        {
            deck.revision = saved.revision.clone();
            deck.authoritative_deck = Some(saved);
            deck.selection_blocked = false;
            changed = true;
        }
    }
    if !app.shares_workspace()
        && deck.selection_blocked
        && deck.selection_dirty
        && deck.deck_read.is_none()
        && deck.deck_job.is_none()
        && deck.deck_read_revision != Some(deck.navigation_revision)
        && let Some(location) = deck.location.clone()
    {
        let owner = app.clone();
        deck.deck_read_revision = Some(deck.navigation_revision);
        deck.deck_read = Some((
            location,
            tokio::spawn(async move { owner.tab_deck().await }),
        ));
    }
    let closed: Vec<_> = deck.closed_owners.keys().cloned().collect();
    for session in closed {
        if let Some(index) = route_index(state, deck, &session)
            && deck.target_ready(state, index).is_ok()
            && projection_drained(state, deck, index)
        {
            let home = deck.closed_owners.remove(&session).flatten();
            close_local_prepared(app, state, deck, index, home);
            changed = true;
        }
    }
    if deck.deck_job.is_none()
        && deck.selection_dirty
        && !deck.selection_blocked
        && !deck.save_disabled
        && deck.navigation_job.is_none()
        && deck.moved_pending.is_none()
        && !tab_route_session(state).is_some_and(|id| deck.closed_owners.contains_key(id))
        && !(app.shares_workspace() && (deck.shared_refresh || deck.workspace_job.is_some()))
    {
        deck.selection_dirty = false;
        // Fresh native adoption can publish membership before its submit ACK.
        // Selection owns only focus/geometry, never membership. In particular,
        // do not filter an already-authoritative candidate out because its
        // retained view deliberately remains unattached until its own ACK.
        let visible = deck.snapshot(state);
        let mut requested = if app.shares_workspace() {
            deck.authoritative_deck
                .clone()
                .unwrap_or_else(|| visible.clone())
        } else {
            visible.clone()
        };
        requested.revision = deck.revision.clone();
        requested.active = visible.active.filter(|id| requested.sessions.contains(id));
        requested.rail_width = visible.rail_width;
        if deck.authoritative_deck.as_ref() != Some(&requested)
            && let Err(error) = start(app, deck, requested, Effect::Selection)
        {
            state.push_note(&error);
        }
    }
    changed
}

pub(super) fn route_index(state: &TuiState, deck: &LoopState, id: &SessionId) -> Option<usize> {
    deck.tabs.iter().enumerate().position(|(index, parked)| {
        let view = if deck.active_tab == Some(index) {
            deck.child_parent.as_deref().unwrap_or(state)
        } else {
            parked.as_ref().expect("parked tab")
        };
        tab_route_session(view) == Some(id)
    })
}

pub(super) fn close_local(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState, index: usize) {
    close_local_prepared(app, state, deck, index, None);
}

fn close_local_prepared(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    index: usize,
    catalog: Option<CatalogSnapshot>,
) {
    if !projection_drained(state, deck, index) {
        let view = if deck.active_tab == Some(index) {
            deck.child_parent.as_deref().unwrap_or(state)
        } else {
            deck.tabs[index].as_ref().expect("closing view")
        };
        if let Some(session) = tab_route_session(view) {
            deck.closed_owners.insert(session.clone(), catalog);
        }
        return;
    }
    let pointer = state.mouse_position();
    if deck.active_tab == Some(index) {
        if deck.tabs.len() > 1 {
            let survivor = if index > 0 { index - 1 } else { 1 };
            let _ = deck.activate(state, survivor);
        } else if deck.home.is_some() {
            deck.restore_home(state);
        } else {
            let mut home = TuiState::new_home(app.clone());
            home.chrome = state.chrome.clone();
            if let Some(catalog) = catalog {
                home.apply_catalog(catalog);
            } else {
                deck.home_pending = Some((home.view_identity(), SelectionAction::Current));
                deck.unresolved_homes.insert(home.view_identity().0);
            }
            home.take_tab_clocks_from(state);
            *state = home;
            deck.active_tab = None;
        }
    }
    deck.tabs.remove(index);
    deck.tab_cards_before.remove(index);
    if let Some(active) = deck.active_tab.as_mut()
        && *active > index
    {
        *active -= 1;
    }
    deck.navigation_revision = deck.navigation_revision.wrapping_add(1);
    deck.selection_dirty = true;
    deck.sync_tabs(state);
    deck.shared_refresh |= app.shares_workspace() && !deck.picker_pending_tabs.is_empty();
    if let Some(pointer) = pointer {
        state.restore_mouse_hover(pointer);
    }
}

pub(super) fn projection_drained(state: &TuiState, deck: &LoopState, index: usize) -> bool {
    let view = if deck.active_tab == Some(index) {
        deck.child_parent.as_deref().unwrap_or(state)
    } else {
        deck.tabs[index].as_ref().expect("closing view")
    };
    view.cancel_transcript_projection();
    view.poll_transcript_projection();
    !view.has_pending_transcript_projection()
}
