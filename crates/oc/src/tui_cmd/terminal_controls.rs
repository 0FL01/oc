//! Captured session controls over CoreApp; navigation never retargets a PTY.
use super::*;
use oc_core::queries::{TerminalAction, TerminalReceipt, TerminalRef};
use oc_tui::terminal_view::TerminalIntent;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq)]
struct Fence {
    identity: (u64, u64),
    session: SessionId,
    location: Option<String>,
    generation: u64,
    visible: Option<TerminalRef>,
    cursor: Option<u64>,
    child: Option<oc_core::queries::ChildJob>,
}
impl Fence {
    fn capture(view: &TuiState) -> Option<Self> {
        Some(Self {
            identity: view.view_identity(),
            session: view.attached_session()?.clone(),
            location: view.chrome.location.clone(),
            generation: view.chrome.terminal_generation,
            visible: view.terminal_visible().cloned(),
            cursor: view.terminal_cursor(),
            child: view.linked_child().cloned(),
        })
    }
}
struct Prepared {
    inventory: oc_core::queries::TerminalInventory,
    snapshot: Option<oc_core::queries::TerminalSnapshot>,
    replay: Option<oc_core::queries::TerminalReplay>,
}
struct Read {
    fence: Fence,
    rerun: bool,
    focus: Option<Focus>,
    job: Option<tokio::task::JoinHandle<Result<Prepared, CoreError>>>,
}
#[derive(Clone)]
struct Focus {
    interaction: u64,
    identity: (u64, u64),
    target: Option<TerminalRef>,
}
struct Resize {
    target: TerminalRef,
    size: oc_core::queries::TerminalSize,
    latest: oc_core::queries::TerminalSize,
    sent: bool,
    job: Option<tokio::task::JoinHandle<Result<TerminalReceipt, CoreError>>>,
}
#[derive(Default)]
pub(super) struct Jobs {
    reads: HashMap<u64, Read>,
    sizes: HashMap<String, Resize>,
    effects: HashMap<SessionId, Mutation>,
}
struct Mutation {
    route: read_jobs::Route,
    focus: bool,
    focus_context: Focus,
    control: bool,
    feedback: bool,
    job: tokio::task::JoinHandle<Result<(), CoreError>>,
    pending: std::collections::VecDeque<TerminalIntent>,
    pending_bytes: usize,
}
impl Jobs {
    /// EOF during an explicit selection belongs to that captured PTY, even
    /// before its asynchronous screen is ready. Ordinary composer input still
    /// changes the interaction revision and cancels any later focus transfer.
    pub(super) fn attaching_eof(
        &mut self,
        view: &TuiState,
        interaction: u64,
    ) -> Option<TerminalRef> {
        if *view.panel() != TuiPanel::None
            || view.approvals.active().is_some()
            || view.questions.active().is_some()
        {
            return None;
        }
        let session = view.attached_session()?;
        let focus =
            if let Some(effect) = self.effects.get_mut(session).filter(|effect| effect.focus) {
                Some(&mut effect.focus_context)
            } else {
                self.reads
                    .get_mut(&view.view_identity().0)
                    .and_then(|read| read.focus.as_mut())
            };
        // An older prepared screen can still be focused while Select is owned.
        // Its EOF must not beat the newer explicit, immutable selection capture.
        if let Some(focus) = focus
            && focus.identity == view.view_identity()
            && focus.interaction == interaction.wrapping_sub(1)
            && let Some(target) = focus.target.clone()
            && target.session == *session
            && Some(&target.location) == view.chrome.location.as_ref()
        {
            // The immutable PTY reference keeps its creation generation across
            // later catalog publications; the owner validates that exact ref.
            focus.interaction = interaction;
            return Some(target);
        }
        (view.terminal_focused() && !view.terminal_ready())
            .then(|| view.terminal_visible().cloned())
            .flatten()
    }
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        view.attached_session()
            .is_some_and(|session| self.effects.contains_key(session))
    }
    pub(super) fn pending(&self) -> bool {
        !self.effects.is_empty()
            || !self.reads.is_empty()
            || self.sizes.values().any(|resize| resize.job.is_some())
    }
    pub(super) fn ready(&self) -> bool {
        self.effects.values().any(|effect| effect.job.is_finished())
            || self
                .reads
                .values()
                .any(|read| read.job.as_ref().is_none_or(|job| job.is_finished()))
            || self
                .sizes
                .values()
                .any(|resize| resize.job.as_ref().is_some_and(|job| job.is_finished()))
    }
    pub(super) fn request(&mut self, view: &TuiState) {
        if !view.chrome.session_terminal {
            return;
        }
        let Some(fence) = Fence::capture(view) else {
            return;
        };
        self.reads
            .entry(fence.identity.0)
            .and_modify(|read| read.rerun = true)
            .or_insert(Read {
                fence,
                rerun: false,
                focus: None,
                job: None,
            });
    }
    pub(super) fn resize(&mut self, view: &TuiState, frame: ratatui::layout::Rect) {
        let Some(target) = view.terminal_visible().cloned() else {
            return;
        };
        let size = view.terminal_size_for_frame(frame);
        if view
            .terminal_snapshot()
            .is_some_and(|snapshot| snapshot.size == size)
        {
            return;
        }
        if let Some(resize) = self.sizes.get_mut(&target.id)
            && resize.target == target
        {
            resize.latest = size;
            return;
        }
        if self.sizes.len() >= 64 {
            let retired = self
                .sizes
                .iter()
                .find(|(_, resize)| resize.job.is_none())
                .map(|(id, _)| id.clone());
            if let Some(retired) = retired {
                self.sizes.remove(&retired);
            } else {
                return;
            }
        }
        self.sizes.insert(
            target.id.clone(),
            Resize {
                target,
                size,
                latest: size,
                sent: false,
                job: None,
            },
        );
    }
    async fn sync(&mut self, app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
        let mut changed = false;
        let interaction = deck.interaction_revision;
        let finished: Vec<_> = self
            .effects
            .iter()
            .filter(|(_, effect)| effect.job.is_finished())
            .map(|(session, _)| session.clone())
            .collect();
        for session in finished {
            let mut effect = self
                .effects
                .remove(&session)
                .expect("finished terminal mutation");
            let result = (&mut effect.job).await.unwrap_or(Err(CoreError::Shutdown));
            let succeeded = result.is_ok();
            if let Some(view) =
                read_jobs::views_mut(state, deck).find(|view| effect.route.same_owner(view))
            {
                match result {
                    Ok(()) if !effect.control => {
                        self.request(view);
                        if effect.focus
                            && let Some(read) = self.reads.get_mut(&view.view_identity().0)
                        {
                            read.focus = Some(effect.focus_context.clone());
                        }
                        self.resize(view, view.detail_area());
                    }
                    Ok(()) => {}
                    Err(error) => {
                        if effect.feedback {
                            view.terminal_failure(true);
                        } else {
                            view.focus_terminal(false);
                            view.push_note(&error.to_string());
                        }
                    }
                }
                changed = true;
                if succeeded && let Some(action) = effect.pending.pop_front() {
                    if let TerminalIntent::Control(TerminalAction::Input { bytes, .. }) = &action {
                        effect.pending_bytes -= bytes.len();
                    }
                    let owner = app.clone();
                    let captured = session.clone();
                    effect.control = true;
                    effect.focus = false;
                    effect.feedback = false;
                    effect.job =
                        tokio::spawn(async move { perform(&owner, captured, action).await });
                    self.effects.insert(session, effect);
                } else if !succeeded && !effect.pending.is_empty() {
                    view.push_note(
                        "terminal outcome unknown or refused; queued input was not sent",
                    );
                }
            }
        }
        let keys: Vec<_> = self.reads.keys().copied().collect();
        for key in keys {
            let Some(view) =
                read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key)
            else {
                if let Some(read) = self.reads.remove(&key)
                    && let Some(job) = read.job
                {
                    job.abort();
                }
                continue;
            };
            let mut read = self.reads.remove(&key).expect("terminal read slot");
            if read.job.as_ref().is_some_and(|job| !job.is_finished()) {
                self.reads.insert(key, read);
                continue;
            }
            if let Some(job) = read.job.take() {
                let result = job.await.unwrap_or(Err(CoreError::Shutdown));
                let succeeded = result.is_ok();
                if Fence::capture(view).as_ref() == Some(&read.fence) {
                    match result {
                        Ok(prepared) => {
                            view.apply_terminal_inventory(&read.fence.session, prepared.inventory);
                            if let Some(snapshot) = prepared.snapshot {
                                view.apply_terminal_snapshot(snapshot);
                            }
                            if let Some(replay) = prepared.replay {
                                view.apply_terminal_replay(replay);
                            }
                        }
                        Err(error) => {
                            let composer = view.terminals_open();
                            view.terminal_failure(composer);
                            if !composer {
                                view.push_note(&error.to_string());
                            }
                        }
                    }
                    changed = true;
                } else {
                    read.rerun = true;
                }
                if !read.rerun {
                    if succeeded
                        && let Some(focus) = &read.focus
                        && focus.interaction == interaction
                        && focus.identity == view.view_identity()
                        && *view.panel() == TuiPanel::None
                    {
                        view.focus_terminal(true);
                    }
                    continue;
                }
            }
            let Some(fence) = Fence::capture(view) else {
                continue;
            };
            let captured = fence.clone();
            let owner = app.clone();
            read.job = Some(tokio::spawn(async move { prepare(&owner, captured).await }));
            read.fence = fence;
            read.rerun = false;
            self.reads.insert(key, read);
        }
        for resize in self.sizes.values_mut() {
            if resize.job.as_ref().is_some_and(|job| job.is_finished()) {
                // Never repeat an unknown resize. Only a distinct coalesced size
                // can issue another captured command.
                let _ = resize.job.take().expect("finished resize").await;
            }
            if resize.job.is_none() && (!resize.sent || resize.latest != resize.size) {
                resize.size = resize.latest;
                resize.sent = true;
                let owner = app.clone();
                let target = resize.target.clone();
                let size = resize.size;
                resize.job = Some(tokio::spawn(async move {
                    owner
                        .terminal(
                            target.session.clone(),
                            TerminalAction::Resize { target, size },
                        )
                        .await
                }));
            }
        }
        changed
    }
}
async fn prepare(app: &CoreApp, fence: Fence) -> Result<Prepared, CoreError> {
    if let Some(child) = &fence.child {
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
    }
    let TerminalReceipt::Inventory(inventory) = app
        .terminal(fence.session.clone(), TerminalAction::List)
        .await?
    else {
        return Err(CoreError::Shutdown);
    };
    let mut snapshot = None;
    let mut replay = None;
    if let Some(target) = inventory.selected.clone() {
        let cursor = if fence.visible.as_ref() == Some(&target) {
            fence.cursor
        } else {
            None
        };
        let cursor = if let Some(cursor) = cursor {
            cursor
        } else {
            let TerminalReceipt::Snapshot(screen) = app
                .terminal(
                    fence.session.clone(),
                    TerminalAction::Snapshot(target.clone()),
                )
                .await?
            else {
                return Err(CoreError::Shutdown);
            };
            let cursor = screen.output_cursor;
            snapshot = Some(screen);
            cursor
        };
        let TerminalReceipt::Replay(page) = app
            .terminal(fence.session, TerminalAction::Replay { target, cursor })
            .await?
        else {
            return Err(CoreError::Shutdown);
        };
        replay = Some(page);
    }
    Ok(Prepared {
        inventory,
        snapshot,
        replay,
    })
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut jobs = std::mem::take(&mut deck.terminals);
    let changed = jobs.sync(app, state, deck).await;
    deck.terminals = jobs;
    changed
}

