//! Linux event delivery for the admitted topology, not another source loader.
//!
//! notify's built-in Linux/polling workers do not expose a joined shutdown. Keep
//! its event/handler boundary, but own the nonblocking Linux FD in one Tokio task.
//! Installing a plan only watches known paths/parent anchors; it never reads bodies,
//! enumerates directories, creates missing roots, or admits a source for execution.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use notify::event::{CreateKind, DataChange, Flag, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventHandler, EventKind};
use oc_core::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceStage};
use tokio::io::unix::AsyncFd;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::JoinHandle;

use super::{super::LoadFailure, WatchInputs};
use crate::defs::InputKind;

const DIRTY: u8 = 1;
const REARM: u8 = 2;
const FAILED: u8 = 4;
const MAX_ANCHORS: usize = 8192;
const MAX_ANCHOR_BYTES: usize = 2 * 1024 * 1024;
const MASK: u32 = libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_MODIFY
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_ONLYDIR;

#[derive(Default)]
struct Dirty {
    epoch: AtomicU64,
    bits: AtomicU8,
    wake: Notify,
}

#[derive(Clone, Copy)]
pub(crate) struct Change {
    pub(crate) rearm: bool,
    pub(crate) failed: bool,
}

impl Dirty {
    fn mark(&self, bits: u8) {
        let overflow = self
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .is_err();
        self.bits.fetch_or(
            bits | DIRTY | if overflow { FAILED } else { 0 },
            Ordering::AcqRel,
        );
        self.wake.notify_one();
    }

    async fn changed(&self) -> Change {
        loop {
            let wait = self.wake.notified();
            let bits = self.bits.swap(0, Ordering::AcqRel);
            if bits != 0 {
                return Change {
                    rearm: bits & REARM != 0,
                    failed: bits & FAILED != 0,
                };
            }
            wait.await;
        }
    }
}

enum Command {
    Install(
        WatchInputs,
        bool,
        oneshot::Sender<Result<(), ServiceDiagnostic>>,
    ),
    Stop,
}

pub(crate) struct SourceWatch {
    commands: mpsc::Sender<Command>,
    dirty: Arc<Dirty>,
    task: Option<JoinHandle<Result<(), ServiceDiagnostic>>>,
}

/// A bounded command/epoch handle, not an observer/task owner. Candidate builds
/// may borrow observation while the Location coordinator retains joined cleanup.
#[derive(Clone)]
pub(crate) struct Observation {
    commands: mpsc::Sender<Command>,
    dirty: Arc<Dirty>,
}

impl Observation {
    pub(crate) fn epoch(&self) -> u64 {
        self.dirty.epoch.load(Ordering::Acquire)
    }

    pub(crate) async fn reconcile(
        &self,
        inputs: &WatchInputs,
        rearm: bool,
    ) -> Result<(), ServiceDiagnostic> {
        let (ack, received) = oneshot::channel();
        self.commands
            .send(Command::Install(inputs.clone(), rearm, ack))
            .await
            .map_err(|_| issue(ServiceCode::SourceUnavailable))?;
        received
            .await
            .map_err(|_| issue(ServiceCode::SourceUnavailable))?
    }
}

impl SourceWatch {
    pub(crate) fn observation(&self) -> Observation {
        Observation {
            commands: self.commands.clone(),
            dirty: self.dirty.clone(),
        }
    }
    pub(crate) fn start(inputs: &WatchInputs) -> Result<Self, ServiceDiagnostic> {
        let live = Live::install(inputs.clone())?;
        let dirty = Arc::new(Dirty::default());
        let (commands, receiver) = mpsc::channel(1);
        let task = tokio::spawn(run(live, receiver, Completion(dirty.clone(), false)));
        // Readiness closes the load→watch window with an admitted rescan. This is
        // one dirty bit, not a queued file-event history or an automatic force.
        dirty.mark(0);
        Ok(Self {
            commands,
            dirty,
            task: Some(task),
        })
    }

    pub(crate) async fn changed(&self) -> Change {
        self.dirty.changed().await
    }

    pub(crate) async fn shutdown(mut self) -> Result<(), ServiceDiagnostic> {
        let _ = self.commands.send(Command::Stop).await;
        self.task
            .take()
            .expect("owned watch task")
            .await
            .map_err(|_| issue(ServiceCode::SourceUnavailable))?
    }
}

impl Drop for SourceWatch {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            // Interruption fallback only. Normal host shutdown must await shutdown.
            task.abort();
        }
    }
}

struct Completion(Arc<Dirty>, bool);

impl Drop for Completion {
    fn drop(&mut self) {
        if !self.1 {
            self.0.mark(REARM | FAILED);
        }
    }
}

