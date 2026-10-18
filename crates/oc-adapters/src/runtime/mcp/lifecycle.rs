//! Supervision and projections of the existing generation-owned MCP resources.
//! No database or independent registry: request views lease these same clients.

use super::*;
use oc_core::queries::{
    McpAction, McpBinding, McpControl, McpServerSnapshot, McpSnapshot, McpStatus, ServiceAction,
    ServiceCode, ServiceDiagnostic, ServiceStage,
};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};

mod lookups;
mod reconcile;
use lookups::{LookupAck, LookupActive, LookupCompleted};
use reconcile::configured_node;

type StatusAck = oneshot::Sender<Result<McpSnapshot, oc_core::session::CoreError>>;
type RequestAck = oneshot::Sender<Result<Arc<McpGeneration>, RuntimeError>>;
type ReconcileAck = oneshot::Sender<Result<McpSnapshot, RuntimeError>>;
type RetireAck = oneshot::Sender<Result<(), RuntimeError>>;
type Retirement = (Vec<(String, Arc<()>)>, RetireAck);

/// An executing root/child retains preparation interest across its issued IO.
/// A disposable metadata reply is deliberately not such an execution owner.
pub(in crate::runtime) struct Operation {
    owner: Arc<McpOwner>,
    active: bool,
}
impl Operation {
    pub(in crate::runtime) async fn cancel(mut self) -> Result<(), RuntimeError> {
        self.release();
        self.owner.retire(false).await
    }
    fn release(&mut self) {
        if self.active {
            self.active = false;
            *self.owner.shared.operations.lock().expect("MCP operations") -= 1;
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Default)]
struct Safety {
    remote: Arc<AtomicBool>,
    cleanup: Arc<AtomicBool>,
}

/// Application-owned supervisor handle. Only the resource task mutates clients;
/// short read locks expose bounded immutable snapshots, never a turn-held mutex.
pub(in crate::runtime) struct McpOwner {
    shared: Arc<Shared>,
    control: Mutex<Option<mpsc::Sender<Command>>>,
    task: tokio::sync::Mutex<Option<JoinHandle<Result<(), RuntimeError>>>>,
}

struct Shared {
    operations: Mutex<usize>,
    safety: RwLock<Safety>,
    publication: RwLock<Publication>,
    events: Mutex<Option<tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>>>,
    stop: AtomicBool,
    wake: Arc<Notify>,
    failure: Notify,
}

struct Publication {
    status: McpSnapshot,
    request: Arc<McpGeneration>,
    fatal: Option<RuntimeError>,
}

enum Command {
    Reconcile {
        config: Box<Generation>,
        publication: u64,
        activation: Option<Arc<crate::composition::McpActivation>>,
        force: bool,
        commit: Option<crate::runtime::publication::Commit>,
        ack: ReconcileAck,
    },
    Control {
        control: McpControl,
        ack: StatusAck,
    },
    Request {
        ack: RequestAck,
        operation: bool,
    },
    Retire {
        poisoned: bool,
        ack: RetireAck,
    },
    Lookup {
        query: oc_core::queries::McpLookup,
        cancel: Arc<AtomicBool>,
        ack: LookupAck,
    },
}

struct Node {
    suspended: bool,
    /// Retained by every worker for this exact physical connection attempt.
    /// An old result keeps its allocation alive, so an address cannot be reused
    /// as a later incarnation while that result can still be delivered.
    incarnation: Arc<()>,
    row: McpServerSnapshot,
    source: String,
    desired: bool,
    cancel: Option<Arc<AtomicBool>>,
    closing: bool,
    replacing: bool,
    catalogs: [Option<oc_core::queries::McpLookupData>; 3],
    catalog_pending: [bool; 3],
}

enum WorkResult {
    Connected(AttachedMcp),
    Refreshed(Vec<mcp_remote::RegistryEntry>),
    Closed,
    Lookup(Box<LookupCompleted>),
}

struct Completed {
    server: String,
    incarnation: Arc<()>,
    refresh: bool,
    result: Result<WorkResult, RuntimeError>,
}

/// The one resource owner; entries are the existing registry and exact clients.
struct Scope {
    shared: Arc<Shared>,
    config: Generation,
    project: std::path::PathBuf,
    env: BTreeMap<String, String>,
    activation: Option<Arc<crate::composition::McpActivation>>,
    nodes: BTreeMap<String, Node>,
    generation: McpGeneration,
    work: JoinSet<Completed>,
    retiring: Vec<(String, Arc<()>, Arc<AttachedServer>)>,
    /// Every connecting/live/retiring physical incarnation occupies one slot
    /// until its connect failure or successful close has actually been joined.
    instances: Vec<(String, Arc<()>)>,
    requests: Vec<(RequestAck, bool)>,
    retirements: Vec<Retirement>,
    lookups: Vec<LookupActive>,
    fatal: Option<RuntimeError>,
    stopping: bool,
}

impl McpOwner {
    pub(in crate::runtime) fn operation(self: &Arc<Self>) -> Operation {
        *self.shared.operations.lock().expect("MCP operations") += 1;
        Operation {
            owner: self.clone(),
            active: true,
        }
    }

    pub(in crate::runtime) fn bind_safety(
        &self,
        remote: Arc<AtomicBool>,
        cleanup: Arc<AtomicBool>,
    ) {
        *self.shared.safety.write().expect("MCP safety") = Safety { remote, cleanup };
    }

