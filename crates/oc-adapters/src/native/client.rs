use super::{
    Lease, endpoint,
    framing::{self, Budget, Message},
    issue,
    protocol::{Bootstrap, Client, Expected, Hello, Server, Startup, Welcome},
    wire::{self, Pending},
};
use crate::application::SpawnDiagnostic;
use oc_core::{
    core_app::{CoreApp, CoreEvent, InboxMsg, NativeAdmissions, NativeControl, WorkerGuard},
    queries::{ServiceCode, StartupNotice},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream as StdStream,
    },
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::{
    net::UnixStream,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

/// Attach the existing concrete command/event facade to the elected native
/// owner. The caller's environment is never sent to an existing owner.
pub async fn attach(
    data: &Path,
    project: Option<&Path>,
    lease: Lease,
) -> Result<(CoreApp, WorkerGuard, Vec<StartupNotice>), SpawnDiagnostic> {
    let root = endpoint::candidate(data).map_err(|error| endpoint::diagnostic(data, error))?;
    let budget = Budget::default();
    loop {
        let mut startup = None;
        let registration = match endpoint::read(&root)
            .map_err(|error| endpoint::diagnostic(&root, error))?
        {
            Some(registration) => match endpoint::connect(&root, &registration).await {
                Ok(stream) => {
                    let exited = process_death(registration.pid);
                    let result =
                        handshake(stream, registration, project, lease, budget.clone(), None).await;
                    // This exact typed cause denotes the private HELLO socket,
                    // not a provider/MCP/configuration transport failure. No
                    // application request has been issued, so after observing
                    // the old process exit it is safe to elect a replacement.
                    if matches!(&result, Err(error) if *error == issue(ServiceCode::Transport)) {
                        let gone = match exited {
                            Ok(exited) => matches!(
                                tokio::time::timeout(Duration::from_secs(10), exited.readable())
                                    .await,
                                Ok(Ok(_))
                            ),
                            Err(error) => error.raw_os_error() == Some(libc::ESRCH),
                        };
                        if gone {
                            continue;
                        }
                    }
                    return result;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) =>
                {
                    launch(&root, project, lease, &budget, &mut startup).await?
                }
                Err(error) => return Err(endpoint::diagnostic(&root, error)),
            },
            None => launch(&root, project, lease, &budget, &mut startup).await?,
        };
        let stream = endpoint::connect(&root, &registration)
            .await
            .map_err(|_| issue(ServiceCode::ConnectionFailed))?;
        return handshake(stream, registration, project, lease, budget, startup).await;
    }
}

async fn launch(
    root: &Path,
    project: Option<&Path>,
    lease: Lease,
    budget: &Budget,
    startup: &mut Option<UnixStream>,
) -> Result<endpoint::Registration, SpawnDiagnostic> {
    let (parent, child) = StdStream::pair().map_err(|_| issue(ServiceCode::SpawnFailed))?;
    parent
        .set_nonblocking(true)
        .map_err(|_| issue(ServiceCode::SpawnFailed))?;
    let descriptor = child.as_raw_fd();
    let mut command = tokio::process::Command::new(
        std::env::current_exe().map_err(|_| issue(ServiceCode::SpawnFailed))?,
    );
    command
        .arg("--data-dir")
        .arg(root)
        .arg("__native-owner")
        .arg("--bootstrap-fd")
        .arg(descriptor.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    let inherit = move || {
        // SAFETY: async-signal-safe operation on this launch's owned socket.
        if unsafe { libc::fcntl(descriptor, libc::F_SETFD, 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: setsid is async-signal-safe and touches no Rust memory.
        if unsafe { libc::setsid() } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    };
    // SAFETY: the closure performs only the async-signal-safe operations above.
    unsafe {
        command.pre_exec(inherit);
    }
    let child_process = command
        .spawn()
        .map_err(|_| issue(ServiceCode::SpawnFailed))?;
    drop(child);
    // Tokio retains responsibility for reaping this child after a viewer
    // detaches; it must not kill an owner that another lease still uses.
    drop(child_process);
    let mut parent = UnixStream::from_std(parent).map_err(|_| issue(ServiceCode::SpawnFailed))?;
    let intent = Startup {
        version: endpoint::VERSION,
        lease,
        project: project.map(|path| path.to_string_lossy().into_owned()),
    };
    framing::write(
        &mut parent,
        &Message::encode(&intent, budget).map_err(|_| issue(ServiceCode::CapacityExceeded))?,
    )
    .await
    .map_err(|_| issue(ServiceCode::Transport))?;
    let response =
        tokio::time::timeout(Duration::from_secs(10), framing::read(&mut parent, budget))
            .await
            .map_err(|_| issue(ServiceCode::Deadline))?
            .map_err(|_| issue(ServiceCode::SpawnFailed))?
            .decode::<Bootstrap>()
            .map_err(|_| issue(ServiceCode::ProtocolMismatch))?;
    match response {
        Bootstrap::Ready(registration) => {
            *startup = Some(parent);
            Ok(registration)
        }
        Bootstrap::Failed(error) => Err(error),
        Bootstrap::Busy => wait_registration(root).await,
    }
}

async fn wait_registration(root: &Path) -> Result<endpoint::Registration, SpawnDiagnostic> {
    use notify::Watcher as _;
    let (tx, mut changes) = mpsc::channel(1);
    let mut watch = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
        let _ = tx.try_send(());
    })
    .map_err(|_| issue(ServiceCode::DataRootBusy))?;
    watch
        .watch(root, notify::RecursiveMode::NonRecursive)
        .map_err(|_| issue(ServiceCode::DataRootBusy))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(registration) =
            endpoint::read(root).map_err(|error| endpoint::diagnostic(root, error))?
            && endpoint::connect(root, &registration).await.is_ok()
        {
            return Ok(registration);
        }
        tokio::time::timeout_at(deadline, changes.recv())
            .await
            .map_err(|_| issue(ServiceCode::DataRootBusy))?
            .ok_or_else(|| issue(ServiceCode::DataRootBusy))?;
    }
}

async fn handshake(
    mut stream: UnixStream,
    registration: endpoint::Registration,
    project: Option<&Path>,
    lease: Lease,
    budget: Budget,
    startup: Option<UnixStream>,
) -> Result<(CoreApp, WorkerGuard, Vec<StartupNotice>), SpawnDiagnostic> {
    if registration.version != endpoint::VERSION {
        return Err(issue(ServiceCode::ProtocolMismatch));
    }
    let death =
        process_death(registration.pid).map_err(|_| issue(ServiceCode::ConnectionFailed))?;
    let hello = Hello {
        version: endpoint::VERSION,
        instance: registration.instance.clone(),
        lease,
        project: project.map(|path| path.to_string_lossy().into_owned()),
    };
    framing::write(
        &mut stream,
        &Message::encode(&hello, &budget).map_err(|_| issue(ServiceCode::CapacityExceeded))?,
    )
    .await
    .map_err(|_| issue(ServiceCode::Transport))?;
    let welcome: Result<Welcome, SpawnDiagnostic> = framing::read(&mut stream, &budget)
        .await
        .map_err(|_| issue(ServiceCode::Transport))?
        .decode()
        .map_err(|_| issue(ServiceCode::ProtocolMismatch))?;
    crate::trace::attach_default();
    let welcome = welcome?;
    if welcome.version != endpoint::VERSION || welcome.instance != registration.instance {
        return Err(issue(ServiceCode::ProtocolMismatch));
    }
    match lease {
        Lease::Tui => crate::trace::log("tui.begin", "project=configured"),
        Lease::Run => crate::trace::log("headless.begin", "project=configured"),
        Lease::Auth | Lease::Sessions => {}
    }
    drop(startup); // Ready attachment now owns the startup intent's lifetime.
    let (app, inbox, controls, events, admissions) =
        CoreApp::native_channel(oc_core::session::MAX_QUEUE_ITEMS, welcome.attachment);
    let notices = welcome.notices;
    let task = tokio::spawn(async move {
        bridge(Bridge {
            stream,
            inbox,
            controls,
            admissions,
            lease,
            events,
            budget,
            epoch: welcome.location_epoch,
            death,
        })
        .await
        .map_err(|_| issue(ServiceCode::Transport).diagnostic)
    });
    Ok((app, WorkerGuard::from_diagnostic_task(task), notices))
}

fn process_death(pid: u32) -> io::Result<tokio::io::unix::AsyncFd<OwnedFd>> {
    // SAFETY: pidfd_open returns a newly owned descriptor for this admitted
    // actual socket peer; it never sends a signal or races PID reuse.
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful syscall returned a fresh owned descriptor.
    tokio::io::unix::AsyncFd::new(unsafe { OwnedFd::from_raw_fd(descriptor as i32) })
}

pub(super) struct Pipes {
    pub incoming: mpsc::Receiver<io::Result<Message>>,
    pub outgoing: mpsc::Sender<Message>,
    pub priority: mpsc::Sender<Message>,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}
impl Pipes {
    pub(super) fn start(stream: UnixStream, budget: Budget) -> Self {
        let (mut read, mut write) = stream.into_split();
        let (input, incoming) = mpsc::channel(oc_core::session::MAX_QUEUE_ITEMS);
        let (outgoing, mut output) = mpsc::channel::<Message>(oc_core::session::MAX_QUEUE_ITEMS);
        let (priority, mut controls) = mpsc::channel::<Message>(16);
        let reader = tokio::spawn(async move {
            loop {
                let message = framing::read(&mut read, &budget).await;
                let failed = message.is_err();
                if input.send(message).await.is_err() || failed {
                    break;
                }
            }
        });
        let writer = tokio::spawn(async move {
            loop {
                let message = tokio::select! {
                    biased;
                    Some(message) = controls.recv() => message,
                    Some(message) = output.recv() => message,
                    else => break,
                };
                if !matches!(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        framing::write(&mut write, &message)
                    )
                    .await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
        });
        Self {
            incoming,
            outgoing,
            priority,
            reader,
            writer,
        }
    }
    pub(super) async fn stop(mut self) {
        self.reader.abort();
        self.writer.abort();
        let _ = (&mut self.reader).await;
        let _ = (&mut self.writer).await;
    }

    pub(super) async fn finish(mut self) {
        self.incoming.close();
        // Close the real writer queues while retaining Drop's abort ownership.
        drop(std::mem::replace(&mut self.outgoing, mpsc::channel(1).0));
        drop(std::mem::replace(&mut self.priority, mpsc::channel(1).0));
        if tokio::time::timeout(Duration::from_secs(10), &mut self.writer)
            .await
            .is_err()
        {
            self.writer.abort();
            let _ = (&mut self.writer).await;
        }
        self.reader.abort();
        let _ = (&mut self.reader).await;
    }
}
impl Drop for Pipes {
    fn drop(&mut self) {
        // In particular WorkerGuard/task abort must close both socket halves,
        // not detach reader/writer tasks and leave a phantom ready TUI lease.
        self.reader.abort();
        self.writer.abort();
    }
}

fn control_session(request: &wire::Request) -> Option<&str> {
    match request {
        wire::Request::Cancel { session }
        | wire::Request::CancelCompaction { session }
        | wire::Request::ChangeConversation { session, .. } => Some(&session.0),
        _ => None,
    }
}
fn submission(request: &wire::Request) -> Option<&str> {
    match request {
        wire::Request::Submit { session, .. }
        | wire::Request::SubmitFresh { session, .. }
        | wire::Request::UserShell { session, .. }
        | wire::Request::Compress { session, .. }
        | wire::Request::CompactSession { session }
        | wire::Request::RegenerateTitle { session } => Some(&session.0),
        _ => None,
    }
}

struct Bridge {
    stream: UnixStream,
    inbox: mpsc::Receiver<InboxMsg>,
    controls: mpsc::Receiver<NativeControl>,
    admissions: NativeAdmissions,
    lease: Lease,
    events: broadcast::Sender<CoreEvent>,
    budget: Budget,
    epoch: u64,
    death: tokio::io::unix::AsyncFd<OwnedFd>,
}

// Observations order snapshots against the wire, while owned request receipts
// remain independently cancellable. Idle observations are temporary tombstones,
// not a completed-session archive or an execution registry.
#[derive(Default)]
struct Operation {
    observed: Option<(u64, Option<u64>)>,
    owned: BTreeMap<u64, OwnedOperation>,
    barrier: u64,
}

struct OwnedOperation {
    origin: Option<u64>,
    completion: OwnedCompletion,
    acknowledged_at: Option<u64>,
}

#[derive(PartialEq, Eq)]
enum OwnedCompletion {
    Reply,
    Turn,
    Compaction,
}

impl Operation {
    fn expected(&self) -> Option<Expected> {
        self.owned
            .last_key_value()
            .map(|(&id, owned)| owned.origin.map_or(Expected::Pending(id), Expected::Origin))
            .or_else(|| {
                self.observed
                    .and_then(|(_, origin)| origin.map(Expected::Origin))
            })
    }

    fn observe(&mut self, revision: u64, origin: Option<u64>, barrier: u64) {
        if self.observed.is_none_or(|(current, _)| revision >= current) {
            self.observed = Some((revision, origin));
            self.barrier = barrier;
        }
    }

    fn snapshot(&mut self, revision: u64, origin: Option<u64>, barrier: u64, request: u64) {
        if self.observed.is_some_and(|(floor, _)| revision < floor) {
            return;
        }
        self.observe(revision, origin, barrier);
        // An accepted turn's start/terminal may have been lost in a wire gap.
        // Only a query SENT after its ACK supersedes that provisional owner
        // with authoritative current (possibly successor/idle) turn state.
        // Live-turn snapshots say nothing about queued compaction.
        self.owned.retain(|_, owned| {
            owned.completion != OwnedCompletion::Turn
                || owned.acknowledged_at.is_none_or(|ack| request <= ack)
        });
    }
}

async fn bridge(bridge: Bridge) -> Result<(), ()> {
    let Bridge {
        stream,
        mut inbox,
        mut controls,
        admissions,
        lease,
        events,
        budget,
        mut epoch,
        death,
    } = bridge;
    let mut pipes = Pipes::start(stream, budget.clone());
    let mut pending: BTreeMap<u64, Pending> = BTreeMap::new();
    let mut control_pending = BTreeSet::new();
    let mut submitted = BTreeMap::new();
    // Control frames are small and cannot be denied by bulk work's byte quota.
    let control_budget = Budget::default();
    let mut operations = BTreeMap::<String, Operation>::new();
    let mut sequence = 0_u64;
    let mut releasing = false;
    let mut closing = false;
    let mut revision = None;
    let mut snapshots = BTreeMap::<u64, (Option<oc_core::domain::SessionId>, u64)>::new();
    let result = 'bridge: loop {
        tokio::select! {
            message = controls.recv(), if !releasing && control_pending.len() < 16 => {
                let Some(NativeControl::Cancel { session, turn, admission, ack }) = message else {
                    releasing = true;
                    if send(&pipes.priority, &control_budget, &Client::Release).is_err() { break Err(()); }
                    continue;
                };
                if admission.as_ref().is_some_and(|entry| entry.request.load(std::sync::atomic::Ordering::Acquire) == 0) {
                    // The queued input has never crossed the socket. Its exact
                    // local identity is cancelled, not the next input/session.
                    let _ = ack.send(Ok(())); continue;
                }
                let (request, reply) = Pending::from_inbox(InboxMsg::Cancel { session: session.clone(), ack }).expect("cancel receipt");
                sequence = match sequence.checked_add(1) { Some(value) => value, None => { reply.finish(Err(wire::unknown())); break Err(()); } };
                let expected = admission.map(|entry| Expected::Pending(entry.request.load(std::sync::atomic::Ordering::Acquire)))
                    .or_else(|| control_session(&request).and_then(|session| operations.get(session).and_then(Operation::expected)));
                let frame = Client::Control { id: sequence, location_epoch: epoch, expected, session, turn };
                if send(&pipes.priority, &control_budget, &frame).is_err() { reply.finish(Err(oc_core::session::CoreError::QueueFull)); break Err(()); }
                pending.insert(sequence, reply); control_pending.insert(sequence);
            },
            incoming = pipes.incoming.recv() => {
                // Only Released confirms joined cleanup. EOF after Release can
                // be the owner's fatal cleanup exit, even with no pending ACKs.
                let Some(Ok(message)) = incoming else { break Err(()); };
                let frame = match message.decode::<Server>() { Ok(frame) => frame, Err(_) => break Err(()) };
                match frame {
                    Server::Reply { id, location_epoch, result } => {
                        epoch = epoch.max(location_epoch);
                        let Some(reply) = pending.remove(&id) else { break Err(()); };
                        control_pending.remove(&id);
                        if let Some(entry) = submitted.remove(&id) { admissions.complete(&entry); }
                        // Failure retracts only this request's provisional owner.
                        // A newer pending input cannot erase the previous origin.
                        let accepted = result.as_ref().is_ok_and(|value| value.get("Ok").is_some());
                        for operation in operations.values_mut() {
                            if operation.owned.get(&id).is_some_and(|owned| !accepted || owned.origin.is_none() || owned.completion == OwnedCompletion::Reply) {
                                operation.owned.remove(&id);
                            } else if let Some(owned) = operation.owned.get_mut(&id) {
                                owned.acknowledged_at = Some(sequence);
                            }
                        }
                        if let Some((selected, sent_epoch)) = snapshots.remove(&id)
                            && sent_epoch == epoch && let Ok(value) = &result
                            && let Ok(Ok(snapshot)) = serde_json::from_value::<Result<oc_core::queries::WorkspaceSnapshot, oc_core::session::CoreError>>(value.clone()) {
                                // A workspace covers only its deck and selected
                                // session. It never consumes the shared stream.
                                for session in snapshot.deck.sessions.iter().chain(selected.iter()) {
                                    let origin = snapshot.turns.iter().find(|turn| &turn.session == session).and_then(|turn| turn.origin);
                                    operations.entry(session.0.clone()).or_default().snapshot(snapshot.revision, origin, sequence, id);
                                }
                        }
                        reply.finish(result);
                    },
                    Server::Bound { id, session, origin } => {
                        if let Some(operation) = operations.get_mut(&session.0)
                            && let Some(owned) = operation.owned.get_mut(&id) {
                                owned.origin = Some(origin);
                        }
                    },
                    Server::Event { revision: next, origin, event } => {
                        if revision.is_some_and(|current| next <= current) { continue; }
                        if revision.is_some_and(|current| next != current + 1) {
                            // Missed terminal facts invalidate observations, not
                            // exact local requests still awaiting their receipt.
                            for operation in operations.values_mut() {
                                if operation.observed.is_none_or(|(floor, _)| floor < next) {
                                    operation.observe(next, None, sequence);
                                }
                            }
                            let _ = events.send(CoreEvent::ProjectionGap);
                        }
                        revision = Some(next);
                        let event = event.into_core();
                        if lease == Lease::Tui && let (Some(origin), CoreEvent::TurnStarted { session, .. }) = (origin, &event) {
                            let operation = operations.entry(session.0.clone()).or_default();
                            operation.observe(next, Some(origin), sequence);
                            operation.owned.retain(|_, owned| owned.origin != Some(origin));
                        }
                        if let CoreEvent::TurnFinished { session, .. } | CoreEvent::TurnInterrupted { session, .. } | CoreEvent::TurnFailed { session, .. } = &event {
                            let operation = operations.entry(session.0.clone()).or_default();
                            if operation.observed.is_none_or(|(_, current)| current.is_none() || current == origin) {
                                operation.observe(next, None, sequence);
                            }
                            if origin.is_some() { operation.owned.retain(|_, owned| owned.origin != origin); }
                        }
                        if let CoreEvent::Compaction(snapshot) = &event
                            && matches!(snapshot.state, oc_core::compaction::CompactionState::Completed | oc_core::compaction::CompactionState::Failed | oc_core::compaction::CompactionState::Cancelled)
                            && let Some(operation) = operations.get_mut(&snapshot.session)
                            && origin.is_some() {
                            operation.owned.retain(|_, owned| owned.completion != OwnedCompletion::Compaction || owned.origin != origin);
                        }
                        let event = if lease == Lease::Tui { CoreEvent::Projection { revision: next, event: Box::new(event) } } else { event };
                        let _ = events.send(event);
                    },
                    Server::Released { closing: final_owner } => { closing = final_owner; break Ok(()); },
                }
            },
            message = inbox.recv(), if !releasing && pending.len() - control_pending.len() < oc_core::session::MAX_QUEUE_ITEMS => {
                let Some(message) = message else { releasing = true; if send(&pipes.priority, &control_budget, &Client::Release).is_err() { break Err(()); } continue; };
                let Some((request, reply)) = Pending::from_inbox(message) else { releasing = true; if send(&pipes.priority, &control_budget, &Client::Release).is_err() { break Err(()); } continue; };
                let admission = submission(&request).and_then(|session| admissions.claim(&oc_core::domain::SessionId(session.into())));
                if let Some(entry) = &admission && entry.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    admissions.complete(entry);
                    reply.finish(Err(oc_core::session::CoreError::Application("submission cancelled before dispatch".into())));
                    continue;
                }
                sequence = match sequence.checked_add(1) { Some(value) => value, None => { reply.finish(Err(wire::unknown())); break Err(()); } };
                if let Some(entry) = admission { entry.request.store(sequence, std::sync::atomic::Ordering::Release); submitted.insert(sequence, entry); }
                let expected = control_session(&request).and_then(|session| operations.get(session).and_then(Operation::expected));
                if let wire::Request::Workspace { selected } = &request { snapshots.insert(sequence, (selected.clone(), epoch)); }
                if let Some(session) = submission(&request) {
                    let completion = match request {
                        wire::Request::Submit { .. } | wire::Request::SubmitFresh { .. } | wire::Request::Compress { .. } => OwnedCompletion::Turn,
                        wire::Request::CompactSession { .. } => OwnedCompletion::Compaction,
                        _ => OwnedCompletion::Reply,
                    };
                    operations.entry(session.into()).or_default().owned.insert(sequence, OwnedOperation { origin: None, completion, acknowledged_at: None });
                }
                let frame = Client::Request { id: sequence, location_epoch: epoch, expected, request: Box::new(request) };
                if send(&pipes.outgoing, &budget, &frame).is_err() { reply.finish(Err(oc_core::session::CoreError::QueueFull)); break Err(()); }
                pending.insert(sequence, reply);
            },
            () = tokio::time::sleep(Duration::from_millis(5)), if pending.values().any(Pending::has_cancellation) => {
                let cancelled: Vec<_> = pending.iter_mut().filter_map(|(&id, pending)| pending.take_cancellation().then_some(id)).collect();
                for id in cancelled { if send(&pipes.priority, &control_budget, &Client::CancelRequest { id }).is_err() { break 'bridge Err(()); } }
            },
        }
        operations.retain(|_, operation| {
            !operation.owned.is_empty()
                || operation.observed.is_some_and(|(floor, origin)| {
                    origin.is_some()
                        || revision.is_none_or(|wire| wire < floor)
                        || snapshots
                            .first_key_value()
                            .is_some_and(|(&id, _)| id <= operation.barrier)
                })
        });
        // One stalled old query can otherwise retain unbounded idle ordering
        // facts. This is a local bookkeeping budget, not a runtime root limit.
        if operations
            .values()
            .map(|operation| 1 + operation.owned.len())
            .sum::<usize>()
            > oc_core::session::MAX_QUEUE_ITEMS * (crate::storage::MAX_TABS + 2)
        {
            break Err(());
        }
    };
    for (_, reply) in pending {
        reply.finish(Err(wire::unknown()));
    }
    for (_, entry) in submitted {
        admissions.complete(&entry);
    }
    if result.is_err() {
        let _ = events.send(CoreEvent::OwnerDisconnected);
    }
    pipes.stop().await;
    if closing {
        // Process exit is terminal readiness; there is no event to consume or clear.
        let _exited = tokio::time::timeout(Duration::from_secs(10), death.readable())
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
    }
    result
}

pub(super) fn send(
    output: &mpsc::Sender<Message>,
    budget: &Budget,
    value: &impl serde::Serialize,
) -> io::Result<()> {
    output
        .try_send(Message::encode(value, budget)?)
        .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "native peer egress full"))
}

#[cfg(test)]
mod tests;