async fn run(
    mut live: Live,
    mut commands: mpsc::Receiver<Command>,
    mut completion: Completion,
) -> Result<(), ServiceDiagnostic> {
    let dirty = completion.0.clone();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Install(inputs, rearm, ack)) => {
                    let result = inputs.validate()
                        .map_err(|LoadFailure::Configuration(d)| d)
                        .and_then(|()| {
                            if !rearm && live.inputs.targets == inputs.targets {
                                Ok(())
                            } else {
                                Live::install(inputs).map(|next| {
                                    // Register the replacement before closing the old FD.
                                    live = next;
                                    dirty.mark(0);
                                })
                            }
                        });
                    if result.is_err() {
                        dirty.mark(REARM);
                    }
                    let _ = ack.send(result);
                }
                Some(Command::Stop) | None => {
                    completion.1 = true;
                    return Ok(());
                }
            },
            readable = live.fd.readable() => {
                let mut ready = readable.map_err(|_| issue(ServiceCode::SourceUnavailable))?;
                let mut bytes = [0_u8; 16 * 1024];
                match ready.try_io(|fd| read(fd.as_raw_fd(), &mut bytes)) {
                    Ok(Ok(length)) if length > 0 => {
                        let mut handler = Handler { inputs: live.inputs.clone(), dirty: dirty.clone() };
                        decode(&bytes[..length], &live.paths, &mut handler)?;
                    }
                    Ok(Ok(_)) | Ok(Err(_)) => return Err(issue(ServiceCode::SourceUnavailable)),
                    Err(_) => {}
                }
            }
        }
    }
}

struct Live {
    inputs: Arc<WatchInputs>,
    fd: AsyncFd<OwnedFd>,
    paths: BTreeMap<i32, Vec<PathBuf>>,
}

impl Live {
    fn install(inputs: WatchInputs) -> Result<Self, ServiceDiagnostic> {
        inputs
            .validate()
            .map_err(|LoadFailure::Configuration(d)| d)?;
        let mut anchors = BTreeSet::new();
        let mut bytes = 0;
        for target in &inputs.targets {
            let parent = target.path.parent().unwrap_or(Path::new("/"));
            for path in [
                Some(parent),
                (target.kind != InputKind::File).then_some(target.path.as_path()),
            ]
            .into_iter()
            .flatten()
            {
                let anchor = existing_directory(path)?;
                if !anchors.contains(&anchor) {
                    let length = anchor.as_os_str().len();
                    if anchors.len() == MAX_ANCHORS || length > MAX_ANCHOR_BYTES - bytes {
                        return Err(issue(ServiceCode::CapacityExceeded));
                    }
                    bytes += length;
                    anchors.insert(anchor);
                }
            }
        }
        // SAFETY: no pointer arguments; the successful descriptor is uniquely owned.
        let raw = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if raw < 0 {
            return Err(io_issue(&std::io::Error::last_os_error()));
        }
        // SAFETY: raw is a newly created valid FD, transferred exactly once.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut paths: BTreeMap<i32, Vec<PathBuf>> = BTreeMap::new();
        for anchor in anchors {
            let name = CString::new(anchor.as_os_str().as_bytes())
                .map_err(|_| issue(ServiceCode::SourceUnavailable))?;
            // SAFETY: fd is live; name is a valid NUL-terminated borrowed path.
            let wd = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), name.as_ptr(), MASK) };
            if wd < 0 {
                return Err(io_issue(&std::io::Error::last_os_error()));
            }
            // Aliases to one directory share a kernel wd, but keep both lexical
            // coordinates for matching. These names grant no file-read authority.
            paths.entry(wd).or_default().push(anchor);
        }
        Ok(Self {
            inputs: Arc::new(inputs),
            fd: AsyncFd::new(fd).map_err(|error| io_issue(&error))?,
            paths,
        })
    }
}

fn existing_directory(mut path: &Path) -> Result<PathBuf, ServiceDiagnostic> {
    loop {
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => return Ok(path.to_owned()),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::PermissionDenied
                ) => {}
            Err(error) => return Err(io_issue(&error)),
        }
        path = path
            .parent()
            .ok_or_else(|| issue(ServiceCode::SourceUnavailable))?;
    }
}

fn read(fd: i32, bytes: &mut [u8]) -> std::io::Result<usize> {
    loop {
        // SAFETY: bytes is initialized, writable for its exact length, and fd is live.
        let length = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
        if length < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        return Ok(length as usize);
    }
}

struct Handler {
    inputs: Arc<WatchInputs>,
    dirty: Arc<Dirty>,
}

impl EventHandler for Handler {
    fn handle_event(&mut self, event: notify::Result<Event>) {
        mark_event(&self.inputs, &self.dirty, event);
    }
}

