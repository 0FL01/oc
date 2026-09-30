//! Application-owned background lifecycles around the existing Shell supervisor.
use super::*;
use crate::storage::{Db, StorageError};
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

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
    pub agent: Option<String>,
    pub agent_digest: Option<String>,
    pub model: String,
    pub provider: String,
    pub command: String,
    pub cwd: String,
    pub selected_shell: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessIdentity {
    pid: i32,
    start_ticks: u64,
    boot: String,
    uid: u32,
    owner_root: String,
    owner_dev: u64,
    owner_ino: u64,
}

impl ProcessIdentity {
    fn read(pid: i32, root: &Path) -> Option<Self> {
        if pid <= 1 {
            return None;
        }
        let path = format!("/proc/{pid}/stat");
        let metadata = std::fs::metadata(&path).ok()?;
        let stat = std::fs::read_to_string(path).ok()?;
        let fields = stat
            .rsplit_once(") ")?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        // fields start at Linux stat field 3; pgrp=5 and starttime=22.
        if fields.get(2)?.parse::<i32>().ok()? != pid {
            return None;
        }
        let owner = std::fs::metadata(root).ok()?;
        Some(Self {
            pid,
            start_ticks: fields.get(19)?.parse().ok()?,
            boot: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .ok()?
                .trim()
                .into(),
            uid: metadata.uid(),
            owner_root: root.to_string_lossy().into(),
            owner_dev: owner.dev(),
            owner_ino: owner.ino(),
        })
    }

