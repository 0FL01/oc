//! Application-owned Code Mode programs and their original foreground waiters.
use super::*;
use crate::storage::{ParentNoticeState, StorageError};
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use serde::{Deserialize, Serialize};

pub(crate) const ACTIVE_JOB_CAP: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    pub version: u8,
    pub session: String,
    pub turn: String,
    pub operation: String,
    pub location: String,
    pub generation: u64,
    pub output_limits: crate::config::ToolOutputLimits,
    pub output_source: String,
    pub agent: Option<String>,
    pub agent_digest: Option<String>,
    pub model: String,
    pub provider: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Outcome {
    pub version: u8,
    pub state: String,
    pub output: String,
    pub presentation: Option<Box<Presentation>>,
}

impl Outcome {
    pub(crate) fn unknown(message: &str) -> Self {
        Self {
            version: 1,
            state: "unknown".into(),
            output: message.into(),
            presentation: None,
        }
    }
    pub(in crate::runtime) fn tool_result(&self) -> (&'static str, String) {
        let state = match self.state.as_str() {
            "completed" => "completed",
            "failed" => "failed",
            "cancelled" => "cancelled",
            _ => "unknown",
        };
        (state, self.output.clone())
    }
}

/// Everything borrowed by the ordinary execution path is frozen at admission.
pub(in crate::runtime) struct Capture {
    issuing: ExecutionGeneration,
    call: crate::tools::ToolCall,
    attached: McpGeneration,
    permissions: BTreeMap<String, Permission>,
    rules: Option<crate::permissions::PermissionRules>,
    root: Option<std::path::PathBuf>,
    mcp: Vec<mcp_remote::RegistryEntry>,
    permit: Option<InvocationPermit>,
    files: crate::files::Files,
    skills: SkillSnapshot,
    webfetch_auth: Option<String>,
    webfetch_allow_private: bool,
    roots: Option<ToolRoots>,
    secrets: Vec<String>,
}

impl Capture {
    pub(in crate::runtime) fn new(
        issuing: &ExecutionGeneration,
        call: &crate::tools::ToolCall,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        attached: &McpGeneration,
        secrets: Vec<String>,
    ) -> Self {
        Self {
            issuing: issuing.clone(),
            call: call.clone(),
            attached: attached.clone(),
            permissions: policy.permissions.clone(),
            rules: policy.rules.cloned(),
            root: policy.root.map(std::path::Path::to_path_buf),
            mcp: policy.mcp.to_vec(),
            permit: policy.permit.clone(),
            files: ctx.files.clone(),
            skills: ctx.snapshot.clone(),
            webfetch_auth: ctx.webfetch_auth.clone(),
            webfetch_allow_private: ctx.webfetch_allow_private,
            roots: ctx.roots.clone(),
            secrets,
        }
    }
}

#[derive(Default)]
struct Control {
    background: bool,
    finished: bool,
    wait_done: bool,
}

struct Work {
    provenance: Provenance,
    _slot: tokio::sync::OwnedSemaphorePermit,
    cancel: Arc<AtomicBool>,
    control: Arc<Mutex<Control>>,
    converted: Arc<tokio::sync::Notify>,
    progress: Arc<Mutex<Option<Box<Presentation>>>>,
    changes: tokio::sync::watch::Sender<u64>,
    completion: Shared<BoxFuture<'static, bool>>,
    runtime: std::sync::Weak<Runtime<'static>>,
    parent_wake: Option<ParentWake>,
}

struct ParentWake {
    session: String,
    origin: Arc<origin::Origin>,
    _work: origin::Work,
}

pub(crate) struct ParentBatch {
    pub(crate) parent: String,
    pub(crate) origin: Arc<origin::Origin>,
    pub(crate) messages: Vec<String>,
    operations: Vec<String>,
}

pub(crate) struct Jobs {
    db: Db,
    slots: Arc<tokio::sync::Semaphore>,
    work: Mutex<BTreeMap<String, Work>>,
    pending: Mutex<BTreeMap<String, ParentWake>>,
    wake: Arc<tokio::sync::Notify>,
    failed: AtomicBool,
}

impl Jobs {
    pub(crate) fn notify(&self) {
        self.wake.notify_one();
    }

    pub(crate) fn new(db: &Db) -> Arc<Self> {
        Arc::new(Self {
            db: db.shared_handle(),
            slots: Arc::new(tokio::sync::Semaphore::new(ACTIVE_JOB_CAP)),
            work: Mutex::default(),
            pending: Mutex::default(),
            wake: Arc::default(),
            failed: AtomicBool::new(false),
        })
    }