fn decode(
    mut bytes: &[u8],
    paths: &BTreeMap<i32, Vec<PathBuf>>,
    handler: &mut impl EventHandler,
) -> Result<(), ServiceDiagnostic> {
    let header = std::mem::size_of::<libc::inotify_event>();
    while !bytes.is_empty() {
        if bytes.len() < header {
            return Err(issue(ServiceCode::SourceUnavailable));
        }
        // SAFETY: the checked byte slice contains a complete header. The read is
        // explicitly unaligned; no reference to a potentially unaligned struct forms.
        let event =
            unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<libc::inotify_event>()) };
        let size = header
            .checked_add(event.len as usize)
            .filter(|size| *size <= bytes.len())
            .ok_or_else(|| issue(ServiceCode::SourceUnavailable))?;
        let mask = event.mask;
        if mask & libc::IN_Q_OVERFLOW != 0 {
            handler.handle_event(Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)));
        } else if let Some(anchors) = paths.get(&event.wd) {
            let name = &bytes[header..size];
            let length = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            let name = OsString::from_vec(name[..length].to_vec());
            let structural = mask
                & (libc::IN_CREATE
                    | libc::IN_DELETE
                    | libc::IN_MOVED_FROM
                    | libc::IN_MOVED_TO
                    | libc::IN_DELETE_SELF
                    | libc::IN_MOVE_SELF
                    | libc::IN_IGNORED)
                != 0;
            let kind = if mask & (libc::IN_IGNORED | libc::IN_MOVE_SELF | libc::IN_DELETE_SELF) != 0
            {
                EventKind::Other
            } else if mask & (libc::IN_MOVED_FROM | libc::IN_MOVED_TO) != 0 {
                EventKind::Modify(ModifyKind::Name(RenameMode::Any))
            } else if mask & libc::IN_CREATE != 0 {
                EventKind::Create(CreateKind::Any)
            } else if mask & libc::IN_DELETE != 0 {
                EventKind::Remove(RemoveKind::Any)
            } else {
                EventKind::Modify(ModifyKind::Data(DataChange::Any))
            };
            for anchor in anchors {
                let mut next = Event::new(kind).add_path(anchor.join(&name));
                // Directory shape affects admission even before any leaf exists.
                if mask & libc::IN_ISDIR != 0 {
                    next.attrs.set_info("directory");
                }
                if structural {
                    next = next.set_flag(Flag::Rescan);
                }
                handler.handle_event(Ok(next));
            }
        }
        bytes = &bytes[size..];
    }
    Ok(())
}

fn mark_event(inputs: &WatchInputs, dirty: &Dirty, event: notify::Result<Event>) {
    let Ok(event) = event else {
        dirty.mark(REARM);
        return;
    };
    if event.kind.is_access() && !event.need_rescan() {
        return;
    }
    let relevant = event.paths.is_empty() && event.need_rescan()
        || event
            .paths
            .iter()
            .any(|path| matches_input(inputs, path, event.info() == Some("directory")));
    if relevant {
        dirty.mark(if event.need_rescan() { REARM } else { 0 });
    }
}

fn matches_input(inputs: &WatchInputs, path: &Path, directory: bool) -> bool {
    inputs.targets.iter().any(|target| {
        if target.path == path || target.path.starts_with(path) {
            return true;
        }
        let Ok(relative) = path.strip_prefix(&target.path) else {
            return false;
        };
        match target.kind {
            InputKind::File => false,
            InputKind::AgentTree => directory || path.extension().is_some_and(|ext| ext == "md"),
            InputKind::MarkdownDirectory => {
                relative.components().count() == 1
                    && !directory
                    && path.extension().is_some_and(|ext| ext == "md")
            }
            InputKind::Skills => match relative.components().count() {
                1 => directory || path.extension().is_some_and(|ext| ext == "md"),
                2 => path.file_name().is_some_and(|name| name == "SKILL.md"),
                _ => false,
            },
            InputKind::SkillDirectory => {
                relative.components().count() == 1
                    && path.file_name().is_some_and(|name| name == "SKILL.md")
            }
        }
    })
}

fn io_issue(error: &std::io::Error) -> ServiceDiagnostic {
    issue(
        if matches!(
            error.raw_os_error(),
            Some(libc::ENOSPC | libc::EMFILE | libc::ENFILE)
        ) {
            ServiceCode::CapacityExceeded
        } else {
            ServiceCode::SourceUnavailable
        },
    )
}

fn issue(code: ServiceCode) -> ServiceDiagnostic {
    crate::config::diagnostic::failure(
        "native reload observer",
        &["watch"],
        ServiceStage::Admission,
        code,
        if code == ServiceCode::CapacityExceeded {
            ServiceAction::ReduceCapacity
        } else {
            ServiceAction::ReviewConfiguration
        },
    )
}

#[cfg(test)]
mod tests;
