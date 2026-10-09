//! Application-owned background lifecycles around the existing Shell supervisor.
use super::*;
use crate::storage::{Db, StorageError};
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub(crate) const ACTIVE_JOB_CAP: usize = 8;
mod output;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provenance {
    pub version: u8,
    pub session: String,
    pub turn: String,
    pub operation: String,
    pub location: String,
    pub generation: u64,
    #[serde(default)]
    pub output_limits: crate::config::ToolOutputLimits,
    #[serde(default)]
    pub output_source: String,
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
    pub(crate) pid: i32,
    start_ticks: u64,
    boot: String,
    uid: u32,
    owner_root: String,
    owner_dev: u64,
    owner_ino: u64,
}

impl ProcessIdentity {
    pub(crate) fn read(pid: i32, root: &Path) -> Option<Self> {
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

    pub(crate) fn matches(&self, root: &Path) -> bool {
        Self::read(self.pid, root).is_some_and(|p| {
            p.start_ticks == self.start_ticks
                && p.boot == self.boot
                && p.uid == self.uid
                && p.owner_root == self.owner_root
                && p.owner_dev == self.owner_dev
                && p.owner_ino == self.owner_ino
        })
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
    #[serde(default)]
    pub stdout_bytes: u64,
    #[serde(default)]
    pub stderr_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_recent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_recent: Option<String>,
    #[serde(default)]
    pub output_prepared: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<crate::storage::tool_output::Resource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_facts: Option<crate::storage::tool_output::ShellStreams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_failure: Option<crate::storage::tool_output::CaptureState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
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
            stdout_bytes: 0,
            stderr_bytes: 0,
            stdout_recent: None,
            stderr_recent: None,
            output_prepared: false,
            capture: None,
            capture_facts: None,
            capture_failure: None,
            output_presentation: None,
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
                stdout_bytes: out.stdout.len() as u64,
                stderr_bytes: out.stderr.len() as u64,
                stdout_recent: None,
                stderr_recent: None,
                output_prepared: false,
                capture: None,
                capture_facts: None,
                capture_failure: None,
                output_presentation: None,
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

    pub(crate) fn tool_result(&self) -> (&'static str, String) {
        let state = match self.state.as_str() {
            "completed" => "completed",
            "cancelled" => "cancelled",
            "timed_out" => "timed_out",
            "failed" => "failed",
            _ => "unknown",
        };
        let mut text = self
            .exit
            .map_or_else(|| "exit signal\n".into(), |code| format!("exit {code}\n"));
        text.push_str(&self.stdout);
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
        (state, text)
    }

    fn prepared_presentation(&self) -> Box<oc_core::tool_output::Presentation> {
        let cap = (oc_core::tool_output::PREVIEW_BYTES - 128) / 2;
        let stdout = recent(&self.stdout, cap);
        let stderr = recent(&self.stderr, cap);
        let body = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("{stdout}\n[stderr]\n{stderr}")
        };
        let bytes = self.stdout.len()
            + self.stderr.len()
            + if self.stderr.is_empty() {
                0
            } else {
                "\n[stderr]\n".len()
            };
        let mut presentation = oc_core::tool_output::Presentation::new(&body, bytes as u64, false);
        presentation.body_limited |= self.stdout_truncated || self.stderr_truncated;
        presentation.capture = self
            .capture
            .as_ref()
            .map(|resource| resource.presentation_capture());
        presentation.shell = Some(oc_core::tool_output::Shell {
            stdout_limited: self.stdout_truncated || stdout.len() < self.stdout.len(),
            stderr_limited: self.stderr_truncated || stderr.len() < self.stderr.len(),
            stdout,
            stderr,
            exit: self.exit,
            signal: self.signal,
            timed_out: self.timeout,
            cancelled: self.cancelled,
        });
        Box::new(presentation)
    }

    /// Legacy/recovered direct-user jobs may lack prepared stream facts. Never
    /// reinterpret a prepared model-facing envelope as captured process output.
    pub(crate) fn user_presentation(&self) -> Box<oc_core::tool_output::Presentation> {
        let mut source = self.clone();
        if source.output_prepared {
            source.stdout = source.stdout_recent.take().unwrap_or_default();
            source.stderr = source.stderr_recent.take().unwrap_or_default();
            source.stdout_truncated = true;
            source.stderr_truncated = true;
        }
        let mut presentation = source.prepared_presentation();
        if let Some(capture) = &mut presentation.capture {
            // No current descriptor lookup in this legacy fallback. The saved
            // descriptor is not authority to expose a cold reference.
            capture.reference = None;
        }
        presentation
    }
}

struct Work {
    _slot: tokio::sync::OwnedSemaphorePermit,
    session: String,
    cancel: Arc<AtomicBool>,
    // All waiters poll the same owned join. Cancelling an idle/select waiter
    // drops only its clone, never the worker or the owner's completion receipt.
    completion: Shared<BoxFuture<'static, bool>>,
    provenance: Option<Provenance>,
    capture: Arc<Capture>,
    control: Arc<Mutex<Control>>,
    converted: Arc<tokio::sync::Notify>,
}

#[derive(Default)]
struct Control {
    background: bool,
    finished: bool,
    pid: Option<i32>,
    wait_done: bool,
}

/// The same bounded buffers are consumed by the supervisor, viewer and outcome.
pub(super) struct Capture {
    pub(super) stdout: Arc<Mutex<DrainState>>,
    pub(super) stderr: Arc<Mutex<DrainState>>,
    pub(super) wake: Arc<tokio::sync::Notify>,
    stream: Arc<Mutex<output::StreamCapture>>,
}

impl Capture {
    fn new(wake: Arc<tokio::sync::Notify>) -> Arc<Self> {
        Arc::new(Self {
            stdout: Arc::default(),
            stderr: Arc::default(),
            wake,
            stream: Arc::new(Mutex::new(output::StreamCapture::new())),
        })
    }
    pub(super) fn shared(&self) -> Self {
        Self {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            wake: self.wake.clone(),
            stream: self.stream.clone(),
        }
    }

