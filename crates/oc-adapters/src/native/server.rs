use super::{
    Lease,
    client::{Pipes, send},
    endpoint::{self, Endpoint},
    framing::{self, Budget, Message},
    issue,
    protocol::{Bootstrap, Client, Expected, Hello, Server, Startup, Welcome},
    wire::{self, Event, Request},
};
use crate::{
    application::{
        self, SpawnDiagnostic, SpawnFailure,
        authority::{Location, Owner},
    },
    runtime::origin::{Kind, Origin},
    storage::{Db, StorageError},
};
use oc_core::{
    core_app::CoreEvent,
    domain::SessionId,
    queries::{ServiceCode, SessionSelectionAction},
    session::CoreError,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    os::unix::net::UnixStream as StdStream,
    path::Path,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    net::UnixStream,
    sync::{mpsc, watch},
    task::JoinSet,
};

#[cfg(test)]
mod tests;

struct Viewer {
    ready: bool,
    lease: Option<Lease>,
    output: Option<mpsc::Sender<Message>>,
    disconnect: watch::Sender<bool>,
}
struct Operation {
    origin: Weak<Origin>,
    attachment: u64,
    request: u64,
}
struct State {
    closing: bool,
    startup: bool,
    sequence: u64,
    revision: u64,
    viewers: BTreeMap<u64, Viewer>,
    operations: BTreeMap<u64, Operation>,
    terminals: BTreeMap<String, (oc_core::queries::TerminalRef, u64, Arc<Location>)>,
    cleanup_failed: bool,
}
impl State {
    fn tui_count(&self) -> usize {
        self.viewers
            .values()
            .filter(|view| view.ready && view.lease == Some(Lease::Tui))
            .count()
    }
    fn disable(&mut self, id: u64, owner: &Owner) {
        let before = self.tui_count();
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.ready = false;
            viewer.output = None;
        }
        if before > 0 && self.tui_count() == 0 {
            owner.resources.tui_present.store(false, Ordering::Release);
            owner.cancel_interactive();
            if owner.stop_interactive_terminals().is_err() {
                self.cleanup_failed = true;
                self.force_close();
            }
            self.terminals.clear();
        }
        for (_, controller, _) in self.terminals.values_mut() {
            if *controller == id {
                *controller = 0;
            }
        }
    }
    fn close_if_empty(&mut self) {
        self.closing |= !self.startup && self.viewers.is_empty();
    }
    fn force_close(&mut self) {
        self.closing = true;
        for viewer in self.viewers.values() {
            viewer.disconnect.send_replace(true);
        }
    }
}

struct Route {
    location: Option<Arc<Location>>,
    epoch: u64,
}
struct Peer {
    id: u64,
    lease: Lease,
    route: Mutex<Route>,
    approval: AtomicU8,
    question: AtomicBool,
    origins: Mutex<Vec<Weak<Origin>>>,
    auth: Mutex<Vec<String>>,
    disconnected: AtomicBool,
    switch_gate: tokio::sync::Mutex<()>,
    admissions: Mutex<BTreeMap<u64, (String, Arc<AtomicBool>)>>,
}
struct LeaseGuard {
    id: u64,
    owner: Arc<Owner>,
    state: Arc<Mutex<State>>,
}
impl Drop for LeaseGuard {
    fn drop(&mut self) {
        let mut shared = self.state.lock().expect("native leases");
        shared.disable(self.id, &self.owner);
        shared.viewers.remove(&self.id);
        shared.close_if_empty();
    }
}
impl Peer {
    fn capture(&self, epoch: u64) -> Result<Option<Arc<Location>>, CoreError> {
        let route = self.route.lock().expect("attachment route");
        if route.epoch != epoch {
            return Err(CoreError::TabDeckConflict);
        }
        Ok(route.location.clone())
    }
    fn location(&self) -> Result<Arc<Location>, CoreError> {
        self.route
            .lock()
            .expect("attachment route")
            .location
            .clone()
            .ok_or_else(|| CoreError::Application("operation requires a project attachment".into()))
    }
    fn epoch(&self) -> u64 {
        self.route.lock().expect("attachment route").epoch
    }
}