    pub(in crate::runtime) async fn retire(&self, poisoned: bool) -> Result<(), RuntimeError> {
        if let Some(error) = self.fatal() {
            return Err(error);
        }
        let Some(sender) = self.control.lock().expect("MCP control").clone() else {
            return Ok(());
        };
        let (ack, reply) = oneshot::channel();
        let result = async {
            sender
                .send(Command::Retire { poisoned, ack })
                .await
                .map_err(|_| RuntimeError::McpShutdown)?;
            tokio::time::timeout(MCP_CLOSE_BUDGET, reply)
                .await
                .map_err(|_| RuntimeError::McpShutdown)?
                .map_err(|_| RuntimeError::McpShutdown)?
        }
        .await;
        if matches!(
            result,
            Err(RuntimeError::McpShutdown
                | RuntimeError::McpAttach {
                    stage: "cleanup",
                    ..
                })
        ) {
            self.shared
                .safety
                .read()
                .expect("MCP safety")
                .cleanup
                .store(true, Ordering::SeqCst);
        }
        result
    }

    pub(in crate::runtime) fn new(location: &str, generation: u64) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let wake = Arc::new(Notify::new());
        Self {
            shared: Arc::new(Shared {
                operations: Mutex::new(0),
                safety: RwLock::new(Safety::default()),
                publication: RwLock::new(Publication {
                    status: McpSnapshot {
                        binding: McpBinding {
                            location: location.into(),
                            generation,
                            instance: NEXT.fetch_add(1, Ordering::SeqCst),
                        },
                        revision: 0,
                        servers: Vec::new(),
                    },
                    request: Arc::new(McpGeneration::empty(generation, wake.clone())),
                    fatal: None,
                }),
                events: Mutex::new(None),
                stop: AtomicBool::new(false),
                wake,
                failure: Notify::new(),
            }),
            control: Mutex::new(None),
            task: tokio::sync::Mutex::new(None),
        }
    }

    pub(in crate::runtime) fn bind_events(
        &self,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) {
        *self.shared.events.lock().expect("MCP events") = Some(events.clone());
        let _ = events.send(oc_core::core_app::CoreEvent::McpChanged(self.snapshot()));
    }

    pub(in crate::runtime) fn snapshot(&self) -> McpSnapshot {
        self.shared
            .publication
            .read()
            .expect("MCP publication")
            .status
            .clone()
    }

    pub(in crate::runtime) fn fatal(&self) -> Option<RuntimeError> {
        self.shared
            .publication
            .read()
            .expect("MCP publication")
            .fatal
            .clone()
    }

    /// Diagnostics may arrive during a held turn; clients/catalog/guidance stay
    /// on its immutable lease. Read only typed degradation from this exact
    /// owner/binding, without requesting a new lease or awaiting startup.
    pub(in crate::runtime) fn completion_warnings(
        &self,
        binding: &McpBinding,
        initial: &[RuntimeError],
    ) -> Vec<String> {
        let latest = {
            let publication = self.shared.publication.read().expect("MCP publication");
            if &publication.status.binding == binding {
                publication.request.degraded.clone()
            } else {
                Vec::new()
            }
        };
        let mut warnings = Vec::new();
        for error in initial.iter().chain(&latest) {
            if !warnings.contains(error) {
                warnings.push(error.clone());
            }
        }
        warnings.iter().map(ToString::to_string).collect()
    }

    pub(in crate::runtime) fn request_view(&self) -> Result<Arc<McpGeneration>, RuntimeError> {
        let publication = self.shared.publication.read().expect("MCP publication");
        if let Some(error) = &publication.fatal {
            return Err(error.clone());
        }
        if publication.request.remote_unknown.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        Ok(publication.request.clone())
    }

    pub(in crate::runtime) async fn wait_failure(&self) -> RuntimeError {
        loop {
            let notification = self.shared.failure.notified();
            tokio::pin!(notification);
            notification.as_mut().enable();
            if let Some(error) = self.fatal() {
                return error;
            }
            notification.await;
        }
    }

    pub(in crate::runtime) fn remote_unknown(&self) -> bool {
        self.shared
            .publication
            .read()
            .expect("MCP publication")
            .request
            .remote_unknown
            .load(Ordering::SeqCst)
    }

    pub(in crate::runtime) fn poisoned(&self) -> bool {
        self.shared
            .publication
            .read()
            .expect("MCP publication")
            .request
            .poisoned
            .load(Ordering::SeqCst)
    }

    pub(in crate::runtime) fn cleanup_failed(&self) -> bool {
        self.shared
            .publication
            .read()
            .expect("MCP publication")
            .request
            .cleanup_error
            .load(Ordering::SeqCst)
    }

