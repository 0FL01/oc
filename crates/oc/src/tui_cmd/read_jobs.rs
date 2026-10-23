//! Concrete bounded reads. Captures contain only owned application DTOs; all
//! view mutation stays on the terminal task, including parked receipt owners.
use super::*;
use oc_core::core_app::WorkerTurnId;
use oc_core::queries::{ChildJob, HistoryPage, ShellSnapshot};
use std::collections::HashMap;
use tokio::task::JoinHandle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Route {
    identity: (u64, u64),
    pub(super) session: SessionId,
    location: Option<String>,
    generation: u64,
    turn: Option<WorkerTurnId>,
    floor: u64,
    epoch: u64,
    pub(super) child: Option<ChildJob>,
}

impl Route {
    pub(super) fn capture(view: &TuiState) -> Option<Self> {
        Some(Self {
            identity: view.view_identity(),
            session: view.attached_session()?.clone(),
            location: view.chrome.location.clone(),
            generation: view.chrome.selection_generation,
            turn: view.active_turn().cloned(),
            floor: view.projection_floor(),
            epoch: view.projection_epoch(),
            child: view.linked_child().cloned(),
        })
    }

    pub(super) fn matches(&self, view: &TuiState) -> bool {
        Self::capture(view).as_ref() == Some(self)
    }

    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.same_owner(view) && view.chrome.selection_generation == self.generation
    }
    /// Effect payloads stay with their node across metadata publication. Reads
    /// additionally require the captured configuration/projection fences.
    pub(super) fn same_owner(&self, view: &TuiState) -> bool {
        self.identity.0 == view.view_identity().0
            && view.attached_session() == Some(&self.session)
            && view.chrome.location == self.location
            && match (view.linked_child(), self.child.as_ref()) {
                (Some(current), Some(captured)) => child_controls::same_launch(current, captured),
                (None, None) => true,
                _ => false,
            }
    }
    pub(super) fn projection_current(&self, view: &TuiState) -> bool {
        self.owns(view)
            && self.epoch == view.projection_epoch()
            && self.turn.as_ref() == view.active_turn()
            && self.floor == view.projection_floor()
    }
    fn same_view(&self, view: &TuiState) -> bool {
        self.owns(view) && self.identity == view.view_identity()
    }

    pub(super) async fn validate_child(
        &self,
        app: &CoreApp,
    ) -> Result<Option<HistoryPage>, CoreError> {
        let Some(child) = &self.child else {
            return Ok(None);
        };
        let page = app
            .read_child_transcript(child.parent.clone(), child.clone())
            .await?;
        if page
            .child_job
            .as_ref()
            .is_some_and(|current| !child_controls::same_launch(child, current))
        {
            return Err(CoreError::LocationChanged);
        }
        Ok(Some(page))
    }
}

struct Prepared {
    resources: bool,
    page: Option<HistoryPage>,
    children: Vec<ChildJob>,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    shells: Vec<oc_core::queries::ShellJob>,
    shell: Option<Result<ShellSnapshot, oc_core::queries::ShellJob>>,
    dcp: Option<oc_core::queries::DcpSnapshot>,
    summaries: Vec<(String, oc_core::dcp_view::DcpSummaryPage)>,
}

struct Read {
    route: Route,
    history: bool,
    resources: bool,
    compress: Option<WorkerTurnId>,
    rerun: bool,
    job: Option<JoinHandle<Result<Prepared, CoreError>>>,
}

#[derive(Default)]
pub(super) struct Reads {
    jobs: HashMap<u64, Read>,
}

impl Reads {
    pub(super) fn pending(&self) -> bool {
        !self.jobs.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.jobs
            .values()
            .any(|read| read.job.as_ref().is_none_or(|job| job.is_finished()))
    }

    pub(super) fn request(&mut self, view: &TuiState, history: bool) {
        self.queue(view, history, true);
    }