async fn select(
    app: &CoreApp,
    session: &SessionId,
    target: Option<TerminalRef>,
) -> Result<(), CoreError> {
    app.terminal(session.clone(), TerminalAction::Select(target))
        .await?;
    Ok(())
}
async fn create(
    app: &CoreApp,
    session: &SessionId,
    action: TerminalAction,
) -> Result<(), CoreError> {
    let TerminalReceipt::Created(entry) = app.terminal(session.clone(), action).await? else {
        return Err(CoreError::Shutdown);
    };
    select(app, session, Some(entry.target)).await
}

pub(super) async fn apply(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    session: SessionId,
    action: TerminalIntent,
) {
    let load_feedback = matches!(
        action,
        TerminalIntent::Refresh
            | TerminalIntent::Create(_)
            | TerminalIntent::Toggle(_)
            | TerminalIntent::Select(_)
    );
    let focus = matches!(
        action,
        TerminalIntent::Create(_) | TerminalIntent::Toggle(_) | TerminalIntent::Select(_)
    );
    let control = matches!(action, TerminalIntent::Control(_));
    let target = match &action {
        TerminalIntent::Select(target) => Some(target.clone()),
        _ => None,
    };
    let route = read_jobs::views_mut(state, deck)
        .find(|view| view.attached_session() == Some(&session))
        .and_then(|view| {
            read_jobs::Route::capture(view).map(|route| (route, view.view_identity()))
        });
    let Some((route, identity)) = route else {
        return;
    };
    if let Some(effect) = deck.terminals.effects.get_mut(&session) {
        let bytes = if let TerminalIntent::Control(TerminalAction::Input { bytes, .. }) = &action {
            bytes.len()
        } else {
            0
        };
        if control
            && effect.pending.len() < 64
            && effect.pending_bytes.saturating_add(bytes) <= 65_536
        {
            effect.pending_bytes += bytes;
            effect.pending.push_back(action);
        } else {
            state.push_note("terminal mutation pending; additional input was not queued");
        }
        return;
    }
    if deck.terminals.effects.len() >= MAX_TABS + 17 {
        state.push_note("terminal mutation pending; original request retained");
        return;
    }
    let owner = app.clone();
    let captured = session.clone();
    let validation = route.clone();
    deck.terminals.effects.insert(
        session,
        Mutation {
            route,
            focus,
            focus_context: Focus {
                interaction: deck.interaction_revision,
                identity,
                target,
            },
            control,
            feedback: load_feedback,
            job: tokio::spawn(async move {
                validation.validate_child(&owner).await?;
                perform(&owner, captured, action).await
            }),
            pending: Default::default(),
            pending_bytes: 0,
        },
    );
}

