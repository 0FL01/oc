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
}

#[derive(Default)]
pub(super) struct Controls {
    dirty: bool,
    job: Option<tokio::task::JoinHandle<Result<Prepared, CoreError>>>,
    effects: HashMap<u64, Mutation>,
}
enum Reply {
    Approval,
    Question(Box<oc_core::question::QuestionReply>),
    Permission(bool),
}
struct Mutation {
    fence: Fence,
    reply: Reply,
    job: tokio::task::JoinHandle<Result<(), String>>,
}

impl Controls {
    pub(super) fn request(&mut self) {
        self.dirty = true;
    }
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.effects.contains_key(&view.view_identity().0)
    }
    pub(super) fn pending(&self) -> bool {
        !self.effects.is_empty() || self.job.is_some() || self.dirty
    }
    pub(super) fn ready(&self) -> bool {
        self.effects.values().any(|effect| effect.job.is_finished())
            || self.job.as_ref().is_some_and(|job| job.is_finished())
    }
    pub(super) fn reply(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        intent: PanelIntent,
        cli: bool,
    ) -> Result<(), String> {
        let fence = fence(view);
        let key = fence.identity.0;
        if self.effects.contains_key(&key) || self.effects.len() >= MAX_TABS + 17 {
            return Err("control reply pending; original request retained".into());
        }
        let owner = app.clone();
        let (reply, job) = match intent {
            PanelIntent::ReplyApproval(reply) => (
                Reply::Approval,
                tokio::spawn(async move {
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
                    tokio::spawn(async move {
                        owner
                            .reply_question(captured)
                            .await
                            .map_err(|error| error.to_string())
                    }),
                )
            }
            PanelIntent::SetPermissionMode { auto_once } => (
                Reply::Permission(auto_once),
                tokio::spawn(async move {
                    owner
                        .set_permission_mode_with_cli_auto(auto_once, cli)
                        .await
                        .map_err(|error| error.to_string())
                }),
            ),
            _ => unreachable!("control reply lane"),
        };
        self.effects.insert(key, Mutation { fence, reply, job });
        Ok(())
    }

    async fn sync(&mut self, app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
        let mut changed = false;
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
            if let Reply::Permission(auto) = effect.reply {
                if result.is_ok()
                    && state.chrome.location == effect.fence.location
                    && state.chrome.selection_generation == effect.fence.generation
                {
                    let effective = auto || deck.cli_auto;
                    deck.permission_auto = Some(effective);
                    for view in read_jobs::views_mut(state, deck) {
                        view.chrome.permissions_auto = auto;
                        view.auto_accept = if effective {
                            oc_core::queries::AutoAcceptState::Enabled
                        } else {
                            oc_core::queries::AutoAcceptState::Disabled
                        };
                        view.permission_mode_applied();
                    }
                } else if let Err(error) = &result
                    && let Some(view) =
                        read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key)
                {
                    view.push_note(&format!("Permission mode unchanged: {error}"));
                }
            } else if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
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
                    Reply::Permission(_) => unreachable!(),
                }
            }
            self.request();
            changed = true;
        }
        if self.job.as_ref().is_some_and(|job| job.is_finished()) {
            let result = self
                .job
                .take()
                .expect("finished controls")
                .await
                .unwrap_or(Err(CoreError::Shutdown));
            match result {
                Ok(prepared) => {
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
            self.job = Some(tokio::spawn(async move {
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
                let mut parents = HashMap::new();
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