    pub(super) fn ingest(&self, stream: Stream, bytes: &[u8], end: bool, complete: bool) {
        let mut capture = self.stream.lock().expect("stream admission");
        let admitted = capture.ingest(stream, bytes, end, complete);
        let state = match stream {
            Stream::Stdout => &self.stdout,
            Stream::Stderr => &self.stderr,
        };
        let mut state = state.lock().expect("stream recent");
        state.total = state.total.saturating_add(bytes.len() as u64);
        output::retain(&mut state, &admitted);
        if end {
            state.done = true;
        }
        drop(state);
        self.wake.notify_one();
    }

    fn text(&self) -> String {
        let out = self.stdout.lock().expect("stdout capture");
        let err = self.stderr.lock().expect("stderr capture");
        format!(
            "status: running\n{}{}{}",
            String::from_utf8_lossy(&out.bytes),
            if err.bytes.is_empty() {
                String::new()
            } else {
                format!("\n[stderr]\n{}", String::from_utf8_lossy(&err.bytes))
            },
            if out.truncated || err.truncated {
                "\n[truncated]"
            } else {
                ""
            }
        )
    }
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

    #[cfg(test)]
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
        self.launch_mode(
            shell,
            env,
            argv,
            cwd,
            timeout,
            approved,
            provenance,
            slot,
            false,
            Vec::new(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn launch_mode(
        self: &Arc<Self>,
        shell: Shell,
        env: BTreeMap<String, String>,
        argv: Vec<String>,
        cwd: String,
        timeout: Duration,
        approved: Arc<PinnedCwd>,
        provenance: Provenance,
        slot: tokio::sync::OwnedSemaphorePermit,
        foreground: bool,
        output_secrets: Vec<String>,
    ) -> Result<String, StorageError> {
        self.db.admit_shell_job_mode(&provenance, foreground)?;
        self.launch_admitted(
            shell,
            env,
            argv,
            cwd,
            timeout,
            approved,
            provenance,
            slot,
            foreground,
            output_secrets,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn launch_user(
        self: &Arc<Self>,
        shell: Shell,
        env: BTreeMap<String, String>,
        argv: Vec<String>,
        cwd: String,
        timeout: Duration,
        approved: Arc<PinnedCwd>,
        admission: crate::storage::AdmittedUserShell,
        slot: tokio::sync::OwnedSemaphorePermit,
        output_secrets: Vec<String>,
    ) -> Result<String, StorageError> {
        self.launch_admitted(
            shell,
            env,
            argv,
            cwd,
            timeout,
            approved,
            admission.into_provenance(),
            slot,
            false,
            output_secrets,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn launch_admitted(
        self: &Arc<Self>,
        shell: Shell,
        env: BTreeMap<String, String>,
        argv: Vec<String>,
        cwd: String,
        timeout: Duration,
        approved: Arc<PinnedCwd>,
        provenance: Provenance,
        slot: tokio::sync::OwnedSemaphorePermit,
        foreground: bool,
        output_secrets: Vec<String>,
    ) -> Result<String, StorageError> {
        let id = provenance.operation.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let owned_cancel = cancel.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let db = self.db.shared_handle();
        let failed = self.failed.clone();
        let operation = id.clone();
        let capture = Capture::new(self.wake.clone());
        let owned_capture = capture.clone();
        let control = Arc::new(Mutex::new(Control {
            background: !foreground,
            wait_done: !foreground,
            ..Control::default()
        }));
        let owned_control = control.clone();
        let output_provenance = provenance.clone();
        let task = tokio::task::spawn_blocking(move || {
            owned_capture
                .stream
                .lock()
                .expect("stream admission")
                .begin(&db, &output_provenance, output_secrets.clone());
            let result = shell.execute_pinned_started(
                &env,
                &argv,
                &cwd,
                None,
                ShellLimits {
                    timeout,
                    kill_grace: Duration::from_millis(500),
                    retain_cap: output::RECENT_CAP,
                },
                &owned_cancel,
                Some(&approved),
                true,
                Some(&owned_capture),
                |pid| {
                    let identity =
                        ProcessIdentity::read(pid, db.root()).ok_or(ShellError::AdmissionFailed)?;
                    db.start_shell_job(&operation, &identity)
                        .map_err(|_| ShellError::AdmissionFailed)?;
                    owned_control.lock().expect("shell control").pid = Some(pid);
                    owned_capture.wake.notify_one();
                    // A dropped caller independently arms its cancellation
                    // guard. Keep supervising to freeze a real cancelled result.
                    let _ = started_tx.send(());
                    Ok(())
                },
            );
            let producer_interrupted = result.as_ref().is_ok_and(|r| r.capture_interrupted);
            let mut outcome = Outcome::from_result(result);
            let execution_state = outcome.state.clone();
            let mut logging_failed = false;
            outcome.stdout_bytes = owned_capture.stdout.lock().expect("stdout capture").total;
            outcome.stderr_bytes = owned_capture.stderr.lock().expect("stderr capture").total;
            let p = &output_provenance;
            let (tail, resource, capture_failed) = owned_capture
                .stream
                .lock()
                .expect("stream admission")
                .finish(
                    &db,
                    p,
                    outcome.cancelled
                        || outcome.timeout
                        || outcome.signal.is_some()
                        || producer_interrupted
                        || outcome.state == "unknown",
                );
            outcome.capture_facts = Some(
                owned_capture
                    .stream
                    .lock()
                    .expect("stream admission")
                    .facts(),
            );
            let context = crate::tools::output::Context {
                db: &db,
                operation: &operation,
                session: &p.session,
                location: &p.location,
                generation: p.generation,
                source: &p.output_source,
                limits: p.output_limits,
                secrets: output_secrets,
            };
            crate::tools::output::redact_string(&mut outcome.stdout, &context.secrets);
            crate::tools::output::redact_string(&mut outcome.stderr, &context.secrets);
            if let Some(diagnostic) = outcome.diagnostic.as_mut() {
                crate::tools::output::redact_string(diagnostic, &context.secrets);
            }
            let oversized = resource.as_ref().is_some_and(|r| {
                r.admitted_bytes > p.output_limits.bytes() as u64
                    || r.admitted_lines > p.output_limits.max_lines as u64
            });
            outcome.capture = resource.clone();
            outcome.capture_failure = owned_capture
                .stream
                .lock()
                .expect("stream admission")
                .failure();
            outcome.output_presentation = Some(outcome.prepared_presentation());
            if p.turn.is_empty() {
                // The supervisor has moved drain buffers into Outcome. Keep its
                // frozen typed stream/exit facts; only the ordered display body
                // comes from the safe publication projection retained by Capture.
                let ordered = owned_capture.presentation();
                let presentation = outcome
                    .output_presentation
                    .as_mut()
                    .expect("prepared output");
                presentation.body = ordered.body;
                presentation.body_bytes = ordered.body_bytes;
                presentation.body_limited = ordered.body_limited;
            }
            // Producer loss is separate from a storage failure or leader exit.
            // Even a short interrupted prefix must advertise its capture state.
            let incomplete = resource
                .as_ref()
                .is_some_and(|r| r.state != crate::storage::tool_output::CaptureState::Complete);
            if oversized || incomplete || capture_failed || outcome.cancelled || outcome.timeout {
                let recent_cap = (crate::storage::TOOL_OP_PREVIEW_BYTES - 128) / 2;
                outcome.stdout_recent = Some(recent(&outcome.stdout, recent_cap));
                outcome.stderr_recent = Some(recent(&outcome.stderr, recent_cap));
                // These are separately admitted bounded UI stream facts, not
                // the provider text or a second full capture owner.
                outcome.output_prepared = true;
                let prepared =
                    context.prepare_stream(tail, resource.as_ref(), outcome.capture_failure);
                if let Some(facts) = prepared.presentation.as_ref()
                    && let Some(presentation) = &mut outcome.output_presentation
                {
                    presentation.generated_guidance = facts.generated_guidance;
                    presentation.capture = facts.capture.clone();
                    presentation.producer_limited = facts.producer_limited;
                }
                outcome.stdout = prepared.text;
                outcome.stderr.clear();
                if capture_failed
                    || (prepared.logging_failed && !outcome.cancelled && !outcome.timeout)
                {
                    logging_failed = true;
                    if matches!(outcome.state.as_str(), "completed" | "failed") {
                        outcome.state = "failed".into();
                    }
                    outcome.diagnostic = Some(
                        "tool output capture failed; execution facts/exit retained; not replayed"
                            .into(),
                    );
                }
            }
            let mut control = owned_control.lock().expect("shell control");
            // Freeze known process effects/exit first. A failure recording the
            // separate logging event must not replace them with invented unknown.
            let result = db.finish_shell_job(&operation, &outcome).and_then(|()| {
                if logging_failed {
                    db.record_output_execution(&operation, &execution_state)
                } else {
                    Ok(())
                }
            });
            control.finished = true;
            if result.is_err() || outcome.state == "unknown" {
                failed.store(true, Ordering::Release);
            }
            result
        });
        self.work.lock().expect("shell jobs").insert(
            id.clone(),
            Work {
                _slot: slot,
                session: provenance.session.clone(),
                cancel: cancel.clone(),
                completion: async move { matches!(task.await, Ok(Ok(()))) }
                    .boxed()
                    .shared(),
                provenance: Some(provenance.clone()),
                capture,
                control,
                converted: Arc::new(tokio::sync::Notify::new()),
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

    pub(crate) fn running(
        &self,
        caller: &str,
    ) -> Result<Vec<oc_core::queries::ShellJob>, StorageError> {
        self.db.shell_family_contains(caller, caller)?;
        let work = self.work.lock().expect("shell jobs");
        let mut rows = Vec::new();
        for (id, job) in work.iter() {
            if !self.db.shell_family_contains(caller, &job.session)? {
                continue;
            }
            let control = job.control.lock().expect("shell control");
            if control.finished || self.db.shell_job_phase(&job.session, id)? != "running" {
                continue;
            }
            if let Some(p) = &job.provenance {
                rows.push(oc_core::queries::ShellJob {
                    session: oc_core::domain::SessionId(p.session.clone()),
                    shell_id: id.clone(),
                    location: p.location.clone(),
                    generation: p.generation,
                    turn: p.turn.clone(),
                    model: p.model.clone(),
                    provider: p.provider.clone(),
                    command: p.command.clone(),
                    pid: control.pid,
                    background: control.background,
                    output: p.turn.is_empty().then(|| job.capture.presentation()),
                });
            }
        }
        Ok(rows)
    }

    pub(crate) fn background(&self, session: &str, id: &str) -> Result<bool, StorageError> {
        let work = self.work.lock().expect("shell jobs");
        let Some(job) = work.get(id).filter(|job| job.session == session) else {
            return Ok(false);
        };
        let mut control = job.control.lock().expect("shell control");
        if control.finished || job.cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        if control.background {
            return Ok(true);
        }
        if !self.db.background_shell_job(id)? {
            return Ok(false);
        }
        control.background = true;
        job.converted.notify_one();
        self.wake.notify_one();
        Ok(true)
    }

    pub(crate) fn output(
        &self,
        session: &str,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Option<(String, i64, Option<i64>)>, StorageError> {
        if let Some(page) = self.db.shell_output(session, id, offset, limit)? {
            return Ok(Some(page));
        }
        let work = self.work.lock().expect("shell jobs");
        let Some(job) = work.get(id).filter(|job| job.session == session) else {
            return Ok(None);
        };
        let text = job.capture.text();
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err(StorageError::OperationNotFound);
        }
        let end = text.floor_char_boundary(
            offset
                .saturating_add(limit.clamp(4, crate::storage::TOOL_OP_PREVIEW_BYTES))
                .min(text.len()),
        );
        Ok(Some((
            text[offset..end].into(),
            text.len() as i64,
            (end < text.len()).then_some(end as i64),
        )))
    }

    pub(crate) fn snapshot(
        &self,
        session: &str,
        id: &str,
    ) -> Result<oc_core::queries::ShellSnapshot, StorageError> {
        let job = self.db.shell_job_identity(session, id)?;
        let phase = self.db.shell_job_phase(session, id)?;
        let cap = (crate::storage::TOOL_OP_PREVIEW_BYTES - 256) / 2;
        let (state, stdout, stderr, stdout_cursor, stderr_cursor, truncated) = if phase
            == "terminal"
        {
            let o = self.db.shell_job_outcome(session, id)?;
            let stdout = recent(o.stdout_recent.as_deref().unwrap_or(&o.stdout), cap);
            let stderr = recent(o.stderr_recent.as_deref().unwrap_or(&o.stderr), cap);
            let preview_truncated = o.stdout.len() > cap || o.stderr.len() > cap;
            (
                o.state,
                stdout,
                stderr,
                o.stdout_bytes,
                o.stderr_bytes,
                o.stdout_truncated || o.stderr_truncated || preview_truncated,
            )
        } else {
            let work = self.work.lock().expect("shell jobs");
            let active = work
                .get(id)
                .filter(|w| w.session == session)
                .ok_or(StorageError::OperationNotFound)?;
            let out = active.capture.stdout.lock().expect("stdout capture");
            let err = active.capture.stderr.lock().expect("stderr capture");
            (
                phase,
                recent(&String::from_utf8_lossy(&out.bytes), cap),
                recent(&String::from_utf8_lossy(&err.bytes), cap),
                out.total,
                err.total,
                out.truncated || err.truncated || out.bytes.len() > cap || err.bytes.len() > cap,
            )
        };
        let mut text = format!(
            "status: {state}\n{stdout}{}{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!("\n[stderr]\n{stderr}")
            },
            if truncated { "\n[truncated]" } else { "" }
        );
        if let Some(resource) = self.db.output_for_operation(id)? {
            text.push_str(&format!(
                "\n[capture {} {:?}, published {} bytes]",
                resource.id, resource.state, resource.bytes
            ));
        }
        Ok(oc_core::queries::ShellSnapshot {
            job,
            state,
            stdout_cursor,
            stderr_cursor,
            truncated,
            text,
        })
    }

    pub(crate) async fn foreground(
        &self,
        session: &str,
        id: &str,
        parent_cancel: &AtomicBool,
    ) -> Result<Option<Outcome>, StorageError> {
        let (completion, control, converted, cancel) = {
            let work = self.work.lock().expect("shell jobs");
            let job = work
                .get(id)
                .filter(|job| job.session == session)
                .ok_or(StorageError::OperationNotFound)?;
            (
                job.completion.clone(),
                job.control.clone(),
                job.converted.clone(),
                job.cancel.clone(),
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
                let mut state = self.1.lock().expect("shell control");
                state.wait_done = true;
                if !self.3 {
                    self.0.store(true, Ordering::Release);
                }
                self.2.notify_one();
            }
        }
        let mut guard = WaitGuard(cancel.clone(), control.clone(), self.wake.clone(), false);
        loop {
            {
                let state = control.lock().expect("shell control");
                if parent_cancel.load(Ordering::Acquire) {
                    cancel.store(true, Ordering::Release);
                }
                // Completion wins once frozen; conversion before completion releases
                // the original await exactly once without touching its parent token.
                if state.background && !state.finished && !cancel.load(Ordering::Acquire) {
                    guard.3 = true;
                    return Ok(None);
                }
            }
            tokio::select! {
                good = completion.clone() => {
                    guard.3 = true;
                    if !good { return Err(StorageError::OperationNotFound); }
                    return self.db.shell_job_outcome(session, id).map(Some);
                }
                () = converted.notified() => {},
                () = tokio::time::sleep(Duration::from_millis(5)) => {
                    if parent_cancel.load(Ordering::Acquire) { cancel.store(true, Ordering::Release); }
                }
            }
        }
    }

    pub(crate) fn cancel_session(&self, session: &str) -> bool {
        let work = self.work.lock().expect("shell jobs");
        let mut cancelled = false;
        for job in work
            .values()
            .filter(|job| job.session == session && job.completion.peek().is_none())
        {
            let control = job.control.lock().expect("shell control");
            if control.finished {
                continue;
            }
            job.cancel.store(true, Ordering::Release);
            self.wake.notify_one();
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
            let control = job.control.lock().expect("shell control");
            if control.finished {
                return false;
            }
            job.cancel.store(true, Ordering::Release);
            self.wake.notify_one();
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
            .filter(|job| {
                let state = job.control.lock().expect("shell control");
                !state.finished || state.wait_done
            })
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
                .filter(|(_, job)| {
                    job.control.lock().expect("shell control").wait_done
                        && job.completion.clone().now_or_never().is_some()
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .map(|id| {
                    let job = work.remove(&id).expect("finished job");
                    (id, job)
                })
                .collect::<Vec<_>>()
        };
        let sessions = {
            let work = self.work.lock().expect("shell jobs");
            work.values()
                .map(|job| job.session.clone())
                .chain(completed.iter().map(|(_, job)| job.session.clone()))
                .collect::<std::collections::BTreeSet<_>>()
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
        for session in sessions {
            let _ = events.send(oc_core::core_app::CoreEvent::ShellChanged {
                session: oc_core::domain::SessionId(session),
            });
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
mod output_tests;
#[cfg(test)]
mod tests;

fn recent(text: &str, cap: usize) -> String {
    text[text.ceil_char_boundary(text.len().saturating_sub(cap))..].into()
}