    pub(in crate::runtime) fn start(
        &self,
        config: Generation,
        project: std::path::PathBuf,
        env: BTreeMap<String, String>,
        activation: Option<Arc<crate::composition::McpActivation>>,
    ) -> Result<(), RuntimeError> {
        let mut sender = self.control.lock().expect("MCP control");
        if sender.is_some() {
            return Ok(());
        }
        let enabled = config.mcp.values().filter(|entry| entry.enabled).count();
        if enabled > MAX_MCP_SERVERS {
            return Err(RuntimeError::InvalidArgs(format!(
                "too many enabled MCP servers: {enabled} exceeds {MAX_MCP_SERVERS}"
            )));
        }
        if tokio::runtime::Handle::try_current().is_err() && !config.mcp.is_empty() {
            return Err(RuntimeError::InvalidArgs(
                "MCP resource runtime unavailable".into(),
            ));
        }
        let mut protected = mcp_redactions(&config, &env);
        for entry in config.mcp.values() {
            protected.extend(crate::config::mcp::entry_credential_values(entry, &env));
        }
        if let Some(activation) = &activation
            && !config.mcp.is_empty()
        {
            protected.extend(
                activation
                    .protected_values(&env)
                    .map_err(RuntimeError::InvalidArgs)?,
            );
        }
        let nodes = config
            .mcp
            .iter()
            .map(|(server, entry)| {
                (
                    server.clone(),
                    configured_node(server, entry, &config, &protected),
                )
            })
            .collect();
        let publication = self.shared.publication.read().expect("MCP publication");
        let mut generation = publication.request.as_ref().clone();
        generation.admitted = Arc::new(config.mcp.clone());
        drop(publication);
        let mut scope = Scope {
            shared: self.shared.clone(),
            config,
            project,
            env,
            activation,
            nodes,
            generation,
            work: JoinSet::new(),
            retiring: Vec::new(),
            instances: Vec::new(),
            requests: Vec::new(),
            retirements: Vec::new(),
            lookups: Vec::new(),
            fatal: None,
            stopping: false,
        };
        for (server, node) in &scope.nodes {
            if let Some(failure) = &node.row.diagnostic {
                record_degradation(
                    &mut scope.generation.degraded,
                    server,
                    RuntimeError::McpAttach {
                        server: failure.service.clone(),
                        stage: failure.stage.as_str(),
                        safe_code: failure.code.as_str(),
                        retryable: false,
                    },
                );
            }
        }
        let initial: Vec<_> = scope
            .nodes
            .iter()
            .filter(|(_, node)| node.desired)
            .map(|(server, _)| server.clone())
            .collect();
        for server in initial {
            scope.connect(&server, scope.config.mcp[&server].clone());
        }
        scope.publish()?;
        // Synchronous empty fixtures need no runtime. An async Location keeps
        // this same supervisor even when its desired inventory becomes empty.
        if scope.nodes.is_empty() && tokio::runtime::Handle::try_current().is_err() {
            return Ok(());
        }
        let (tx, rx) = mpsc::channel(64);
        let task = tokio::spawn(scope.run(rx));
        *self.task.try_lock().expect("new MCP owner") = Some(task);
        *sender = Some(tx);
        Ok(())
    }

    /// Validate scope before activation can read a credential/resource.
    fn validate(&self, control: &McpControl) -> Result<(), RuntimeError> {
        let snapshot = self.snapshot();
        if snapshot.binding != control.binding || self.shared.stop.load(Ordering::SeqCst) {
            return Err(RuntimeError::InvalidArgs(
                "stale MCP scope; reopen servers".into(),
            ));
        }
        let row = snapshot
            .servers
            .iter()
            .find(|row| row.id == control.server)
            .ok_or_else(|| RuntimeError::InvalidArgs("unknown MCP server".into()))?;
        if !row.actions.contains(&control.action) && row.pending_action != Some(control.action) {
            return Err(RuntimeError::InvalidArgs(
                "MCP action unsupported in current state".into(),
            ));
        }
        if control.action != McpAction::Disconnect && self.remote_unknown() {
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        Ok(())
    }

    pub(in crate::runtime) fn server_name(
        &self,
        control: &McpControl,
        config: &Generation,
    ) -> Result<String, RuntimeError> {
        self.validate(control)?;
        use sha2::{Digest, Sha256};
        config
            .mcp
            .keys()
            .find(|server| {
                let digest = Sha256::digest(server.as_bytes());
                format!(
                    "mcp-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    digest[0],
                    digest[1],
                    digest[2],
                    digest[3],
                    digest[4],
                    digest[5],
                    digest[6],
                    digest[7]
                ) == control.server
            })
            .cloned()
            .ok_or_else(|| RuntimeError::InvalidArgs("unknown MCP server".into()))
    }

    pub(in crate::runtime) fn control(&self, control: McpControl, ack: StatusAck) {
        let command = Command::Control { control, ack };
        let failed = match self.control.lock().expect("MCP control").as_ref() {
            Some(sender) => sender
                .try_send(command)
                .err()
                .map(|error| error.into_inner()),
            None => Some(command),
        };
        if let Some(Command::Control { ack, .. }) = failed {
            let _ = ack.send(Err(oc_core::session::CoreError::Application(
                "MCP owner unavailable or control queue full".into(),
            )));
        }
    }

    pub(in crate::runtime) async fn request(
        &self,
        cancel: &AtomicBool,
        operation: bool,
    ) -> Result<Arc<McpGeneration>, RuntimeError> {
        if let Some(error) = self.fatal() {
            return Err(error);
        }
        let empty = {
            let publication = self.shared.publication.read().expect("MCP publication");
            publication.status.servers.is_empty() && publication.request.admitted.is_empty()
        };
        // There can be no list-changed notification or connect waiter in an
        // empty admitted inventory. Keep this common no-MCP request local,
        // without moving the whole execution future through another task.
        if empty {
            return self.request_view();
        }
        let sender = self.control.lock().expect("MCP control").clone();
        let Some(sender) = sender else {
            return Ok(self
                .shared
                .publication
                .read()
                .expect("MCP publication")
                .request
                .clone());
        };
        // Connect/relist work belongs to the supervisor. This caller owns only
        // its reply, never the lifetime of another turn's admitted clients.
        let (ack, mut reply) = oneshot::channel();
        sender
            .send(Command::Request { ack, operation })
            .await
            .map_err(|_| RuntimeError::McpShutdown)?;
        loop {
            tokio::select! {
                result = &mut reply => {
                    return result.map_err(|_| RuntimeError::McpShutdown)?;
                },
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if cancel.load(Ordering::SeqCst) {
                        return Err(RuntimeError::Cancelled);
                    }
                }
            }
        }
    }