    pub(super) fn shells(&mut self, view: &TuiState) {
        self.queue(view, false, false);
    }

    fn queue(&mut self, view: &TuiState, history: bool, resources: bool) {
        let Some(route) = Route::capture(view) else {
            return;
        };
        self.jobs
            .entry(route.identity.0)
            .and_modify(|read| {
                read.rerun = true;
                read.history |= history;
                read.resources |= resources;
            })
            .or_insert(Read {
                route,
                history,
                resources,
                compress: None,
                rerun: false,
                job: None,
            });
    }

    pub(super) fn compress(&mut self, view: &TuiState, turn: WorkerTurnId) {
        self.request(view, true);
        if let Some(read) = self.jobs.get_mut(&view.view_identity().0) {
            read.compress = Some(turn);
        }
    }

    /// Joining only is_finished handles never waits for the owner. A stale read
    /// gets one coalesced replacement against the retained current binding.
    pub(super) async fn sync(
        &mut self,
        app: &CoreApp,
        state: &mut TuiState,
        deck: &mut LoopState,
    ) -> bool {
        let mut changed = false;
        let keys: Vec<_> = self.jobs.keys().copied().collect();
        for key in keys {
            let Some(view) = views_mut(state, deck).find(|view| view.view_identity().0 == key)
            else {
                if let Some(read) = self.jobs.remove(&key)
                    && let Some(job) = read.job
                {
                    job.abort();
                }
                continue;
            };
            let mut read = self.jobs.remove(&key).expect("known read slot");
            if read.job.as_ref().is_some_and(|job| !job.is_finished()) {
                self.jobs.insert(key, read);
                continue;
            }
            if let Some(job) = read.job.take() {
                let result = job.await.unwrap_or(Err(CoreError::Shutdown));
                if read.route.matches(view) || !read.resources && read.route.same_view(view) {
                    match result {
                        Ok(prepared) => {
                            if let Some(page) = prepared.page {
                                if let Some(current) = page.child_job.as_deref() {
                                    view.reconcile_linked_terminal(current, &page);
                                } else {
                                    view.refresh_completed_page(&page);
                                }
                                if let Some(turn) = &read.compress {
                                    report_compress_outcome(view, turn, &page);
                                }
                            }
                            if prepared.resources {
                                if let Some(linked) = view.linked_child().cloned()
                                    && let Some(current) = prepared.children.iter().find(|job| {
                                        job.parent == linked.parent
                                            && job.child == linked.child
                                            && job.operation == linked.operation
                                            && job.generation == linked.generation
                                            && job.delivery_id == linked.delivery_id
                                            && job.location == linked.location
                                    })
                                {
                                    view.refresh_linked_child(current.clone());
                                }
                                view.apply_child_jobs(prepared.children);
                                view.apply_compaction_history(prepared.compactions);
                            }
                            view.apply_shell_jobs(prepared.shells);
                            if let Some(shell) = prepared.shell {
                                match shell {
                                    Ok(snapshot) => view.apply_shell_snapshot(snapshot),
                                    Err(job) => view.apply_shell_read_failure(&job),
                                }
                            }
                            if let Some(dcp) = prepared.dcp {
                                view.apply_dcp_snapshot(dcp);
                            }
                            for (op, page) in prepared.summaries {
                                view.apply_dcp_summary(&read.route.session, &op, page);
                            }
                            changed = true;
                        }
                        Err(CoreError::LocationChanged) => {}
                        Err(error) => {
                            view.push_note(&format!("Refresh failed: {error}"));
                            changed = true;
                        }
                    }
                } else {
                    read.rerun = true;
                }
                if !read.rerun {
                    continue;
                }
            }
            let Some(route) = Route::capture(view) else {
                continue;
            };
            let history = read.history;
            let resources = read.resources;
            let summaries = if resources {
                view.dcp_summary_requests()
            } else {
                Vec::new()
            };
            let shell = view.shell_viewer().cloned();
            let dcp = resources && (*view.panel() == TuiPanel::Dcp || read.compress.is_some());
            let owner = app.clone();
            let captured = route.clone();
            read.job = Some(oc_core::wake::spawn(async move {
                let route = captured;
                // Qualify the exact retained launch before any linked resources.
                let mut linked_page = route.validate_child(&owner).await?;
                let mut page = if history {
                    Some(if let Some(page) = linked_page.take() {
                        page
                    } else {
                        owner
                            .transcript_page(route.session.clone(), None, None, HISTORY_PAGE_LIMIT)
                            .await?
                    })
                } else {
                    None
                };
                let mut children = if resources {
                    owner.child_jobs(route.session.clone()).await?
                } else {
                    Vec::new()
                };
                if resources && let Some(linked) = route.child {
                    children.extend(owner.child_jobs(linked.parent.clone()).await?);
                    if page.is_none()
                        && route.turn.is_some()
                        && let Some(current) = children.iter().find(|job| {
                            job.parent == linked.parent
                                && job.child == linked.child
                                && job.operation == linked.operation
                                && job.generation == linked.generation
                                && job.delivery_id == linked.delivery_id
                                && job.location == linked.location
                                && !matches!(
                                    job.state,
                                    oc_core::queries::ChildState::Admitted
                                        | oc_core::queries::ChildState::Running
                                )
                        })
                    {
                        let _ = current;
                        page = linked_page.take();
                    }
                }
                children.sort_by_key(|job| {
                    !matches!(
                        job.state,
                        oc_core::queries::ChildState::Admitted
                            | oc_core::queries::ChildState::Running
                    )
                });
                children.truncate(16);
                let compactions = if resources {
                    owner.compaction_history(route.session.clone()).await?
                } else {
                    Vec::new()
                };
                let shells = owner.shell_jobs(route.session.clone()).await?;
                let shell = if let Some(shell) = shell {
                    Some(
                        owner
                            .shell_snapshot(shell.session.clone(), shell.shell_id.clone())
                            .await
                            .map_err(|_| shell),
                    )
                } else {
                    None
                };
                let dcp = if dcp {
                    Some(owner.dcp_snapshot(route.session.clone()).await?)
                } else {
                    None
                };
                let mut pages = Vec::new();
                for (op, index) in summaries {
                    if let Ok(Some(page)) = owner
                        .dcp_summary_page(
                            route.session.clone(),
                            op.clone(),
                            index,
                            0,
                            oc_tui::dcp_view::SUMMARY_PAGE_BYTES,
                        )
                        .await
                    {
                        pages.push((op, page));
                    }
                }
                Ok(Prepared {
                    resources,
                    page,
                    children,
                    compactions,
                    shells,
                    shell,
                    dcp,
                    summaries: pages,
                })
            }));
            read.route = route;
            read.rerun = false;
            self.jobs.insert(key, read);
        }
        changed
    }
}

pub(super) fn views<'a>(
    state: &'a TuiState,
    deck: &'a LoopState,
) -> impl Iterator<Item = &'a TuiState> {
    std::iter::once(state)
        .chain(deck.tabs.iter().flatten())
        .chain(deck.home.iter())
        .chain(deck.child_parent.as_deref())
        .chain(deck.child_views.values())
}

pub(super) fn views_mut<'a>(
    state: &'a mut TuiState,
    deck: &'a mut LoopState,
) -> impl Iterator<Item = &'a mut TuiState> {
    std::iter::once(state)
        .chain(deck.tabs.iter_mut().flatten())
        .chain(deck.home.iter_mut())
        .chain(deck.child_parent.as_deref_mut())
        .chain(deck.child_views.values_mut())
}

pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut reads = std::mem::take(&mut deck.reads);
    let changed = reads.sync(app, state, deck).await;
    deck.reads = reads;
    changed
}