    pub(crate) fn reserve(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        let pending = self.pending.lock().expect("execute handoffs");
        if self.slots.available_permits() <= pending.len() {
            return None;
        }
        self.slots.clone().try_acquire_owned().ok()
    }

    pub(in crate::runtime) fn launch(
        self: &Arc<Self>,
        runtime: Runtime<'static>,
        p: Provenance,
        capture: Capture,
        slot: tokio::sync::OwnedSemaphorePermit,
        background: bool,
    ) -> Result<String, StorageError> {
        let parent_wake = if self.db.session_meta(&p.session)?.parent_id.is_none() {
            runtime
                .native_origin
                .as_ref()
                .filter(|o| o.location().is_some())
                .map(|origin| ParentWake {
                    session: p.session.clone(),
                    origin: origin.clone(),
                    _work: origin.work(),
                })
        } else {
            None
        };
        self.db.admit_execute_job(&p, background)?;
        self.db.start_execute_job(&p.operation)?;
        let id = p.operation.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let control = Arc::new(Mutex::new(Control {
            background,
            finished: false,
            wait_done: background,
        }));
        let converted = Arc::new(tokio::sync::Notify::new());
        let progress = Arc::new(Mutex::new(None));
        let (changes, _) = tokio::sync::watch::channel(0u64);
        let (gate, start) = tokio::sync::oneshot::channel::<()>();
        let runtime = Arc::new(runtime);
        let runtime_weak = Arc::downgrade(&runtime);
        let mcp_operation = runtime.mcp_owner().operation();
        let origin_work = runtime.native_origin.as_ref().map(|origin| origin.work());
        let child_jobs = runtime.child_jobs.clone();
        let task_db = self.db.shared_handle();
        let task_p = p.clone();
        let task_cancel = cancel.clone();
        let task_control = control.clone();
        let task_progress = progress.clone();
        let task_changes = changes.clone();
        let wake = self.wake.clone();
        let task = tokio::spawn(async move {
            let _origin_work = origin_work;
            if start.await.is_err() {
                return false;
            }
            let policy = RuntimePolicy {
                permissions: &capture.permissions,
                rules: capture.rules.as_ref(),
                root: capture.root.as_deref(),
                mcp: &capture.mcp,
                permit: capture.permit.clone(),
            };
            let ctx = ToolContext {
                files: &capture.files,
                shell: &runtime.shell,
                parent_env: &runtime.parent_env,
                webfetch_auth: capture.webfetch_auth.clone(),
                webfetch_allow_private: capture.webfetch_allow_private,
                policy: &policy,
                subagent: None,
                snapshot: &capture.skills,
                cancel: &task_cancel,
                roots: capture.roots.clone(),
            };
            let mut event = |_: &str, event: &ToolCallEvent| {
                if let ToolCallEvent::Progress {
                    output_presentation,
                    ..
                } = event
                {
                    *task_progress.lock().expect("execute progress") =
                        Some(output_presentation.clone());
                    task_changes.send_modify(|revision| *revision = revision.wrapping_add(1));
                    wake.notify_one();
                }
            };
            let (state, output, mut facts) = {
                let execution = runtime.execute_code_mode(
                    &capture.issuing,
                    &capture.call,
                    &task_p.session,
                    &task_p.turn,
                    &task_p.operation,
                    &ctx,
                    &policy,
                    &capture.attached,
                    &task_cancel,
                    task_p.agent.clone(),
                    task_p.agent_digest.clone(),
                    &capture.secrets,
                    &mut event,
                );
                tokio::pin!(execution);
                tokio::select! {
                    biased;
                    () = async {
                        match &runtime.native_origin {
                            Some(origin) => origin.cancelled().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        task_cancel.store(true, Ordering::Release);
                        execution.await
                    }
                    result = &mut execution => result,
                }
            };
            let prepared = crate::tools::output::Context {
                db: &task_db,
                operation: &task_p.operation,
                session: &task_p.session,
                location: &task_p.location,
                generation: task_p.generation,
                source: &task_p.output_source,
                limits: task_p.output_limits,
                secrets: capture.secrets.clone(),
            }
            .prepare_envelope(output, false, None);
            let state = if prepared.logging_failed && state != "unknown" {
                "failed"
            } else {
                state
            };
            let mut locked = task_control.lock().expect("execute control");
            facts.background = locked.background;
            facts.state = Some(state.into());
            facts.error |= state != "completed";
            let mut presentation = prepared.presentation.unwrap_or_else(|| {
                Box::new(Presentation::new(
                    &prepared.text,
                    prepared.text.len() as u64,
                    false,
                ))
            });
            presentation.label_only = false;
            presentation.execute = Some(facts);
            let presentation = Db::bounded_mcp_presentation(&task_p.operation, Some(presentation));
            let outcome = Outcome {
                version: 1,
                state: state.into(),
                output: prepared.text,
                presentation,
            };
            let result = task_db
                .finish_execute_job(&task_p.operation, &outcome)
                .is_ok();
            locked.finished = true;
            drop(locked);
            drop(mcp_operation);
            child_jobs.notify();
            task_changes.send_modify(|revision| *revision = revision.wrapping_add(1));
            wake.notify_one();
            result
        });
        self.work.lock().expect("execute jobs").insert(
            id.clone(),
            Work {
                provenance: p,
                _slot: slot,
                cancel,
                control,
                converted,
                progress,
                changes,
                completion: async move { task.await.unwrap_or(false) }.boxed().shared(),
                runtime: runtime_weak,
                parent_wake,
            },
        );
        let _ = gate.send(());
        self.wake.notify_one();
        Ok(id)
    }

    pub(crate) fn running(
        &self,
        caller: &str,
    ) -> Result<Vec<oc_core::queries::ExecuteJob>, StorageError> {
        self.db.shell_family_contains(caller, caller)?;
        let work = self.work.lock().expect("execute jobs");
        let mut rows = Vec::new();
        for (id, job) in work.iter() {
            if !self
                .db
                .shell_family_contains(caller, &job.provenance.session)?
            {
                continue;
            }
            let control = job.control.lock().expect("execute control");
            if control.finished {
                continue;
            }
            let mut identity = self.db.execute_job_identity(&job.provenance.session, id)?;
            identity.background = control.background;
            identity.output = job.progress.lock().expect("execute progress").clone();
            if let Some(facts) = identity.output.as_mut().and_then(|p| p.execute.as_mut()) {
                facts.background = control.background;
                facts.state = Some("running".into());
            }
            rows.push(identity);
        }
        Ok(rows)
    }

    pub(crate) fn snapshot(
        &self,
        session: &str,
        id: &str,
    ) -> Result<oc_core::queries::ExecuteSnapshot, StorageError> {
        let mut job = self.db.execute_job_identity(session, id)?;
        let phase = self.db.execute_job_phase(session, id)?;
        let (state, output) = if phase == "terminal" {
            let outcome = self.db.execute_job_outcome(session, id)?;
            (outcome.state, outcome.presentation)
        } else {
            let work = self.work.lock().expect("execute jobs");
            let active = work
                .get(id)
                .filter(|job| job.provenance.session == session)
                .ok_or(StorageError::OperationNotFound)?;
            let control = active.control.lock().expect("execute control");
            let mut output = active.progress.lock().expect("execute progress").clone();
            if let Some(facts) = output.as_mut().and_then(|p| p.execute.as_mut()) {
                facts.background = control.background;
                facts.state = Some("running".into());
            }
            job.background = control.background;
            ("running".into(), output)
        };
        let (delivery_id, message_id) = self.db.execute_delivery_identity(session, id)?;
        Ok(oc_core::queries::ExecuteSnapshot {
            job,
            state,
            output,
            delivery_id,
            message_id,
        })
    }

    pub(crate) fn background(
        &self,
        session: &str,
        turn: &str,
        id: &str,
    ) -> Result<bool, StorageError> {
        let work = self.work.lock().expect("execute jobs");
        let Some(job) = work
            .get(id)
            .filter(|job| job.provenance.session == session && job.provenance.turn == turn)
        else {
            return Ok(false);
        };
        let mut control = job.control.lock().expect("execute control");
        if control.finished || job.cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        if control.background {
            return Ok(true);
        }
        if !self.db.background_execute_job(session, id)? {
            return Ok(false);
        }
        control.background = true;
        job.converted.notify_one();
        self.wake.notify_one();
        Ok(true)
    }

    pub(in crate::runtime) async fn foreground(
        &self,
        session: &str,
        id: &str,
        parent_cancel: &AtomicBool,
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) -> Result<Option<Outcome>, StorageError> {
        let (completion, control, converted, cancel, mut changes, progress, turn) = {
            let work = self.work.lock().expect("execute jobs");
            let job = work
                .get(id)
                .filter(|job| job.provenance.session == session)
                .ok_or(StorageError::OperationNotFound)?;
            (
                job.completion.clone(),
                job.control.clone(),
                job.converted.clone(),
                job.cancel.clone(),
                job.changes.subscribe(),
                job.progress.clone(),
                job.provenance.turn.clone(),
            )
        };
        struct WaitGuard(
            Arc<AtomicBool>,
            Arc<Mutex<Control>>,
            Arc<tokio::sync::Notify>,
            bool,
        );
        impl Drop for WaitGuard {
            fn drop(&mut self) {
                let mut control = self.1.lock().expect("execute control");
                control.wait_done = true;
                if !self.3 {
                    self.0.store(true, Ordering::Release);
                }
                self.2.notify_one();
            }
        }
        let mut guard = WaitGuard(cancel.clone(), control.clone(), self.wake.clone(), false);
        loop {
            {
                let control = control.lock().expect("execute control");
                if parent_cancel.load(Ordering::Acquire) {
                    cancel.store(true, Ordering::Release);
                }
                // An accepted conversion owns the result channel even if the
                // worker finishes or is cancelled before this waiter resumes.
                if control.background {
                    guard.3 = true;
                    return Ok(None);
                }
            }
            tokio::select! {
                good = completion.clone() => {
                    guard.3 = true;
                    let detached = control.lock().expect("execute control").background;
                    if detached {
                        return Ok(None);
                    }
                    if !good { return Err(StorageError::OperationNotFound); }
                    let outcome = self.db.execute_job_outcome(session, id).map(Some);
                    drop(guard);
                    // A standalone Runtime has no application pump. Settled
                    // foreground work needs no handoff and releases its slot here.
                    self.work.lock().expect("execute jobs").remove(id);
                    return outcome;
                }
                _ = changes.changed() => {
                    if let Some(output_presentation) = progress.lock().expect("execute progress").clone() {
                        tool_event(&turn, &ToolCallEvent::Progress { op: id.into(), output_presentation });
                    }
                }
                () = converted.notified() => {},
                () = tokio::time::sleep(Duration::from_millis(5)) => {
                    if parent_cancel.load(Ordering::Acquire) { cancel.store(true, Ordering::Release); }
                }
            }
        }
    }

    pub(in crate::runtime) fn source_for(
        &self,
        binding: &oc_core::approval::ApprovalBinding,
    ) -> Option<Arc<Runtime<'static>>> {
        self.work
            .lock()
            .expect("execute jobs")
            .get(&binding.operation)
            .and_then(|job| {
                if job.provenance.session != binding.session
                    || job.provenance.turn != binding.turn
                    || job.provenance.location != binding.location
                    || job.provenance.generation != binding.generation
                {
                    return None;
                }
                let runtime = job.runtime.upgrade()?;
                runtime
                    .approvals
                    .pending()
                    .iter()
                    .any(|request| request.binding == *binding)
                    .then_some(runtime)
            })
    }

