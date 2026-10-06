//! Bounded child lifetime ownership within the application/runtime lineage.
use super::*;
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use oc_core::queries::{ChildJob, ChildState};

#[cfg(test)]
mod tests;

pub(super) enum Database<'a> {
    Borrowed(&'a Db),
    Owned(Db),
}
impl std::ops::Deref for Database<'_> {
    type Target = Db;
    fn deref(&self) -> &Db {
        match self {
            Self::Borrowed(db) => db,
            Self::Owned(db) => db,
        }
    }
}

pub(crate) const JOB_CAP: usize = 8;
const SESSION_CAP: usize = 4;

struct Work {
    identity: ChildJob,
    parent_turn: String,
    runtime: std::sync::Weak<Runtime<'static>>,
    mcp: Arc<mcp::McpOwner>,
    cancel: Arc<AtomicBool>,
    completion: Shared<BoxFuture<'static, bool>>,
    background: Arc<AtomicBool>,
    mode_changed: Arc<tokio::sync::Notify>,
    foreground_wait: Arc<AtomicBool>,
    parent_rejected: AtomicBool,
    foreground_result: Arc<Mutex<Option<SubagentOutcome>>>,
}

struct WaitRelease(Arc<AtomicBool>);
impl Drop for WaitRelease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) struct Jobs {
    db: Db,
    slots: Arc<tokio::sync::Semaphore>,
    sessions: Arc<Mutex<BTreeMap<String, String>>>,
    work: Mutex<BTreeMap<String, Work>>,
    wake: Arc<tokio::sync::Notify>,
    failed: Arc<AtomicBool>,
    closing: AtomicBool,
    retired: Mutex<Vec<Arc<mcp::McpOwner>>>,
    remote_unknown: AtomicBool,
}

pub(super) struct Reservation {
    _slot: tokio::sync::OwnedSemaphorePermit,
    sessions: Arc<Mutex<BTreeMap<String, String>>>,
    child: String,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.sessions
            .lock()
            .expect("child sessions")
            .remove(&self.child);
    }
}

impl Jobs {
    pub(super) fn permission_rejected(&self, operation: &str) {
        if let Some(work) = self.work.lock().expect("child work").get(operation) {
            work.parent_rejected.store(true, Ordering::Release);
            self.wake.notify_one();
        }
    }

    pub(super) fn permission_rejected_session(&self, session: &str) {
        let operation = self
            .work
            .lock()
            .expect("child work")
            .values()
            .find(|w| {
                w.identity.child.0 == session && w.completion.clone().now_or_never().is_none()
            })
            .map(|w| w.identity.operation.clone());
        if let Some(operation) = operation {
            self.permission_rejected(&operation);
        }
    }

    pub(super) fn take_parent_rejection(&self, session: &str) -> bool {
        self.work
            .lock()
            .expect("child work")
            .values()
            .filter(|w| w.identity.parent.0 == session)
            .fold(false, |rejected, w| {
                let pending = w.parent_rejected.swap(false, Ordering::AcqRel);
                (pending
                    && self
                        .db
                        .turn_result(&w.parent_turn)
                        .is_ok_and(|(state, _)| state == "started"))
                    || rejected
            })
    }

    /// Scoped observer of the real parent's borrowed cancellation token. Child
    /// execution remains owned; only plain permission rejection reaches here.
    pub(super) async fn observe_parent<F: std::future::Future>(
        &self,
        parent: &str,
        cancel: &AtomicBool,
        future: F,
    ) -> F::Output {
        tokio::pin!(future);
        loop {
            if self.take_parent_rejection(parent) {
                cancel.store(true, Ordering::Release);
            }
            tokio::select! {
                output = &mut future => return output,
                () = tokio::time::sleep(Duration::from_millis(5)) => {
                    if self.take_parent_rejection(parent) { cancel.store(true,Ordering::Release); }
                }
            }
        }
    }
    pub(crate) fn new(db: &Db) -> Arc<Self> {
        Arc::new(Self {
            db: db.shared_handle(),
            slots: Arc::new(tokio::sync::Semaphore::new(JOB_CAP)),
            sessions: Arc::default(),
            work: Mutex::default(),
            wake: Arc::new(tokio::sync::Notify::new()),
            failed: Arc::new(AtomicBool::new(false)),
            closing: AtomicBool::new(false),
            retired: Mutex::default(),
            remote_unknown: AtomicBool::new(false),
        })
    }

