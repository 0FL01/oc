//! Bounded picker reads and exact title writes; editors remain local.
use super::*;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) enum Request {
    List,
    Rename {
        target: SessionId,
        title: String,
        direct: Option<bool>,
    },
    Delete {
        target: SessionId,
    },
}
struct Read {
    identity: u64,
    location: Option<String>,
    generation: u64,
    search: String,
    all: bool,
    rerun: bool,
    open: Option<Open>,
    request: Request,
    task: tokio::task::JoinHandle<Result<Data, CoreError>>,
}
struct Open {
    identity: (u64, u64),
    navigation: u64,
    interaction: u64,
    search: String,
    all: bool,
    cursor: usize,
    action: KeyAction,
}
impl Open {
    fn matches(&self, view: &TuiState, navigation: u64, interaction: u64) -> bool {
        self.identity == view.view_identity()
            && self.navigation == navigation
            && self.interaction == interaction
            && self.search == view.session_search()
            && self.all == view.sessions_all_projects()
            && self.cursor == view.sessions_cursor()
            && *view.panel() == TuiPanel::Sessions
    }
}
enum Data {
    List(
        oc_core::queries::SessionPickerContext,
        Vec<oc_core::queries::SessionListEntry>,
    ),
    Renamed,
    Deleted,
}
#[derive(Default)]
pub(super) struct Jobs {
    reads: HashMap<u64, Read>,
    titles: HashMap<SessionId, tokio::task::JoinHandle<Result<(), CoreError>>>,
}
impl Jobs {
    pub(super) fn defer_open(
        &mut self,
        view: &TuiState,
        navigation: u64,
        interaction: u64,
        pointer: bool,
    ) -> Result<bool, String> {
        self.defer_action(view, navigation, interaction, pointer, KeyAction::Enter)
    }
    pub(super) fn defer_action(
        &mut self,
        view: &TuiState,
        navigation: u64,
        interaction: u64,
        pointer: bool,
        action: KeyAction,
    ) -> Result<bool, String> {
        let Some(read) = self.reads.get_mut(&view.view_identity().0) else {
            return Ok(false);
        };
        if !matches!(read.request, Request::List) {
            return Err("session picker operation pending; selection unchanged".into());
        }
        if pointer {
            return Err("session list preparing; select the row after refresh".into());
        }
        read.open = Some(Open {
            identity: view.view_identity(),
            navigation,
            interaction,
            search: view.session_search(),
            all: view.sessions_all_projects(),
            cursor: view.sessions_cursor(),
            action,
        });
        Ok(true)
    }
    pub(super) fn pending(&self) -> bool {
        !self.reads.is_empty() || !self.titles.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.reads.values().any(|read| read.task.is_finished())
            || self.titles.values().any(|job| job.is_finished())
    }
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.reads.values().any(|read| match &read.request {
            Request::Rename { target, .. } => view.tab_route_session() == Some(target),
            Request::Delete { .. } => {
                read.identity == view.view_identity().0 && read.location == view.chrome.location
            }
            Request::List => false,
        })
    }
    pub(super) fn cancel_title(&mut self, app: &CoreApp, session: SessionId) {
        if self.titles.contains_key(&session) || self.titles.len() >= MAX_TABS {
            return;
        }
        let owner = app.clone();
        let target = session.clone();
        self.titles.insert(
            session,
            oc_core::wake::spawn(async move { owner.cancel_title(target).await }),
        );
    }
    pub(super) fn request(
        &mut self,
        app: &CoreApp,
        view: &mut TuiState,
        request: Request,
    ) -> Result<(), String> {
        let key = view.view_identity().0;
        if let Some(read) = self.reads.get_mut(&key) {
            if matches!(read.request, Request::List) && matches!(request, Request::List) {
                read.rerun = true;
                return Ok(());
            }
            return Err("session picker operation pending; original request retained".into());
        }
        if self.reads.len() >= MAX_TABS + 17 {
            return Err("session picker operation capacity reached".into());
        }
        let owner = app.clone();
        let scope = view.session_scope_update();
        let search = view.session_search();
        let all = view.sessions_all_projects();
        let query = search.clone();
        let captured = request.clone();
        self.reads.insert(
            key,
            Read {
                identity: key,
                location: view.chrome.location.clone(),
                generation: view.chrome.selection_generation,
                search,
                all,
                rerun: false,
                open: None,
                request,
                task: oc_core::wake::spawn(async move {
                    match captured {
                        Request::Rename {
                            target,
                            title,
                            direct,
                        } => {
                            if direct.is_some() {
                                owner.rename_session(target, title).await?;
                            } else {
                                owner
                                    .picker_session_action(
                                        target,
                                        query,
                                        all,
                                        oc_core::queries::SessionPickerAction::Rename(title),
                                    )
                                    .await?;
                            }
                            Ok(Data::Renamed)
                        }
                        Request::Delete { target } => {
                            match owner
                                .picker_session_action(
                                    target,
                                    query,
                                    all,
                                    oc_core::queries::SessionPickerAction::Delete,
                                )
                                .await?
                            {
                                oc_core::queries::SessionPickerResult::Deleted(_) => {
                                    Ok(Data::Deleted)
                                }
                                _ => Err(CoreError::TabDeckConflict),
                            }
                        }
                        Request::List => {
                            let context = owner.session_picker_context(scope).await?;
                            let entries = owner.session_list(query, context.all_projects).await?;
                            Ok(Data::List(context, entries))
                        }
                    }
                }),
            },
        );
        Ok(())
    }
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut jobs = std::mem::take(&mut deck.picker);
    let mut changed = false;
    let mut action = None;
    let navigation = deck.navigation_revision;
    let interaction = deck.interaction_revision;
    let done: Vec<_> = jobs
        .titles
        .iter()
        .filter(|(_, task)| task.is_finished())
        .map(|(id, _)| id.clone())
        .collect();
    for id in done {
        let _ = jobs
            .titles
            .remove(&id)
            .expect("finished title cancellation")
            .await;
    }
    let done: Vec<_> = jobs
        .reads
        .iter()
        .filter(|(_, read)| read.task.is_finished())
        .map(|(key, _)| *key)
        .collect();
    for key in done {
        let read = jobs.reads.remove(&key).expect("finished picker operation");
        let result = read.task.await.unwrap_or(Err(CoreError::Shutdown));
        match &read.request {
            Request::List => {
                if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
                    view.view_identity().0 == read.identity
                        && view.chrome.location == read.location
                        && view.chrome.selection_generation == read.generation
                }) {
                    let current = view.session_search() == read.search
                        && view.sessions_all_projects() == read.all;
                    let prepared = result.is_ok();
                    let pending = read
                        .open
                        .filter(|open| open.matches(view, navigation, interaction));
                    if current {
                        match result {
                            Ok(Data::List(context, entries)) => {
                                view.apply_session_picker_context(context);
                                view.apply_session_entries(entries);
                            }
                            Err(error) => view.push_note(&error.to_string()),
                            _ => {}
                        }
                        changed = true;
                    }
                    if read.rerun {
                        let _ = jobs.request(app, view, Request::List);
                        if let Some(next) = jobs.reads.get_mut(&key) {
                            next.open = pending;
                        }
                    } else if current
                        && prepared
                        && let Some(pending) = pending
                    {
                        action = Some((view.view_identity(), pending.action));
                    }
                }
            }
            Request::Rename {
                target,
                title,
                direct,
            } => {
                for view in read_jobs::views_mut(state, deck)
                    .filter(|view| view.chrome.location == read.location)
                {
                    if result.is_ok() {
                        view.selected_session_renamed(&target.0, title.clone());
                    }
                    if view.view_identity().0 == read.identity {
                        match (&result, direct) {
                            (Ok(_), Some(true)) => {
                                view.rename_session_direct_applied(title.clone())
                            }
                            (Ok(_), Some(false)) => view.rename_session_applied(title.clone()),
                            (Err(error), Some(true)) => view.rename_session_direct_rejected(
                                format!("session rename unavailable: {error}"),
                            ),
                            (Err(error), _) => view.rename_session_rejected(format!(
                                "session rename unavailable: {error}"
                            )),
                            _ => {}
                        }
                    }
                }
                changed = true;
            }
            Request::Delete { .. } => {
                if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
                    view.view_identity().0 == read.identity && view.chrome.location == read.location
                }) {
                    match result {
                        Ok(Data::Deleted) if *view.panel() == TuiPanel::Sessions => {
                            // Query the current surface; never adopt the foreign
                            // Location's returned deck or repeat the deletion.
                            let _ = jobs.request(app, view, Request::List);
                        }
                        Err(error) => view.session_delete_rejected(error.to_string()),
                        _ => {}
                    }
                    changed = true;
                }
            }
        }
    }
    deck.picker = jobs;
    if let Some((identity, action)) = action
        && state.view_identity() == identity
    {
        let outcome = state.handle_panel_key(action);
        Box::pin(apply_outcome(app, state, deck, outcome, false)).await;
        changed = true;
    }
    changed
}
