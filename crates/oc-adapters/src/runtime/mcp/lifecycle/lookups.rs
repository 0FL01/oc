//! Explicit queries on the supervisor's exact existing clients and metadata.
use super::*;
use oc_core::queries::{McpLookup, McpLookupData, McpLookupError, McpLookupOp, McpLookupReply};

pub(super) type LookupAck = oneshot::Sender<Result<McpLookupReply, McpLookupError>>;
pub(super) struct LookupActive {
    pub(super) server: String,
    pub(super) cancel: Arc<AtomicBool>,
}
pub(super) struct LookupCompleted {
    query: McpLookup,
    cancel: Arc<AtomicBool>,
    ack: LookupAck,
    result: Result<McpLookupData, McpLookupError>,
    remote: bool,
    dispatched: bool,
    connection: std::sync::Weak<AttachedServer>,
    source: String,
}
fn catalog_slot(op: &McpLookupOp) -> Option<usize> {
    match op {
        McpLookupOp::ListPrompts => Some(0),
        McpLookupOp::ListResources => Some(1),
        McpLookupOp::ListResourceTemplates => Some(2),
        _ => None,
    }
}

impl McpOwner {
    pub(in crate::runtime) fn lookup_server(
        &self,
        query: &McpLookup,
    ) -> Result<String, McpLookupError> {
        let publication = self.shared.publication.read().expect("MCP publication");
        if publication.fatal.is_some() {
            return Err(McpLookupError::NativeFailure);
        }
        if publication.status.binding != query.binding || self.shared.stop.load(Ordering::SeqCst) {
            return Err(McpLookupError::StaleBinding);
        }
        let row = publication
            .status
            .servers
            .iter()
            .find(|row| row.id == query.server)
            .ok_or(McpLookupError::Unavailable)?;
        if row.status != McpStatus::Connected {
            return Err(McpLookupError::Unavailable);
        }
        publication
            .request
            .servers
            .iter()
            .find(|server| {
                use sha2::{Digest, Sha256};
                let digest = Sha256::digest(server.server_id.as_bytes());
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
                ) == row.id
            })
            .map(|server| server.server_id.clone())
            .ok_or(McpLookupError::Unavailable)
    }
    pub(in crate::runtime) fn enqueue_lookup(
        &self,
        query: McpLookup,
        cancel: Arc<AtomicBool>,
        ack: LookupAck,
    ) {
        let command = Command::Lookup { query, cancel, ack };
        let failed = match self.control.lock().expect("MCP control").as_ref() {
            Some(sender) => sender
                .try_send(command)
                .err()
                .map(|error| error.into_inner()),
            None => Some(command),
        };
        if let Some(Command::Lookup { ack, .. }) = failed {
            let _ = ack.send(Err(McpLookupError::Unavailable));
        }
    }
}

