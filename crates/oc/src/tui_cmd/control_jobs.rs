//! One control inventory/ancestry read for every retained route.
use super::*;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Clone, PartialEq, Eq)]
struct Fence {
    identity: (u64, u64),
    session: Option<SessionId>,
    location: Option<String>,
    generation: u64,
}
fn fence(view: &TuiState) -> Fence {
    Fence {
        identity: view.view_identity(),
        session: tab_route_session(view).cloned(),
        location: view.chrome.location.clone(),
        generation: view.chrome.selection_generation,
    }
}

struct Prepared {
    fences: Vec<Fence>,
    approvals: Vec<oc_core::approval::ApprovalRequest>,
    questions: Vec<oc_core::question::QuestionRequest>,
    ancestry: HashMap<SessionId, Vec<SessionId>>,
    parents: HashMap<SessionId, Option<SessionId>>,
}
#[derive(Clone, PartialEq, Eq)]
struct PermissionTarget {
    identity: Option<u64>,
    location: String,
    generation: u64,
    auto: bool,
    preference: Option<bool>,
}
struct Permission {
    target: PermissionTarget,
    job: tokio::task::JoinHandle<Result<(), CoreError>>,
}

/// A session's parent is fixed at creation; bound the memo, not its validity.
const MAX_PARENT_MEMO: usize = 1024;

#[derive(Default)]
pub(super) struct Controls {
    permission: Option<Permission>,
    permission_latest: Option<PermissionTarget>,
    dirty: bool,
    job: Option<tokio::task::JoinHandle<Result<Prepared, CoreError>>>,
    effects: HashMap<u64, Mutation>,
    /// Parent links learned by earlier refreshes: a pending request's
    /// ancestry is read once, not again on every periodic refresh.
    parents: HashMap<SessionId, Option<SessionId>>,
}
enum Reply {
    Approval,
    Question(Box<oc_core::question::QuestionReply>),
}
struct Mutation {
    fence: Fence,
    reply: Reply,
    job: tokio::task::JoinHandle<Result<(), String>>,
}

