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
use lookups::{LookupAck, LookupActive, LookupCompleted};

type StatusAck = oneshot::Sender<Result<McpSnapshot, oc_core::session::CoreError>>;
type RequestAck = oneshot::Sender<Result<Arc<McpGeneration>, RuntimeError>>;

/// Application-owned supervisor handle. Only the resource task mutates clients;
/// short read locks expose bounded immutable snapshots, never a turn-held mutex.
pub(in crate::runtime) struct McpOwner {
    shared: Arc<Shared>,
    control: Mutex<Option<mpsc::Sender<Command>>>,
    task: tokio::sync::Mutex<Option<JoinHandle<Result<(), RuntimeError>>>>,
}

struct Shared {
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

struct RequestWait<'a> {
    owner: &'a McpOwner,
    armed: bool,
}

impl Drop for RequestWait<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.owner
                .shared
                .publication
                .read()
                .expect("MCP publication")
                .request
                .poisoned
                .store(true, Ordering::SeqCst);
            self.owner.shared.stop.store(true, Ordering::SeqCst);
            self.owner.shared.wake.notify_one();
        }
    }
}

enum Command {
    Control {
        control: McpControl,
        ack: StatusAck,
    },
    Request(RequestAck),
    Lookup {
        query: oc_core::queries::McpLookup,
        cancel: Arc<AtomicBool>,
        ack: LookupAck,
    },
}

struct Node {
    row: McpServerSnapshot,
    source: String,
    desired: bool,
    cancel: Option<Arc<AtomicBool>>,
    closing: bool,
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
    retiring: Vec<(String, Arc<AttachedServer>)>,
    requests: Vec<RequestAck>,
    lookups: Vec<LookupActive>,
    fatal: Option<RuntimeError>,
    stopping: bool,
}