    pub(in crate::runtime) async fn stop(&self) -> Result<(), RuntimeError> {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_one();
        // Keep the handle in its owner when the awaiting future is dropped.
        let mut task = self.task.lock().await;
        let Some(handle) = task.as_mut() else {
            return self.fatal().map_or(Ok(()), Err);
        };
        let result = tokio::time::timeout(MCP_CLOSE_BUDGET, &mut *handle).await;
        let result = match result {
            Ok(Ok(result)) => {
                task.take();
                self.control.lock().expect("MCP control").take();
                result
            }
            Ok(Err(_)) => {
                task.take();
                Err(RuntimeError::McpShutdown)
            }
            Err(_) => {
                handle.abort();
                let _ = handle.await;
                task.take();
                Err(RuntimeError::McpShutdown)
            }
        };
        if let Err(error) = &result {
            let mut publication = self.shared.publication.write().expect("MCP publication");
            publication.fatal = Some(error.clone());
            publication
                .request
                .cleanup_error
                .store(true, Ordering::SeqCst);
            drop(publication);
            self.shared.failure.notify_waiters();
        }
        result
    }
}

impl Drop for McpOwner {
    fn drop(&mut self) {
        // Explicit clean shutdown joins and removes this handle. An interrupted
        // owner must not detach its supervisor (whose JoinSet owns every job).
        // Abort is interruption, never a confirmed shutdown/reap success.
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_one();
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}

impl Scope {
    fn protect_labels(&mut self, protected: &[String]) {
        for (server, node) in &mut self.nodes {
            // Only mask the existing label: never restore it from a raw key.
            if !safe_display_label(&node.row.name, protected) {
                node.row.name = safe_server_id(server);
            }
        }
    }

    fn connect(&mut self, server: &str, entry: crate::config::McpEntry) {
        debug_assert!(self.instances.len() < MAX_MCP_SERVERS);
        let cancel = Arc::new(AtomicBool::new(false));
        let incarnation = Arc::new(());
        let node = self.nodes.get_mut(server).expect("MCP node");
        node.incarnation = incarnation.clone();
        node.cancel = Some(cancel.clone());
        node.replacing = false;
        self.instances.push((server.into(), incarnation.clone()));
        let server = server.to_string();
        let project = self.project.clone();
        let env = self.env.clone();
        let mut config = self.config.clone();
        config.mcp.insert(server.clone(), entry.clone());
        let redactions = mcp_redactions(&config, &env);
        self.work.spawn(async move {
            let result = connect_one(&server, &entry, &project, &env, &redactions, &cancel)
                .await
                .map(WorkResult::Connected);
            Completed {
                server,
                incarnation,
                refresh: false,
                result,
            }
        });
    }

    fn publish(&mut self) -> Result<(), RuntimeError> {
        {
            let safety = self.shared.safety.read().expect("MCP safety");
            if self.generation.remote_unknown.load(Ordering::SeqCst) {
                safety.remote.store(true, Ordering::SeqCst);
            }
            if self.generation.cleanup_error.load(Ordering::SeqCst)
                || matches!(
                    self.fatal,
                    Some(
                        RuntimeError::McpShutdown
                            | RuntimeError::McpAttach {
                                stage: "cleanup",
                                ..
                            }
                    )
                )
            {
                safety.cleanup.store(true, Ordering::SeqCst);
            }
        }
        self.generation.entries = mcp_remote::merge_registries(
            self.generation
                .servers
                .iter()
                .map(|server| server.registry.clone())
                .collect(),
        )
        .map_err(|error| match error {
            mcp_remote::McpError::CatalogLimited => RuntimeError::McpCatalogLimit,
            other => remote_attach_error("generation", other),
        })?;
        self.lookup_catalog_budget()
            .map_err(|_| RuntimeError::McpCatalogLimit)?;
        let mut publication = self.shared.publication.write().expect("MCP publication");
        publication.status.binding.generation = self.generation.publication;
        publication.status.revision += 1;
        for (name, node) in &mut self.nodes {
            node.row.actions = if node.closing
                || node.row.pending_action.is_some()
                || self.stopping
                || self.fatal.is_some()
            {
                Vec::new()
            } else {
                match node.row.status {
                    McpStatus::Connected => vec![McpAction::Disconnect],
                    McpStatus::Disabled if self.activation.is_some() => vec![McpAction::Connect],
                    McpStatus::Failed
                        if node
                            .row
                            .diagnostic
                            .as_ref()
                            .is_some_and(|d| d.action == ServiceAction::RetryConnection) =>
                    {
                        vec![McpAction::Retry]
                    }
                    _ => Vec::new(),
                }
            };
            node.row.tools = self
                .generation
                .entries
                .iter()
                .filter(|entry| &entry.server == name)
                .count();
        }
        publication.status.servers = self.nodes.values().map(|node| node.row.clone()).collect();
        publication
            .status
            .servers
            .sort_by(|one, two| one.name.cmp(&two.name).then_with(|| one.id.cmp(&two.id)));
        debug_assert_eq!(
            publication.status.binding.generation,
            self.generation.publication
        );
        publication.request = Arc::new(self.generation.clone());
        publication.fatal = self.fatal.clone();
        let status = publication.status.clone();
        drop(publication);
        if self.fatal.is_some() {
            self.shared.failure.notify_waiters();
        }
        if let Some(events) = self.shared.events.lock().expect("MCP events").as_ref() {
            let _ = events.send(oc_core::core_app::CoreEvent::McpChanged(status));
        }
        Ok(())
    }

    fn stop_server(&mut self, server: &str) {
        self.retire_server(server, true);
    }

