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
    runtime: std::sync::Weak<Runtime<'static>>,
    mcp: Arc<mcp::McpOwner>,
    cancel: Arc<AtomicBool>,
    completion: Shared<BoxFuture<'static, bool>>,
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
                .filter(|(_, job)| job.completion.clone().now_or_never().is_some())
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
            (if fresh {
                self.db.admit_fresh_child_job(&identity)
            } else {
                self.db.admit_child_job(&identity)
            })
            .map_err(|_| ToolError::Failed {
                tool: SUBAGENT_TOOL.into(),
                reason: "child admission failed".into(),
            })?;
            let cancel = Arc::new(AtomicBool::new(false));
            let token = cancel.clone();
            let db = self.db.shared_handle();
            let id = identity.clone();
            let wake = self.wake.clone();
            let failed = self.failed.clone();
            let (started, launched) = tokio::sync::oneshot::channel();
            let runtime = Arc::new(runtime);
            let mcp = runtime.mcp_owner();
            let child_runtime = runtime.clone();
            let task = tokio::spawn(async move {
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
                    )
                    .await;
                let (state, result) = match result {
                    Ok(report) if report.status == TurnStatus::Completed => (
                        ChildState::Completed,
                        if report.text.is_empty() {
                            SUBAGENT_NO_TEXT.into()
                        } else {
                            report.text
                        },
                    ),
                    Ok(report) if report.status == TurnStatus::Cancelled => {
                        (ChildState::Cancelled, "Subagent cancelled".into())
                    }
                    Ok(report) => (
                        ChildState::Error,
                        report
                            .diagnostic
                            .unwrap_or_else(|| "child execution did not complete".into()),
                    ),
                    Err(_) => (ChildState::Error, "child execution failed".into()),
                };
                let ok = db.finish_child_job(&id.operation, state, &result).is_ok();
                if !ok {
                    failed.store(true, Ordering::Release);
                }
                wake.notify_one();
                ok
            });
            let completion = async move { task.await.unwrap_or(false) }.boxed().shared();
            work.insert(
                identity.operation.clone(),
                Work {
                    identity: identity.clone(),
                    runtime: Arc::downgrade(&runtime),
                    mcp,
                    cancel,
                    completion,
                },
            );
            launched
        };
        launched.await.map_err(|_| ToolError::Failed {
            tool: SUBAGENT_TOOL.into(),
            reason: "child launch failed; inspect child state".into(),
        })?;
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