impl McpOwner {
    pub(in crate::runtime) fn new(location: &str, generation: u64) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let wake = Arc::new(Notify::new());
        Self {
            shared: Arc::new(Shared {
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
        let mut nodes = BTreeMap::new();
        for (server, entry) in &config.mcp {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(server.as_bytes());
            let source = config
                .provenance
                .get(&format!("mcp.{server}"))
                .cloned()
                .unwrap_or_else(|| "native config".into());
            nodes.insert(
                server.clone(),
                Node {
                    row: McpServerSnapshot {
                        id: format!(
                            "mcp-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                            digest[0],
                            digest[1],
                            digest[2],
                            digest[3],
                            digest[4],
                            digest[5],
                            digest[6],
                            digest[7]
                        ),
                        name: safe_server_id(server),
                        configured_enabled: entry.enabled,
                        status: if entry.failure.is_some() {
                            McpStatus::Failed
                        } else if entry.enabled {
                            McpStatus::Pending
                        } else {
                            McpStatus::Disabled
                        },
                        pending_action: if entry.enabled && entry.failure.is_none() {
                            Some(McpAction::Connect)
                        } else {
                            None
                        },
                        tools: 0,
                        diagnostic: entry.failure.clone(),
                        actions: Vec::new(),
                    },
                    source,
                    desired: entry.enabled && entry.failure.is_none(),
                    cancel: None,
                    closing: false,
                    catalogs: [None, None, None],
                    catalog_pending: [false; 3],
                },
            );
        }
        let publication = self.shared.publication.read().expect("MCP publication");
        let generation = McpGeneration::empty(
            publication.status.binding.generation,
            self.shared.wake.clone(),
        );
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
            requests: Vec::new(),
            lookups: Vec::new(),
            fatal: None,
            stopping: false,
        };
        for node in scope.nodes.values() {
            if let Some(failure) = &node.row.diagnostic {
                record_degradation(
                    &mut scope.generation.degraded,
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
        // Empty configs need no background task, including synchronous fixtures.
        if scope.nodes.is_empty() {
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
    ) -> Result<Arc<McpGeneration>, RuntimeError> {
        if let Some(error) = self.fatal() {
            return Err(error);
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
        let (ack, mut reply) = oneshot::channel();
        let mut wait = RequestWait {
            owner: self,
            armed: true,
        };
        sender
            .send(Command::Request(ack))
            .await
            .map_err(|_| RuntimeError::McpShutdown)?;
        loop {
            tokio::select! {
                result = &mut reply => {
                    wait.armed = false;
                    return result.map_err(|_| RuntimeError::McpShutdown)?;
                },
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if cancel.load(Ordering::SeqCst) {
                        self.shared.publication.read().expect("MCP publication").request.poisoned.store(true, Ordering::SeqCst);
                        self.stop().await?;
                        wait.armed = false;
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
    fn connect(&mut self, server: &str, entry: crate::config::McpEntry) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.nodes.get_mut(server).expect("MCP node").cancel = Some(cancel.clone());
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
                refresh: false,
                result,
            }
        });
    }

    fn publish(&mut self) -> Result<(), RuntimeError> {
        self.generation.entries = mcp_remote::merge_registries(
            self.generation
                .servers
                .iter()
                .map(|server| server.registry.clone())
                .collect(),
        )
        .map_err(|error| remote_attach_error("generation", error))?;
        self.lookup_catalog_budget()
            .map_err(|_| remote_attach_error("generation", mcp_remote::McpError::CatalogLimited))?;
        let mut publication = self.shared.publication.write().expect("MCP publication");
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
        for lookup in &self.lookups {
            if lookup.server == server {
                lookup.cancel.store(true, Ordering::SeqCst);
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
            self.retiring.push((server.to_string(), entry.client));
        } else if node.cancel.is_none() {
            node.row.pending_action = None;
            node.row.status = McpStatus::Disabled;
        }
    }

    fn drain_retiring(&mut self) {
        let mut waiting = Vec::new();
        for (server, client) in self.retiring.drain(..) {
            match Arc::try_unwrap(client) {
                Ok(client) => {
                    self.work.spawn(async move {
                        Completed {
                            server,
                            refresh: false,
                            result: close_client(client)
                                .await
                                .map(|()| WorkResult::Closed)
                                .map_err(|_| RuntimeError::McpShutdown),
                        }
                    });
                }
                Err(client) => waiting.push((server, client)),
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
        for ack in self.requests.drain(..) {
            let _ = ack.send(Err(RuntimeError::Cancelled));
        }
        if let Err(error) = self.publish() {
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
        let server = completed.server;
        let node = self.nodes.get_mut(&server).expect("MCP completion scope");
        node.cancel = None;
        // A relist's lease may finish before its result is consumed. A close
        // confirmation can therefore arrive first. Never let that stale relist
        // overwrite a disconnected node or reopen its controls during cleanup.
        if completed.refresh && !node.desired {
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
            Ok(WorkResult::Connected(client)) if node.desired && !self.stopping => {
                node.catalogs = [None, None, None];
                node.catalog_pending = [false; 3];
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
                self.retiring.push((server.clone(), client.client));
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
                    record_degradation(&mut self.generation.degraded, error);
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

    fn request(&mut self, ack: RequestAck) {
        if self.requests.len() >= MAX_MCP_SERVERS {
            let _ = ack.send(Err(RuntimeError::TurnActive));
            return;
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
            node.cancel = Some(cancel.clone());
            self.work.spawn(async move {
                let result = list_registry(&id, &client, &cancel).await;
                if result.is_err() {
                    restore_catalog_changed(&client);
                }
                Completed {
                    server: id,
                    refresh: true,
                    result: result.map(WorkResult::Refreshed),
                }
            });
        }
        self.requests.push(ack);
        self.release_requests();
    }

    fn release_requests(&mut self) {
        if self
            .nodes
            .values()
            .any(|node| node.cancel.is_some() && node.row.status == McpStatus::Connected)
        {
            return;
        }
        let publication = self.shared.publication.read().expect("MCP publication");
        for ack in self.requests.drain(..) {
            let result = self
                .fatal
                .clone()
                .map_or_else(|| Ok(publication.request.clone()), Err);
            let _ = ack.send(result);
        }
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
                if self
                    .nodes
                    .iter()
                    .filter(|(name, node)| *name != server && node.desired)
                    .count()
                    >= MAX_MCP_SERVERS
                {
                    return Err("too many enabled MCP servers");
                }
                let activation = self
                    .activation
                    .as_ref()
                    .ok_or("MCP activation admission unavailable")?;
                let entry = activation
                    .activate(&self.config, &self.project, &self.env, server)
                    .map_err(|_| "MCP activation admission refused")?;
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
                },
                command = controls.recv(), if !self.stopping => match command {
                    Some(Command::Control { control, ack }) => self.control(control, ack),
                    Some(Command::Request(ack)) => self.request(ack),
                    Some(Command::Lookup { query, cancel, ack }) => self.lookup(query, cancel, ack),
                    None => self.begin_stop(),
                },
            }
        }
        self.fatal.map_or(Ok(()), Err)
    }
}

fn connection_diagnostic(server: &str, source: &str, error: &RuntimeError) -> ServiceDiagnostic {
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
        client: Arc::new(client),
        registry,
    })
}

#[cfg(test)]
mod tests;