    fn retire_server(&mut self, server: &str, cancel_lookups: bool) {
        if cancel_lookups {
            for lookup in &self.lookups {
                if lookup.server == server {
                    lookup.cancel.store(true, Ordering::SeqCst);
                }
            }
        }
        let node = self.nodes.get_mut(server).expect("MCP node");
        node.desired = false;
        node.row.pending_action = Some(McpAction::Disconnect);
        node.row.status = McpStatus::Pending;
        if let Some(cancel) = &node.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(index) = self
            .generation
            .servers
            .iter()
            .position(|entry| entry.server_id == server)
        {
            node.closing = true;
            let entry = self.generation.servers.remove(index);
            self.retiring
                .push((server.to_string(), node.incarnation.clone(), entry.client));
        } else if node.cancel.is_none() {
            node.row.pending_action = None;
            node.row.status = McpStatus::Disabled;
        }
    }

    fn drain_retiring(&mut self) {
        let mut waiting = Vec::new();
        for (server, incarnation, client) in self.retiring.drain(..) {
            match Arc::try_unwrap(client) {
                Ok(client) => {
                    self.work.spawn(async move {
                        Completed {
                            server,
                            incarnation,
                            refresh: false,
                            result: close_client(client)
                                .await
                                .map(|()| WorkResult::Closed)
                                .map_err(|_| RuntimeError::McpShutdown),
                        }
                    });
                }
                Err(client) => waiting.push((server, incarnation, client)),
            }
        }
        self.retiring = waiting;
    }

    fn begin_stop(&mut self) {
        if self.stopping {
            // A later cleanup/job failure must wake application monitors even
            // when retirement was already requested by cancel or shutdown.
            let published = self
                .shared
                .publication
                .read()
                .expect("MCP publication")
                .fatal
                .clone();
            if self.fatal.is_some() && self.fatal != published {
                let _ = self.publish();
            }
            return;
        }
        self.stopping = true;
        let names: Vec<_> = self.nodes.keys().cloned().collect();
        for server in names {
            self.stop_server(&server);
        }
        for (ack, _) in self.requests.drain(..) {
            let _ = ack.send(Err(RuntimeError::Cancelled));
        }
        // Empty inventory has no service state to retire. In particular a
        // no-MCP Location must not emit a spurious service event after its
        // terminal turn event merely because it now retains a supervisor.
        if (!self.nodes.is_empty() || self.fatal.is_some())
            && let Err(error) = self.publish()
        {
            self.fatal = Some(error);
        }
        self.drain_retiring();
    }

    fn complete(&mut self, completed: Completed) {
        let completed = match completed {
            Completed {
                server,
                result: Ok(WorkResult::Lookup(lookup)),
                ..
            } => {
                self.complete_lookup(&server, *lookup);
                return;
            }
            other => other,
        };
        if matches!(&completed.result, Ok(WorkResult::Closed))
            || (!completed.refresh
                && matches!(&completed.result, Err(error) if !matches!(error, RuntimeError::McpShutdown | RuntimeError::McpAttach { stage: "cleanup", .. })))
        {
            self.instances
                .retain(|(_, incarnation)| !Arc::ptr_eq(incarnation, &completed.incarnation));
        }
        let server = completed.server;
        if self
            .nodes
            .get(&server)
            .is_none_or(|node| !Arc::ptr_eq(&node.incarnation, &completed.incarnation))
        {
            match completed.result {
                Ok(WorkResult::Connected(client)) => {
                    // A late successful connect still owns a real process/socket.
                    // Retire and join it; never expose it under the replacement.
                    self.retiring
                        .push((server, completed.incarnation, client.client));
                }
                Err(
                    RuntimeError::McpShutdown
                    | RuntimeError::McpAttach {
                        stage: "cleanup", ..
                    },
                ) => {
                    self.generation.cleanup_error.store(true, Ordering::SeqCst);
                    self.fatal = Some(RuntimeError::McpShutdown);
                    self.begin_stop();
                }
                _ => {}
            }
            self.drain_retiring();
            self.release_requests();
            return;
        }
        let node = self.nodes.get_mut(&server).expect("MCP completion scope");
        node.cancel = None;
        // A relist's lease may finish before its result is consumed. A close
        // confirmation can therefore arrive first. Never let that stale relist
        // overwrite a disconnected node or reopen its controls during cleanup.
        if (completed.refresh && !node.desired)
            || (node.replacing
                && !matches!(
                    &completed.result,
                    Ok(WorkResult::Connected(_) | WorkResult::Closed)
                ))
        {
            if matches!(
                &completed.result,
                Err(RuntimeError::McpShutdown
                    | RuntimeError::McpAttach {
                        stage: "cleanup",
                        ..
                    })
            ) {
                self.fatal = Some(RuntimeError::McpShutdown);
                self.begin_stop();
            }
            self.release_requests();
            self.drain_retiring();
            return;
        }
        match completed.result {
            Ok(WorkResult::Connected(mut client))
                if node.desired && !node.replacing && !self.stopping =>
            {
                if let Some(entry) = self.config.mcp.get(&server) {
                    client.code_mode = entry.codemode != Some(false);
                }
                node.catalogs = [None, None, None];
                node.catalog_pending = [false; 3];
                node.suspended = false;
                node.row.status = McpStatus::Connected;
                node.row.pending_action = None;
                node.row.diagnostic = None;
                clear_degradation(&mut self.generation.degraded, &server);
                self.generation.servers.push(client);
                self.generation
                    .servers
                    .sort_by(|a, b| a.server_id.cmp(&b.server_id));
            }
            Ok(WorkResult::Connected(client)) => {
                node.closing = true;
                self.retiring
                    .push((server.clone(), completed.incarnation.clone(), client.client));
            }
            Ok(WorkResult::Refreshed(registry)) => {
                if let Some(client) = self
                    .generation
                    .servers
                    .iter_mut()
                    .find(|client| client.server_id == server)
                {
                    client.registry = registry;
                    clear_degradation(&mut self.generation.degraded, &server);
                    node.row.diagnostic = None;
                }
            }
            Ok(WorkResult::Closed) => {
                node.catalogs = [None, None, None];
                node.catalog_pending = [false; 3];
                node.closing = false;
                node.row.status = McpStatus::Disabled;
                node.row.pending_action = None;
                node.row.diagnostic = None;
                clear_degradation(&mut self.generation.degraded, &server);
            }
            Ok(WorkResult::Lookup(_)) => unreachable!("lookup completed above"),
            Err(RuntimeError::Cancelled) if !node.desired || self.stopping => {
                if !node.closing {
                    node.row.status = McpStatus::Disabled;
                    node.row.pending_action = None;
                }
            }
            Err(error) => {
                node.suspended = false;
                if matches!(error, RuntimeError::McpShutdown | RuntimeError::Cancelled)
                    || matches!(
                        error,
                        RuntimeError::McpAttach {
                            stage: "cleanup",
                            ..
                        }
                    )
                {
                    self.fatal = Some(if error == RuntimeError::Cancelled {
                        error.clone()
                    } else {
                        RuntimeError::McpShutdown
                    });
                }
                node.row.diagnostic = Some(connection_diagnostic(&server, &node.source, &error));
                if !self
                    .generation
                    .servers
                    .iter()
                    .any(|client| client.server_id == server)
                {
                    node.row.status = if node
                        .row
                        .diagnostic
                        .as_ref()
                        .is_some_and(|d| d.code == ServiceCode::Unauthorized)
                    {
                        McpStatus::NeedsAuth
                    } else {
                        McpStatus::Failed
                    };
                }
                node.row.pending_action = None;
                if matches!(error, RuntimeError::McpAttach { .. }) {
                    record_degradation(&mut self.generation.degraded, &server, error);
                }
            }
        }
        if let Err(error) = self.publish() {
            self.fatal = Some(error);
        }
        if self.fatal.is_some() {
            self.begin_stop();
        }
        self.drain_retiring();
    }