    pub(super) fn reserve(&self, parent: &str, child: &str) -> Result<Reservation, ToolError> {
        let busy = || ToolError::Failed {
            tool: SUBAGENT_TOOL.into(),
            reason: "child busy or child capacity exhausted; no prompt admitted".into(),
        };
        if self.closing.load(Ordering::Acquire) {
            return Err(busy());
        }
        if self.db.child_job_outstanding().map_err(|_| busy())? >= JOB_CAP as i64 {
            return Err(busy());
        }
        if self.db.child_job_unresolved(child).map_err(|_| busy())? {
            return Err(ToolError::Failed {
                tool: SUBAGENT_TOOL.into(),
                reason: "child busy or unresolved; explicit recovery required; no prompt admitted"
                    .into(),
            });
        }
        let slot = self.slots.clone().try_acquire_owned().map_err(|_| busy())?;
        let mut sessions = self.sessions.lock().expect("child sessions");
        if sessions.contains_key(child)
            || sessions
                .values()
                .filter(|owner| owner.as_str() == parent)
                .count()
                >= SESSION_CAP
        {
            return Err(busy());
        }
        sessions.insert(child.into(), parent.into());
        Ok(Reservation {
            _slot: slot,
            sessions: self.sessions.clone(),
            child: child.into(),
        })
    }