/// Internal ELF role. The inherited startup socket is admitted before opening
/// storage; the winning actual Db is retained through the entire authority life.
pub async fn serve(data: &Path, startup: StdStream) -> Result<(), SpawnDiagnostic> {
    startup
        .set_nonblocking(true)
        .map_err(|_| issue(ServiceCode::Transport))?;
    let mut startup = UnixStream::from_std(startup).map_err(|_| issue(ServiceCode::Transport))?;
    if startup
        .peer_cred()
        .map_err(|_| issue(ServiceCode::Unauthorized))?
        .uid()
        != endpoint::uid()
    {
        return Err(issue(ServiceCode::Unauthorized));
    }
    let budget = Budget::default();
    let intent: Startup = tokio::time::timeout(
        Duration::from_secs(3),
        framing::read_limited(&mut startup, &budget, 8192),
    )
    .await
    .map_err(|_| issue(ServiceCode::Deadline))?
    .map_err(|_| issue(ServiceCode::Transport))?
    .decode()
    .map_err(|_| issue(ServiceCode::ProtocolMismatch))?;
    if intent.version != endpoint::VERSION {
        return Err(issue(ServiceCode::ProtocolMismatch));
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let admitted = match (intent.lease, intent.project) {
        (Lease::Tui | Lease::Run, Some(project)) => {
            match crate::composition::load_local_with_env(Path::new(&project), env.clone()).await {
                Ok(composition) => Some(composition),
                Err(error) => {
                    let failure = SpawnDiagnostic::configuration(error);
                    bootstrap(&mut startup, &budget, Bootstrap::Failed(failure.clone())).await?;
                    return Err(failure);
                }
            }
        }
        (Lease::Auth | Lease::Sessions, None) => None,
        _ => return Err(issue(ServiceCode::ProtocolMismatch)),
    };
    let root = match endpoint::root(data) {
        Ok(root) => root,
        Err(error) => {
            let failure = endpoint::diagnostic(data, error);
            bootstrap(&mut startup, &budget, Bootstrap::Failed(failure.clone())).await?;
            return Err(failure);
        }
    };
    let db = match Db::open(&root) {
        Ok(db) => db,
        Err(StorageError::DataRootBusy) => {
            bootstrap(&mut startup, &budget, Bootstrap::Busy).await?;
            return Ok(());
        }
        Err(error) => {
            let failure = SpawnDiagnostic {
                category: match error {
                    StorageError::UnsafeRoot(_) => SpawnFailure::UnsafeDataRoot,
                    StorageError::Io(_) => SpawnFailure::DataRootUnavailable,
                    _ => SpawnFailure::Storage,
                },
                diagnostic: application::storage_diagnostic(&root, &error),
            };
            bootstrap(&mut startup, &budget, Bootstrap::Failed(failure.clone())).await?;
            return Err(failure);
        }
    };
    // Only this winner can truncate its startup trace. Ordinary clients and
    // losing contenders never initialize an authority trace sink.
    crate::trace::init_default();
    crate::trace::log("startup.begin", "role=native-owner");
    let owner = match Owner::new(db, env) {
        Ok(owner) => owner,
        Err(error) => {
            bootstrap(&mut startup, &budget, Bootstrap::Failed(error.clone())).await?;
            return Err(error);
        }
    };
    if let Some(composition) = admitted {
        owner.prime(composition);
    }
    let endpoint = match Endpoint::bind(&root) {
        Ok(endpoint) => endpoint,
        Err(_) => {
            let _ = owner.shutdown().await;
            return Err(issue(ServiceCode::UnsafeDataRoot));
        }
    };
    if bootstrap(
        &mut startup,
        &budget,
        Bootstrap::Ready(endpoint.registration.clone()),
    )
    .await
    .is_err()
    {
        let _ = owner.shutdown().await;
        return Ok(());
    }
    let state = Arc::new(Mutex::new(State {
        closing: false,
        startup: true,
        sequence: 0,
        revision: 0,
        viewers: BTreeMap::new(),
        operations: BTreeMap::new(),
        terminals: BTreeMap::new(),
        cleanup_failed: false,
    }));
    let mut peers = JoinSet::new();
    let mut tails = Vec::new();
    let mut events = owner.events.subscribe();
    let mut live = owner.resources.live.subscribe();
    let mut startup_open = true;
    let mut fatal = None;
    loop {
        let closing = state.lock().expect("native leases").closing;
        if closing && peers.is_empty() {
            break;
        }
        tokio::select! {
            accepted = endpoint.listener.accept(), if !closing => {
                let Ok((stream, _)) = accepted else { fatal = Some(issue(ServiceCode::Transport)); state.lock().expect("native leases").force_close(); continue; };
                if !stream.peer_cred().is_ok_and(|peer| peer.uid() == endpoint::uid()) { continue; }
                let mut shared = state.lock().expect("native leases");
                if shared.closing || shared.viewers.len() >= oc_core::session::MAX_QUEUE_ITEMS { continue; }
                shared.sequence = shared.sequence.checked_add(1).ok_or_else(|| issue(ServiceCode::CapacityExceeded))?;
                let id = shared.sequence;
                let (disconnect, closed) = watch::channel(false);
                shared.viewers.insert(id, Viewer { ready: false, lease: None, output: None, disconnect });
                drop(shared);
                peers.spawn(connection(stream, id, endpoint.registration.clone(), owner.clone(), state.clone(), budget.clone(), closed));
            },
            _ = startup.read_u8(), if startup_open => {
                startup_open = false;
                let mut shared = state.lock().expect("native leases");
                shared.startup = false; shared.close_if_empty();
            },
            settled = peers.join_next(), if !peers.is_empty() => {
                match settled {
                    Some(Ok(Some(tail))) => tails.push(tail),
                    Some(Err(_)) => { fatal = Some(issue(ServiceCode::RuntimeFailed)); state.lock().expect("native leases").force_close(); },
                    _ => {},
                }
                if let Err(diagnostic) = owner.retire_idle().await { fatal = Some(SpawnDiagnostic { category: SpawnFailure::Runtime, diagnostic }); state.lock().expect("native leases").force_close(); }
            },
            result = owner.next_root() => {
                if let Err(diagnostic) = result { fatal = Some(SpawnDiagnostic { category: SpawnFailure::Runtime, diagnostic }); state.lock().expect("native leases").force_close(); }
                if let Err(diagnostic) = owner.retire_idle().await { fatal = Some(SpawnDiagnostic { category: SpawnFailure::Runtime, diagnostic }); state.lock().expect("native leases").force_close(); }
            },
            () = owner.resources.retired.notified() => {
                if let Err(diagnostic) = owner.retire_idle().await { fatal = Some(SpawnDiagnostic { category: SpawnFailure::Runtime, diagnostic }); state.lock().expect("native leases").force_close(); }
            },
            event = events.recv(), if !closing => {
                match event {
                    Ok(event) if application::authority::live::identity(&event).is_none() => owner.resources.live.emit("", None, event),
                    Ok(_) => {},
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // A peer must not continue pretending its event stream
                        // is complete. Reattachment obtains a fresh projection.
                        owner.resources.live.emit("", None, CoreEvent::ProjectionGap);
                    },
                    Err(_) => { state.lock().expect("native leases").force_close(); },
                }
            },
            event = live.recv(), if !closing => {
                match event {
                    Ok(event) => publish(&owner, &state, &budget, event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => owner.resources.live.emit("", None, CoreEvent::ProjectionGap),
                    Err(_) => { state.lock().expect("native leases").force_close(); },
                }
            },
        }
    }
    let cleanup = owner.shutdown().await;
    if state.lock().expect("native leases").cleanup_failed {
        fatal = Some(issue(ServiceCode::RuntimeFailed));
    }
    for tail in tails {
        if cleanup.is_ok() && fatal.is_none() {
            let _ = send(&tail.outgoing, &budget, &Server::Released { closing: true });
        }
        tail.finish().await;
    }
    drop(owner);
    drop(endpoint);
    if let Some(error) = fatal {
        return Err(error);
    }
    cleanup.map_err(|diagnostic| SpawnDiagnostic {
        category: SpawnFailure::Runtime,
        diagnostic,
    })
}

async fn bootstrap(
    stream: &mut UnixStream,
    budget: &Budget,
    message: Bootstrap,
) -> Result<(), SpawnDiagnostic> {
    framing::write(
        stream,
        &Message::encode(&message, budget).map_err(|_| issue(ServiceCode::CapacityExceeded))?,
    )
    .await
    .map_err(|_| issue(ServiceCode::Transport))
}

async fn connection(
    mut stream: UnixStream,
    id: u64,
    registration: endpoint::Registration,
    owner: Arc<Owner>,
    state: Arc<Mutex<State>>,
    budget: Budget,
    mut closed: watch::Receiver<bool>,
) -> Option<Pipes> {
    let _lease = LeaseGuard {
        id,
        owner: owner.clone(),
        state: state.clone(),
    };
    let hello = match tokio::time::timeout(
        Duration::from_secs(3),
        framing::read_limited(&mut stream, &budget, 8192),
    )
    .await
    {
        Ok(Ok(message)) => message.decode::<Hello>().ok(),
        _ => None,
    };
    let result = async {
        let hello = hello.ok_or_else(|| issue(ServiceCode::ProtocolMismatch))?;
        if hello.version != endpoint::VERSION || hello.instance != registration.instance {
            return Err(issue(ServiceCode::ProtocolMismatch));
        }
        let location = match (hello.lease, hello.project) {
            (Lease::Tui | Lease::Run, Some(project)) => Some(
                owner
                    .location(Path::new(&project), hello.lease == Lease::Tui)
                    .await?,
            ),
            (Lease::Auth | Lease::Sessions, None) => None,
            _ => return Err(issue(ServiceCode::ProtocolMismatch)),
        };
        Ok((hello.lease, location))
    }
    .await;
    let (lease, location) = match result {
        Ok(value) => value,
        Err(error) => {
            if let Ok(message) = Message::encode(&Err::<Welcome, _>(error), &budget) {
                let _ = framing::write(&mut stream, &message).await;
            }
            let mut shared = state.lock().expect("native leases");
            shared.viewers.remove(&id);
            shared.close_if_empty();
            return None;
        }
    };
    // A successful ready ACK is a lifetime guarantee: B must already count
    // before A can read it and disconnect. Live hints missed before output is
    // installed are covered by B's initial authoritative workspace snapshot.
    {
        let mut shared = state.lock().expect("native leases");
        if shared.closing {
            return None;
        }
        if let Some(viewer) = shared.viewers.get_mut(&id) {
            viewer.ready = true;
            viewer.lease = Some(lease);
        }
        if lease == Lease::Tui {
            owner.resources.tui_present.store(true, Ordering::Release);
        }
    }
    let welcome = Welcome {
        version: endpoint::VERSION,
        instance: registration.instance,
        attachment: id,
        location_epoch: 0,
        notices: location
            .as_ref()
            .map(|location| location.notices.clone())
            .unwrap_or_default(),
    };
    if *closed.borrow()
        || framing::write(
            &mut stream,
            &Message::encode(&Ok::<_, SpawnDiagnostic>(welcome), &budget).ok()?,
        )
        .await
        .is_err()
    {
        return None;
    }
    let mut pipes = Pipes::start(stream, budget.clone());
    let peer = Arc::new(Peer {
        id,
        lease,
        route: Mutex::new(Route { location, epoch: 0 }),
        approval: AtomicU8::new(0),
        question: AtomicBool::new(false),
        origins: Mutex::default(),
        auth: Mutex::default(),
        disconnected: AtomicBool::new(false),
        switch_gate: tokio::sync::Mutex::new(()),
        admissions: Mutex::default(),
    });
    let refused = {
        let mut shared = state.lock().expect("native leases");
        if shared.closing {
            true
        } else {
            if let Some(viewer) = shared.viewers.get_mut(&id) {
                viewer.ready = true;
                viewer.lease = Some(lease);
                viewer.output = Some(pipes.outgoing.clone());
            }
            false
        }
    };
    if refused {
        pipes.stop().await;
        return None;
    }
    let mut work = JoinSet::new();
    let mut cancellation = BTreeMap::<u64, (Arc<AtomicBool>, bool)>::new();
    let mut pending_cancellations = BTreeSet::<u64>::new();
    let mut sequence = 0;
    let mut control_sequence = 0;
    let mut overtaken = BTreeMap::<u64, String>::new();
    let mut released = false;
    loop {
        tokio::select! {
            incoming = pipes.incoming.recv() => {
                let Some(Ok(message)) = incoming else { break; };
                let frame = match message.decode::<Client>() { Ok(frame) => frame, Err(_) => break };
                match frame {
                    Client::Release => { released = true; break; },
                    Client::CancelRequest { id } => {
                        if let Some((cancel, _)) = cancellation.get(&id) { cancel.store(true, Ordering::Release); }
                        else if id > sequence {
                            // Priority frames can overtake their queued work.
                            // Retain only this finite ingress window, never a
                            // durable retry ledger or cancellation by session.
                            if pending_cancellations.len() >= oc_core::session::MAX_QUEUE_ITEMS && !pending_cancellations.contains(&id) { break; }
                            pending_cancellations.insert(id);
                        }
                    },
                    Client::Control { id: control_id, location_epoch, expected, session, turn } => {
                        if control_id <= control_sequence { break; }
                        control_sequence = control_id;
                        let result = if peer.epoch() != location_epoch { Err(CoreError::TabDeckConflict) }
                        else if let Some(Expected::Pending(request)) = expected {
                            if let Some((owned_session, cancel)) = peer.admissions.lock().expect("peer admissions").get(&request) {
                                if owned_session == &session.0 { cancel.store(true, Ordering::Release); Ok(()) }
                                else { Err(CoreError::TurnNotActive) }
                            } else if request > sequence && request - sequence <= (oc_core::session::MAX_QUEUE_ITEMS + 16) as u64 && overtaken.len() < oc_core::session::MAX_QUEUE_ITEMS {
                                overtaken.insert(request, session.0.clone()); Ok(())
                            } else { cancel_operation(&owner, &state, &peer, &session, expected, turn.as_ref()) }
                        } else { cancel_operation(&owner, &state, &peer, &session, expected, turn.as_ref()) };
                        let result = serde_json::to_value(result).map_err(|_| wire::unknown());
                        if send(&pipes.priority, &budget, &Server::Reply { id: control_id, location_epoch: peer.epoch(), result }).is_err() { break; }
                    },
                    Client::Request { id: request_id, location_epoch, expected, request } => {
                        let mut request = *request;
                        request.bind_attachment(peer.id);
                        if request_id <= sequence || work.len() >= oc_core::session::MAX_QUEUE_ITEMS { break; }
                        sequence = request_id;
                        if pending_cancellations.remove(&request_id) {
                            let Some(flag) = request.cancellation() else { break; };
                            flag.store(true, Ordering::Release);
                        }
                        pending_cancellations.retain(|id| *id > sequence);
                        if let Some(session) = execution(&request) {
                            let cancel = request.cancellation().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
                            if let Some(cancelled_session) = overtaken.remove(&request_id) {
                                if cancelled_session != session.0 { break; }
                                cancel.store(true, Ordering::Release);
                            }
                            peer.admissions.lock().expect("peer admissions").insert(request_id, (session.0.clone(), cancel));
                        }
                        if let Some(flag) = request.cancellation() { cancellation.insert(request_id, (flag, matches!(request, Request::UserShell { .. }))); }
                        let output = pipes.outgoing.clone(); let request_budget = budget.clone(); let request_owner = owner.clone(); let request_peer = peer.clone(); let request_state = state.clone();
                        work.spawn(async move {
                            // Retain the ingress reservation through typed request
                            // ownership and acknowledgement, including a lost viewer.
                            let _ingress = message;
                            let result = handle(&request_owner, &request_state, &request_peer, &output, &request_budget, request_id, location_epoch, expected, request).await;
                            let frame = Server::Reply { id: request_id, location_epoch: request_peer.epoch(), result };
                            let sent = send(&output, &request_budget, &frame).is_ok();
                            (request_id, sent)
                        });
                    },
                }
            },
            joined = work.join_next(), if !work.is_empty() => {
                match joined { Some(Ok((id, true))) => { cancellation.remove(&id); peer.admissions.lock().expect("peer admissions").remove(&id); owner.resources.retired.notify_one(); }, _ => break }
            },
            _ = closed.changed() => { if *closed.borrow() { break; } },
        }
    }
    peer.disconnected.store(true, Ordering::Release);
    let early_release = {
        let mut shared = state.lock().expect("native leases");
        shared.disable(id, &owner);
        released
            && lease == Lease::Tui
            && shared
                .viewers
                .iter()
                .any(|(&other, viewer)| other != id && viewer.ready)
    };
    if early_release {
        let _ = send(
            &pipes.priority,
            &budget,
            &Server::Released { closing: false },
        );
    }
    for (flag, interactive_shell) in cancellation.values() {
        if !interactive_shell || lease != Lease::Tui {
            flag.store(true, Ordering::Release);
        }
    }
    if lease != Lease::Tui {
        for origin in peer
            .origins
            .lock()
            .expect("peer operations")
            .iter()
            .filter_map(Weak::upgrade)
        {
            origin.cancel();
        }
    }
    while work.join_next().await.is_some() {}
    if lease != Lease::Tui {
        let origins: Vec<_> = peer
            .origins
            .lock()
            .expect("peer operations")
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for origin in origins {
            owner.settle_origin(origin.id).await;
            origin.settle().await;
        }
    }
    let attempts = std::mem::take(&mut *peer.auth.lock().expect("peer authentication"));
    owner.cancel_auth_attempts(attempts).await;
    let closing = {
        let mut shared = state.lock().expect("native leases");
        shared.viewers.remove(&id);
        shared.close_if_empty();
        shared.closing
    };
    drop(peer);
    if closing && released {
        return Some(pipes);
    }
    if released {
        if !early_release {
            let _ = send(
                &pipes.outgoing,
                &budget,
                &Server::Released { closing: false },
            );
        }
        // Early handoff still owns the queued release ACK. Drain it before
        // closing; aborting the writer here turns a healthy detach into EOF.
        pipes.finish().await;
    } else {
        pipes.stop().await;
    }
    None
}

fn cancel_operation(
    owner: &Owner,
    state: &Mutex<State>,
    peer: &Peer,
    session: &SessionId,
    expected: Option<Expected>,
    turn: Option<&oc_core::core_app::WorkerTurnId>,
) -> Result<(), CoreError> {
    let location = peer.location()?;
    let origin = match expected {
        Some(Expected::Origin(origin)) => origin,
        Some(Expected::Pending(request)) => state
            .lock()
            .expect("native leases")
            .operations
            .iter()
            .find(|(_, operation)| operation.attachment == peer.id && operation.request == request)
            .map(|(&origin, _)| origin)
            .ok_or(CoreError::TurnNotActive)?,
        None => return Err(CoreError::TurnNotActive),
    };
    owner.cancel_root(&location, &session.0, origin, turn)
}

fn publish(
    owner: &Owner,
    state: &Mutex<State>,
    budget: &Budget,
    event: application::authority::live::Publication,
) {
    let terminals = if let CoreEvent::TerminalChanged { session } = &event.event {
        owner
            .resources
            .terminals
            .lock()
            .expect("terminal owner")
            .as_mut()
            .ok()
            .and_then(|terminals| terminals.inventory(session).ok())
            .map(|inventory| {
                (
                    session.clone(),
                    inventory
                        .entries
                        .into_iter()
                        .filter(|entry| entry.state == oc_core::queries::TerminalState::Running)
                        .map(|entry| entry.target)
                        .collect::<Vec<_>>(),
                )
            })
    } else {
        None
    };
    let mut shared = state.lock().expect("native leases");
    if let Some((session, running)) = terminals {
        let count = shared.terminals.len();
        shared
            .terminals
            .retain(|_, (target, _, _)| target.session != session || running.contains(target));
        if shared.terminals.len() != count {
            owner.resources.retired.notify_one();
        }
    }
    shared.revision = event.revision;
    let frame = Server::Event {
        revision: shared.revision,
        origin: event.origin,
        event: Box::new(Event::from(event.event)),
    };
    for viewer in shared
        .viewers
        .values()
        .filter(|viewer| viewer.ready && matches!(viewer.lease, Some(Lease::Tui | Lease::Run)))
    {
        if let Some(output) = &viewer.output
            && send(output, budget, &frame).is_err()
        {
            viewer.disconnect.send_replace(true);
        }
    }
}

fn execution(message: &Request) -> Option<&SessionId> {
    match message {
        Request::Submit { session, .. }
        | Request::SubmitFresh { session, .. }
        | Request::UserShell { session, .. }
        | Request::Compress { session, .. }
        | Request::CompactSession { session }
        | Request::RegenerateTitle { session } => Some(session),
        _ => None,
    }
}
fn mutation(message: &Request) -> Option<&str> {
    match message {
        Request::Create { id } => Some(&id.0),
        Request::ChangeConversation { session, .. }
        | Request::RenameSession { session, .. }
        | Request::DeleteSession { session }
        | Request::PickerSessionAction { session, .. }
        | Request::OpenPickerSession { session, .. } => Some(&session.0),
        Request::ForkSession { source, .. } => Some(&source.0),
        Request::SessionSelection {
            session, action, ..
        } if !matches!(
            action,
            SessionSelectionAction::Current
                | SessionSelectionAction::Model(_)
                | SessionSelectionAction::Variant(_)
                | SessionSelectionAction::Commit(_)
        ) =>
        {
            Some(&session.0)
        }
        _ => None,
    }
}
fn cancel_session(message: &Request) -> Option<&str> {
    match message {
        Request::Cancel { session }
        | Request::CancelCompaction { session }
        | Request::ChangeConversation { session, .. } => Some(&session.0),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle(
    owner: &Arc<Owner>,
    state: &Mutex<State>,
    peer: &Arc<Peer>,
    output: &mpsc::Sender<Message>,
    budget: &Budget,
    id: u64,
    epoch: u64,
    expected: Option<Expected>,
    request: Request,
) -> Result<serde_json::Value, CoreError> {
    let allowed = match peer.lease {
        Lease::Auth => matches!(
            request,
            Request::AuthMethods { .. }
                | Request::Authenticate { .. }
                | Request::ProviderAccounts { .. }
        ),
        Lease::Sessions => matches!(request, Request::List {}),
        Lease::Tui | Lease::Run => true,
    };
    if !allowed {
        return Err(CoreError::Application(
            "operation is outside this attachment lease".into(),
        ));
    }
    if matches!(request, Request::Terminal { .. }) && peer.lease != Lease::Tui {
        return Err(CoreError::Application(
            "user terminals require an interactive attachment".into(),
        ));
    }
    let captured = peer.capture(epoch)?;
    match &request {
        Request::Submit { text, .. } | Request::SubmitFresh { text, .. } => text.check_budget()?,
        Request::RegisterApprovalConsumer { auto_once, .. } => {
            peer.approval
                .store(if *auto_once { 2 } else { 1 }, Ordering::Release);
        }
        Request::RegisterQuestionConsumer {} => {
            peer.question.store(true, Ordering::Release);
        }
        _ => {}
    }
    if let Request::SwitchLocation { path } | Request::SwitchLocationHome { path } = &request {
        let home = matches!(request, Request::SwitchLocationHome { .. });
        return switch(owner, peer, epoch, path, home).await;
    }
    let standalone = matches!(request, Request::List {}) || peer.lease == Lease::Auth;
    let authentication = matches!(request, Request::Authenticate { .. });
    if let Request::Authenticate {
        action:
            oc_core::queries::AuthAction::Status { attempt }
            | oc_core::queries::AuthAction::Cancel { attempt },
        ..
    } = &request
        && !peer
            .auth
            .lock()
            .expect("peer authentication")
            .contains(attempt)
    {
        return Err(CoreError::Application(
            "authorization attempt unavailable".into(),
        ));
    }
    if standalone {
        let changed = matches!(
            request,
            Request::ProviderAccounts {
                action: Some(_),
                ..
            }
        );
        if let Request::Authenticate {
            action:
                oc_core::queries::AuthAction::Status { attempt }
                | oc_core::queries::AuthAction::Cancel { attempt },
            ..
        } = &request
            && !peer
                .auth
                .lock()
                .expect("peer authentication")
                .contains(attempt)
        {
            return Err(CoreError::Application(
                "authorization attempt unavailable".into(),
            ));
        }
        let (message, reply) = request.into_local();
        owner.standalone(message).await?;
        let value = reply.await?;
        if let Some(id) = value
            .get("Ok")
            .and_then(|value| value.get("id"))
            .and_then(serde_json::Value::as_str)
        {
            let mut attempts = peer.auth.lock().expect("peer authentication");
            owner.retain_auth_attempts(&mut attempts);
            if !attempts.iter().any(|attempt| attempt == id) {
                attempts.push(id.into());
            }
        }
        if changed && value.get("Ok").is_some() {
            for location in owner.loaded_locations().await {
                let _ = location.app.provider_accounts("openai".into(), None).await;
            }
        }
        return Ok(value);
    }
    let location = captured
        .ok_or_else(|| CoreError::Application("operation requires a project attachment".into()))?;
    if let Request::SessionSelection {
        session, action, ..
    } = &request
        && matches!(
            action,
            SessionSelectionAction::Model(_)
                | SessionSelectionAction::Variant(_)
                | SessionSelectionAction::Commit(_)
        )
        && let Some(root) = owner.root(&session.0)
    {
        if root.location.project != location.project {
            return Err(CoreError::SessionNotFound);
        }
        let (message, reply) = request.into_local();
        root.ready().await?;
        root.app.dispatch(message).await?;
        return reply.await;
    }
    if let Request::OpenPickerSession {
        session,
        search,
        all_projects,
        old_deck,
    } = request
    {
        return open_picker(
            owner,
            peer,
            epoch,
            location,
            session,
            search,
            all_projects,
            old_deck,
        )
        .await;
    }
    let admission = execution(&request)
        .map(|session| owner.reserve_mutation(&session.0))
        .transpose()?;
    if let Request::Workspace { selected } = &request {
        let _deck = location.deck_gate.lock().await;
        let deck = location.app.tab_deck().await?;
        return serde_json::to_value(Ok::<_, CoreError>(
            owner.resources.live.snapshot(deck, selected.as_ref()),
        ))
        .map_err(|_| wire::unknown());
    }
    let fresh = match &request {
        Request::SubmitFresh { session, .. }
        | Request::UserShell {
            session,
            selection: oc_core::core_app::UserShellSelection::Fresh(_),
            ..
        } => Some(session.clone()),
        _ => None,
    };
    let deck_write = fresh.is_some()
        || matches!(
            request,
            Request::SaveTabDeck { .. }
                | Request::DeleteSession { .. }
                | Request::PickerSessionAction {
                    action: oc_core::queries::SessionPickerAction::Delete,
                    ..
                }
                | Request::OpenPickerSession { .. }
        );
    let _deck = if deck_write || matches!(request, Request::History { .. }) {
        Some(location.deck_gate.lock().await)
    } else {
        None
    };
    if let Request::History { session, .. } = &request {
        // Share the move's short publication gate so admission cannot race its
        // durable Location/deck commit. Late live hints get a typed refusal,
        // never data read through a retired Source capability.
        let binding = owner
            .db
            .get_pref(&format!("tui.session_location.{}", session.0))
            .map_err(|_| CoreError::TabDeckStorage)?;
        if binding
            .as_deref()
            .is_some_and(|directory| directory != location.project.to_string_lossy())
        {
            return Err(CoreError::LocationChanged);
        }
    }
    if fresh.is_some() && location.app.tab_deck().await?.sessions.len() >= crate::storage::MAX_TABS
    {
        return Err(CoreError::InvalidTabDeck);
    }
    if let Some(session) = cancel_session(&request)
        && let Some(root) = owner.root(session)
    {
        let matches = match expected {
            Some(Expected::Origin(origin)) => root.origin.id == origin,
            Some(Expected::Pending(request)) => state
                .lock()
                .expect("native leases")
                .operations
                .get(&root.origin.id)
                .is_some_and(|operation| {
                    operation.attachment == peer.id && operation.request == request
                }),
            None => false,
        };
        if !matches {
            return Err(CoreError::TurnNotActive);
        }
        let (message, reply) = request.into_local();
        root.app.dispatch(message).await?;
        return reply.await;
    }
    let mut removals = Vec::new();
    if let Request::SaveTabDeck { deck } = &request {
        let old = location.app.tab_deck().await?;
        for session in old
            .sessions
            .iter()
            .filter(|session| !deck.sessions.contains(session))
        {
            removals.push(owner.reserve_mutation(&session.0)?);
        }
    }
    let reservation = mutation(&request)
        .map(|session| owner.reserve_mutation(session))
        .transpose()?;
    let target = if let Some(session) = execution(&request) {
        let cancel = peer
            .admissions
            .lock()
            .expect("peer admissions")
            .get(&id)
            .map(|(_, cancel)| cancel.clone())
            .ok_or(CoreError::Shutdown)?;
        let origin = Arc::new(
            Origin::with_foreground(
                if peer.lease == Lease::Tui {
                    Kind::Interactive
                } else {
                    Kind::Run
                },
                peer.approval.load(Ordering::Acquire) == 2,
                cancel,
            )
            .at(location.clone()),
        );
        let root = {
            let mut shared = state.lock().expect("native leases");
            if peer.disconnected.load(Ordering::Acquire)
                || shared.closing
                || peer.lease == Lease::Tui && shared.tui_count() == 0
            {
                return Err(CoreError::Shutdown);
            }
            let root = owner.begin(
                location.clone(),
                session.0.clone(),
                origin.clone(),
                peer.approval.load(Ordering::Acquire),
                peer.question.load(Ordering::Acquire),
                admission.as_ref(),
            )?;
            shared
                .operations
                .retain(|_, operation| operation.origin.strong_count() > 0);
            shared.operations.insert(
                origin.id,
                Operation {
                    origin: Arc::downgrade(&origin),
                    attachment: peer.id,
                    request: id,
                },
            );
            root
        };
        {
            let mut origins = peer.origins.lock().expect("peer operations");
            origins.retain(|origin| origin.strong_count() > 0);
            origins.push(Arc::downgrade(&origin));
        }
        let _ = send(
            output,
            budget,
            &Server::Bound {
                id,
                session: session.clone(),
                origin: origin.id,
            },
        );
        root.ready().await?;
        root.app.clone()
    } else {
        location.app.clone()
    };
    let request = match request {
        // Registration belongs to this attachment/issuing operation. It never
        // auto-resolves another root's or an older background operation's Ask.
        Request::RegisterApprovalConsumer { preference, .. } => Request::RegisterApprovalConsumer {
            auto_once: false,
            preference,
        },
        other => other,
    };
    let terminal = if let Request::Terminal { session, action } = &request {
        Some((session.clone(), action.clone()))
    } else {
        None
    };
    if let Some((_, action)) = &terminal {
        use oc_core::queries::TerminalAction;
        let mut shared = state.lock().expect("native leases");
        match action {
            TerminalAction::Select(Some(target)) => {
                if let Some((bound, controller, _)) = shared.terminals.get_mut(&target.id) {
                    if bound != target {
                        return Err(CoreError::Application("stale terminal control".into()));
                    }
                    if *controller == 0 {
                        *controller = peer.id;
                    }
                }
            }
            TerminalAction::Resize { target, .. }
            | TerminalAction::Input { target, .. }
            | TerminalAction::Scroll { target, .. }
            | TerminalAction::Remove(target) => {
                let Some((bound, controller, _)) = shared.terminals.get(&target.id) else {
                    return Err(CoreError::Application("stale terminal control".into()));
                };
                if bound != target {
                    return Err(CoreError::Application("stale terminal control".into()));
                }
                if *controller != peer.id {
                    if matches!(action, TerminalAction::Resize { .. }) {
                        return serde_json::to_value(Ok::<_, CoreError>(
                            oc_core::queries::TerminalReceipt::Applied,
                        ))
                        .map_err(|_| wire::unknown());
                    }
                    return Err(CoreError::Application(
                        "terminal is controlled by another attachment; select it after release"
                            .into(),
                    ));
                }
            }
            _ => {}
        }
    }
    let (message, reply) = request.into_local();
    target.dispatch(message).await?;
    let value = reply.await?;
    if authentication
        && let Some(id) = value
            .get("Ok")
            .and_then(|value| value.get("id"))
            .and_then(serde_json::Value::as_str)
    {
        let mut attempts = peer.auth.lock().expect("peer authentication");
        owner.retain_auth_attempts(&mut attempts);
        if !attempts.iter().any(|attempt| attempt == id) {
            attempts.push(id.into());
        }
    }
    if let Some((session, action)) = terminal {
        if let Ok(Ok(oc_core::queries::TerminalReceipt::Created(entry))) = serde_json::from_value::<
            Result<oc_core::queries::TerminalReceipt, CoreError>,
        >(value.clone())
        {
            let mut shared = state.lock().expect("native leases");
            if shared.tui_count() > 0 {
                let controller = if peer.disconnected.load(Ordering::Acquire) {
                    0
                } else {
                    peer.id
                };
                shared.terminals.insert(
                    entry.target.id.clone(),
                    (entry.target, controller, location.clone()),
                );
            }
        }
        if value.get("Ok").is_some()
            && let oc_core::queries::TerminalAction::Remove(ref target) = action
        {
            state
                .lock()
                .expect("native leases")
                .terminals
                .remove(&target.id);
        }
        if value.get("Ok").is_some()
            && matches!(action, oc_core::queries::TerminalAction::Select(None))
        {
            for (target, controller, _) in
                state.lock().expect("native leases").terminals.values_mut()
            {
                if *controller == peer.id && target.session == session {
                    *controller = 0;
                }
            }
        }
    }
    if value.get("Ok").is_some()
        && let Some(session) = fresh
    {
        let mut deck = location.app.tab_deck().await?;
        deck.new_session_titles.resize(deck.sessions.len(), false);
        if !deck.sessions.contains(&session) {
            // Empty flags are the compact all-false representation. Expand
            // before adding the first fresh root so every viewer gets its
            // accepted "New session" label, including an initially empty deck.
            deck.sessions.push(session.clone());
            deck.new_session_titles.push(true);
        }
        // A durable pending adoption may already project the accepted root
        // before this receipt arrives. It is still a promoted Home tab.
        if let Some(index) = deck.sessions.iter().position(|id| id == &session) {
            deck.new_session_titles[index] = true;
        }
        deck.active = Some(session);
        location.app.save_tab_deck(deck).await?;
    }
    if deck_write && value.get("Ok").is_some() {
        owner.resources.live.emit(
            "",
            None,
            CoreEvent::WorkspaceChanged {
                location: location.project.to_string_lossy().into_owned(),
            },
        );
    }
    drop(reservation);
    drop(removals);
    Ok(value)
}

async fn switch(
    owner: &Arc<Owner>,
    peer: &Peer,
    epoch: u64,
    path: &str,
    home: bool,
) -> Result<serde_json::Value, CoreError> {
    let _switch = peer.switch_gate.lock().await;
    if peer.epoch() != epoch {
        return Err(CoreError::TabDeckConflict);
    }
    let target = owner
        .location(Path::new(path), peer.lease == Lease::Tui)
        .await
        .map_err(location_error)?;
    let catalog = target.app.catalog().await?;
    let session = if home {
        None
    } else {
        let deck = target.app.tab_deck().await?;
        match deck.active.or_else(|| deck.sessions.first().cloned()) {
            Some(session) => Some(session),
            None => {
                let id = format!(
                    "s-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                );
                let session = SessionId(id);
                target.app.create_session(session.clone()).await?;
                Some(session)
            }
        }
    };
    {
        let mut route = peer.route.lock().expect("attachment route");
        if route.epoch != epoch {
            return Err(CoreError::TabDeckConflict);
        }
        route.epoch = route
            .epoch
            .checked_add(1)
            .ok_or(CoreError::TabDeckConflict)?;
        route.location = Some(target.clone());
    }
    let generation = catalog.chrome.terminal_generation;
    if home {
        serde_json::to_value(Ok::<_, CoreError>(oc_core::queries::HomeLocationSnapshot {
            location: target.project.to_string_lossy().into_owned(),
            generation,
            catalog,
            diagnostics: Vec::new(),
            notices: target.notices.clone(),
        }))
        .map_err(|_| wire::unknown())
    } else {
        let session = session.expect("prepared attached Location");
        serde_json::to_value(Ok::<_, CoreError>(oc_core::queries::LocationSnapshot {
            location: target.project.to_string_lossy().into_owned(),
            generation,
            session: session.0,
            catalog,
            diagnostics: Vec::new(),
            notices: target.notices.clone(),
        }))
        .map_err(|_| wire::unknown())
    }
}

fn location_error(issue: SpawnDiagnostic) -> CoreError {
    use oc_core::session::LocationSwitchFailure;
    let category = match issue.category {
        SpawnFailure::Configuration | SpawnFailure::MissingCredential => {
            LocationSwitchFailure::Configuration
        }
        SpawnFailure::Storage => LocationSwitchFailure::Storage,
        _ => LocationSwitchFailure::Runtime,
    };
    CoreError::LocationSwitch {
        category,
        detail: issue.to_string(),
        diagnostic: Some(issue.diagnostic),
    }
}

/// Prepare a bounded target projection, then CAS both decks and move only this
/// attachment's route. Never run the legacy whole-Location replacement branch.
#[allow(clippy::too_many_arguments)]
async fn open_picker(
    owner: &Arc<Owner>,
    peer: &Peer,
    epoch: u64,
    source: Arc<Location>,
    session: SessionId,
    search: String,
    all_projects: bool,
    old_deck: oc_core::queries::TabDeckSnapshot,
) -> Result<serde_json::Value, CoreError> {
    let _switch = peer.switch_gate.lock().await;
    if peer.epoch() != epoch || old_deck.location != source.project.to_string_lossy() {
        return Err(CoreError::TabDeckConflict);
    }
    let next_epoch = epoch.checked_add(1).ok_or(CoreError::TabDeckConflict)?;
    let _session = owner.reserve_mutation(&session.0)?;
    let context = source.app.session_picker_context(None).await?;
    if context.all_projects != all_projects {
        return Err(CoreError::Application(
            "Sessions scope changed; reopen the picker".into(),
        ));
    }
    let directory = owner
        .db
        .session_list(
            &search,
            (!all_projects).then_some(old_deck.location.as_str()),
        )
        .map_err(|_| CoreError::TabDeckStorage)?
        .into_iter()
        .find(|row| row.id == session)
        .and_then(|row| row.directory)
        .ok_or(CoreError::SessionNotFound)?;
    if owner
        .db
        .session_family_running(&session.0)
        .map_err(|_| CoreError::TabDeckStorage)?
    {
        return Err(CoreError::TurnBusy);
    }
    let target = owner
        .location(Path::new(&directory), peer.lease == Lease::Tui)
        .await
        .map_err(|error| CoreError::Diagnostic(error.diagnostic))?;
    let (first, second) = if source.project <= target.project {
        (&source, &target)
    } else {
        (&target, &source)
    };
    let _first = first.deck_gate.lock().await;
    let _second = if first.project != second.project {
        Some(second.deck_gate.lock().await)
    } else {
        None
    };
    let current = source.app.tab_deck().await?;
    let mut removed = Vec::new();
    for id in current
        .sessions
        .iter()
        .filter(|id| !old_deck.sessions.contains(id))
    {
        removed.push(owner.reserve_mutation(&id.0)?);
    }
    if target.app.probe_session(session.clone()).await? != oc_core::queries::SessionProbe::Root {
        return Err(CoreError::SessionNotFound);
    }
    let catalog = target
        .app
        .session_selection(session.clone(), false, SessionSelectionAction::Current)
        .await?;
    let page = target
        .app
        .history_page(session.clone(), None, None, application::HISTORY_PAGE_LIMIT)
        .await?;
    let mut deck = if source.project == target.project {
        old_deck.clone()
    } else {
        target.app.tab_deck().await?
    };
    if deck.projected() {
        return Err(CoreError::StoredTabDeck);
    }
    if !deck.sessions.contains(&session) {
        if deck.sessions.len() >= crate::storage::MAX_TABS {
            return Err(CoreError::InvalidTabDeck);
        }
        deck.sessions.push(session.clone());
        if !deck.new_session_titles.is_empty() {
            deck.new_session_titles.push(false);
        }
    }
    deck.active = Some(session.clone());
    let mut decks = if source.project == target.project {
        vec![deck]
    } else {
        vec![old_deck, deck]
    };
    owner.db.save_picker_decks(&mut decks)?;
    {
        let mut route = peer.route.lock().expect("attachment route");
        route.location = Some(target.clone());
        route.epoch = next_epoch;
    }
    for location in [&source, &target] {
        owner.resources.live.emit(
            "",
            None,
            CoreEvent::WorkspaceChanged {
                location: location.project.to_string_lossy().into_owned(),
            },
        );
    }
    let result = oc_core::queries::SessionPickerOpen {
        session,
        location: directory,
        catalog,
        page,
        previous_deck: decks.first().expect("prepared old deck").clone(),
        deck: decks.pop().expect("prepared target deck"),
    };
    serde_json::to_value(Ok::<_, CoreError>(result)).map_err(|_| wire::unknown())
}