impl Scope {
    pub(super) fn lookup(&mut self, query: McpLookup, cancel: Arc<AtomicBool>, ack: LookupAck) {
        let publication = self.shared.publication.read().expect("MCP publication");
        if publication.status.binding != query.binding || self.stopping {
            let _ = ack.send(Err(McpLookupError::StaleBinding));
            return;
        }
        drop(publication);
        if cancel.load(Ordering::SeqCst) {
            let _ = ack.send(Err(McpLookupError::Cancelled));
            return;
        }
        if let Err(error) = crate::mcp_lookup::validate_operation(&query.operation) {
            let _ = ack.send(Err(error));
            return;
        }
        let Some((server, node)) = self
            .nodes
            .iter_mut()
            .find(|(_, node)| node.row.id == query.server)
        else {
            let _ = ack.send(Err(McpLookupError::Unavailable));
            return;
        };
        if node.row.status != McpStatus::Connected || node.closing || !node.desired {
            let _ = ack.send(Err(McpLookupError::Unavailable));
            return;
        }
        if let Some(slot) = catalog_slot(&query.operation) {
            if !query.refresh
                && let Some(data) = &node.catalogs[slot]
            {
                let source = crate::config::mcp::failure(
                    server,
                    &node.source,
                    "lookup",
                    ServiceCode::Transport,
                )
                .source;
                let _ = ack.send(Ok(McpLookupReply {
                    binding: query.binding,
                    server: query.server,
                    source,
                    cached: true,
                    data: data.clone(),
                }));
                return;
            }
            if node.catalog_pending[slot] {
                let _ = ack.send(Err(McpLookupError::Unavailable));
                return;
            }
        }
        let Some(client) = self
            .generation
            .servers
            .iter()
            .find(|client| &client.server_id == server)
        else {
            let _ = ack.send(Err(McpLookupError::Unavailable));
            return;
        };
        let remote = matches!(client.client.as_ref(), AttachedServer::Remote(_));
        let source =
            crate::config::mcp::failure(server, &node.source, "lookup", ServiceCode::Transport)
                .source;
        if remote
            && !query.operation.is_catalog()
            && (self.generation.remote_unknown.load(Ordering::SeqCst)
                || self
                    .shared
                    .safety
                    .read()
                    .expect("MCP safety")
                    .remote
                    .load(Ordering::SeqCst))
        {
            let _ = ack.send(Err(McpLookupError::UnsafeRetry));
            return;
        }
        if self.lookups.len() >= MCP_PENDING_CAP {
            let _ = ack.send(Err(McpLookupError::Unavailable));
            return;
        }
        if let Some(slot) = catalog_slot(&query.operation) {
            node.catalog_pending[slot] = true;
        }
        let client = client.client.clone();
        let incarnation = node.incarnation.clone();
        let connection = Arc::downgrade(&client);
        let server = server.clone();
        let wake = self.shared.wake.clone();
        self.lookups.push(LookupActive {
            server: server.clone(),
            cancel: cancel.clone(),
        });
        self.work.spawn(async move {
            let dispatched = AtomicBool::new(false);
            let result = match client.as_ref() {
                AttachedServer::Remote(client) => {
                    client
                        .lookup_owned(&query.operation, &cancel, &dispatched)
                        .await
                }
                AttachedServer::Stdio(client) => {
                    client
                        .lookup_owned(&query.operation, &cancel, &dispatched)
                        .await
                }
            };
            // Retirement must be woken after releasing this target-only lease.
            drop(client);
            wake.notify_one();
            Completed {
                server,
                incarnation,
                refresh: false,
                result: Ok(WorkResult::Lookup(Box::new(LookupCompleted {
                    query,
                    cancel,
                    ack,
                    result,
                    remote,
                    connection,
                    source,
                    dispatched: dispatched.load(Ordering::SeqCst),
                }))),
            }
        });
    }