async fn perform(
    app: &CoreApp,
    session: SessionId,
    action: TerminalIntent,
) -> Result<(), CoreError> {
    // One invocation owns its immutable source, including after child close.
    async {
        match action {
            TerminalIntent::Refresh | TerminalIntent::CloseComposer => {}
            TerminalIntent::Create(action) => create(app, &session, action).await?,
            TerminalIntent::Select(target) => select(app, &session, Some(target)).await?,
            TerminalIntent::Hide => select(app, &session, None).await?,
            TerminalIntent::Remove(target) => {
                app.terminal(session.clone(), TerminalAction::Remove(target))
                    .await?;
            }
            TerminalIntent::Control(control) => {
                app.terminal(session.clone(), control).await?;
            }
            TerminalIntent::Toggle(creation) => {
                let TerminalReceipt::Inventory(inventory) =
                    app.terminal(session.clone(), TerminalAction::List).await?
                else {
                    return Err(CoreError::Shutdown);
                };
                if inventory.selected.is_some() {
                    select(app, &session, None).await?;
                } else if let Some(last) = inventory.entries.last() {
                    select(app, &session, Some(last.target.clone())).await?;
                } else {
                    create(app, &session, creation).await?;
                }
            }
        }
        Ok::<_, CoreError>(())
    }
    .await
}

#[cfg(test)]
mod tests;