    fn request(&mut self, ack: RequestAck, operation: bool) {
        self.requests.retain(|(reply, _)| !reply.is_closed());
        if ack.is_closed() {
            return;
        }
        if self.requests.len() >= MAX_MCP_SERVERS {
            let _ = ack.send(Err(RuntimeError::TurnActive));
            return;
        }
        if operation {
            for (name, node) in &mut self.nodes {
                if node.suspended
                    && !node.desired
                    && self
                        .config
                        .mcp
                        .get(name)
                        .is_some_and(|entry| entry.enabled && entry.failure.is_none())
                {
                    node.desired = true;
                    node.replacing = true;
                    node.row.status = McpStatus::Pending;
                }
            }
            self.pump_replacements();
            self.release_retirements();
        }
        for server in &self.generation.servers {
            let node = self
                .nodes
                .get_mut(&server.server_id)
                .expect("MCP registry owner");
            let changed = match server.client.as_ref() {
                AttachedServer::Remote(client) => client.claim_catalog_changed(),
                AttachedServer::Stdio(client) => client.claim_catalog_changed(),
            };
            if !changed {
                continue;
            }
            if node.cancel.is_some() || node.row.pending_action.is_some() {
                restore_catalog_changed(&server.client);
                continue;
            }
            let client = server.client.clone();
            let id = server.server_id.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let incarnation = node.incarnation.clone();
            node.cancel = Some(cancel.clone());
            self.work.spawn(async move {
                let result = list_registry(&id, &client, &cancel).await;
                if result.is_err() {
                    restore_catalog_changed(&client);
                }
                Completed {
                    server: id,
                    incarnation,
                    refresh: true,
                    result: result.map(WorkResult::Refreshed),
                }
            });
        }
        self.requests.push((ack, operation));
        self.release_requests();
    }

    fn release_requests(&mut self) {
        if self.nodes.values().any(|node| {
            node.cancel.is_some() && node.row.status == McpStatus::Connected
                || node.suspended && node.desired
        }) {
            return;
        }
        let publication = self.shared.publication.read().expect("MCP publication");
        for (ack, _) in self.requests.drain(..) {
            let result = self
                .fatal
                .clone()
                .map_or_else(|| Ok(publication.request.clone()), Err);
            let _ = ack.send(result);
        }
    }

    fn retire(&mut self, poisoned: bool, ack: RetireAck) {
        if self.retirements.len() >= MAX_MCP_SERVERS {
            let _ = ack.send(Err(RuntimeError::TurnActive));
            return;
        }
        // Execution interest is synchronized with the actual retirement decision.
        // Metadata request cancellation alone never retires a shared incarnation.
        let shared = self.shared.clone();
        let interests = shared.operations.lock().expect("MCP operations");
        if !poisoned
            && (*interests != 0
                || self
                    .requests
                    .iter()
                    .any(|(reply, operation)| !operation && !reply.is_closed())
                || !self.lookups.is_empty())
        {
            let _ = ack.send(Ok(()));
            return;
        }
        let names: Vec<_> = if poisoned {
            self.generation
                .servers
                .iter()
                .filter(|server| server.poisoned.load(Ordering::SeqCst))
                .map(|server| server.server_id.clone())
                .collect()
        } else {
            self.nodes
                .iter()
                .filter(|(_, node)| {
                    node.cancel.is_some() && node.row.pending_action != Some(McpAction::Disconnect)
                })
                .map(|(name, _)| name.clone())
                .collect()
        };
        let mut incarnations = Vec::new();
        for name in names {
            incarnations.push((name.clone(), self.nodes[&name].incarnation.clone()));
            self.retire_server(&name, true);
            let node = self.nodes.get_mut(&name).expect("retiring MCP node");
            node.suspended = true;
            node.replacing = true;
            // Retire the uncertain incarnation, not the configured connection.
            // A replacement may restore its catalog after confirmed close; it
            // never retries the tool effect or dispatches a provider request.
            // Remote replacements still pass the owner-lifetime quarantine.
            node.desired = poisoned;
        }
        // Never wait for another root's legitimate captured client lease.
        let wait = *interests <= usize::from(poisoned);
        drop(interests);
        if poisoned {
            self.generation.poisoned = Arc::new(AtomicBool::new(false));
        }
        if let Err(error) = self.publish() {
            let _ = ack.send(Err(error));
            return;
        }
        self.drain_retiring();
        if wait {
            self.retirements.push((incarnations, ack));
            self.release_retirements();
        } else {
            let _ = ack.send(Ok(()));
        }
    }