    pub(super) fn complete_lookup(&mut self, server: &str, lookup: LookupCompleted) {
        self.lookups
            .retain(|active| !Arc::ptr_eq(&active.cancel, &lookup.cancel));
        let same_connection = self
            .generation
            .servers
            .iter()
            .find(|client| client.server_id == server)
            .is_some_and(|client| {
                std::sync::Weak::ptr_eq(&lookup.connection, &Arc::downgrade(&client.client))
            });
        let slot = catalog_slot(&lookup.query.operation);
        if same_connection && let Some(slot) = slot {
            self.nodes
                .get_mut(server)
                .expect("lookup scope")
                .catalog_pending[slot] = false;
        }
        let mut result = lookup.result;
        let uncertain = matches!(
            result,
            Err(McpLookupError::Cancelled
                | McpLookupError::Deadline
                | McpLookupError::Transport
                | McpLookupError::UnsupportedResult
                | McpLookupError::InvalidData
                | McpLookupError::BodyLimit
                | McpLookupError::SensitiveBinary
                | McpLookupError::SensitiveIdentity)
        );
        if lookup.remote && lookup.dispatched && slot.is_none() && uncertain {
            self.generation.remote_unknown.store(true, Ordering::SeqCst);
            if let Some(node) = self.nodes.get_mut(server) {
                node.row.diagnostic = Some(connection_diagnostic(
                    server,
                    &node.source,
                    &RuntimeError::McpAttach {
                        server: safe_server_id(server),
                        stage: "call",
                        safe_code: "unsafe_retry",
                        retryable: false,
                    },
                ));
            }
            if let Err(error) = self.publish() {
                self.fatal = Some(error);
                self.begin_stop();
            }
        }
        if same_connection
            && matches!(result, Err(McpLookupError::BodyLimit))
            && let Some((bytes, limit)) = self
                .generation
                .servers
                .iter()
                .find(|client| client.server_id == server)
                .and_then(|client| match client.client.as_ref() {
                    AttachedServer::Remote(client) => client.message_fault.get(),
                    AttachedServer::Stdio(client) => client.message_fault.get(),
                })
        {
            let node = self.nodes.get_mut(server).expect("lookup scope");
            if node
                .row
                .diagnostic
                .as_ref()
                .is_none_or(|diagnostic| diagnostic.code != ServiceCode::UnsafeRetry)
            {
                node.row.diagnostic = Some(connection_diagnostic(
                    server,
                    &node.source,
                    &RuntimeError::McpMessageLimit {
                        stage: if slot.is_some() { "tools-list" } else { "call" },
                        bytes,
                        limit,
                    },
                ));
            }
            node.row
                .diagnostic
                .as_mut()
                .expect("message diagnostic")
                .details = Some(Box::new(vec![format!(
                "MCP message: {bytes} bytes observed; limit: {limit} bytes"
            )]));
            if let Err(error) = self.publish() {
                self.fatal = Some(error);
                self.begin_stop();
            }
        }
        if matches!(result, Err(McpLookupError::CleanupFailed)) {
            self.generation.cleanup_error.store(true, Ordering::SeqCst);
            self.fatal = Some(RuntimeError::McpShutdown);
            self.begin_stop();
        }
        if self.stopping || lookup.cancel.load(Ordering::SeqCst) {
            if result.is_ok() {
                result = Err(McpLookupError::Cancelled);
            }
        } else if same_connection && let (Some(slot), Ok(data)) = (slot, &result) {
            let old = self.nodes.get_mut(server).expect("lookup scope").catalogs[slot]
                .replace(data.clone());
            if let Err(limit) = self.catalog_budget(None) {
                self.nodes.get_mut(server).expect("lookup scope").catalogs[slot] = old;
                result = Err(McpLookupError::CatalogLimit);
                let node = self.nodes.get_mut(server).expect("lookup scope");
                node.row.diagnostic = Some(connection_diagnostic(
                    server,
                    &node.source,
                    &RuntimeError::McpCatalogLimit(limit),
                ));
                if let Err(error) = self.publish() {
                    self.fatal = Some(error);
                    self.begin_stop();
                }
            }
        }
        let _ = lookup.ack.send(result.map(|data| McpLookupReply {
            binding: lookup.query.binding,
            server: lookup.query.server,
            source: lookup.source,
            cached: false,
            data,
        }));
        self.drain_retiring();
    }

    pub(super) fn catalog_budget(
        &self,
        replacement: Option<(&str, &[mcp_remote::RegistryEntry])>,
    ) -> Result<(), mcp_remote::CatalogLimit> {
        let mut total = 0usize;
        for (server, node) in &self.nodes {
            let registry = replacement
                .filter(|(name, _)| name == server)
                .map(|(_, registry)| registry)
                .or_else(|| {
                    self.generation
                        .servers
                        .iter()
                        .find(|client| &client.server_id == server)
                        .map(|client| client.registry.as_slice())
                });
            let mut bytes = registry.map_or(0, |registry| mcp_remote::metadata_bytes(&registry));
            for data in node.catalogs.iter().flatten() {
                bytes = bytes.saturating_add(mcp_remote::metadata_bytes(data));
            }
            mcp_remote::CatalogLimit::check(bytes, mcp_remote::CATALOG_BYTES_CAP)?;
            total = total.saturating_add(bytes);
        }
        mcp_remote::CatalogLimit::check(total, mcp_remote::GENERATION_CATALOG_BYTES_CAP)
    }
}
