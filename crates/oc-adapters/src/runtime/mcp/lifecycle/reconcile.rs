//! Desired configuration changes on the one Location resource supervisor.
//! Replacements wait for physical leases and confirmed cleanup, not a counter.
use super::*;

impl McpOwner {
    pub(in crate::runtime) async fn reconcile(
        &self,
        config: Generation,
        publication: u64,
        activation: Option<Arc<crate::composition::McpActivation>>,
        force: bool,
    ) -> Result<McpSnapshot, RuntimeError> {
        if let Some(error) = self.fatal() {
            return Err(error);
        }
        let sender = self
            .control
            .lock()
            .expect("MCP control")
            .clone()
            .ok_or(RuntimeError::McpShutdown)?;
        let (ack, reply) = oneshot::channel();
        sender
            .send(Command::Reconcile {
                config,
                publication,
                activation,
                force,
                ack,
            })
            .await
            .map_err(|_| RuntimeError::McpShutdown)?;
        reply.await.map_err(|_| RuntimeError::McpShutdown)?
    }
}

impl Scope {
    pub(super) fn reconcile(
        &mut self,
        config: Generation,
        publication: u64,
        activation: Option<Arc<crate::composition::McpActivation>>,
        force: bool,
    ) -> Result<McpSnapshot, RuntimeError> {
        if self.stopping || self.shared.stop.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpShutdown);
        }
        let enabled = config.mcp.values().filter(|entry| entry.enabled).count();
        if enabled > MAX_MCP_SERVERS {
            return Err(RuntimeError::InvalidArgs(format!(
                "too many enabled MCP servers: {enabled} exceeds {MAX_MCP_SERVERS}"
            )));
        }
        let mut protected = mcp_redactions(&config, &self.env);
        for entry in config.mcp.values() {
            protected.extend(crate::config::mcp::entry_credential_values(
                entry, &self.env,
            ));
        }
        if let Some(activation) = &activation
            && !config.mcp.is_empty()
        {
            protected.extend(
                activation
                    .protected_values(&self.env)
                    .map_err(RuntimeError::InvalidArgs)?,
            );
        }
        // Admission and all fallible source reads precede retirement. The source
        // coordinator supplies only fully validated, still-current candidates.
        let names: Vec<_> = self.nodes.keys().cloned().collect();
        for server in names {
            let next = config.mcp.get(&server);
            let unchanged = !force
                && self
                    .config
                    .mcp
                    .get(&server)
                    .zip(next)
                    .is_some_and(|(old, new)| same_connection(old, new));
            if unchanged {
                let entry = next.expect("unchanged admitted entry");
                if let Some(client) = self
                    .generation
                    .servers
                    .iter_mut()
                    .find(|client| client.server_id == server)
                {
                    client.code_mode = entry.codemode != Some(false);
                }
                continue;
            }
            // Automatic replacement never cancels an already dispatched lookup.
            // Removing it from the new registry only releases the owner's lease.
            self.retire_server(&server, false);
            let node = self.nodes.get_mut(&server).expect("retiring node");
            node.replacing = true;
            node.catalogs = [None, None, None];
            node.catalog_pending = [false; 3];
            clear_degradation(&mut self.generation.degraded, &server);
            if let Some(entry) = next {
                let row = configured_node(&server, entry, &config, &protected);
                node.source = row.source;
                node.desired = row.desired;
                node.row.configured_enabled = entry.enabled;
                node.row.diagnostic = row.row.diagnostic;
            } else {
                node.desired = false;
                node.row.configured_enabled = false;
            }
            node.row.status = McpStatus::Pending;
            node.row.pending_action = Some(McpAction::Disconnect);
        }
        for (server, entry) in &config.mcp {
            if !self.nodes.contains_key(server) {
                self.nodes.insert(
                    server.clone(),
                    configured_node(server, entry, &config, &protected),
                );
            }
        }
        self.generation.admitted = Arc::new(config.mcp.clone());
        self.config = config;
        self.activation = activation;
        self.generation.publication = publication;
        self.protect_labels(&protected);
        self.drain_retiring();
        self.pump_replacements();
        if let Some(error) = &self.fatal {
            return Err(error.clone());
        }
        if let Err(error) = self.publish() {
            self.fatal = Some(error.clone());
            self.begin_stop();
            return Err(error);
        }
        Ok(self
            .shared
            .publication
            .read()
            .expect("MCP publication")
            .status
            .clone())
    }

    pub(super) fn pump_replacements(&mut self) {
        if self.stopping || self.fatal.is_some() {
            return;
        }
        let names: Vec<_> = self.nodes.keys().cloned().collect();
        let mut changed = false;
        for server in names {
            if self.instances.iter().any(|(name, _)| name == &server) {
                continue;
            }
            let Some(entry) = self.config.mcp.get(&server).cloned() else {
                self.nodes.remove(&server);
                changed = true;
                continue;
            };
            let node = self.nodes.get_mut(&server).expect("desired node");
            if node.replacing {
                node.closing = false;
                node.cancel = None;
                node.row.status = if entry.failure.is_some() {
                    McpStatus::Failed
                } else if node.desired {
                    McpStatus::Pending
                } else {
                    McpStatus::Disabled
                };
                node.row.pending_action = node.desired.then_some(McpAction::Connect);
                node.row.diagnostic = entry.failure.clone();
                node.replacing = false;
                changed = true;
            }
            if !node.desired
                || node.cancel.is_some()
                || node.row.status != McpStatus::Pending
                || self.instances.len() >= MAX_MCP_SERVERS
            {
                continue;
            }
            if entry.kind == "remote" && self.generation.remote_unknown.load(Ordering::SeqCst) {
                node.row.status = McpStatus::Failed;
                node.row.pending_action = None;
                node.row.diagnostic = Some(connection_diagnostic(
                    &server,
                    &node.source,
                    &RuntimeError::McpAttach {
                        server: safe_server_id(&server),
                        stage: "call",
                        safe_code: "unsafe_retry",
                        retryable: false,
                    },
                ));
            } else {
                self.connect(&server, entry);
            }
            changed = true;
        }
        if changed && let Err(error) = self.publish() {
            self.fatal = Some(error);
            self.begin_stop();
        }
    }
}

fn same_connection(old: &crate::config::McpEntry, new: &crate::config::McpEntry) -> bool {
    let mut old = old.clone();
    let mut new = new.clone();
    // Routing is immutable request-view metadata, not physical client identity.
    old.codemode = None;
    new.codemode = None;
    old == new
}

pub(super) fn configured_node(
    server: &str,
    entry: &crate::config::McpEntry,
    config: &Generation,
    protected: &[String],
) -> Node {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(server.as_bytes());
    Node {
        incarnation: Arc::new(()),
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
            name: if safe_display_label(server, protected) {
                server.into()
            } else {
                safe_server_id(server)
            },
            configured_enabled: entry.enabled,
            status: if entry.failure.is_some() {
                McpStatus::Failed
            } else if entry.enabled {
                McpStatus::Pending
            } else {
                McpStatus::Disabled
            },
            pending_action: (entry.enabled && entry.failure.is_none())
                .then_some(McpAction::Connect),
            tools: 0,
            diagnostic: entry.failure.clone(),
            actions: Vec::new(),
        },
        source: config
            .provenance
            .get(&format!("mcp.{server}"))
            .cloned()
            .unwrap_or_else(|| "native config".into()),
        desired: entry.enabled && entry.failure.is_none(),
        cancel: None,
        closing: false,
        replacing: false,
        catalogs: [None, None, None],
        catalog_pending: [false; 3],
    }
}