    fn release_retirements(&mut self) {
        let mut pending = Vec::new();
        for (incarnations, ack) in self.retirements.drain(..) {
            if let Some(error) = &self.fatal {
                let _ = ack.send(Err(error.clone()));
            } else if incarnations.iter().any(|(_, incarnation)| {
                self.instances
                    .iter()
                    .any(|(_, actual)| Arc::ptr_eq(actual, incarnation))
            }) {
                pending.push((incarnations, ack));
            } else {
                let _ = ack.send(Ok(()));
            }
        }
        self.retirements = pending;
    }

    fn control(&mut self, control: McpControl, ack: StatusAck) {
        let binding = self
            .shared
            .publication
            .read()
            .expect("MCP publication")
            .status
            .binding
            .clone();
        let name = self
            .nodes
            .iter()
            .find(|(_, node)| node.row.id == control.server)
            .map(|(name, _)| name.clone());
        let result = (|| -> Result<(), &'static str> {
            if self.stopping || binding != control.binding {
                return Err("stale MCP scope; reopen servers");
            }
            let server = name.as_ref().ok_or("unknown MCP server")?;
            let node = &self.nodes[server];
            if node.row.pending_action == Some(control.action) {
                return Ok(());
            }
            if node.row.pending_action.is_some() || !node.row.actions.contains(&control.action) {
                return Err("MCP action unsupported in current state");
            }
            if control.action == McpAction::Disconnect {
                self.stop_server(server);
            } else {
                if self.generation.remote_unknown.load(Ordering::SeqCst)
                    || self.generation.poisoned.load(Ordering::SeqCst)
                {
                    return Err("MCP outcome unknown; retry unsafe");
                }
                if self.instances.len() >= MAX_MCP_SERVERS {
                    return Err("too many enabled MCP servers");
                }
                let activation = self
                    .activation
                    .as_ref()
                    .ok_or("MCP activation admission unavailable")?;
                let (entry, protected) =
                    activation.activate(&self.config, &self.project, &self.env, server);
                self.protect_labels(&protected);
                let entry = entry.map_err(|_| "MCP activation admission refused")?;
                let node = self.nodes.get_mut(server).expect("MCP node");
                if let Some(failure) = &entry.failure {
                    node.row.status = McpStatus::Failed;
                    node.row.diagnostic = Some(failure.clone());
                    node.row.pending_action = None;
                    return Ok(());
                }
                node.desired = true;
                node.row.status = McpStatus::Pending;
                node.row.pending_action = Some(control.action);
                node.row.diagnostic = None;
                self.connect(server, entry);
            }
            Ok(())
        })();
        if let Err(error) = self.publish() {
            self.fatal = Some(error);
            self.begin_stop();
        }
        self.drain_retiring();
        let snapshot = self
            .shared
            .publication
            .read()
            .expect("MCP publication")
            .status
            .clone();
        let _ = ack.send(
            result
                .map(|()| snapshot)
                .map_err(|message| oc_core::session::CoreError::Application(message.into())),
        );
    }

    async fn run(mut self, mut controls: mpsc::Receiver<Command>) -> Result<(), RuntimeError> {
        loop {
            if self.shared.stop.load(Ordering::SeqCst) {
                self.begin_stop();
            }
            self.pump_replacements();
            self.release_retirements();
            if self.stopping && self.work.is_empty() && self.retiring.is_empty() {
                break;
            }
            tokio::select! {
                _ = self.shared.wake.notified() => { self.drain_retiring(); },
                result = self.work.join_next(), if !self.work.is_empty() => {
                    match result {
                        Some(Ok(result)) => self.complete(result),
                        Some(Err(_)) => { self.fatal = Some(RuntimeError::McpShutdown); self.begin_stop(); },
                        None => {},
                    }
                    self.release_requests();
                    self.release_retirements();
                },
                command = controls.recv(), if !self.stopping => match command {
                    Some(Command::Reconcile { config, publication, activation, force, commit, ack }) => {
                        let result = self.reconcile(*config, publication, activation, force, commit);
                        let _ = ack.send(result);
                    },
                    Some(Command::Control { control, ack }) => self.control(control, ack),
                    Some(Command::Request { ack, operation }) => self.request(ack, operation),
                    Some(Command::Retire { poisoned, ack }) => self.retire(poisoned, ack),
                    Some(Command::Lookup { query, cancel, ack }) => self.lookup(query, cancel, ack),
                    None => self.begin_stop(),
                },
            }
        }
        self.fatal.map_or(Ok(()), Err)
    }
}