    fn reserve_resume(&self, identity: &ChildJob) -> Result<Reservation, RuntimeError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(RuntimeError::Storage);
        }
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| RuntimeError::Storage)?;
        let mut sessions = self.sessions.lock().expect("child sessions");
        if sessions.contains_key(&identity.child.0)
            || sessions
                .values()
                .filter(|p| *p == &identity.parent.0)
                .count()
                >= SESSION_CAP
        {
            return Err(RuntimeError::Storage);
        }
        sessions.insert(identity.child.0.clone(), identity.parent.0.clone());
        Ok(Reservation {
            _slot: slot,
            sessions: self.sessions.clone(),
            child: identity.child.0.clone(),
        })
    }

    pub(crate) async fn changed(&self) {
        let completions = self
            .work
            .lock()
            .expect("child work")
            .values()
            .map(|job| job.completion.clone())
            .collect::<Vec<_>>();
        if completions.is_empty() {
            self.wake.notified().await;
        } else {
            tokio::select! {
                () = self.wake.notified() => {},
                _ = futures_util::future::select_all(completions) => {},
            }
        }
    }

    pub(crate) fn interrupt(&self, caller: &str, selected: &ChildJob) -> bool {
        let work = self.work.lock().expect("child work");
        let Some(work) = work.get(&selected.operation) else {
            return false;
        };
        if work.identity.parent.0 != caller
            || work.identity.parent != selected.parent
            || work.identity.child != selected.child
            || work.identity.generation != selected.generation
            || work.identity.location != selected.location
            || work.identity.delivery_id != selected.delivery_id
            || !self
                .db
                .child_job_active(&selected.operation)
                .unwrap_or(false)
        {
            return false;
        }
        if !self.db.child_job_live(&selected.operation).unwrap_or(false) {
            return false;
        }
        work.cancel.store(true, Ordering::Release);
        self.wake.notify_one();
        true
    }

    pub(crate) fn background(&self, caller: &str, selected: &ChildJob) -> bool {
        let work = self.work.lock().expect("child work");
        let Some(work) = work.get(&selected.operation) else {
            return false;
        };
        if work.identity.parent.0 != caller
            || work.identity.parent != selected.parent
            || work.identity.child != selected.child
            || work.identity.generation != selected.generation
            || work.identity.location != selected.location
            || work.identity.delivery_id != selected.delivery_id
        {
            return false;
        }
        if !self
            .db
            .background_child_job(&selected.operation)
            .unwrap_or(false)
        {
            return false;
        }
        work.background.store(true, Ordering::Release);
        work.mode_changed.notify_waiters();
        self.wake.notify_one();
        true
    }

    /// Release only this foreground barrier, retaining the actual shared join.
    pub(super) async fn wait_foreground(
        &self,
        identity: &ChildJob,
        parent_cancel: &AtomicBool,
    ) -> Result<SubagentOutcome, ToolError> {
        let (completion, background, changed, cancel, foreground_wait, foreground_result) = {
            let work = self.work.lock().expect("child work");
            let work = work
                .get(&identity.operation)
                .ok_or_else(|| ToolError::Failed {
                    tool: SUBAGENT_TOOL.into(),
                    reason: "child join unavailable".into(),
                })?;
            (
                work.completion.clone(),
                work.background.clone(),
                work.mode_changed.clone(),
                work.cancel.clone(),
                work.foreground_wait.clone(),
                work.foreground_result.clone(),
            )
        };
        let _release = WaitRelease(foreground_wait);
        let mut completion = Box::pin(completion);
        loop {
            let notified = changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.take_parent_rejection(&identity.parent.0) {
                parent_cancel.store(true, Ordering::Release);
            }
            if parent_cancel.load(Ordering::Acquire) {
                cancel.store(true, Ordering::Release);
                // Cancellation drains actual work even if conversion raced it.
            } else if background.load(Ordering::Acquire) {
                return Ok(SubagentOutcome::Running {
                    session_id: identity.child.0.clone(),
                    operation: identity.operation.clone(),
                    generation: identity.generation,
                    delivery_id: identity.delivery_id.clone(),
                });
            }
            tokio::select! {
                ok = &mut completion => {
                    if self.take_parent_rejection(&identity.parent.0) { parent_cancel.store(true,Ordering::Release); }
                    if !ok { return Err(ToolError::Failed { tool: SUBAGENT_TOOL.into(), reason: "child settlement failed".into() }); }
                    let job = self.db.child_jobs(&identity.parent.0).map_err(|_| ToolError::Failed { tool: SUBAGENT_TOOL.into(), reason: "child state unavailable".into() })?
                        .into_iter().find(|j| j.operation == identity.operation).ok_or_else(|| ToolError::Failed { tool: SUBAGENT_TOOL.into(), reason: "child state unavailable".into() })?;
                    // Conversion won the journal race: return the same running
                    // launch and leave terminal delivery to the notice owner.
                    if job.background && !parent_cancel.load(Ordering::Acquire) {
                        return Ok(SubagentOutcome::Running { session_id: job.child.0, operation: job.operation, generation: job.generation, delivery_id: job.delivery_id });
                    }
                    if let Some(outcome) = foreground_result.lock().expect("child foreground result").take() {
                        return Ok(outcome);
                    }
                    return Ok(match job.state {
                        ChildState::Completed => SubagentOutcome::Completed { session_id: job.child.0, text: job.result.unwrap_or_else(|| SUBAGENT_NO_TEXT.into()) },
                        ChildState::Cancelled => SubagentOutcome::Cancelled { session_id: job.child.0 },
                        _ => SubagentOutcome::Failed { session_id: Some(job.child.0), reason: job.result.unwrap_or_else(|| "child execution failed".into()) },
                    });
                },
                () = &mut notified => {},
                () = tokio::time::sleep(Duration::from_millis(5)) => {},
            }
        }
    }

    /// A same-batch explicit continuation joins execution, not a released UI
    /// barrier. The existing per-child admission lock remains authoritative.
    pub(super) async fn join_child(&self, child: &str, parent_cancel: &AtomicBool) -> bool {
        let work = self
            .work
            .lock()
            .expect("child work")
            .values()
            .find(|w| w.identity.child.0 == child)
            .map(|w| (w.completion.clone(), w.cancel.clone()));
        let Some((completion, cancel)) = work else {
            return true;
        };
        tokio::pin!(completion);
        loop {
            if parent_cancel.load(Ordering::Acquire) {
                cancel.store(true, Ordering::Release);
            }
            tokio::select! {
                ok = &mut completion => return ok,
                () = tokio::time::sleep(Duration::from_millis(5)) => {},
            }
        }
    }

    pub(crate) fn cancel_session(&self, parent: &str) -> bool {
        let mut found = false;
        for work in self.work.lock().expect("child work").values() {
            if work.identity.parent.0 == parent
                || self
                    .db
                    .shell_family_contains(parent, &work.identity.child.0)
                    .unwrap_or(false)
            {
                work.cancel.store(true, Ordering::Release);
                found = true;
            }
        }
        self.wake.notify_one();
        found
    }

    pub(crate) fn deliver(
        &self,
        events: Option<&tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>>,
    ) -> Result<(), StorageError> {
        if self.failed.load(Ordering::Acquire) {
            return Err(StorageError::OperationNotFound);
        }
        for notice in self.db.deliver_child_notices()? {
            if let Some(events) = events {
                let _ = events.send(oc_core::core_app::CoreEvent::ChildNotice(notice));
            }
        }
        Ok(())
    }

    pub(crate) async fn reap(&self) -> Result<(), StorageError> {
        let finished = {
            let work = self.work.lock().expect("child work");
            work.iter()
                .filter(|(_, job)| {
                    !job.foreground_wait.load(Ordering::Acquire)
                        && !job.parent_rejected.load(Ordering::Acquire)
                        && job.completion.clone().now_or_never().is_some()
                })
                .map(|(id, job)| (id.clone(), job.completion.clone(), job.mcp.clone()))
                .collect::<Vec<_>>()
        };
        for (id, completion, owner) in finished {
            if owner.remote_unknown() {
                self.remote_unknown.store(true, Ordering::Release);
            }
            if !completion.await {
                self.failed.store(true, Ordering::Release);
            }
            // Shutdown retains every Work through its source-stop await.
            // Ready joins alone cannot let a concurrent reaper remove it.
            let mut work = self.work.lock().expect("child work");
            if !self.closing.load(Ordering::Acquire) {
                work.remove(&id);
            }
        }
        let retired = {
            let work = self.work.lock().expect("child work");
            self.retired
                .lock()
                .expect("retired child source")
                .iter()
                .filter(|owner| !work.values().any(|job| Arc::ptr_eq(&job.mcp, owner)))
                .cloned()
                .collect::<Vec<_>>()
        };
        for owner in retired {
            if owner.remote_unknown() {
                self.remote_unknown.store(true, Ordering::Release);
            }
            if owner.stop().await.is_err() {
                self.failed.store(true, Ordering::Release);
            }
            if owner.remote_unknown() {
                self.remote_unknown.store(true, Ordering::Release);
            }
            // Dropping this waiter drops a clone, never the owner's receipt.
            self.retired
                .lock()
                .expect("retired child source")
                .retain(|held| !Arc::ptr_eq(held, &owner));
        }
        if self.failed.load(Ordering::Acquire) {
            Err(StorageError::OperationNotFound)
        } else {
            Ok(())
        }
    }

    pub(crate) async fn shutdown(&self) -> Result<(), StorageError> {
        self.closing.store(true, Ordering::Release);
        let work = {
            let work = self.work.lock().expect("child work");
            for job in work.values() {
                job.cancel.store(true, Ordering::Release);
            }
            work.iter()
                .map(|(id, job)| (id.clone(), job.completion.clone(), job.mcp.clone()))
                .collect::<Vec<_>>()
        };
        for (id, completion, owner) in work {
            if !completion.await {
                self.failed.store(true, Ordering::Release);
            }
            if owner.remote_unknown() {
                self.remote_unknown.store(true, Ordering::Release);
            }
            if owner.stop().await.is_err() {
                self.failed.store(true, Ordering::Release);
            }
            if owner.remote_unknown() {
                self.remote_unknown.store(true, Ordering::Release);
            }
            self.work.lock().expect("child work").remove(&id);
        }
        self.reap().await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn launch(
        self: &Arc<Self>,
        runtime: Runtime<'static>,
        parent_turn: String,
        agent: SubagentAgent,
        parent_lane: TurnLane,
        identity: ChildJob,
        prompt: String,
        model: ResolvedModel,
        catalog: ModelCatalog,
        provider: ResponsesConfig,
        attached: mcp::McpGeneration,
        reservation: Reservation,
        fresh: bool,
        resume: Option<TurnLog>,
    ) -> Result<SubagentOutcome, ToolError> {
        self.reap().await.map_err(|_| ToolError::Failed {
            tool: SUBAGENT_TOOL.into(),
            reason: "child worker cleanup failed".into(),
        })?;
        let launched = {
            // Publication and shutdown's snapshot share this existing owner lock.
            // A reservation made before closing cannot publish late owned work.
            let mut work = self.work.lock().expect("child work");
            if self.closing.load(Ordering::Acquire) {
                return Err(ToolError::Failed {
                    tool: SUBAGENT_TOOL.into(),
                    reason: "child owner closing; no prompt admitted".into(),
                });
            }
            if resume.is_none() {
                let mut fence = serde_json::json!({
                    "version":1,"identity":identity,"parent_turn":parent_turn,"lane":parent_lane,
                    "generation_fingerprint":runtime.child_recovery_fingerprint(),
                    "launch_source":self.db.child_launch_fingerprint(&identity.operation).map_err(|_|ToolError::Failed {tool:SUBAGENT_TOOL.into(),reason:"child admission source missing".into()})?,
                    "route":crate::compaction::route_identity(&catalog.provider,&model.id,provider.for_selection(&model.id,model.variant.as_deref())).map_err(|_|ToolError::Failed {tool:SUBAGENT_TOOL.into(),reason:"child route invalid".into()})?,
                });
                fence["fingerprint"] = crate::compaction::fingerprint(&fence).into();
                self.db
                    .admit_recoverable_child_job(&identity, fresh, &fence)
                    .map_err(|_| ToolError::Failed {
                        tool: SUBAGENT_TOOL.into(),
                        reason: "child admission failed".into(),
                    })?;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let token = cancel.clone();
            let background = Arc::new(AtomicBool::new(identity.background));
            let mode_changed = Arc::new(tokio::sync::Notify::new());
            let foreground_wait = Arc::new(AtomicBool::new(!identity.background));
            let waiting = foreground_wait.clone();
            let foreground_result = Arc::new(Mutex::new(None));
            let actual_result = foreground_result.clone();
            let db = self.db.shared_handle();
            let id = identity.clone();
            let wake = self.wake.clone();
            let failed = self.failed.clone();
            let (started, launched) = tokio::sync::oneshot::channel();
            let runtime = Arc::new(runtime);
            let mcp = runtime.mcp_owner();
            let child_runtime = runtime.clone();
            // The captured request future is large; transfer a heap-pinned
            // future rather than moving it through spawn's stack frames.
            let task = tokio::spawn(Box::pin(async move {
                let _reservation = reservation;
                // Poll the actual child future before releasing the launch waiter.
                // Its first physical request is fenced by the ordinary durable
                // child acceptance callback, not an invented running callback.
                let result = child_runtime
                    .run_child_turn(
                        &agent,
                        &parent_lane,
                        &id.operation,
                        &id.child.0,
                        prompt,
                        &model,
                        0,
                        &catalog,
                        &provider,
                        &attached,
                        &token,
                        Some(started),
                        resume,
                    )
                    .await;
                let (state, result) = match result {
                    Ok(report) if report.status == TurnStatus::Completed => {
                        let text = if report.text.is_empty() {
                            SUBAGENT_NO_TEXT.into()
                        } else {
                            report.text
                        };
                        let text = if report.warnings.is_empty() {
                            text
                        } else {
                            format!("Warning: {}\n\n{text}", report.warnings.join("\nWarning: "))
                        };
                        (ChildState::Completed, text)
                    }
                    Ok(report) if report.status == TurnStatus::Cancelled => {
                        (ChildState::Cancelled, "Subagent cancelled".into())
                    }
                    Ok(report) => {
                        let reason = report
                            .diagnostic
                            .unwrap_or_else(|| "child execution did not complete".into());
                        let reason = if report.warnings.is_empty() {
                            reason
                        } else {
                            format!(
                                "Warning: {}\n\n{reason}",
                                report.warnings.join("\nWarning: ")
                            )
                        };
                        (ChildState::Error, reason)
                    }
                    Err(_) => (ChildState::Error, "child execution failed".into()),
                };
                let ok = db.finish_child_job(&id.operation, state, &result).is_ok();
                if waiting.load(Ordering::Acquire) {
                    *actual_result.lock().expect("child foreground result") = Some(match state {
                        ChildState::Completed => SubagentOutcome::Completed {
                            session_id: id.child.0.clone(),
                            text: result,
                        },
                        ChildState::Cancelled => SubagentOutcome::Cancelled {
                            session_id: id.child.0.clone(),
                        },
                        _ => SubagentOutcome::Failed {
                            session_id: Some(id.child.0.clone()),
                            reason: result,
                        },
                    });
                }
                if !ok {
                    failed.store(true, Ordering::Release);
                }
                wake.notify_one();
                ok
            }));
            let completion = async move { task.await.unwrap_or(false) }.boxed().shared();
            work.insert(
                identity.operation.clone(),
                Work {
                    identity: identity.clone(),
                    parent_turn,
                    runtime: Arc::downgrade(&runtime),
                    mcp,
                    cancel,
                    completion,
                    background,
                    mode_changed,
                    foreground_wait,
                    parent_rejected: AtomicBool::new(false),
                    foreground_result,
                },
            );
            launched
        };
        if launched.await.is_err() {
            if let Some(work) = self
                .work
                .lock()
                .expect("child work")
                .get(&identity.operation)
            {
                work.foreground_wait.store(false, Ordering::Release);
            }
            return Err(ToolError::Failed {
                tool: SUBAGENT_TOOL.into(),
                reason: "child launch failed; inspect child state".into(),
            });
        }
        Ok(SubagentOutcome::Running {
            session_id: identity.child.0,
            operation: identity.operation,
            generation: identity.generation,
            delivery_id: identity.delivery_id,
        })
    }

    pub(super) fn source_for(
        &self,
        binding: &oc_core::approval::ApprovalBinding,
    ) -> Option<Arc<Runtime<'static>>> {
        self.work
            .lock()
            .expect("child work")
            .values()
            .find(|job| {
                job.identity.location == binding.location
                    && job.identity.generation == binding.generation
                    && self
                        .db
                        .shell_family_contains(&job.identity.child.0, &binding.session)
                        .unwrap_or(false)
            })
            .and_then(|job| job.runtime.upgrade())
    }

    fn retain_source(&self, owner: Arc<mcp::McpOwner>) -> bool {
        if !self
            .work
            .lock()
            .expect("child work")
            .values()
            .any(|job| Arc::ptr_eq(&job.mcp, &owner))
        {
            return false;
        }
        let mut retired = self.retired.lock().expect("retired child source");
        if !retired.iter().any(|held| Arc::ptr_eq(held, &owner)) {
            retired.push(owner);
        }
        true
    }

    pub(super) fn remote_unknown(&self) -> bool {
        self.remote_unknown.load(Ordering::Acquire)
            || self
                .work
                .lock()
                .expect("child work")
                .values()
                .any(|job| job.mcp.remote_unknown())
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        for work in self.work.get_mut().expect("child work").values() {
            work.cancel.store(true, Ordering::Release);
        }
    }
}