    pub(in crate::runtime) fn pending_approvals(&self) -> Vec<oc_core::approval::ApprovalRequest> {
        self.work
            .lock()
            .expect("execute jobs")
            .values()
            .filter_map(|job| job.runtime.upgrade())
            .flat_map(|runtime| runtime.approvals.pending())
            .collect()
    }

    pub(in crate::runtime) fn reload_approvals(
        &self,
        location: &str,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) {
        for runtime in self
            .work
            .lock()
            .expect("execute jobs")
            .values()
            .filter(|job| job.provenance.location == location)
            .filter_map(|job| job.runtime.upgrade())
        {
            runtime.approvals.reload(None, events);
        }
    }

    pub(crate) fn cancel_session(&self, session: &str) -> bool {
        let mut work = self.work.lock().expect("execute jobs");
        let mut pending = self.pending.lock().expect("execute handoffs");
        let before = pending.len();
        pending.retain(|_, wake| wake.session != session);
        let mut cancelled = before != pending.len();
        for job in work
            .values_mut()
            .filter(|job| job.provenance.session == session)
        {
            cancelled |= job.parent_wake.take().is_some();
            if !job.control.lock().expect("execute control").finished {
                job.cancel.store(true, Ordering::Release);
                cancelled = true;
            }
        }
        if cancelled {
            self.wake.notify_one();
        }
        cancelled
    }