fn safe_display_label(label: &str, protected: &[String]) -> bool {
    !label.is_empty()
        && !label.chars().any(|c| {
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        && !label.contains(['/', '\\'])
        && !label.contains("{env:")
        && !label.contains("{file:")
        && reqwest::Url::parse(label).is_err()
        && !protected
            .iter()
            .any(|value| !value.is_empty() && label.contains(value))
}

pub(in crate::runtime) fn connection_diagnostic(
    server: &str,
    source: &str,
    error: &RuntimeError,
) -> ServiceDiagnostic {
    // These are typed native error tags, never remote text/status regexes.
    let (stage, code, retryable) = match error {
        RuntimeError::McpAttach {
            stage,
            safe_code,
            retryable,
            ..
        } => (*stage, *safe_code, *retryable),
        RuntimeError::Cancelled => ("initialize", "cancelled", false),
        _ => ("cleanup", "cleanup_failed", false),
    };
    let code = match code {
        "private_host" => ServiceCode::PrivateHost,
        "resolution_failed" => ServiceCode::ResolutionFailed,
        "connection_failed" => ServiceCode::ConnectionFailed,
        "spawn_failed" => ServiceCode::SpawnFailed,
        "deadline" => ServiceCode::Deadline,
        "transport" => ServiceCode::Transport,
        "unauthorized" => ServiceCode::Unauthorized,
        "forbidden" => ServiceCode::Forbidden,
        "protocol_mismatch" => ServiceCode::ProtocolMismatch,
        "catalog_limit" => ServiceCode::CatalogLimit,
        "invalid_catalog" => ServiceCode::InvalidCatalog,
        "cleanup_failed" => ServiceCode::CleanupFailed,
        "cancelled" => ServiceCode::Cancelled,
        "unsafe_retry" => ServiceCode::UnsafeRetry,
        "invalid_header" => ServiceCode::InvalidHeader,
        "authorization_header_conflict" => ServiceCode::AuthorizationHeaderConflict,
        "header_conflict" => ServiceCode::HeaderConflict,
        _ => ServiceCode::InvalidConfig,
    };
    let mut diagnostic = crate::config::mcp::failure(server, source, "connection", code);
    diagnostic.stage = match stage {
        "config" => ServiceStage::Config,
        "DNS" => ServiceStage::Dns,
        "connect" | "spawn" => ServiceStage::Connection,
        "tools-list" => ServiceStage::Catalog,
        "cleanup" => ServiceStage::Cleanup,
        "call" => ServiceStage::Call,
        _ => ServiceStage::Initialize,
    };
    diagnostic.action = if code == ServiceCode::Unauthorized {
        ServiceAction::SignInUnsupported
    } else if code == ServiceCode::UnsafeRetry || code == ServiceCode::CleanupFailed {
        ServiceAction::RestartApplication
    } else if retryable || code == ServiceCode::SpawnFailed {
        ServiceAction::RetryConnection
    } else {
        ServiceAction::ReviewConfiguration
    };
    diagnostic
}

fn restore_catalog_changed(client: &AttachedServer) {
    match client {
        AttachedServer::Remote(client) => client.restore_catalog_changed(),
        AttachedServer::Stdio(client) => client.restore_catalog_changed(),
    }
}

async fn list_registry(
    server: &str,
    client: &AttachedServer,
    cancel: &AtomicBool,
) -> Result<Vec<mcp_remote::RegistryEntry>, RuntimeError> {
    let tools = match client {
        AttachedServer::Remote(client) => client
            .list_tools(cancel)
            .await
            .map_err(|error| remote_attach_error(server, error))?,
        AttachedServer::Stdio(client) => client
            .list_tools(cancel)
            .await
            .map_err(|error| stdio_attach_error(server, error))?,
    };
    mcp_remote::map_registry(
        server,
        tools
            .into_iter()
            .map(|tool| (tool.name, tool.description, tool.input_schema))
            .collect(),
    )
    .map_err(|error| remote_attach_error(server, error))
}

async fn close_client(client: AttachedServer) -> Result<(), ()> {
    match client {
        AttachedServer::Remote(client) => client.close().await.map_err(|_| ()),
        AttachedServer::Stdio(client) => client.shutdown().await.map_err(|_| ()),
    }
}

async fn connect_one(
    server: &str,
    entry: &crate::config::McpEntry,
    project: &std::path::Path,
    env: &BTreeMap<String, String>,
    redactions: &[String],
    cancel: &AtomicBool,
) -> Result<AttachedMcp, RuntimeError> {
    let client = if entry.kind == "remote" {
        let codex_web = server == "codex_web";
        let mut config = if codex_web {
            mcp_remote::CodexWebConfig::from_entry(entry)
        } else {
            mcp_remote::CodexWebConfig::from_remote_entry(entry)
        }
        .map_err(|error| remote_attach_error_at(server, "config", error))?;
        config.allow_private = env
            .get("OC_TEST_ALLOW_LOOPBACK")
            .is_some_and(|value| value == "1");
        AttachedServer::Remote(
            CodexWebClient::connect_redacted(&config, codex_web, cancel, redactions)
                .await
                .map_err(|error| remote_attach_error_at(server, "initialize", error))?,
        )
    } else if entry.kind == "local" {
        let config = StdioConfig::from_entry(server, entry, project, env)
            .map_err(|error| stdio_attach_error_at(server, "config", error))?;
        AttachedServer::Stdio(
            StdioClient::launch_redacted(&config, cancel, redactions)
                .await
                .map_err(|error| stdio_attach_error_at(server, "initialize", error))?,
        )
    } else {
        return Err(RuntimeError::McpAttach {
            server: safe_server_id(server),
            stage: "config",
            safe_code: "unsupported_transport",
            retryable: false,
        });
    };
    let registry = match list_registry(server, &client, cancel).await {
        Ok(registry) => registry,
        Err(error) => {
            close_client(client)
                .await
                .map_err(|_| RuntimeError::McpShutdown)?;
            return Err(error);
        }
    };
    Ok(AttachedMcp {
        server_id: server.into(),
        poisoned: Arc::new(AtomicBool::new(false)),
        client: Arc::new(client),
        registry,
        code_mode: entry.codemode != Some(false),
    })
}

#[cfg(test)]
mod tests;