impl Runtime<'_> {
    /// Only digests leave the product configuration owner; no credentials are
    /// copied into the journal. Captured parent authority is checked separately.
    fn child_recovery_fingerprint(&self) -> String {
        let current = self.current.read().expect("generation");
        let workspace = self.workspace.read().expect("workspace");
        crate::compaction::fingerprint(&(
            &self.location,
            &current.config.providers,
            &current.config.mcp,
            &current.config.permissions,
            &current.config.permission_rules,
            &current.config.provenance,
            &workspace.subagents,
            &workspace.instructions,
            &workspace.skills_projection,
            &workspace.command_digest,
            format!("{:?}", *self.dcp_config.read().expect("dcp")),
        ))
    }

    /// Startup-only, bounded inspection through the same application lineage.
    /// Viewing history, changing tabs and cached request retries never call this.
    pub(crate) async fn recover_background_children(
        &self,
        catalog: &ModelCatalog,
        provider: &ResponsesConfig,
    ) -> Result<(), RuntimeError> {
        for identity in self.db.child_recovery_candidates()? {
            let recovered = async {
                if !identity.background
                    || identity.location != self.location
                    || identity.generation != self.generation_id()
                {
                    return Err(RuntimeError::Storage);
                }
                let mut fence = self.db.child_recovery_fence(&identity)?;
                let fingerprint = fence
                    .as_object_mut()
                    .ok_or(RuntimeError::Storage)?
                    .remove("fingerprint")
                    .ok_or(RuntimeError::Storage)?;
                if fingerprint != crate::compaction::fingerprint(&fence) {
                    return Err(RuntimeError::Storage);
                }
                if fence["launch_source"]
                    != self.db.child_launch_fingerprint(&identity.operation)?
                {
                    return Err(RuntimeError::Storage);
                }
                let saved: ChildJob = serde_json::from_value(fence["identity"].clone())
                    .map_err(|_| RuntimeError::Storage)?;
                let mut expected = identity.clone();
                expected.state = ChildState::Admitted;
                expected.turn = None;
                expected.background = saved.background;
                if saved != expected
                    || fence["version"] != 1
                    || fence["generation_fingerprint"] != self.child_recovery_fingerprint()
                {
                    return Err(RuntimeError::Storage);
                }
                let parent_lane: TurnLane = serde_json::from_value(fence["lane"].clone())
                    .map_err(|_| RuntimeError::Storage)?;
                let workspace = self.workspace.read().expect("workspace").clone();
                let command_primary = self.command_child_receipt(&fence, &identity)?;
                let agent = workspace
                    .subagents
                    .as_ref()
                    .and_then(|c| c.agents.get(&identity.agent))
                    .filter(|a| !a.primary || command_primary)
                    .cloned()
                    .ok_or(RuntimeError::Storage)?;
                let (provider_id, reference) = identity
                    .model
                    .split_once('/')
                    .ok_or(RuntimeError::Storage)?;
                if provider_id != catalog.provider {
                    return Err(RuntimeError::Storage);
                }
                let (id, variant) = reference
                    .split_once('#')
                    .map_or((reference, None), |(id, v)| (id, Some(v.to_owned())));
                let model = ResolvedModel {
                    id: id.into(),
                    variant,
                };
                self.admit_provider_variant(
                    catalog,
                    &model.id,
                    model.variant.as_deref(),
                    provider,
                )?;
                let base =
                    models::select_model(catalog, &model.id).map_err(|_| RuntimeError::Storage)?;
                models::select_variant(&base, model.variant.as_deref())
                    .map_err(|_| RuntimeError::Storage)?;
                if fence["route"]
                    != crate::compaction::route_identity(
                        provider_id,
                        &model.id,
                        provider.for_selection(&model.id, model.variant.as_deref()),
                    )
                    .map_err(|_| RuntimeError::Provider)?
                {
                    return Err(RuntimeError::Storage);
                }
                let turn = identity.turn.as_deref().ok_or(RuntimeError::Storage)?;
                let (state, checkpoint) = self.db.turn_result(turn)?;
                let checkpoint = checkpoint.ok_or(RuntimeError::Storage)?;
                let log = TurnLog::from_json(
                    &serde_json::from_str(&checkpoint).map_err(|_| RuntimeError::Storage)?,
                )
                .map_err(|_| RuntimeError::Storage)?;
                if state != "started"
                    || log.turn_id != turn
                    || log.model != model.id
                    || log.provider != provider_id
                    || log.agent_digest != agent.digest
                    || log.display["location"] != identity.location
                    || log.display["config_generation"] != identity.generation
                    || log.display["owning_operation"] != identity.operation
                    || log.display["agent"] != identity.agent
                    || log.user_message.is_none()
                {
                    return Err(RuntimeError::Storage);
                }
                let parent_turn = fence["parent_turn"]
                    .as_str()
                    .ok_or(RuntimeError::Storage)?
                    .to_owned();
                let reservation = self.child_jobs.reserve_resume(&identity)?;
                self.db.claim_child_resume(&identity, &checkpoint)?;
                let attached = self.mcp_owner().request_view()?.as_ref().clone();
                self.child_jobs
                    .launch(
                        self.owned_child_snapshot(),
                        parent_turn,
                        agent,
                        parent_lane,
                        identity.clone(),
                        String::new(),
                        model,
                        catalog.clone(),
                        provider.clone(),
                        attached,
                        reservation,
                        false,
                        Some(log),
                    )
                    .await
                    .map_err(|_| RuntimeError::Storage)?;
                Ok::<_, RuntimeError>(())
            }
            .await;
            if recovered.is_err() {
                self.db.finish_child_job(&identity.operation,ChildState::Unknown,"Child recovery refused: identity/configuration or committed safety unavailable; explicit recovery required; not replayed")?;
                if let Some(turn) = &identity.turn {
                    let (_, checkpoint) = self.db.turn_result(turn)?;
                    self.db
                        .finish_turn(turn, "unknown", checkpoint.as_deref())?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn inherit_application_owners(&mut self, source: &Runtime<'_>) {
        self.shell_jobs = source.shell_jobs.clone();
        self.child_jobs = source.child_jobs.clone();
        self.approvals = source.approvals.clone();
        self.questions = source.questions.clone();
    }

    pub(crate) async fn retire_or_shutdown_mcp(&self) -> Result<(), RuntimeError> {
        if self.child_jobs.retain_source(self.mcp_owner()) {
            Ok(())
        } else {
            self.shutdown_mcp().await
        }
    }
    /// Freeze this lane, retaining the SAME storage/resource ownership lineage.
    /// No new database, MCP connection, catalog discovery or authority grants.
    pub(super) fn owned_child_snapshot(&self) -> Runtime<'static> {
        Runtime {
            prepared_moves: Mutex::default(),
            shell_jobs: self.shell_jobs.clone(),
            child_jobs: self.child_jobs.clone(),
            approvals: self.approvals.clone(),
            questions: self.questions.clone(),
            compactions: Mutex::default(),
            compaction_events: Mutex::new(self.compaction_events.lock().expect("events").clone()),
            native_compaction: RwLock::new(
                self.native_compaction.read().expect("compaction").clone(),
            ),
            db: Database::Owned(self.db.shared_handle()),
            location: self.location.clone(),
            current: RwLock::new(self.current.read().expect("generation").clone()),
            provider_state: RwLock::new(self.provider_state.read().expect("provider").clone()),
            active: AtomicBool::new(false),
            protected: self.protected.clone(),
            files: self.files.clone(),
            shell: self.shell.clone(),
            parent_env: self.parent_env.clone(),
            roots: self.roots.clone(),
            webfetch_auth: self.webfetch_auth.clone(),
            webfetch_allow_private: self.webfetch_allow_private,
            dcp_config: RwLock::new(self.dcp_config.read().expect("dcp").clone()),
            dcp_protected: RwLock::new(self.dcp_protected.read().expect("dcp protection").clone()),
            workspace: RwLock::new(self.workspace.read().expect("workspace").clone()),
            nudge_state: Mutex::default(),
            compression_commit: self.compression_commit.clone(),
            stats: Mutex::new(DcpStats::default()),
            mcp_generation: RwLock::new(self.mcp_generation.read().expect("mcp").clone()),
            mcp_activation: RwLock::new(
                self.mcp_activation.read().expect("mcp activation").clone(),
            ),
            mcp_unsafe_retry: AtomicBool::new(self.mcp_unsafe_retry.load(Ordering::Acquire)),
            mcp_cleanup_failed: AtomicBool::new(self.mcp_cleanup_failed.load(Ordering::Acquire)),
            subagent_seq: AtomicU64::new(0),
        }
    }
}