impl Controls {
    pub(super) async fn reattached(&mut self) {
        if let Some(permission) = self.permission.take() {
            permission.job.abort();
            let _ = permission.job.await;
        }
        self.permission_latest = None;
    }
    pub(super) fn request(&mut self) {
        self.dirty = true;
    }
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.effects.contains_key(&view.view_identity().0)
            || self
                .permission
                .as_ref()
                .is_some_and(|p| p.target.identity == Some(view.view_identity().0))
    }
    pub(super) fn pending(&self) -> bool {
        !self.effects.is_empty() || self.job.is_some() || self.dirty || self.permission.is_some()
    }
    pub(super) fn ready(&self) -> bool {
        self.effects.values().any(|effect| effect.job.is_finished())
            || self.job.as_ref().is_some_and(|job| job.is_finished())
            || self
                .permission
                .as_ref()
                .is_some_and(|p| p.job.is_finished())
    }

    pub(super) fn adopt_permissions(
        &mut self,
        app: &CoreApp,
        location: Option<&str>,
        cli: bool,
        confirmed: Option<bool>,
        catalog: &CatalogSnapshot,
    ) {
        let Some(location) = location else {
            return;
        };
        if catalog.chrome.location.as_deref() != Some(location)
            || catalog.auto_accept == oc_core::queries::AutoAcceptState::Unsupported
        {
            return;
        }
        let target = PermissionTarget {
            identity: None,
            location: location.to_owned(),
            generation: catalog.chrome.selection_generation,
            auto: cli || catalog.chrome.permissions_auto,
            preference: None,
        };
        // A canonical read from before a user's setting must not undo it while
        // persistence/watch publication catches up.
        if self.permission_latest.as_ref().is_some_and(|latest| {
            latest.location == target.location
                && (latest.generation > target.generation
                    || latest.generation == target.generation && latest.preference.is_some())
        }) {
            return;
        }
        if self.permission.is_none() && confirmed == Some(target.auto) {
            self.permission_latest = Some(target);
            return;
        }
        self.permission_request(app, target, cli);
    }

    fn permission_request(&mut self, app: &CoreApp, target: PermissionTarget, cli: bool) {
        self.permission_latest = Some(target.clone());
        if self.permission.is_some() {
            return;
        }
        let owner = app.clone();
        let captured = target.clone();
        self.permission = Some(Permission {
            target,
            job: oc_core::wake::spawn(async move {
                if let Some(preference) = captured.preference {
                    owner
                        .set_permission_mode_with_cli_auto(preference, cli)
                        .await
                } else {
                    owner.register_approval_consumer(captured.auto).await
                }
            }),
        });
    }
    pub(super) fn reply(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        intent: PanelIntent,
        cli: bool,
    ) -> Result<(), String> {
        if let PanelIntent::SetPermissionMode { auto_once } = intent {
            let location = view
                .chrome
                .location
                .clone()
                .ok_or("permission Location unavailable")?;
            self.permission_request(
                app,
                PermissionTarget {
                    identity: Some(view.view_identity().0),
                    location,
                    generation: view.chrome.selection_generation,
                    auto: auto_once || cli,
                    preference: Some(auto_once),
                },
                cli,
            );
            return Ok(());
        }
        let fence = fence(view);
        let key = fence.identity.0;
        if self.effects.contains_key(&key) || self.effects.len() >= MAX_TABS + 17 {
            return Err("control reply pending; original request retained".into());
        }
        let owner = app.clone();
        let (reply, job) = match intent {
            PanelIntent::ReplyApproval(reply) if owner.is_opencode() => {
                let request = view
                    .opencode_permission(&reply)
                    .map_err(|error| error.to_string())?;
                (
                    Reply::Approval,
                    oc_core::wake::spawn(async move {
                        owner
                            .opencode(request)
                            .await
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    }),
                )
            }
            PanelIntent::ReplyQuestion(reply) if owner.is_opencode() => {
                let request = view
                    .opencode_form(&reply)
                    .map_err(|error| error.to_string())?;
                (
                    Reply::Question(Box::new(reply)),
                    oc_core::wake::spawn(async move {
                        owner
                            .opencode(request)
                            .await
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    }),
                )
            }
            PanelIntent::ReplyApproval(reply) => (
                Reply::Approval,
                oc_core::wake::spawn(async move {
                    owner
                        .reply_approval(reply)
                        .await
                        .map_err(|error| error.to_string())
                }),
            ),
            PanelIntent::ReplyQuestion(reply) => {
                let captured = reply.clone();
                (
                    Reply::Question(Box::new(reply)),
                    oc_core::wake::spawn(async move {
                        owner
                            .reply_question(captured)
                            .await
                            .map_err(|error| error.to_string())
                    }),
                )
            }
            _ => unreachable!("control reply lane"),
        };
        self.effects.insert(key, Mutation { fence, reply, job });
        Ok(())
    }

    async fn sync(&mut self, app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
        let mut changed = false;
        if self
            .permission
            .as_ref()
            .is_some_and(|p| p.job.is_finished())
        {
            let permission = self.permission.take().expect("finished permission ACK");
            let result = permission.job.await.unwrap_or(Err(CoreError::Shutdown));
            if let Some(next) = self.permission_latest.clone().filter(|latest| {
                latest.location != permission.target.location
                    || latest.auto != permission.target.auto
                    || latest.preference.is_some()
                        && latest.preference != permission.target.preference
            }) {
                self.permission_request(app, next, deck.cli_auto);
            } else if deck.location.as_deref() == Some(permission.target.location.as_str()) {
                match result {
                    Ok(()) => {
                        deck.permission_auto = Some(permission.target.auto);
                        let preference = self
                            .permission_latest
                            .as_ref()
                            .and_then(|latest| latest.preference);
                        for view in read_jobs::views_mut(state, deck) {
                            if view.chrome.location.as_deref()
                                != Some(permission.target.location.as_str())
                                || view.parent_id.is_some()
                            {
                                continue;
                            }
                            if let Some(preference) = preference {
                                view.chrome.permissions_auto = preference;
                            }
                            if view.auto_accept != oc_core::queries::AutoAcceptState::Unsupported {
                                view.auto_accept = if permission.target.auto {
                                    oc_core::queries::AutoAcceptState::Enabled
                                } else {
                                    oc_core::queries::AutoAcceptState::Disabled
                                };
                            }
                            view.permission_mode_applied();
                        }
                    }
                    Err(error) => state.push_note(&format!("Permission mode unchanged: {error}")),
                }
            }
            self.request();
            changed = true;
        }
        let finished: Vec<_> = self
            .effects
            .iter()
            .filter(|(_, effect)| effect.job.is_finished())
            .map(|(key, _)| *key)
            .collect();
        for key in finished {
            let effect = self.effects.remove(&key).expect("finished control reply");
            let result = effect
                .job
                .await
                .unwrap_or_else(|_| Err("control outcome unknown".into()));
            if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
                view.view_identity().0 == key
                    && view.chrome.location == effect.fence.location
                    && tab_route_session(view) == effect.fence.session.as_ref()
            }) {
                match effect.reply {
                    Reply::Question(reply) => view
                        .questions
                        .reply_result(&reply, result.as_ref().err().map(ToString::to_string)),
                    Reply::Approval => {
                        if let Err(error) = &result {
                            view.approvals.error = Some(error.to_string());
                        }
                    }
                }
            }
            self.request();
            changed = true;
        }
        if app.is_opencode() {
            // Source requests are projected by OpenCode reads/events. Keep the
            // reply lane above, never poll native approval/question authority.
            self.dirty = false;
            return changed;
        }
        if self.job.as_ref().is_some_and(|job| job.is_finished()) {
            let result = self
                .job
                .take()
                .expect("finished controls")
                .await
                .unwrap_or(Err(CoreError::Shutdown));
            match result {
                Ok(mut prepared) => {
                    self.parents = std::mem::take(&mut prepared.parents);
                    for view in read_jobs::views_mut(state, deck) {
                        if !prepared.fences.contains(&fence(view)) {
                            self.dirty = true;
                            continue;
                        }
                        view.project_pending_approvals(&prepared.approvals);
                        let mut visible = Vec::new();
                        let mut roots = BTreeSet::new();
                        for request in &prepared.approvals {
                            let source = SessionId(request.binding.session.clone());
                            if view.parent_id.is_none()
                                && view.user_shell_admission_session() == Some(&source)
                            {
                                roots.insert(source.0);
                                visible.push((request.clone(), false));
                                continue;
                            }
                            let Some(ancestors) = prepared.ancestry.get(&source) else {
                                continue;
                            };
                            if let Some(root) = ancestors.last() {
                                roots.insert(root.0.clone());
                            }
                            if view.parent_id.is_none()
                                && view
                                    .attached_session()
                                    .is_some_and(|id| ancestors.contains(id))
                            {
                                visible.push((request.clone(), ancestors.len() > 1));
                            }
                        }
                        visible.sort_by_key(|(request, _)| {
                            (
                                view.attached_session().map(|id| &id.0)
                                    != Some(&request.binding.session),
                                request.id,
                            )
                        });
                        view.approval_roots = roots;
                        view.approvals.reconcile(visible);
                        let mut visible: Vec<_> = prepared
                            .questions
                            .iter()
                            .filter(|request| {
                                prepared
                                    .ancestry
                                    .get(&SessionId(request.binding.session.clone()))
                                    .is_some_and(|ancestors| {
                                        view.attached_session()
                                            .is_some_and(|id| ancestors.contains(id))
                                    })
                            })
                            .cloned()
                            .collect();
                        visible.sort_by_key(|request| {
                            (
                                view.attached_session().map(|id| &id.0)
                                    != Some(&request.binding.session),
                                request.id,
                            )
                        });
                        view.questions.reconcile(&prepared.questions, visible);
                    }
                    changed = true;
                }
                Err(error) => {
                    state.approvals.error = Some(error.to_string());
                    changed = true;
                }
            }
        }
        if self.job.is_none() && self.dirty {
            self.dirty = false;
            let fences = read_jobs::views_mut(state, deck)
                .map(|view| fence(view))
                .collect();
            let unadmitted: HashSet<_> = read_jobs::views_mut(state, deck)
                .filter(|view| view.attached_session().is_none())
                .filter_map(|view| view.user_shell_admission_session().cloned())
                .collect();
            let owner = app.clone();
            let mut parents = std::mem::take(&mut self.parents);
            if parents.len() > MAX_PARENT_MEMO {
                parents.clear();
            }
            self.job = Some(oc_core::wake::spawn(async move {
                let approvals = owner.pending_approvals().await?;
                let questions = owner.pending_questions().await?;
                let sources: HashSet<_> = approvals
                    .iter()
                    .map(|request| SessionId(request.binding.session.clone()))
                    .chain(
                        questions
                            .iter()
                            .map(|request| SessionId(request.binding.session.clone())),
                    )
                    .collect();
                let mut ancestry = HashMap::new();
                for source in sources {
                    if unadmitted.contains(&source) {
                        continue;
                    }
                    let mut chain = Vec::new();
                    let mut current = source.clone();
                    for _ in 0..64 {
                        if chain.contains(&current) {
                            break;
                        }
                        chain.push(current.clone());
                        if !parents.contains_key(&current) {
                            let page = owner
                                .transcript_page(current.clone(), None, None, 1)
                                .await?;
                            parents.insert(current.clone(), page.parent_id.map(SessionId));
                        }
                        let Some(parent) = parents.get(&current).cloned().flatten() else {
                            break;
                        };
                        current = parent;
                    }
                    ancestry.insert(source, chain);
                }
                Ok(Prepared {
                    fences,
                    approvals,
                    questions,
                    ancestry,
                    parents,
                })
            }));
        }
        changed
    }
}

pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut controls = std::mem::take(&mut deck.controls);
    let changed = controls.sync(app, state, deck).await;
    deck.controls = controls;
    changed
}