    pub(crate) fn suppress_origin(&self, origin: u64) {
        let mut work = self.work.lock().expect("execute jobs");
        for job in work.values_mut() {
            if job
                .parent_wake
                .as_ref()
                .is_some_and(|wake| wake.origin.id == origin)
            {
                job.parent_wake = None;
            }
        }
        self.pending
            .lock()
            .expect("execute handoffs")
            .retain(|_, wake| wake.origin.id != origin);
        self.wake.notify_one();
    }

    pub(crate) fn suppress_wakes(&self) {
        for job in self.work.lock().expect("execute jobs").values_mut() {
            job.parent_wake = None;
        }
        self.pending.lock().expect("execute handoffs").clear();
        self.wake.notify_one();
    }

    pub(crate) fn finish_batch(&self, batch: &ParentBatch) {
        let mut pending = self.pending.lock().expect("execute handoffs");
        for operation in &batch.operations {
            if pending
                .get(operation)
                .is_some_and(|wake| wake.origin.id == batch.origin.id)
            {
                pending.remove(operation);
            }
        }
        self.wake.notify_one();
    }

    pub(crate) fn batch_current(&self, batch: &ParentBatch) -> Result<bool, StorageError> {
        let pending = self.pending.lock().expect("execute handoffs");
        if batch.origin.flag.load(Ordering::Acquire) {
            return Ok(false);
        }
        for (operation, message) in batch.operations.iter().zip(&batch.messages) {
            if pending
                .get(operation)
                .is_none_or(|wake| wake.origin.id != batch.origin.id)
                || self.db.parent_notice_state(&batch.parent, message)? != ParentNoticeState::Ready
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn parent_batches(&self) -> Result<Vec<ParentBatch>, StorageError> {
        let mut pending = self.pending.lock().expect("execute handoffs");
        let mut groups: BTreeMap<(String, u64), ParentBatch> = BTreeMap::new();
        let mut retired = Vec::new();
        for (operation, wake) in pending.iter() {
            let message = self.db.execute_notice_message(&wake.session, operation)?;
            let state = message
                .as_ref()
                .map(|id| self.db.parent_notice_state(&wake.session, id))
                .transpose()?;
            if wake.origin.flag.load(Ordering::Acquire) || state == Some(ParentNoticeState::Retired)
            {
                retired.push(operation.clone());
                continue;
            }
            if state == Some(ParentNoticeState::Held) {
                continue;
            }
            let Some(message) = message else {
                continue;
            };
            let batch = groups
                .entry((wake.session.clone(), wake.origin.id))
                .or_insert_with(|| ParentBatch {
                    parent: wake.session.clone(),
                    origin: wake.origin.clone(),
                    messages: Vec::new(),
                    operations: Vec::new(),
                });
            batch.messages.push(message);
            batch.operations.push(operation.clone());
        }
        for id in retired {
            pending.remove(&id);
        }
        Ok(groups.into_values().collect())
    }

    pub(crate) async fn changed(&self) {
        let joins = self
            .work
            .lock()
            .expect("execute jobs")
            .values()
            .filter(|job| {
                let state = job.control.lock().expect("execute control");
                !state.finished || state.wait_done
            })
            .map(|job| job.completion.clone())
            .collect::<Vec<_>>();
        if joins.is_empty() {
            self.wake.notified().await;
        } else {
            tokio::select! { () = self.wake.notified() => {}, _ = futures_util::future::select_all(joins) => {} }
        }
    }

    pub(crate) fn deliver(
        &self,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) -> Result<(), StorageError> {
        let completed = {
            let mut work = self.work.lock().expect("execute jobs");
            let mut pending = self.pending.lock().expect("execute handoffs");
            let ids = work
                .iter()
                .filter(|(_, job)| {
                    job.control.lock().expect("execute control").wait_done
                        && job.completion.clone().now_or_never().is_some()
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .map(|id| {
                    let mut job = work.remove(&id).expect("finished execute");
                    if job.control.lock().expect("execute control").background
                        && let Some(wake) = job.parent_wake.take()
                    {
                        pending.insert(id.clone(), wake);
                    }
                    (id, job)
                })
                .collect::<Vec<_>>()
        };
        let sessions = {
            let work = self.work.lock().expect("execute jobs");
            work.values()
                .map(|job| job.provenance.session.clone())
                .chain(
                    completed
                        .iter()
                        .map(|(_, job)| job.provenance.session.clone()),
                )
                .collect::<std::collections::BTreeSet<_>>()
        };
        for (id, job) in completed {
            if job.completion.peek() != Some(&true) {
                self.failed.store(true, Ordering::Release);
                self.db.finish_execute_job(
                    &id,
                    &Outcome::unknown("execute worker interrupted; effects unknown; not replayed"),
                )?;
            }
        }
        for notice in self.db.deliver_due_execute_notices()? {
            let _ = events.send(oc_core::core_app::CoreEvent::ExecuteNotice(notice));
        }
        for session in sessions {
            let _ = events.send(oc_core::core_app::CoreEvent::ExecuteChanged {
                session: oc_core::domain::SessionId(session),
            });
        }
        Ok(())
    }

    pub(crate) async fn shutdown(&self) -> Result<(), StorageError> {
        self.suppress_wakes();
        let tasks = {
            let work = self.work.lock().expect("execute jobs");
            for job in work.values() {
                job.cancel.store(true, Ordering::Release);
            }
            work.iter()
                .map(|(id, job)| (id.clone(), job.completion.clone()))
                .collect::<Vec<_>>()
        };
        for (id, completion) in tasks {
            if !completion.await {
                self.failed.store(true, Ordering::Release);
                self.db.finish_execute_job(
                    &id,
                    &Outcome::unknown("execute cleanup failed; effects unknown; not replayed"),
                )?;
            }
            self.work.lock().expect("execute jobs").remove(&id);
        }
        if self.failed.load(Ordering::Acquire) {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        for job in self.work.get_mut().expect("execute jobs").values() {
            job.cancel.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_accepted_conversion_returns_receipt_even_if_completion_precedes_waiter() {
        accepted_conversion_receipt(false).await;
    }

    #[tokio::test]
    async fn execute_accepted_conversion_returns_receipt_after_session_cancellation() {
        accepted_conversion_receipt(true).await;
    }

    async fn accepted_conversion_receipt(cancel_after_conversion: bool) {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "task").unwrap();
        db.record_tool_intent("op", "s", Some("t"), "execute", "{\"code\":\"x\"}")
            .unwrap();
        let provenance = Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: "/original".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "m".into(),
            provider: "p".into(),
        };
        db.admit_execute_job(&provenance, false).unwrap();
        db.start_execute_job("op").unwrap();
        let jobs = Jobs::new(&db);
        let control = Arc::new(Mutex::new(Control::default()));
        let (done, completion) = tokio::sync::oneshot::channel();
        let (changes, _) = tokio::sync::watch::channel(0);
        jobs.work.lock().unwrap().insert(
            "op".into(),
            Work {
                provenance,
                _slot: jobs.reserve().unwrap(),
                cancel: Arc::new(AtomicBool::new(false)),
                control: control.clone(),
                converted: Arc::new(tokio::sync::Notify::new()),
                progress: Arc::new(Mutex::new(None)),
                changes,
                completion: async move { completion.await.unwrap_or(false) }
                    .boxed()
                    .shared(),
                runtime: std::sync::Weak::new(),
                parent_wake: None,
            },
        );
        let cancel = AtomicBool::new(false);
        let mut event = |_: &str, _: &ToolCallEvent| {};
        let mut waiter = Box::pin(jobs.foreground("s", "op", &cancel, &mut event));
        assert!(futures_util::poll!(&mut waiter).is_pending());
        assert!(jobs.background("s", "t", "op").unwrap());
        if cancel_after_conversion {
            cancel.store(true, Ordering::Release);
            assert!(jobs.cancel_session("s"));
            assert!(
                matches!(
                    futures_util::poll!(&mut waiter),
                    std::task::Poll::Ready(Ok(None))
                ),
                "accepted conversion returns its receipt without waiting for cancellation"
            );
        }
        db.finish_execute_job(
            "op",
            &Outcome {
                version: 1,
                state: "completed".into(),
                output: "single result".into(),
                presentation: None,
            },
        )
        .unwrap();
        control.lock().unwrap().finished = true;
        done.send(true).unwrap();
        if !cancel_after_conversion {
            assert!(
                waiter.await.unwrap().is_none(),
                "only the background notice carries the result"
            );
        }
        db.record_tool_outcome("op", "completed", Some("running receipt"))
            .unwrap();
        let notices = db.deliver_due_execute_notices().unwrap();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].text.contains("single result"));
        assert!(db.deliver_due_execute_notices().unwrap().is_empty());
    }
}