    /// Recovered PIDs confer authority only while their exact leader still exists.
    /// Missing/reused leaders are quarantined, never blindly signalled.
    pub(crate) fn quarantine(&self, root: &Path) -> &'static str {
        let Some(current) = Self::read(self.pid, root) else {
            return if std::path::Path::new(&format!("/proc/{}", self.pid)).exists() {
                "process ownership unverified; no signal sent"
            } else {
                "recorded leader absent; no signal sent"
            };
        };
        if current.start_ticks != self.start_ticks
            || current.boot != self.boot
            || current.uid != self.uid
            || current.owner_root != self.owner_root
            || current.owner_dev != self.owner_dev
            || current.owner_ino != self.owner_ino
        {
            return "process ownership unverified; no signal sent";
        }
        kill_group(self.pid, Duration::from_millis(500));
        // This leader is not a child after restart; no invented successful wait.
        // Recovery always reports unknown execution, even after verified cleanup.
        "verified owned process group quarantined"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Outcome {
    pub version: u8,
    pub state: String,
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timeout: bool,
    pub cancelled: bool,
    pub diagnostic: Option<String>,
}

impl Outcome {
    pub(crate) fn unknown(diagnostic: &str) -> Self {
        Self {
            version: 1,
            state: "unknown".into(),
            exit: None,
            signal: None,
            stdout: String::new(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            timeout: false,
            cancelled: false,
            diagnostic: Some(diagnostic.into()),
        }
    }

    fn from_result(result: Result<ShellOutcome, ShellError>) -> Self {
        match result {
            Ok(out) => Self {
                version: 1,
                state: if out.cancelled {
                    "cancelled"
                } else if out.timed_out {
                    "timed_out"
                } else if out.code == Some(0) {
                    "completed"
                } else {
                    "failed"
                }
                .into(),
                exit: out.code,
                signal: out.signal,
                stdout: String::from_utf8_lossy(&out.stdout).into(),
                stderr: String::from_utf8_lossy(&out.stderr).into(),
                stdout_truncated: out.stdout_truncated,
                stderr_truncated: out.stderr_truncated,
                timeout: out.timed_out,
                cancelled: out.cancelled,
                diagnostic: None,
            },
            Err(error) => {
                let mut outcome = Self::unknown(&format!("shell supervisor: {error}"));
                if !matches!(error, ShellError::Reap | ShellError::AdmissionFailed) {
                    outcome.state = "failed".into();
                }
                outcome
            }
        }
    }

    pub(crate) fn text(&self) -> String {
        let mut text = format!(
            "status: {}\nexit: {:?}\nsignal: {:?}\n{}",
            self.state, self.exit, self.signal, self.stdout
        );
        if !self.stderr.is_empty() {
            text.push_str(&format!("\n[stderr]\n{}", self.stderr));
        }
        if self.stdout_truncated || self.stderr_truncated {
            text.push_str("\n[truncated]");
        }
        if self.timeout {
            text.push_str("\n[timeout]");
        }
        if self.cancelled {
            text.push_str("\n[cancelled]");
        }
        if let Some(diagnostic) = &self.diagnostic {
            text.push_str(&format!("\n{diagnostic}"));
        }
        text
    }
}

struct Work {
    _slot: tokio::sync::OwnedSemaphorePermit,
    session: String,
    cancel: Arc<AtomicBool>,
    // All waiters poll the same owned join. Cancelling an idle/select waiter
    // drops only its clone, never the worker or the owner's completion receipt.
    completion: Shared<BoxFuture<'static, bool>>,
}

/// One process-local owner, shared across Location runtime replacements.
pub(crate) struct Jobs {
    slots: Arc<tokio::sync::Semaphore>,
    db: Db,
    work: Mutex<BTreeMap<String, Work>>,
    wake: Arc<tokio::sync::Notify>,
    failed: Arc<AtomicBool>,
}

impl Jobs {
    pub(crate) fn new(db: &Db) -> Arc<Self> {
        Arc::new(Self {
            slots: Arc::new(tokio::sync::Semaphore::new(ACTIVE_JOB_CAP)),
            db: db.shared_handle(),
            work: Mutex::new(BTreeMap::new()),
            wake: Arc::new(tokio::sync::Notify::new()),
            failed: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn reserve(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.slots.clone().try_acquire_owned().ok()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn launch(
        self: &Arc<Self>,
        shell: Shell,
        env: BTreeMap<String, String>,
        argv: Vec<String>,
        cwd: String,
        timeout: Duration,
        approved: Arc<PinnedCwd>,
        provenance: Provenance,
        slot: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<String, StorageError> {
        let id = provenance.operation.clone();
        self.db.admit_shell_job(&provenance)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let owned_cancel = cancel.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let db = self.db.shared_handle();
        let failed = self.failed.clone();
        let operation = id.clone();
        let task = tokio::task::spawn_blocking(move || {
            let result = shell.execute_pinned_started(
                &env,
                &argv,
                &cwd,
                None,
                ShellLimits {
                    timeout,
                    kill_grace: Duration::from_millis(500),
                    retain_cap: RETAIN_CAP_BYTES,
                },
                &owned_cancel,
                Some(&approved),
                true,
                |pid| {
                    let identity =
                        ProcessIdentity::read(pid, db.root()).ok_or(ShellError::AdmissionFailed)?;
                    db.start_shell_job(&operation, &identity)
                        .map_err(|_| ShellError::AdmissionFailed)?;
                    // A dropped caller independently arms its cancellation
                    // guard. Keep supervising to freeze a real cancelled result.
                    let _ = started_tx.send(());
                    Ok(())
                },
            );
            let outcome = Outcome::from_result(result);
            let result = db.finish_shell_job(&operation, &outcome);
            if result.is_err() || outcome.state == "unknown" {
                failed.store(true, Ordering::Release);
            }
            result
        });
        self.work.lock().expect("shell jobs").insert(
            id.clone(),
            Work {
                _slot: slot,
                session: provenance.session,
                cancel: cancel.clone(),
                completion: async move { matches!(task.await, Ok(Ok(()))) }
                    .boxed()
                    .shared(),
            },
        );
        // Refresh an idle owner's join set when a new worker is admitted.
        self.wake.notify_one();
        struct AdmissionGuard(Arc<AtomicBool>, bool);
        impl Drop for AdmissionGuard {
            fn drop(&mut self) {
                if !self.1 {
                    self.0.store(true, Ordering::Release);
                }
            }
        }
        let mut guard = AdmissionGuard(cancel, false);
        started_rx
            .await
            .map_err(|_| StorageError::OperationNotFound)?;
        guard.1 = true;
        Ok(id)
    }

    pub(crate) fn cancel_session(&self, session: &str) -> bool {
        let work = self.work.lock().expect("shell jobs");
        let mut cancelled = false;
        for job in work
            .values()
            .filter(|job| job.session == session && job.completion.peek().is_none())
        {
            job.cancel.store(true, Ordering::Release);
            cancelled = true;
        }
        cancelled
    }

    pub(crate) fn cancel_job(&self, session: &str, id: &str) -> bool {
        let work = self.work.lock().expect("shell jobs");
        if let Some(job) = work
            .get(id)
            .filter(|job| job.session == session && job.completion.peek().is_none())
        {
            job.cancel.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    pub(crate) async fn changed(&self) {
        let joins = self
            .work
            .lock()
            .expect("shell jobs")
            .values()
            .map(|job| job.completion.clone())
            .collect::<Vec<_>>();
        if joins.is_empty() {
            self.wake.notified().await;
        } else {
            tokio::select! {
                () = self.wake.notified() => {},
                _ = futures_util::future::select_all(joins) => {},
            }
        }
    }

    pub(crate) fn deliver(
        &self,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) -> Result<(), StorageError> {
        // Workers freeze terminal outcomes before this history admission.
        let completed = {
            let mut work = self.work.lock().expect("shell jobs");
            let ids = work
                .iter()
                .filter(|(_, job)| job.completion.clone().now_or_never().is_some())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .map(|id| {
                    let job = work.remove(&id).expect("finished job");
                    (id, job)
                })
                .collect::<Vec<_>>()
        };
        for (id, job) in completed {
            if job.completion.peek() != Some(&true) {
                self.failed.store(true, Ordering::Release);
                self.db.finish_shell_job(
                    &id,
                    &Outcome::unknown(
                        "supervisor worker interrupted; effect unknown; not replayed",
                    ),
                )?;
            }
        }
        for notice in self.db.deliver_shell_notices()? {
            let _ = events.send(oc_core::core_app::CoreEvent::ShellNotice(notice));
        }
        Ok(())
    }

    pub(crate) async fn shutdown(&self) -> Result<(), StorageError> {
        let tasks = {
            let work = self.work.lock().expect("shell jobs");
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
                if self
                    .db
                    .finish_shell_job(
                        &id,
                        &Outcome::unknown(
                            "supervisor cleanup failed; effect unknown; not replayed",
                        ),
                    )
                    .is_err()
                {
                    self.failed.store(true, Ordering::Release);
                }
            }
            self.work.lock().expect("shell jobs").remove(&id);
        }
        if self.failed.load(Ordering::Acquire) {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        // Covers owner-future abort as well as orderly async shutdown. Workers
        // retain the same storage lock until their existing supervisor reaps.
        for job in self.work.get_mut().expect("shell jobs").values() {
            job.cancel.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests;
