//! Exact-launch child reads. Only prepared DTOs cross the task boundary.
use super::*;
use oc_core::queries::{ChildJob, HistoryPage};
use std::collections::HashMap;

pub(super) struct Prepared {
    selected: ChildJob,
    page: HistoryPage,
    catalog: Option<CatalogSnapshot>,
    attached: Option<shared::Attached>,
}
struct Open {
    target: SessionId,
    retained: Option<read_jobs::Route>,
    identity: (u64, u64),
    route: read_jobs::Route,
    navigation: u64,
    draining: bool,
    job: tokio::task::JoinHandle<Result<Prepared, String>>,
}
#[derive(Default)]
pub(super) struct Jobs {
    open: Option<Open>,
    discovery: HashMap<SessionId, Discovery>,
}
type Discovery = tokio::task::JoinHandle<Result<Option<(read_jobs::Route, Prepared)>, String>>;
impl Jobs {
    pub(super) fn opening(&self, session: &SessionId) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| &open.target == session)
    }
    pub(super) fn pending(&self) -> bool {
        self.open.is_some() || !self.discovery.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.job.is_finished() && !open.draining)
            || self.discovery.values().any(|job| job.is_finished())
    }
    pub(super) fn discover(
        &mut self,
        app: &CoreApp,
        owner: SessionId,
        turn: oc_core::core_app::WorkerTurnId,
        parents: Vec<read_jobs::Route>,
    ) {
        if self.discovery.contains_key(&owner) || self.discovery.len() >= 16 {
            return;
        }
        let app = app.clone();
        let child = owner.clone();
        self.discovery.insert(
            owner,
            tokio::spawn(async move {
                for parent in parents {
                    let jobs = app
                        .child_jobs(parent.session.clone())
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(selected) = jobs.into_iter().find(|job| {
                        job.child == child
                            && job.state == oc_core::queries::ChildState::Running
                            && job.turn.as_deref() == Some(turn.0.as_str())
                    }) {
                        return prepare(&app, selected, false)
                            .await
                            .map(|prepared| Some((parent, prepared)));
                    }
                }
                Ok(None)
            }),
        );
    }
}
pub(super) fn open(
    app: &CoreApp,
    state: &TuiState,
    deck: &mut LoopState,
    selected: ChildJob,
) -> Result<(), String> {
    deck.departure_ready(state)?;
    let parent = deck
        .child_parent
        .as_deref()
        .and_then(TuiState::attached_session)
        .or_else(|| state.attached_session());
    if parent != Some(&selected.parent)
        && state.attached_session() != Some(&selected.parent)
        && state.linked_child().map(|job| &job.parent) != Some(&selected.parent)
    {
        return Err("foreign child selection".into());
    }
    if deck.children.open.is_some() {
        return Err("child open pending".into());
    }
    if !child_controls::room_for(deck, &selected.child) {
        return Err("retained child views full; settle or close a child surface first".into());
    }
    let retained = deck
        .child_views
        .get(&selected.child)
        .filter(|view| {
            view.linked_child()
                .is_some_and(|job| child_controls::same_launch(job, &selected))
        })
        .and_then(read_jobs::Route::capture);
    let route = read_jobs::Route::capture(state).ok_or("no session yet")?;
    deck.navigation_revision = deck.navigation_revision.wrapping_add(1);
    let owner = app.clone();
    deck.children.open = Some(Open {
        target: selected.child.clone(),
        retained: retained.clone(),
        identity: state.view_identity(),
        route,
        navigation: deck.navigation_revision,
        draining: false,
        job: tokio::spawn(async move { prepare(&owner, selected, retained.is_some()).await }),
    });
    Ok(())
}
async fn prepare(app: &CoreApp, selected: ChildJob, retained: bool) -> Result<Prepared, String> {
    let current = app
        .child_jobs(selected.parent.clone())
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|job| child_controls::same_launch(job, &selected));
    let page = app
        .read_child_transcript(selected.parent.clone(), selected.clone())
        .await
        .map_err(|error| error.to_string())?;
    let selected = page
        .child_job
        .as_deref()
        .filter(|job| child_controls::same_launch(job, &selected))
        .cloned()
        .or(current)
        .or_else(|| {
            (!matches!(
                selected.state,
                oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
            ))
            .then_some(selected)
        })
        .ok_or("child generation unavailable")?;
    let catalog = if retained {
        None
    } else {
        Some(app.catalog().await.map_err(|error| error.to_string())?)
    };
    let attached = if retained {
        None
    } else {
        shared::prepare_attached(
            app,
            selected.child.clone(),
            Some(selected.location.clone()),
            Some(selected.clone()),
            0,
        )
        .await?
    };
    Ok(Prepared {
        selected,
        page,
        catalog,
        attached,
    })
}
async fn view(app: &CoreApp, prepared: Prepared) -> TuiState {
    let Prepared {
        selected,
        page,
        catalog,
        attached,
    } = prepared;
    let mut catalog =
        catalog.expect("new child catalog; retained targets are leased by the open slot");
    if let Some((provider, model)) = selected.model.split_once('/') {
        catalog.provider = provider.into();
        let (model, variant) = model
            .split_once('#')
            .map_or((model, None), |(model, variant)| {
                (model, Some(variant.to_owned()))
            });
        catalog.model_id = model.into();
        catalog.variant = variant;
    }
    catalog.agent_id = Some(selected.agent.clone());
    catalog.chrome.location = Some(selected.location.clone());
    catalog.chrome.selection_generation = selected.generation;
    let mut view = TuiState::new(app.clone(), selected.child.clone());
    view.attach_page(&page);
    view.apply_catalog(catalog);
    view.attach_linked_child(selected.clone());
    if selected.state == oc_core::queries::ChildState::Running
        && let Some(turn) = selected.turn
    {
        view.begin_linked_turn(oc_core::core_app::WorkerTurnId(turn));
    }
    shared::apply_attached(app, &mut view, attached).await;
    view
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut changed = false;
    let waiting_projection = deck.children.open.as_ref().is_some_and(|open| {
        if open.job.is_finished()
            && let Some(view) = deck.child_views.get(&open.target)
            && open
                .retained
                .as_ref()
                .is_none_or(|route| !route.same_owner(view))
        {
            view.cancel_transcript_projection();
            view.poll_transcript_projection();
            view.has_pending_transcript_projection()
        } else {
            false
        }
    });
    if waiting_projection {
        deck.children
            .open
            .as_mut()
            .expect("pending child open")
            .draining = true;
    }
    if deck
        .children
        .open
        .as_ref()
        .is_some_and(|open| open.job.is_finished() && !waiting_projection)
    {
        let open = deck.children.open.take().expect("finished child open");
        if open.draining
            && let Some(view) = deck.child_views.get(&open.target)
        {
            view.resume_transcript_projection();
        }
        let result = open
            .job
            .await
            .unwrap_or_else(|_| Err("child open interrupted".into()));
        if state.view_identity() == open.identity
            && open.route.matches(state)
            && deck.navigation_revision == open.navigation
        {
            match result {
                Ok(prepared) => {
                    let selected = prepared.selected.clone();
                    let mut stale_target = false;
                    let mut next = if let Some(mut retained) =
                        deck.child_views.remove(&selected.child)
                        && retained
                            .linked_child()
                            .is_some_and(|job| child_controls::same_launch(job, &selected))
                    {
                        stale_target = !open
                            .retained
                            .as_ref()
                            .is_some_and(|route| route.matches(&retained));
                        if !stale_target {
                            retained.attach_linked_child(selected.clone());
                            retained.reconcile_linked_terminal(&selected, &prepared.page);
                        }
                        retained
                    } else {
                        view(app, prepared).await
                    };
                    next.take_tab_clocks_from(state);
                    next.resume_transcript_projection();
                    state.hide_children();
                    if deck.child_parent.is_some() {
                        let old = std::mem::replace(state, next);
                        child_controls::park(deck, old);
                    } else {
                        deck.child_parent_read_only = deck.read_only;
                        deck.child_parent = Some(Box::new(std::mem::replace(state, next)));
                    }
                    deck.read_only = true;
                    deck.reads.request(state, stale_target);
                    deck.controls.request();
                }
                Err(error) => state.push_note(&error),
            }
            changed = true;
        }
    }
    let finished: Vec<_> = deck
        .children
        .discovery
        .iter()
        .filter(|(_, job)| job.is_finished())
        .map(|(id, _)| id.clone())
        .collect();
    for id in finished {
        let job = deck
            .children
            .discovery
            .remove(&id)
            .expect("finished child discovery");
        if let Ok(Ok(Some((route, prepared)))) = job.await {
            if !read_jobs::views_mut(state, deck).any(|view| route.owns(view)) {
                continue;
            }
            let parent = prepared.selected.parent.clone();
            if !deck.child_views.contains_key(&id) && state.attached_session() != Some(&id) {
                let child = view(app, prepared).await;
                child_controls::park(deck, child);
            }
            let mut reads = std::mem::take(&mut deck.reads);
            for view in read_jobs::views_mut(state, deck)
                .filter(|view| view.attached_session() == Some(&parent))
            {
                reads.request(view, false);
            }
            deck.reads = reads;
            deck.shared_gap |= app.shares_workspace();
            deck.shared_refresh |= app.shares_workspace();
            changed = true;
        }
    }
    changed
}
