//! Single native application owner behind the core command/event interface.
//!
//! The worker owns storage, the runtime and the effective model/variant/agent
//! selection. Frontends query bounded view snapshots and send actions; they
//! never open the database or duplicate config/persistence logic (T39).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use oc_core::core_app::{
    CoreApp, CoreEvent, InboxMsg, WorkerGuard, WorkerTurnId, normalized_session_title,
};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, DcpSnapshot, FileSuggestionsSnapshot, HistoryMessage, HistoryPage,
    HomeLocationSnapshot, LocationSnapshot, ModelEntry, ReloadLocationSnapshot, SessionProbe,
    SkillCard, StartupNotice, ToolOpPage, ToolOpView, VariantEntry,
};
use oc_core::session::{CoreError, LocationSwitchFailure, MAX_QUEUE_ITEMS, MessageId, Role};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::composition::{self, Composition};
use crate::runtime::{
    Runtime, RuntimeError, SubagentAgent, SubagentCatalog, ToolCallEvent, TurnParams, TurnStatus,
};
use crate::storage::{Db, StorageError};
use crate::trace;
use crate::tui_workspace::{AgentEntry as WorkspaceAgent, WorkspaceError, WorkspaceRegistry};

#[path = "application_selection.rs"]
mod selection;
#[path = "application_tab_deck.rs"]
mod tab_deck;

/// Bounded focus bytes accepted for a manual compress request.
pub const COMPRESS_FOCUS_MAX: usize = 256;
/// Bounded rows served per history page.
pub const HISTORY_PAGE_LIMIT: usize = 100;
/// Bounded rows served per tool-operation page.
pub const TOOL_OPS_PAGE_LIMIT: usize = 100;
/// Byte cap for the DCP token estimate input.
const ESTIMATE_BYTES: usize = 4 * 1024 * 1024;

/// Allowlisted stage/category for an interactive startup failure. No paths,
/// config values, provider responses or underlying error text cross this API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnFailure {
    /// Loading or validating the configured generation failed.
    Configuration,
    /// Selected provider has no nonempty API key in the configured generation.
    MissingCredential,
    /// Selected id cannot be resolved because discovery returned 401.
    DiscoveryUnauthorized,
    /// Selected id cannot be resolved because discovery returned 403.
    DiscoveryForbidden,
    /// Discovery endpoint returned a different unsuccessful HTTP status.
    DiscoveryHttp,
    /// Discovery could not reach the endpoint or timed out.
    DiscoveryNetwork,
    /// Discovery endpoint sent an invalid or empty catalog.
    DiscoveryInvalidResponse,
    /// Configured discovery URL, credential or headers were invalid.
    DiscoveryInvalidConfig,
    /// Discovery was cancelled before the selected id could be resolved.
    DiscoveryCancelled,
    /// Discovery succeeded but did not list the selected id.
    SelectedModelAbsent,
    /// Another process currently owns the exclusive data-root lock.
    DataRootBusy,
    /// Data-root validation refused an unsafe path or ownership.
    UnsafeDataRoot,
    /// The data directory cannot be opened or created.
    DataRootUnavailable,
    /// SQLite initialization failed after the data root was opened.
    Storage,
    /// Recovery of interrupted operations failed.
    Recovery,
    /// Native runtime construction/publication failed.
    Runtime,
}

#[derive(Debug)]
struct SpawnIssue {
    category: SpawnFailure,
    // Only the legacy headless route may consume this detail. TUI uses the
    // category alone, never Display/Debug of this issue.
    detail: String,
}

impl SpawnIssue {
    fn new(category: SpawnFailure, detail: String) -> Self {
        Self { category, detail }
    }
}

fn storage_failure(error: &StorageError) -> SpawnFailure {
    match error {
        StorageError::DataRootBusy => SpawnFailure::DataRootBusy,
        StorageError::UnsafeRoot(_) => SpawnFailure::UnsafeDataRoot,
        StorageError::Io(_) => SpawnFailure::DataRootUnavailable,
        _ => SpawnFailure::Storage,
    }
}

fn storage_class(error: &StorageError) -> &'static str {
    match error {
        StorageError::DataRootBusy => "DataRootBusy",
        StorageError::UnsafeRoot(_) => "UnsafeRoot",
        StorageError::StorageFull => "StorageFull",
        StorageError::BlobNotFound => "BlobNotFound",
        StorageError::SessionNotFound => "SessionNotFound",
        StorageError::OperationNotFound => "OperationNotFound",
        StorageError::SessionAlreadyExists => "SessionAlreadyExists",
        StorageError::CompressionConflict => "CompressionConflict",
        StorageError::Sqlite(_) => "Sqlite",
        StorageError::Io(_) => "Io",
    }
}

/// Compose and start one application. Both frontends use this entry point.
pub async fn spawn(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<String>), String> {
    let env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    spawn_with_env(project, data, env).await
}

/// Same as [`spawn`] with an explicit environment for config composition.
///
/// Integration tests and other embedders use this instead of mutating the
/// process environment (which is global and shared by parallel tests).
pub async fn spawn_with_env(
    project: &Path,
    data: &Path,
    env: BTreeMap<String, String>,
) -> Result<(CoreApp, WorkerGuard, Vec<String>), String> {
    spawn_inner(project, data, env)
        .await
        .map(|(app, guard, diagnostics, _)| (app, guard, diagnostics))
        .map_err(|issue| issue.detail)
}

/// Start the same application for a TUI, exposing only a static category on
/// failure; successful workers and their normal diagnostics are unchanged.
pub async fn spawn_diagnostic(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<StartupNotice>), SpawnFailure> {
    let env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    spawn_inner(project, data, env)
        .await
        .map(|(app, guard, _, notices)| (app, guard, notices))
        .map_err(|issue| issue.category)
}

async fn spawn_inner(
    project: &Path,
    data: &Path,
    env: BTreeMap<String, String>,
) -> Result<(CoreApp, WorkerGuard, Vec<String>, Vec<StartupNotice>), SpawnIssue> {
    trace::log(
        "spawn.begin",
        &format!(
            "project={} data={} env={}",
            project.display(),
            data.display(),
            env.len()
        ),
    );
    let result = spawn_stages(project, data, env).await;
    if let Err(issue) = &result {
        trace::log("spawn.fail", &format!("category={:?}", issue.category));
    }
    result
}

async fn spawn_stages(
    project: &Path,
    data: &Path,
    env: BTreeMap<String, String>,
) -> Result<(CoreApp, WorkerGuard, Vec<String>, Vec<StartupNotice>), SpawnIssue> {
    let composition = composition::load_with_env_diagnostic(project, env)
        .await
        .map_err(|failure| match failure {
            composition::LoadFailure::Configuration(detail) => {
                SpawnIssue::new(SpawnFailure::Configuration, detail)
            }
            composition::LoadFailure::MissingCredential(detail) => {
                SpawnIssue::new(SpawnFailure::MissingCredential, detail)
            }
            composition::LoadFailure::Discovery { reason, detail } => {
                use crate::discovery::DiscoveryFailure;
                use composition::SelectedCatalogFailure;
                let category = match reason {
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::Unauthorized) => {
                        SpawnFailure::DiscoveryUnauthorized
                    }
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::Forbidden) => {
                        SpawnFailure::DiscoveryForbidden
                    }
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::Http) => {
                        SpawnFailure::DiscoveryHttp
                    }
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::Network) => {
                        SpawnFailure::DiscoveryNetwork
                    }
                    SelectedCatalogFailure::Refresh(
                        DiscoveryFailure::InvalidResponse | DiscoveryFailure::EmptyResponse,
                    ) => SpawnFailure::DiscoveryInvalidResponse,
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::InvalidConfig) => {
                        SpawnFailure::DiscoveryInvalidConfig
                    }
                    SelectedCatalogFailure::Refresh(DiscoveryFailure::Cancelled) => {
                        SpawnFailure::DiscoveryCancelled
                    }
                    SelectedCatalogFailure::Absent => SpawnFailure::SelectedModelAbsent,
                };
                SpawnIssue::new(category, detail)
            }
        })?;
    let mut diagnostics = composition.diagnostics.clone();
    let mut notices = composition.startup_notices.clone();
    let db = match Db::open(data) {
        Ok(db) => {
            trace::log("storage.open", &format!("data_root={} ok", data.display()));
            db
        }
        Err(error) => {
            trace::log(
                "storage.open",
                &format!(
                    "data_root={} fail class={}",
                    data.display(),
                    storage_class(&error)
                ),
            );
            return Err(SpawnIssue::new(
                storage_failure(&error),
                format!("storage: {error}"),
            ));
        }
    };
    db.recover_interrupted_tools()
        .map_err(|e| SpawnIssue::new(SpawnFailure::Recovery, format!("recovery: {e}")))?;
    let (app, inbox, events) = CoreApp::channel(MAX_QUEUE_ITEMS);
    let (ready, ready_rx) = oneshot::channel();
    let handle = tokio::spawn(start_worker(db, composition, inbox, events, ready));
    let guard = WorkerGuard::from_task(handle);
    match ready_rx.await {
        Ok(Ok(worker_diagnostics)) => {
            trace::log(
                "runtime.ready",
                &format!("diagnostics={}", worker_diagnostics.len()),
            );
            if !worker_diagnostics.is_empty() {
                notices.push(StartupNotice::SavedSelection);
            }
            diagnostics.extend(worker_diagnostics);
            Ok((app, guard, diagnostics, notices))
        }
        result => {
            let _ = guard.join().await;
            Err(match result {
                Ok(Err(issue)) => issue,
                _ => SpawnIssue::new(
                    SpawnFailure::Runtime,
                    "application worker closed".to_string(),
                ),
            })
        }
    }
}

/// Effective model/variant/agent selection for the next turn.
#[derive(Clone)]
struct Effective {
    model_id: String,
    variant: Option<String>,
    agent_id: Option<String>,
    agent_prompt: Option<String>,
    agent_digest: Option<String>,
    legacy_epoch: u64,
}

impl Effective {
    fn from_composition(composition: &Composition) -> Self {
        Self {
            model_id: composition.model_id.clone(),
            variant: composition.variant.clone(),
            agent_id: composition.default_agent.clone(),
            agent_prompt: composition.agent_prompt.clone(),
            agent_digest: composition.agent_digest.clone(),
            legacy_epoch: 0,
        }
    }

    /// Apply the persisted frontend model choice; retired ids stay visible
    /// and never silently fall back.
    fn apply_persisted_model(&mut self, db: &Db, composition: &Composition) -> Vec<String> {
        let raw = match db.get_pref(oc_core::queries::PREF_MODEL_SELECTION) {
            Ok(Some(raw)) => raw,
            Ok(None) => return Vec::new(),
            Err(error) => return vec![format!("model selection unreadable: {error}")],
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return vec!["model selection record is malformed".to_string()];
        };
        if value.get("provider").and_then(|v| v.as_str())
            != Some(composition.catalog.provider.as_str())
        {
            return Vec::new();
        }
        let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
            return vec!["model selection record has no id".to_string()];
        };
        let variant = value.get("variant").and_then(|v| v.as_str());
        match crate::models::select_model(&composition.catalog, id)
            .and_then(|base| crate::models::select_variant(&base, variant))
        {
            Ok(selection) => {
                self.model_id = selection.id.clone();
                self.variant = selection.variant.map(|variant| variant.name);
                Vec::new()
            }
            Err(error) => {
                // Preserve the exact retired choice. A stale global preference
                // must not silently authorize the configured fallback either.
                self.model_id = id.to_string();
                self.variant = variant.map(str::to_string);
                vec![format!("selected model {id} is unavailable: {error}")]
            }
        }
    }

    /// Apply the persisted primary agent for this generation.
    fn apply_persisted_agent(
        &mut self,
        db: &Db,
        composition: &Composition,
        registry: &mut WorkspaceRegistry,
    ) -> Vec<String> {
        match registry.load_primary(db) {
            Ok(id) => {
                if let Err(error) = self.set_agent(composition, &id) {
                    return vec![error.to_string()];
                }
                Vec::new()
            }
            Err(WorkspaceError::NoPrimaryAgent) => Vec::new(),
            Err(error) => vec![format!("primary agent: {error}")],
        }
    }

    /// Switch the effective agent; a pinned model must resolve exactly.
    fn set_agent(&mut self, composition: &Composition, id: &str) -> Result<(), CoreError> {
        let agent = composition.agents.get(id).ok_or_else(|| {
            app_error(format!(
                "unknown agent {id}; available: {}",
                composition
                    .agents
                    .values()
                    .filter(|agent| agent.primary_capable())
                    .map(|agent| agent.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        if !agent.primary_capable() {
            return Err(app_error(format!(
                "agent {id} is subagent-only and cannot be a primary agent"
            )));
        }
        if let Some(model) = agent.model.as_deref() {
            // Retain the existing exact bare-ID alias; full profile references
            // share the child/title resolver, including IDs with slashes.
            let (model, variant) = if composition.catalog.models.contains_key(model) {
                (model.to_string(), agent.variant.clone())
            } else {
                let resolved = crate::runtime::resolve_subagent_model(&composition.catalog, model)
                    .map_err(|error| app_error(format!("agent {id}: {error}")))?;
                (resolved.id, agent.variant.clone().or(resolved.variant))
            };
            let selection = crate::models::select_model(&composition.catalog, &model)
                .and_then(|base| crate::models::select_variant(&base, variant.as_deref()))
                .map_err(|error| app_error(format!("agent {id}: {error}")))?;
            self.model_id = selection.id;
            self.variant = selection.variant.map(|v| v.name);
        } else if agent.variant.is_some() {
            let base = crate::models::select_model(&composition.catalog, &self.model_id)
                .map_err(|error| app_error(error.to_string()))?;
            crate::models::select_variant(&base, agent.variant.as_deref())
                .map_err(|error| app_error(format!("agent {id}: {error}")))?;
            self.variant = agent.variant.clone();
        }
        self.agent_id = Some(id.to_string());
        self.agent_prompt = Some(agent.body.clone());
        self.agent_digest = Some(crate::defs::agent_digest(agent));
        Ok(())
    }

    /// Catalog plus this effective selection.
    fn snapshot(&self, composition: &Composition) -> CatalogSnapshot {
        let mut models: Vec<ModelEntry> = composition
            .catalog
            .models
            .iter()
            .map(|(id, spec)| ModelEntry {
                id: id.clone(),
                display_name: spec
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(id)
                    .to_string(),
                provider_name: composition
                    .generation
                    .providers
                    .get(&composition.catalog.provider)
                    .and_then(|p| p.name.clone())
                    .unwrap_or_else(|| composition.catalog.provider.clone()),
                price: (|| {
                    let input = spec.pointer("/cost/input")?;
                    let output = spec.pointer("/cost/output")?;
                    if input.as_f64()? < 0.0 || output.as_f64()? < 0.0 {
                        return None;
                    }
                    Some(oc_core::queries::ModelPrice {
                        input: input.to_string(),
                        output: output.to_string(),
                    })
                })(),
                variants: spec
                    .get("variants")
                    .and_then(|value| value.as_object())
                    .map(|variants| {
                        variants
                            .iter()
                            .map(|(name, value)| VariantEntry {
                                name: name.clone(),
                                disabled: value
                                    .get("disabled")
                                    .and_then(|flag| flag.as_bool())
                                    .unwrap_or(false),
                                reasoning_effort: value
                                    .get("reasoningEffort")
                                    .and_then(|effort| effort.as_str())
                                    .map(str::to_string),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                context: spec
                    .pointer("/limit/context")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                context_known: spec
                    .pointer("/limit/context")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0)
                    .is_some(),
                output: spec
                    .pointer("/limit/output")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                output_known: spec
                    .pointer("/limit/output")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0)
                    .is_some(),
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        let agents = composition
            .agents
            .values()
            .enumerate()
            .filter(|(_, agent)| agent.primary_capable())
            .map(|(color_index, agent)| AgentEntry {
                id: agent.id.clone(),
                description: agent.description.clone(),
                model: agent.model.clone(),
                variant: agent.variant.clone(),
                color_index,
            })
            .collect();
        CatalogSnapshot {
            chrome: composition.tui_chrome.clone(),
            // RuntimePolicy has allow/deny/ask-as-denial, no pending request
            // queue or reply API. That is not upstream's autoaccept mode.
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            provider: composition.catalog.provider.clone(),
            models,
            model_id: self.model_id.clone(),
            variant: self.variant.clone(),
            agents,
            agent_id: self.agent_id.clone(),
            commands: composition.commands.keys().cloned().collect(),
            command_descriptions: composition.command_descriptions.clone(),
        }
    }
}

/// Build one complete runtime for a composition (no publication yet).
fn build_runtime<'a>(db: &'a Db, composition: &Composition) -> Result<Runtime<'a>, String> {
    let files = crate::files::Files::new(&composition.project, db.root())
        .map_err(|e| format!("files: {e}"))?;
    let shell =
        crate::shell::Shell::new(&composition.project).map_err(|e| format!("shell: {e}"))?;
    let runtime = Runtime::new(
        db,
        &composition.project.to_string_lossy(),
        composition.generation.clone(),
        crate::patch::ProtectedGlobs {
            patterns: Vec::new(),
        },
        files,
        shell,
        composition.parent_env.clone(),
        crate::tools::ToolRoots {
            project: composition.project.clone(),
            data: db.root().to_path_buf(),
        },
        None,
        false,
        composition.dcp_config.clone(),
    )
    .map_err(|error| error.to_string())?;
    runtime
        .publish_dcp_protection(composition.dcp_protected.clone())
        .map_err(|error| error.to_string())?;
    runtime
        .publish_subagents(subagent_catalog(composition))
        .map_err(|error| error.to_string())?;
    Ok(runtime)
}

/// Snapshot every admitted profile for the subagent tool; `None` when this
/// generation has no subagent-capable agents.
fn subagent_catalog(composition: &Composition) -> Option<SubagentCatalog> {
    if !composition
        .agents
        .values()
        .any(|agent| agent.subagent_capable())
    {
        return None;
    }
    let agents = composition
        .agents
        .values()
        .map(|agent| {
            (
                agent.id.clone(),
                SubagentAgent {
                    id: agent.id.clone(),
                    description: agent.description.clone(),
                    primary: !agent.subagent_capable(),
                    model: agent.model.clone(),
                    variant: agent.variant.clone(),
                    prompt: agent.body.clone(),
                    permissions: agent.permissions.clone(),
                    permission_rules: agent.permission_rules.clone(),
                    hidden: agent.hidden,
                    digest: Some(crate::defs::agent_digest(agent)),
                },
            )
        })
        .collect();
    Some(SubagentCatalog {
        agents,
        depth_limit: composition.subagent_depth,
    })
}

/// What the command loop returns to the supervisor.
enum WorkerOutcome {
    /// Inbox closed or an explicit shutdown was requested.
    Stop,
    /// The owner asked to switch Location; the supervisor owns the rebuild.
    Switch {
        /// Target project path.
        path: String,
        /// Acceptance after the complete target generation is published.
        ack: SwitchAck,
    },
}

enum SwitchAck {
    Session(oneshot::Sender<Result<LocationSnapshot, CoreError>>),
    Home(oneshot::Sender<Result<HomeLocationSnapshot, CoreError>>),
    Reload(oneshot::Sender<Result<ReloadLocationSnapshot, CoreError>>),
}

/// Own the whole application task: build the runtime, publish the workspace
/// exactly once, then run the command loop and close owned MCP resources.
///
/// A Location switch builds the complete target generation (config, catalog,
/// agents/skills/commands, MCP resources, runtime, and for attached switches
/// the session) before publication; a failure keeps the current Location.
async fn start_worker(
    db: Db,
    mut composition: Composition,
    mut inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
    ready: oneshot::Sender<Result<Vec<String>, SpawnIssue>>,
) -> Result<(), String> {
    let mut runtime = match build_runtime(&db, &composition) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(SpawnIssue::new(SpawnFailure::Runtime, error)));
            return Ok(());
        }
    };
    let mut effective = Effective::from_composition(&composition);
    effective.legacy_epoch = match selection::legacy_epoch(&db, &composition) {
        Ok(epoch) => epoch,
        Err(error) => {
            let _ = ready.send(Err(SpawnIssue::new(
                SpawnFailure::Storage,
                error.to_string(),
            )));
            return Ok(());
        }
    };
    let mut registry = WorkspaceRegistry::bind(
        runtime.generation_id(),
        runtime.location(),
        &composition.generation,
        workspace_agents(&composition),
        skill_metas(&composition),
    );
    let mut sessions: BTreeMap<String, String> = BTreeMap::new();
    let mut home_choices: BTreeMap<String, Effective> = BTreeMap::new();
    let mut diagnostics = effective.apply_persisted_model(&db, &composition);
    diagnostics.extend(effective.apply_persisted_agent(&db, &composition, &mut registry));
    if let Err(error) = publish_workspace(&runtime, &composition, &effective) {
        let _ = ready.send(Err(SpawnIssue::new(
            SpawnFailure::Runtime,
            error.to_string(),
        )));
        return Ok(());
    }
    if ready.send(Ok(diagnostics)).is_err() {
        return Ok(());
    }
    // This worker, not any one Location runtime, owns unresolved remote calls.
    // No endpoint identity or credential leaves the runtime/application boundary.
    let mut remote_retry_quarantined = false;
    // Runtime publication ids restart at 1 after a Location rebuild. A
    // worker-wide epoch distinguishes even a return to the same Location.
    let location_epoch = Arc::new(AtomicU64::new(1));
    let suggestion_queue = Arc::new(Mutex::new(SuggestionQueue::default()));
    loop {
        let outcome = worker(
            &runtime,
            &db,
            &composition,
            &mut effective,
            &mut registry,
            &mut inbox,
            &events,
            &mut sessions,
            &mut home_choices,
            &location_epoch,
            &suggestion_queue,
        )
        .await?;
        match outcome {
            WorkerOutcome::Stop => {
                runtime
                    .shutdown_mcp()
                    .await
                    .map_err(|error| error.to_string())?;
                break;
            }
            WorkerOutcome::Switch { path, ack } => {
                let home = !matches!(ack, SwitchAck::Session(_));
                match switch_target(
                    &db,
                    &path,
                    &mut sessions,
                    composition.parent_env.clone(),
                    home,
                )
                .await
                {
                    Ok((next, next_composition, next_effective, next_registry, session, notes)) => {
                        if matches!(ack, SwitchAck::Reload(_)) {
                            let validation = validate_reload_selections(
                                &db,
                                &runtime,
                                &next,
                                &next_composition,
                                &next_effective,
                                &sessions,
                            );
                            if let Err(error) = validation {
                                let _ = next.shutdown_mcp().await;
                                if let SwitchAck::Reload(ack) = ack {
                                    let _ = ack.send(Err(error));
                                }
                                continue;
                            }
                        }
                        let home_catalog = if matches!(ack, SwitchAck::Home(_)) {
                            let selected = match home_choices.get(next.location()) {
                                Some(selected) => Ok(selected.clone()),
                                None => {
                                    selection::home_current(&db, &next_composition, &next_effective)
                                }
                            };
                            match selected {
                                Ok(selected) => Some(selected.snapshot(&next_composition)),
                                Err(error) => {
                                    let _ = next.shutdown_mcp().await;
                                    if let SwitchAck::Home(ack) = ack {
                                        let _ = ack.send(Err(CoreError::LocationSwitch {
                                            category: LocationSwitchFailure::Storage,
                                            detail: error.to_string(),
                                        }));
                                    }
                                    continue;
                                }
                            }
                        } else {
                            None
                        };
                        // The target generation is complete: only now drop the
                        // old Location's MCP resources and swap the state.
                        runtime
                            .shutdown_mcp()
                            .await
                            .map_err(|error| error.to_string())?;
                        remote_retry_quarantined |= runtime.remote_retry_quarantined();
                        if remote_retry_quarantined {
                            next.quarantine_remote_retries();
                        }
                        runtime = next;
                        location_epoch.fetch_add(1, Ordering::SeqCst);
                        let location = runtime.location().to_string();
                        let mut notices = next_composition.startup_notices.clone();
                        if notes.len() > next_composition.diagnostics.len() {
                            notices.push(StartupNotice::SavedSelection);
                        }
                        composition = next_composition;
                        effective = next_effective;
                        registry = next_registry;
                        match ack {
                            SwitchAck::Session(ack) => {
                                let _ = ack.send(Ok(LocationSnapshot {
                                    location,
                                    generation: location_epoch.load(Ordering::SeqCst),
                                    session: session.expect("attached switch has a session").0,
                                    catalog: effective.snapshot(&composition),
                                    diagnostics: notes,
                                    notices,
                                }));
                            }
                            SwitchAck::Home(ack) => {
                                let _ = ack.send(Ok(HomeLocationSnapshot {
                                    location,
                                    generation: location_epoch.load(Ordering::SeqCst),
                                    catalog: home_catalog.expect("Home switch has a catalog"),
                                    diagnostics: notes,
                                    notices,
                                }));
                            }
                            SwitchAck::Reload(ack) => {
                                // A cached Home draft belongs to the old
                                // config. Recompute it from the new generation
                                // on the next Home query, without altering the
                                // persisted deck or any session selection.
                                home_choices.remove(&location);
                                let _ = ack.send(Ok(ReloadLocationSnapshot {
                                    location,
                                    generation: location_epoch.load(Ordering::SeqCst),
                                    catalog: effective.snapshot(&composition),
                                    diagnostics: notes,
                                    notices,
                                }));
                            }
                        }
                    }
                    Err(issue) => {
                        let category = match issue.category {
                            SpawnFailure::Configuration | SpawnFailure::MissingCredential => {
                                LocationSwitchFailure::Configuration
                            }
                            SpawnFailure::Storage => LocationSwitchFailure::Storage,
                            _ => LocationSwitchFailure::Runtime,
                        };
                        let error = CoreError::LocationSwitch {
                            category,
                            detail: issue.detail,
                        };
                        match ack {
                            SwitchAck::Session(ack) => {
                                let _ = ack.send(Err(error));
                            }
                            SwitchAck::Home(ack) => {
                                let _ = ack.send(Err(error));
                            }
                            SwitchAck::Reload(ack) => {
                                let _ = ack.send(Err(error));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// A reload retains the same Location's tab deck and scoped session choices.
/// Resolve them through the owner's Current path before publishing the new
/// generation; a configured fallback is allowed only where that path really
/// replaces the old choice (for example an unpinned session model).
fn validate_reload_selections(
    db: &Db,
    old: &Runtime<'_>,
    next: &Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
    sessions: &BTreeMap<String, String>,
) -> Result<(), CoreError> {
    let storage_error = || CoreError::LocationSwitch {
        category: LocationSwitchFailure::Storage,
        detail: "retained tab deck unavailable".into(),
    };
    let selection_error = || CoreError::LocationSwitch {
        category: LocationSwitchFailure::Configuration,
        detail: "retained session selection unavailable in reloaded configuration".into(),
    };
    let old_deck = tab_deck::load(db, old).map_err(|_| storage_error())?;
    let next_deck = tab_deck::load(db, next).map_err(|_| storage_error())?;
    // Projection must not make an existing saved root disappear on reload.
    if old_deck.sessions != next_deck.sessions || old_deck.active != next_deck.active {
        return Err(storage_error());
    }
    let mut retained = old_deck.sessions;
    if let Some(id) = sessions.get(old.location())
        && !retained.iter().any(|session| &session.0 == id)
    {
        retained.push(SessionId(id.clone()));
    }
    for session in retained {
        next.open_session(&session.0).map_err(|_| storage_error())?;
        let selected = selection::apply(
            db,
            composition,
            effective,
            &session.0,
            false,
            oc_core::queries::SessionSelectionAction::Current,
        )
        .map_err(|_| selection_error())?;
        let turn = selection::for_turn(db, composition, effective, &session.0)
            .map_err(|_| selection_error())?;
        for selected in [&selected, &turn] {
            crate::models::select_model(&composition.catalog, &selected.model_id)
                .and_then(|base| crate::models::select_variant(&base, selected.variant.as_deref()))
                .map_err(|_| selection_error())?;
        }
    }
    Ok(())
}

/// Build the complete target generation for a Location switch.
///
/// Nothing is published and no old state is dropped here: the caller swaps
/// only after this returns successfully.
#[allow(clippy::type_complexity)]
async fn switch_target<'a>(
    db: &'a Db,
    path: &str,
    sessions: &mut BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    home: bool,
) -> Result<
    (
        Runtime<'a>,
        Composition,
        Effective,
        WorkspaceRegistry,
        Option<SessionId>,
        Vec<String>,
    ),
    SpawnIssue,
> {
    let composition = composition::load_with_env(Path::new(path), env)
        .await
        .map_err(|detail| SpawnIssue::new(SpawnFailure::Configuration, detail))?;
    let runtime = build_runtime(db, &composition)
        .map_err(|detail| SpawnIssue::new(SpawnFailure::Runtime, detail))?;
    let mut effective = Effective::from_composition(&composition);
    effective.legacy_epoch = selection::legacy_epoch(db, &composition)
        .map_err(|e| SpawnIssue::new(SpawnFailure::Storage, e.to_string()))?;
    let mut registry = WorkspaceRegistry::bind(
        runtime.generation_id(),
        runtime.location(),
        &composition.generation,
        workspace_agents(&composition),
        skill_metas(&composition),
    );
    let mut notes = composition.diagnostics.clone();
    notes.extend(effective.apply_persisted_model(db, &composition));
    notes.extend(effective.apply_persisted_agent(db, &composition, &mut registry));
    publish_workspace(&runtime, &composition, &effective)
        .map_err(|error| SpawnIssue::new(SpawnFailure::Runtime, error.to_string()))?;
    // Home publishes only the target generation. Attached switches retain
    // their existing Location-bound reopen/create behavior.
    let location = runtime.location().to_string();
    let session = if home {
        None
    } else {
        Some(match sessions.get(&location).cloned() {
            Some(id) => {
                runtime
                    .open_session(&id)
                    .map_err(|error| SpawnIssue::new(SpawnFailure::Storage, error.to_string()))?;
                SessionId(id)
            }
            None => {
                let id = format!("s-loc-{}", nanos());
                runtime
                    .create_session(&id)
                    .map_err(|error| SpawnIssue::new(SpawnFailure::Storage, error.to_string()))?;
                sessions.insert(location, id.clone());
                SessionId(id)
            }
        })
    };
    Ok((runtime, composition, effective, registry, session, notes))
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// Publish the effective agent's prompt and policy with instructions and skills.
fn publish_workspace(
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
) -> Result<(), RuntimeError> {
    let agent = effective
        .agent_id
        .as_ref()
        .and_then(|id| composition.agents.get(id));
    runtime.publish_workspace(
        effective.agent_prompt.as_deref(),
        &composition.instructions,
        composition.skills.clone(),
        composition.skill_errors.clone(),
        effective.agent_digest.clone(),
        effective.agent_id.clone(),
        effective
            .agent_id
            .as_ref()
            .and_then(|id| composition.agents.values().position(|a| &a.id == id)),
        agent
            .map(|agent| agent.permissions.clone())
            .unwrap_or_default(),
        agent
            .map(|agent| agent.permission_rules.clone())
            .unwrap_or_default(),
    )
}

fn workspace_agents(composition: &Composition) -> Vec<WorkspaceAgent> {
    composition
        .agents
        .values()
        .filter(|agent| agent.primary_capable())
        .map(|agent| WorkspaceAgent {
            id: agent.id.clone(),
            description: agent.description.clone(),
            model: agent.model.clone().unwrap_or_default(),
            variant: agent.variant.clone(),
        })
        .collect()
}

fn skill_metas(composition: &Composition) -> Vec<crate::config::SkillMeta> {
    composition
        .skills
        .iter()
        .filter_map(|(id, body)| crate::config::parse_skill(id, body).ok())
        .collect()
}

fn skill_cards(composition: &Composition) -> Vec<SkillCard> {
    skill_metas(composition)
        .into_iter()
        .map(|meta| SkillCard {
            id: meta.id,
            name: meta.name,
            description: meta.description,
        })
        .collect()
}

fn app_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Application(error.to_string())
}

fn file_suggestion_error(error: crate::files::FileToolError) -> CoreError {
    // InvalidPattern may carry arbitrary text in other Files operations; no
    // underlying filesystem/configuration details cross the frontend boundary.
    use crate::files::FileToolError;
    let category = match error {
        FileToolError::InvalidPattern(_) => "invalid file suggestion query",
        FileToolError::Io => "file suggestions unavailable",
        FileToolError::SymlinkEscape => "file suggestion root unavailable",
        FileToolError::OutsideRoot | FileToolError::OwnDataRoot => "file suggestions refused",
        FileToolError::BudgetExhausted => "file suggestion budget exhausted",
        FileToolError::NotFound | FileToolError::Binary => "file suggestions unavailable",
    };
    app_error(category)
}

fn file_suggestion_reply(
    epoch: &AtomicU64,
    generation: u64,
    location: String,
    result: Result<crate::files::FileSuggestionResult, CoreError>,
) -> Result<FileSuggestionsSnapshot, CoreError> {
    if epoch.load(Ordering::SeqCst) != generation {
        return Err(app_error("file suggestions belong to previous Location"));
    }
    result.map(|result| FileSuggestionsSnapshot {
        location,
        generation,
        paths: result.paths,
        truncated: result.truncated,
    })
}

type SuggestionWork = Box<
    dyn FnOnce() -> Result<crate::files::FileSuggestionResult, crate::files::FileToolError> + Send,
>;

struct SuggestionRequest {
    work: SuggestionWork,
    location: String,
    generation: u64,
    ack: oneshot::Sender<Result<FileSuggestionsSnapshot, CoreError>>,
}

#[derive(Default)]
struct SuggestionQueue {
    running: bool,
    pending: Option<SuggestionRequest>,
}

fn enqueue_file_suggestion(
    queue: &Arc<Mutex<SuggestionQueue>>,
    epoch: &Arc<AtomicU64>,
    request: SuggestionRequest,
) {
    if request.ack.is_closed() {
        return;
    }
    let (previous, start) = {
        let mut queue = queue.lock().expect("file suggestion queue poisoned");
        let previous = queue.pending.replace(request);
        let start = !queue.running;
        queue.running = true;
        (previous, start)
    };
    if let Some(previous) = previous {
        let _ = previous
            .ack
            .send(Err(app_error("file suggestions superseded")));
    }
    if start {
        let queue = Arc::clone(queue);
        let epoch = Arc::clone(epoch);
        tokio::spawn(async move {
            loop {
                let Some(request) = ({
                    let mut queue = queue.lock().expect("file suggestion queue poisoned");
                    match queue.pending.take() {
                        Some(request) => Some(request),
                        None => {
                            queue.running = false;
                            None
                        }
                    }
                }) else {
                    break;
                };
                // An aborted UI task closes ack. Skip it (and old Location
                // requests) before occupying a blocking thread. Once started,
                // always join the bounded walk before taking the next request.
                if request.ack.is_closed() {
                    continue;
                }
                if epoch.load(Ordering::SeqCst) != request.generation {
                    let _ = request.ack.send(Err(app_error(
                        "file suggestions belong to previous Location",
                    )));
                    continue;
                }
                let result = tokio::task::spawn_blocking(request.work)
                    .await
                    .map_err(|_| app_error("file suggestions unavailable"))
                    .and_then(|result| result.map_err(file_suggestion_error));
                if !request.ack.is_closed() {
                    let result =
                        file_suggestion_reply(&epoch, request.generation, request.location, result);
                    let _ = request.ack.send(result);
                }
            }
        });
    }
}

/// Prompt for a manual `/dcp-compress` request: the model drives the
/// compress tool over the closed span, exactly like an automatic nudge.
fn compress_prompt(focus: &str) -> String {
    let focus = focus.trim();
    let focus = if focus.len() > COMPRESS_FOCUS_MAX {
        &focus[..focus.floor_char_boundary(COMPRESS_FOCUS_MAX)]
    } else {
        focus
    };
    let focus = if focus.is_empty() {
        "no extra focus: compress the earliest closed span".to_string()
    } else {
        format!("focus: {focus}")
    };
    format!(
        "Manual context compression request. Call the compress tool to replace the \
         earliest closed span of this conversation with a durable summary that keeps \
         the facts needed to continue, then reply with the stored block ids. {focus}."
    )
}

/// Handle one owner-only query or action. Never runs while a turn streams
/// except for read-only snapshots.
#[allow(clippy::too_many_arguments)]
fn query(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    sessions: &mut BTreeMap<String, String>,
    home_choices: &mut BTreeMap<String, Effective>,
    location_epoch: &Arc<AtomicU64>,
    suggestion_queue: &Arc<Mutex<SuggestionQueue>>,
    message: InboxMsg,
) {
    match message {
        InboxMsg::Create { id, ack } => {
            let result = runtime.create_session(&id.0).map_err(app_error);
            if result.is_ok() {
                sessions.insert(runtime.location().to_string(), id.0.clone());
            }
            let _ = ack.send(result);
        }
        InboxMsg::RenameSession {
            session,
            title,
            ack,
        } => {
            let result = (|| -> Result<(), CoreError> {
                let title = normalized_session_title(&title)
                    .ok_or_else(|| app_error("invalid session title"))?;
                runtime
                    .open_session(&session.0)
                    .map_err(|error| match error {
                        RuntimeError::SessionNotFound | RuntimeError::LocationMismatch { .. } => {
                            CoreError::SessionNotFound
                        }
                        _ => app_error("session storage unavailable"),
                    })?;
                let meta = db.session_meta(&session.0).map_err(|error| match error {
                    StorageError::SessionNotFound => CoreError::SessionNotFound,
                    _ => app_error("session storage unavailable"),
                })?;
                if meta.parent_id.is_some() {
                    return Err(CoreError::SessionNotFound);
                }
                db.rename_root_session(&session.0, title)
                    .map_err(|error| match error {
                        StorageError::SessionNotFound => CoreError::SessionNotFound,
                        _ => app_error("session storage unavailable"),
                    })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::RegenerateTitle { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::CancelTitle { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::List { ack } => {
            let _ = ack.send(
                db.list_sessions()
                    .map(|ids| ids.into_iter().map(SessionId).collect())
                    .map_err(app_error),
            );
        }
        InboxMsg::ProbeSession { id, ack } => {
            // A missing binding alone does not prove absence: an unbound row
            // must never be claimed by a new Location-bound root. A foreign
            // binding is likewise a refusal, even when its row is missing.
            let result = match runtime.open_session(&id.0) {
                Ok(()) => db
                    .session_meta(&id.0)
                    .map(|meta| {
                        if meta.parent_id.is_some() {
                            SessionProbe::Child
                        } else {
                            SessionProbe::Root
                        }
                    })
                    .map_err(app_error),
                Err(RuntimeError::SessionNotFound) => match db.session_meta(&id.0) {
                    Err(StorageError::SessionNotFound) => Ok(SessionProbe::Absent),
                    Ok(_) => Err(app_error("session has no Location binding")),
                    Err(error) => Err(app_error(error)),
                },
                Err(RuntimeError::LocationMismatch { .. }) => {
                    Err(app_error("session belongs to another Location"))
                }
                Err(error) => Err(app_error(error)),
            };
            let _ = ack.send(result);
        }
        InboxMsg::TabDeck { ack } => {
            let _ = ack.send(tab_deck::load(db, runtime));
        }
        InboxMsg::SaveTabDeck { deck, ack } => {
            let _ = ack.send(tab_deck::save(db, runtime, &deck));
        }
        InboxMsg::Read { session, ack } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(app_error)
                .and_then(|()| {
                    db.read_history_full(&session.0)
                        .map_err(app_error)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|(id, role, text)| oc_core::session::Message {
                                    id: MessageId(id),
                                    role: if role == "user" {
                                        Role::User
                                    } else {
                                        Role::Assistant
                                    },
                                    text,
                                })
                                .collect()
                        })
                });
            let _ = ack.send(result);
        }
        InboxMsg::History {
            session,
            before_seq,
            after_seq,
            limit,
            ack,
        } => {
            let result = (|| -> Result<HistoryPage, CoreError> {
                runtime.open_session(&session.0).map_err(app_error)?;
                let (min, max) = db.history_bounds(&session.0).map_err(app_error)?;
                let total = db.history_len(&session.0).map_err(app_error)?;
                let limit = limit.min(HISTORY_PAGE_LIMIT);
                let (mut page, ascending) = match after_seq {
                    Some(after) => (
                        db.read_history_after(&session.0, limit, after)
                            .map_err(app_error)?,
                        true,
                    ),
                    None => (
                        db.read_history_page(&session.0, limit, before_seq)
                            .map_err(app_error)?,
                        false,
                    ),
                };
                let has_newer = if ascending {
                    matches!((page.last(), max), (Some((seq, ..)), Some(max)) if *seq < max)
                } else {
                    matches!((page.first(), max), (Some((seq, ..)), Some(max)) if *seq < max)
                };
                let has_older = if ascending {
                    matches!((page.first(), min), (Some((seq, ..)), Some(min)) if *seq > min)
                } else {
                    matches!((page.last(), min), (Some((seq, ..)), Some(min)) if *seq > min)
                };
                if !ascending {
                    page.reverse();
                }
                let rows = page
                    .into_iter()
                    .map(|(seq, role, text)| {
                        Ok(HistoryMessage {
                            turn: db
                                .history_turn(&session.0, seq)
                                .map_err(app_error)?
                                .or_else(|| {
                                    (role == "assistant").then(|| oc_core::queries::HistoryTurn {
                                        legacy_text_only: true,
                                        ..Default::default()
                                    })
                                }),
                            seq,
                            role: if role == "user" {
                                Role::User
                            } else {
                                Role::Assistant
                            },
                            text,
                        })
                    })
                    .collect::<Result<Vec<_>, CoreError>>()?;
                Ok(HistoryPage {
                    parent_id: db.session_meta(&session.0).map_err(app_error)?.parent_id,
                    title: db.session_meta(&session.0).map_err(app_error)?.title,
                    rows,
                    total,
                    has_older,
                    has_newer,
                })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::ToolOps {
            session,
            before_rowid,
            limit,
            ack,
        } => {
            let result = (|| -> Result<ToolOpPage, CoreError> {
                runtime.open_session(&session.0).map_err(app_error)?;
                let total = db.tool_ops_len(&session.0).map_err(app_error)?;
                let page = db
                    .list_tool_ops_page(&session.0, limit.min(TOOL_OPS_PAGE_LIMIT), before_rowid)
                    .map_err(app_error)?;
                let has_older = match (
                    page.last(),
                    db.tool_ops_bounds(&session.0).map_err(app_error)?.0,
                ) {
                    (Some(row), Some(min)) => row.rowid > min,
                    _ => false,
                };
                let rows = page
                    .into_iter()
                    .map(|row| ToolOpView {
                        op: row.op,
                        rowid: row.rowid,
                        name: row.name,
                        state: row.state,
                        input: row.input,
                        output: row.output,
                        output_bytes: row.output_bytes,
                        output_truncated: row.output_truncated,
                    })
                    .collect();
                Ok(ToolOpPage {
                    rows,
                    total,
                    has_older,
                })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::ToolOutput {
            session,
            op,
            offset,
            limit,
            ack,
        } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(app_error)
                .and_then(|_| {
                    db.read_session_tool_output(&session.0, &op, offset, limit)
                        .map(
                            |(text, total_bytes, next_offset)| oc_core::queries::ToolOutputPage {
                                text,
                                total_bytes,
                                next_offset,
                            },
                        )
                        .map_err(app_error)
                });
            let _ = ack.send(result);
        }
        InboxMsg::Catalog { ack } => {
            let _ = ack.send(Ok(effective.snapshot(composition)));
        }
        InboxMsg::SessionSelection {
            session,
            home,
            action,
            ack,
        } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                runtime.open_session(&session.0).map_err(app_error)?;
                selection::apply(db, composition, effective, &session.0, home, action)
                    .map(|selected| selected.snapshot(composition))
            })();
            let _ = ack.send(result);
        }
        InboxMsg::HomeSelection { action, ack } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                let location = runtime.location();
                let current = match home_choices.get(location) {
                    Some(selected) => selected.clone(),
                    None => selection::home_current(db, composition, effective)?,
                };
                let explicit = action != oc_core::queries::SessionSelectionAction::Current;
                let selected = selection::home(db, composition, effective, &current, action)?;
                let snapshot = selected.snapshot(composition);
                if explicit {
                    home_choices.insert(location.to_string(), selected);
                }
                Ok(snapshot)
            })();
            let _ = ack.send(result);
        }
        InboxMsg::Skills { ack } => {
            let _ = ack.send(Ok(skill_cards(composition)));
        }
        InboxMsg::FileSuggestions { query, limit, ack } => {
            if ack.is_closed() {
                return;
            }
            let (files, location) = runtime.file_suggestion_source();
            let generation = location_epoch.load(Ordering::SeqCst);
            enqueue_file_suggestion(
                suggestion_queue,
                location_epoch,
                SuggestionRequest {
                    work: Box::new(move || files.suggest(&query, limit)),
                    location,
                    generation,
                    ack,
                },
            );
        }
        InboxMsg::SelectModel { id, variant, ack } => {
            let result = (|| -> Result<CatalogSnapshot, CoreError> {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                let base = crate::models::select_model(&composition.catalog, &id)
                    .map_err(|error| app_error(error.to_string()))?;
                let selection = crate::models::select_variant(&base, variant.as_deref())
                    .map_err(|error| app_error(error.to_string()))?;
                let record = serde_json::json!({
                    "provider": composition.catalog.provider,
                    "id": selection.id,
                    "variant": selection.variant.as_ref().map(|variant| variant.name.clone()),
                });
                let (epoch_key, epoch_value) = selection::next_legacy_epoch(db, composition)?;
                db.set_prefs(&[
                    (
                        oc_core::queries::PREF_MODEL_SELECTION.into(),
                        record.to_string(),
                    ),
                    (epoch_key, epoch_value),
                ])
                .map_err(app_error)?;
                effective.legacy_epoch += 1;
                effective.model_id = selection.id.clone();
                effective.variant = selection.variant.map(|variant| variant.name);
                Ok(effective.snapshot(composition))
            })();
            let _ = ack.send(result);
        }
        InboxMsg::SelectAgent { id, ack } => {
            let result = (|| -> Result<CatalogSnapshot, CoreError> {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                registry
                    .select_primary(&id, runtime.generation_id(), db)
                    .map_err(|error| app_error(error.to_string()))?;
                effective.set_agent(composition, &id)?;
                let (epoch_key, epoch_value) = selection::next_legacy_epoch(db, composition)?;
                db.set_pref(&epoch_key, &epoch_value).map_err(app_error)?;
                effective.legacy_epoch += 1;
                publish_workspace(runtime, composition, effective).map_err(app_error)?;
                Ok(effective.snapshot(composition))
            })();
            let _ = ack.send(result);
        }
        InboxMsg::SwitchLocation { ack, .. } => {
            // A switch never races the active turn: it is refused while the
            // worker is streaming and can be retried afterwards.
            let _ = ack.send(Err(app_error("turn active; location switch refused")));
        }
        InboxMsg::SwitchLocationHome { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::ReloadLocation { ack } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::Dcp { session, ack } => {
            let result = (|| -> Result<DcpSnapshot, CoreError> {
                runtime.open_session(&session.0).map_err(app_error)?;
                let stats = runtime.dcp_stats();
                let blocks = crate::dcp::load_blocks(db, &session.0)
                    .map_err(|error| app_error(error.to_string()))?
                    .len();
                let turns_since_compress = runtime
                    .dcp_turn_state(&session.0)
                    .map(|state| state.turns_since_compress)
                    .unwrap_or(0);
                // Estimate from the active projection, not the archive: the
                // DCP panel must not materialise pruned/covered history.
                let after_seq = db
                    .prune_bound(&session.0)
                    .map_err(app_error)?
                    .map(|(_, seq)| seq)
                    .unwrap_or(0);
                let active = db
                    .active_history(
                        &session.0,
                        after_seq,
                        crate::runtime::ACTIVE_CONTEXT_BYTES_CAP,
                    )
                    .map_err(app_error)?;
                let estimated_tokens = if active.overflow {
                    // Above the safety budget: report the exact byte-derived
                    // estimate instead of a silent zero.
                    active.bytes / 4
                } else {
                    let mut text = String::new();
                    for (_, _, body) in &active.rows {
                        if text.len() >= ESTIMATE_BYTES {
                            break;
                        }
                        text.push_str(body);
                    }
                    crate::runtime::estimate_tokens(&text)
                };
                let selected = selection::for_turn(db, composition, effective, &session.0)?;
                let model_context = composition
                    .catalog
                    .models
                    .get(&selected.model_id)
                    .and_then(|spec| spec.pointer("/limit/context"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let thresholds = composition
                    .dcp_config
                    .effective_for_context(&selected.model_id, model_context);
                Ok(DcpSnapshot {
                    estimated_tokens,
                    max_context: thresholds.max_context,
                    turns_since_compress,
                    blocks,
                    compressions: stats.compressions,
                    nudges: stats.nudges_emitted,
                    prunes: stats.prunes,
                })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::Cancel { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnNotActive));
        }
        InboxMsg::Submit { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::SubmitFresh { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::Compress { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::Shutdown => {}
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker(
    runtime: &Runtime<'_>,
    db: &Db,
    composition: &Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    inbox: &mut mpsc::Receiver<InboxMsg>,
    events: &broadcast::Sender<CoreEvent>,
    sessions: &mut BTreeMap<String, String>,
    home_choices: &mut BTreeMap<String, Effective>,
    location_epoch: &Arc<AtomicU64>,
    suggestion_queue: &Arc<Mutex<SuggestionQueue>>,
) -> Result<WorkerOutcome, String> {
    while let Some(message) = inbox.recv().await {
        // A manual compress request is a real turn: the model drives the
        // compress tool exactly like an automatic nudge.
        let message = match message {
            InboxMsg::Compress {
                session,
                focus,
                ack,
            } => InboxMsg::Submit {
                session,
                text: compress_prompt(&focus),
                ack,
            },
            other => other,
        };
        match message {
            InboxMsg::Shutdown => return Ok(WorkerOutcome::Stop),
            InboxMsg::RegenerateTitle { session, ack } => {
                let prepared = (|| -> Result<_, CoreError> {
                    runtime
                        .open_session(&session.0)
                        .map_err(|_| CoreError::SessionNotFound)?;
                    let meta = db
                        .session_meta(&session.0)
                        .map_err(|_| CoreError::SessionNotFound)?;
                    if meta.parent_id.is_some() {
                        return Err(CoreError::SessionNotFound);
                    }
                    let (expected_title, expected_event) = db
                        .root_title_stamp(&session.0)
                        .map_err(app_error)?
                        .ok_or(CoreError::SessionNotFound)?;
                    let text = db
                        .title_context(&session.0, expected_title.is_some())
                        .map_err(app_error)?
                        .ok_or_else(|| app_error("no user request to title"))?;
                    let selected = selection::for_turn(db, composition, effective, &session.0)?;
                    // A retired primary selection is not permission to call an
                    // unrelated fallback model, even if the title agent pins one.
                    crate::models::select_model(&composition.catalog, &selected.model_id)
                        .and_then(|base| {
                            crate::models::select_variant(&base, selected.variant.as_deref())
                        })
                        .map_err(|_| app_error("selected model/variant unavailable"))?;
                    let agent = composition.agents.get("title");
                    let (id, variant) = if let Some(raw) = agent.and_then(|a| a.model.as_deref()) {
                        let resolved =
                            crate::runtime::resolve_subagent_model(&composition.catalog, raw)
                                .map_err(|_| app_error("title agent model unavailable"))?;
                        (
                            resolved.id,
                            agent.and_then(|a| a.variant.clone()).or(resolved.variant),
                        )
                    } else {
                        (
                            selected.model_id,
                            agent.and_then(|a| a.variant.clone()).or(selected.variant),
                        )
                    };
                    let selection = crate::models::select_model(&composition.catalog, &id)
                        .and_then(|base| crate::models::select_variant(&base, variant.as_deref()))
                        .map_err(|_| app_error("title agent model/variant unavailable"))?;
                    let fallback = composition
                        .generation
                        .providers
                        .get(&composition.catalog.provider)
                        .map(|p| p.options.native_fallback_limits)
                        .unwrap_or_default();
                    let budget = crate::models::budget(&selection, 256, fallback);
                    let input = vec![
                        crate::provider::InputItem::message(crate::provider::InputRole::Developer,
                            agent.map(|a| a.body.as_str()).unwrap_or("Generate a short session title from the user's request. Output only the title, in at most 100 characters.")),
                        crate::provider::InputItem::message(crate::provider::InputRole::User, &text),
                    ];
                    let tools: [crate::provider::ToolDef; 0] = [];
                    let tokens = crate::runtime::estimate_tokens(
                        &serde_json::to_string(&(&input, &tools)).map_err(app_error)?,
                    );
                    crate::models::admit_budget(&selection, tokens, &budget)
                        .map_err(|_| app_error("title request exceeds model budget"))?;
                    Ok((
                        expected_title,
                        expected_event,
                        selection,
                        input,
                        budget.output,
                    ))
                })();
                let (expected, expected_event, selection, input, output) = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        let _ = ack.send(Err(error));
                        continue;
                    }
                };
                let cancel = AtomicBool::new(false);
                let operation = async {
                    let generation = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        crate::provider::stream_input_observed(
                            &composition.provider,
                            &selection.id,
                            selection.variant.as_ref(),
                            &input,
                            &[],
                            output,
                            &cancel,
                            &mut |_| {},
                        ),
                    )
                    .await
                    .map_err(|_| app_error("title request timed out"))?
                    .map_err(|_| app_error("title request failed"))?;
                    if cancel.load(Ordering::Relaxed) {
                        return Err(app_error("title request cancelled"));
                    }
                    let canonical = generation
                        .output
                        .iter()
                        .filter(|item| item["type"] == "message" && item["role"] == "assistant")
                        .filter_map(|item| item.get("content").and_then(|v| v.as_array()))
                        .flatten()
                        .filter(|part| part["type"] == "output_text")
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    let text = if generation.text.is_empty() {
                        &canonical
                    } else {
                        &generation.text
                    };
                    let title: String = text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or_default()
                        .trim()
                        .trim_matches('"')
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(100)
                        .collect();
                    let title = normalized_session_title(&title)
                        .ok_or_else(|| app_error("title agent returned an invalid title"))?;
                    if !db
                        .compare_and_set_root_title(
                            &session.0,
                            expected.as_deref(),
                            expected_event,
                            title,
                        )
                        .map_err(|_| app_error("title storage unavailable"))?
                    {
                        return Err(app_error("session title changed during generation"));
                    }
                    Ok(title.to_string())
                };
                tokio::pin!(operation);
                let mut shutdown = false;
                let result = loop {
                    tokio::select! {
                        result = &mut operation => break result,
                        command = inbox.recv(), if !shutdown => match command {
                            None | Some(InboxMsg::Shutdown) => {
                                shutdown = true;
                                cancel.store(true, Ordering::Relaxed);
                            }
                            Some(InboxMsg::CancelTitle { session: target, ack }) if target == session => {
                                cancel.store(true, Ordering::Relaxed);
                                let _ = ack.send(Ok(()));
                            }
                            Some(command) => query(db, runtime, composition, effective, registry, sessions, home_choices, location_epoch, suggestion_queue, command),
                        },
                    }
                };
                let _ = ack.send(result);
                if shutdown {
                    return Ok(WorkerOutcome::Stop);
                }
            }
            InboxMsg::SwitchLocation { path, ack } => {
                return Ok(WorkerOutcome::Switch {
                    path,
                    ack: SwitchAck::Session(ack),
                });
            }
            InboxMsg::SwitchLocationHome { path, ack } => {
                return Ok(WorkerOutcome::Switch {
                    path,
                    ack: SwitchAck::Home(ack),
                });
            }
            InboxMsg::ReloadLocation { ack } => {
                return Ok(WorkerOutcome::Switch {
                    path: runtime.location().to_string(),
                    ack: SwitchAck::Reload(ack),
                });
            }
            message @ (InboxMsg::Submit { .. } | InboxMsg::SubmitFresh { .. }) => {
                let (session, text, fresh, ack) = match message {
                    InboxMsg::Submit { session, text, ack } => (session, text, None, ack),
                    InboxMsg::SubmitFresh {
                        session,
                        text,
                        selection,
                        ack,
                    } => (session, text, Some(selection), ack),
                    _ => unreachable!(),
                };
                if text.trim().is_empty() {
                    let _ = ack.send(Err(app_error("empty prompt")));
                    continue;
                }
                let (prompt, invocation) = match resolve_submission(composition, text) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        let _ = ack.send(Err(app_error(error)));
                        continue;
                    }
                };
                let cancel = AtomicBool::new(false);
                let is_fresh = fresh.is_some();
                // Resolve from the owning session, never whichever TUI tab was
                // most recently viewed. Title and inherited child requests use
                // this same selection and agent workspace.
                let (turn_selection, initial_selection) = match (|| -> Result<_, CoreError> {
                    if is_fresh {
                        match db.session_meta(&session.0) {
                            Ok(_) => return Err(CoreError::SessionAlreadyExists),
                            Err(StorageError::SessionNotFound) => {}
                            Err(error) => return Err(app_error(error)),
                        }
                    }
                    let (selected, initial_selection) = match fresh {
                        Some(Some(choice)) => {
                            let (selected, record) =
                                selection::fresh(composition, effective, &session.0, choice)?;
                            (selected, Some(record))
                        }
                        Some(None) => {
                            let home = match home_choices.get(runtime.location()) {
                                Some(selected) => selected.clone(),
                                None => selection::home_current(db, composition, effective)?,
                            };
                            let choice = oc_core::core_app::FreshSelection {
                                agent_id: home.agent_id.clone(),
                                model_id: home.model_id.clone(),
                                variant: home.variant.clone(),
                            };
                            let (selected, record) =
                                selection::fresh(composition, effective, &session.0, choice)?;
                            (selected, Some(record))
                        }
                        None => (
                            selection::for_turn(db, composition, effective, &session.0)?,
                            None,
                        ),
                    };
                    crate::models::select_model(&composition.catalog, &selected.model_id)
                                .and_then(|base| crate::models::select_variant(&base, selected.variant.as_deref()))
                                .map_err(|e| app_error(format!("selected model/variant unavailable; select an admitted replacement or Default: {e}")))?;
                    publish_workspace(runtime, composition, &selected).map_err(app_error)?;
                    Ok((selected, initial_selection))
                })() {
                    Ok(selected) => selected,
                    Err(error) => {
                        let _ = ack.send(Err(error));
                        continue;
                    }
                };
                // Resolve before accepting a turn: an invalid configured title
                // profile is a configuration error, never a silent fallback.
                let title_agent = composition.agents.get("title");
                let title_selection = (|| -> Result<_, String> {
                    let (id, variant) =
                        if let Some(raw) = title_agent.and_then(|a| a.model.as_deref()) {
                            let resolved =
                                crate::runtime::resolve_subagent_model(&composition.catalog, raw)?;
                            (
                                resolved.id,
                                title_agent
                                    .and_then(|a| a.variant.clone())
                                    .or(resolved.variant),
                            )
                        } else {
                            (
                                turn_selection.model_id.clone(),
                                title_agent
                                    .and_then(|a| a.variant.clone())
                                    .or(turn_selection.variant.clone()),
                            )
                        };
                    crate::models::select_model(&composition.catalog, &id)
                        .and_then(|base| crate::models::select_variant(&base, variant.as_deref()))
                        .map_err(|e| e.to_string())
                })();
                let title_selection = match title_selection {
                    Ok(selection) => selection,
                    Err(error) => {
                        let _ = ack.send(Err(app_error(format!("title agent: {error}"))));
                        continue;
                    }
                };
                let params = TurnParams {
                    session: session.0.clone(),
                    prompt,
                    invocation,
                    catalog: &composition.catalog,
                    model_id: turn_selection.model_id.clone(),
                    variant: turn_selection.variant.clone(),
                    // The runtime resolves its native default against known
                    // metadata and fallback caps; capacity is not a request.
                    max_output: 0,
                    provider: composition.provider.clone(),
                    cancel: &cancel,
                    max_rounds: crate::runtime::MAX_ROUNDS,
                };
                let title_prompt = params.prompt.clone();
                let mut ack = Some(ack);
                let mut turn = None;
                let mut shutdown = false;
                let result;
                {
                    let operation = async {
                        let on_accept = |id: &str| {
                            let id = WorkerTurnId(id.to_string());
                            turn = Some(id.clone());
                            if let Some(ack) = ack.take() {
                                let _ = ack.send(Ok(id.clone()));
                            }
                            let _ = events.send(CoreEvent::TurnStarted {
                                session: session.clone(),
                                turn: id,
                            });
                        };
                        let on_text = |id: &str, delta: &str| {
                            let _ = events.send(CoreEvent::TextDelta {
                                session: session.clone(),
                                turn: WorkerTurnId(id.to_string()),
                                delta: delta.to_string(),
                            });
                        };
                        let on_reasoning = |id: &str, delta: &str| {
                            let _ = events.send(CoreEvent::ReasoningDelta {
                                session: session.clone(),
                                turn: WorkerTurnId(id.to_string()),
                                delta: delta.to_string(),
                            });
                        };
                        let on_tool = |id: &str, event: &ToolCallEvent| {
                            let turn = WorkerTurnId(id.to_string());
                            let _ = events.send(match event {
                                ToolCallEvent::Started { op, name, input } => {
                                    CoreEvent::ToolCallStarted {
                                        session: session.clone(),
                                        turn,
                                        op: op.clone(),
                                        name: name.clone(),
                                        input: input.clone(),
                                    }
                                }
                                ToolCallEvent::Finished {
                                    op,
                                    name,
                                    state,
                                    output,
                                    output_bytes,
                                    output_truncated,
                                } => CoreEvent::ToolCallFinished {
                                    session: session.clone(),
                                    turn,
                                    op: op.clone(),
                                    name: name.clone(),
                                    state: state.clone(),
                                    output: output.clone(),
                                    output_bytes: *output_bytes,
                                    output_truncated: *output_truncated,
                                },
                            });
                            if let Ok(Some(projection)) = db.turn_presentation(&session.0, id) {
                                let _ = events.send(CoreEvent::TurnPresentation {
                                    session: session.clone(),
                                    turn: WorkerTurnId(id.to_string()),
                                    projection,
                                });
                            }
                        };
                        let mut report = if is_fresh {
                            runtime
                                .run_fresh_turn_with_tool_events(
                                    params,
                                    initial_selection
                                        .as_ref()
                                        .map(|(key, value)| (key.as_str(), value.as_str())),
                                    on_accept,
                                    on_text,
                                    on_reasoning,
                                    on_tool,
                                )
                                .await?
                        } else {
                            runtime
                                .run_turn_with_tool_events(
                                    params,
                                    on_accept,
                                    on_text,
                                    on_reasoning,
                                    on_tool,
                                )
                                .await?
                        };
                        if let Some(projection) =
                            db.turn_presentation(&session.0, &report.turn_id)?
                        {
                            let _ = events.send(CoreEvent::TurnPresentation {
                                session: session.clone(),
                                turn: WorkerTurnId(report.turn_id.clone()),
                                projection,
                            });
                        }
                        // The default/configured title profile uses the selected
                        // provider adapter, without tools or a second conversation.
                        // Failure leaves the honest untitled state and can be retried
                        // on a later turn. Existing/child titles are never replaced.
                        if report.status == TurnStatus::Completed
                            && !cancel.load(Ordering::Relaxed)
                            && db.session_meta(&session.0)?.title.is_none()
                        {
                            let selection = &title_selection;
                            let fallback = composition
                                .generation
                                .providers
                                .get(&composition.catalog.provider)
                                .map(|provider| provider.options.native_fallback_limits)
                                .unwrap_or_default();
                            let budget = crate::models::budget(selection, 256, fallback);
                            if let Some(warning) = &budget.warning {
                                report.warnings.push(format!("title generation: {warning}"));
                            }
                            let input = vec![
                                    crate::provider::InputItem::message(
                                        crate::provider::InputRole::Developer,
                                        title_agent.map(|a| a.body.as_str()).unwrap_or("Generate a short session title from the user's request. Output only the title, in at most 100 characters."),
                                    ),
                                    crate::provider::InputItem::message(
                                        crate::provider::InputRole::User,
                                        &title_prompt[..title_prompt.floor_char_boundary(title_prompt.len().min(8192))],
                                    ),
                                ];
                            let tools = [];
                            let input_tokens = crate::runtime::estimate_tokens(
                                &serde_json::to_string(&(&input, &tools))
                                    .map_err(|_| RuntimeError::Storage)?,
                            );
                            if let Err(error) =
                                crate::models::admit_budget(selection, input_tokens, &budget)
                            {
                                report
                                    .warnings
                                    .push(format!("title generation skipped: {error}"));
                            } else if let Ok(Ok(generation)) = tokio::time::timeout(
                                std::time::Duration::from_secs(10),
                                crate::provider::stream_input_observed(
                                    &composition.provider,
                                    &selection.id,
                                    selection.variant.as_ref(),
                                    &input,
                                    &tools,
                                    budget.output,
                                    &cancel,
                                    &mut |_| {},
                                ),
                            )
                            .await
                            {
                                let canonical = generation
                                    .output
                                    .iter()
                                    .filter(|item| {
                                        item["type"] == "message" && item["role"] == "assistant"
                                    })
                                    .filter_map(|value| {
                                        value.get("content").and_then(|v| v.as_array())
                                    })
                                    .flatten()
                                    .filter(|c| c["type"] == "output_text")
                                    .filter_map(|c| c["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("");
                                let title = if generation.text.is_empty() {
                                    &canonical
                                } else {
                                    &generation.text
                                }
                                .lines()
                                .find(|line| !line.trim().is_empty())
                                .unwrap_or_default()
                                .trim()
                                .trim_matches('"');
                                let title: String = title
                                    .chars()
                                    .filter(|c| !c.is_control())
                                    .take(100)
                                    .collect();
                                if !title.is_empty() {
                                    db.set_generated_title(&session.0, &title)?;
                                }
                            }
                        }
                        Ok::<_, RuntimeError>(report)
                    };
                    tokio::pin!(operation);
                    result = loop {
                        tokio::select! {
                            result = &mut operation => break result,
                            command = inbox.recv(), if !shutdown => match command {
                                None | Some(InboxMsg::Shutdown) => {
                                    shutdown = true;
                                    cancel.store(true, Ordering::Relaxed);
                                }
                                Some(InboxMsg::Cancel { session: target, ack }) if target == session => {
                                    cancel.store(true, Ordering::Relaxed);
                                    let _ = ack.send(Ok(()));
                                }
                                Some(command) => query(
                                    db,
                                    runtime,
                                    composition,
                                    effective,
                                    registry,
                                    sessions,
                                    home_choices,
                                    location_epoch,
                                    suggestion_queue,
                                    command,
                                ),
                            }
                        }
                    };
                }
                if turn.is_some() {
                    sessions.insert(runtime.location().to_string(), session.0.clone());
                }
                if let Some(ack) = ack {
                    let error = result.err().unwrap_or(RuntimeError::Storage);
                    let _ = ack.send(Err(app_error(error)));
                } else if let Some(turn) = turn {
                    // Provider usage is forwarded only when the provider
                    // reported it; never synthesized.
                    if let Ok(report) = &result
                        && let Some((input_tokens, output_tokens)) = report.usage
                    {
                        let _ = events.send(CoreEvent::TurnUsage {
                            session: session.clone(),
                            turn: turn.clone(),
                            input_tokens,
                            output_tokens,
                            streamed_ms: report.streamed_ms,
                        });
                    }
                    let duration_ms = match &result {
                        Ok(report) => report.duration_ms,
                        Err(_) => 0,
                    };
                    // Degraded MCP servers stay visible on the terminal event;
                    // a failed turn has no report and therefore no notices.
                    let warnings = match &result {
                        Ok(report) => report.warnings.clone(),
                        Err(_) => Vec::new(),
                    };
                    let event = match result {
                        Err(RuntimeError::Cancelled) => CoreEvent::TurnInterrupted {
                            session: session.clone(),
                            turn,
                            partial: String::new(),
                            duration_ms,
                        },
                        Ok(report) if report.status == TurnStatus::Completed => {
                            CoreEvent::TurnFinished {
                                session: session.clone(),
                                turn,
                                text: report.text,
                                duration_ms,
                                warnings,
                            }
                        }
                        Ok(report) if report.status == TurnStatus::Cancelled => {
                            CoreEvent::TurnInterrupted {
                                session: session.clone(),
                                turn,
                                partial: report.text,
                                duration_ms,
                            }
                        }
                        Ok(report) if report.status == TurnStatus::Incomplete => {
                            CoreEvent::TurnFailed {
                                session: session.clone(),
                                turn,
                                error: app_error(report.diagnostic.as_deref().unwrap_or(
                                    "turn incomplete: response ended early or round limit reached",
                                )),
                                warnings,
                            }
                        }
                        Ok(report) => CoreEvent::TurnFailed {
                            session: session.clone(),
                            turn,
                            error: app_error(
                                report.diagnostic.as_deref().unwrap_or("provider error"),
                            ),
                            warnings,
                        },
                        Err(error) => CoreEvent::TurnFailed {
                            session: session.clone(),
                            turn,
                            error: app_error(error),
                            warnings,
                        },
                    };
                    let _ = events.send(event);
                }
                if shutdown {
                    break;
                }
            }
            message => query(
                db,
                runtime,
                composition,
                effective,
                registry,
                sessions,
                home_choices,
                location_epoch,
                suggestion_queue,
                message,
            ),
        }
    }
    Ok(WorkerOutcome::Stop)
}

fn resolve_submission(
    composition: &Composition,
    text: String,
) -> Result<(String, Option<String>), RuntimeError> {
    let Some(command) = text.strip_prefix('/') else {
        return Ok((text, None));
    };
    let split = command.find(char::is_whitespace).unwrap_or(command.len());
    let id = &command[..split];
    let Some(template) = composition.commands.get(id) else {
        return Ok((text, None));
    };
    let remainder = command[split..].trim();
    if id == "review" && composition.builtin_review {
        let expanded = crate::runtime::expand_command(template, &[remainder.to_string()])?;
        return Ok((expanded, Some(text)));
    }
    let args = remainder
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let expanded = crate::runtime::expand_command(template, &args)?;
    Ok((expanded, Some(text)))
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use oc_core::queries::TerminalCopyMode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn terminal_copy_and_animations_follow_global_location_switch_and_reload() {
        let root = tempfile::tempdir().unwrap();
        let global = root.path().join("global");
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        for path in [&global, &a, &b, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let model = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(global.join("opencode.json"), model.to_string()).unwrap();
        std::fs::write(
            global.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"manual"},"animations":false}"#,
        )
        .unwrap();
        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"select"},"animations":true}"#,
        )
        .unwrap();
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        )]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        let initial = app.catalog().await.unwrap();
        assert_eq!(initial.chrome.terminal_copy, Some(TerminalCopyMode::Manual));
        assert_eq!(initial.chrome.animations, Some(false));

        std::fs::write(b.join("opencode.jsonc"), r#"{"animations":"invalid"}"#).unwrap();
        let failure = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap_err();
        assert!(matches!(
            failure,
            CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            }
        ));
        assert_eq!(app.catalog().await.unwrap(), initial);
        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"select"},"animations":true}"#,
        )
        .unwrap();
        let switched = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            switched.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Select)
        );
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            switched.catalog.chrome.terminal_copy
        );
        assert_eq!(switched.catalog.chrome.animations, Some(true));
        assert_eq!(app.catalog().await.unwrap().chrome.animations, Some(true));

        std::fs::write(b.join("opencode.jsonc"), r#"{"animations":null}"#).unwrap();
        let failure = app.reload_location().await.unwrap_err();
        assert!(matches!(
            failure,
            CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            }
        ));
        assert_eq!(app.catalog().await.unwrap(), switched.catalog);

        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"sensitive-fixture"},"animations":true}"#,
        )
        .unwrap();
        let failure = app.reload_location().await.unwrap_err();
        assert!(!failure.to_string().contains("sensitive-fixture"));
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            Some(TerminalCopyMode::Select)
        );
        assert_eq!(app.catalog().await.unwrap().chrome.animations, Some(true));

        std::fs::write(b.join("opencode.jsonc"), "{}").unwrap();
        let reloaded = app.reload_location().await.unwrap();
        assert!(reloaded.generation > switched.generation);
        assert_eq!(
            reloaded.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Manual)
        );
        assert_eq!(reloaded.catalog.chrome.animations, Some(false));
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            reloaded.catalog.chrome.terminal_copy
        );
        let returned = app
            .switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            returned.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Manual)
        );
        assert_eq!(returned.catalog.chrome.animations, Some(false));

        std::fs::write(global.join("opencode.jsonc"), "{}").unwrap();
        let unconfigured = app.reload_location().await.unwrap();
        assert_eq!(unconfigured.catalog.chrome.terminal_copy, None);
        assert_eq!(unconfigured.catalog.chrome.animations, None);
        assert!(unconfigured.catalog.chrome.animations_enabled());
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn catalog_descriptions_track_current_location_and_reload() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        for path in [&a, &b, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut config = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": "http://127.0.0.1:9/v1", "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(a.join("opencode.json"), config.to_string()).unwrap();
        config["command"] = serde_json::json!({"review": {
            "template": "local review $ARGUMENTS", "description": "from B"
        }});
        std::fs::write(b.join("opencode.json"), config.to_string()).unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        app.switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "from B"
        );
        config["command"]["review"]["description"] = "reloaded".into();
        std::fs::write(b.join("opencode.json"), config.to_string()).unwrap();
        app.reload_location().await.unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "reloaded"
        );
        app.switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[test]
    fn review_replaces_only_source_placeholders_and_enforces_expansion_cap() {
        let input = "'branch name'  $ARGUMENTS  ${path}  $1";
        let source = include_str!("../assets/upstream/v2/review.txt");
        assert_eq!(
            crate::runtime::expand_command(source, &[input.into()]).unwrap(),
            source.replace("$ARGUMENTS", input)
        );
        assert!(
            crate::runtime::expand_command(
                source,
                &["x".repeat(crate::runtime::COMMAND_BYTES_CAP)]
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn workspace_review_override_keeps_positional_expansion() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("opencode.json"),
            serde_json::json!({
                "model": "fixture/m", "provider": {"fixture": {
                    "options": {"baseURL": "https://example.invalid/v1", "apiKey": "dummy"},
                    "models": {"m": {}}
                }},
                "command": {"review": {"template": "local $1; all: $ARGUMENTS"}}
            })
            .to_string(),
        )
        .unwrap();
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            root.path().join("config").to_string_lossy().into_owned(),
        )]);
        let composition = composition::load_with_env(&project, env).await.unwrap();
        assert!(!composition.builtin_review);
        assert_eq!(
            resolve_submission(&composition, "/review   one  two ".into()).unwrap(),
            (
                "local one; all: one two".into(),
                Some("/review   one  two ".into())
            )
        );
        assert_eq!(
            resolve_submission(&composition, "/something else".into()).unwrap(),
            ("/something else".into(), None)
        );
    }

    #[tokio::test]
    async fn fallback_review_submits_exact_prompt_and_replays_original_invocation() {
        let root = tempfile::tempdir().expect("fixture");
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&data).expect("data");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake provider");
        let config = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": format!("http://{}/v1", listener.local_addr().unwrap()),
                            "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env.clone())
            .await
            .expect("spawn");
        assert_eq!(app.catalog().await.expect("catalog").commands, ["review"]);
        assert_eq!(
            app.catalog().await.expect("catalog").command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        let composed = composition::load_with_env(&project, env.clone())
            .await
            .expect("admitted generation");
        let (default_prompt, default_invocation) =
            resolve_submission(&composed, "/review".into()).expect("default review");
        assert_eq!(default_invocation.as_deref(), Some("/review"));
        assert_eq!(
            default_prompt,
            include_str!("../assets/upstream/v2/review.txt").replace("$ARGUMENTS", "")
        );
        let session = SessionId("review-fallback".into());
        app.create_session(session.clone()).await.expect("session");
        let original = "/review   'branch name'  $ARGUMENTS  ${path}   ";
        let expected_input = "'branch name'  $ARGUMENTS  ${path}";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("request");
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            let (headers_end, content_length) = loop {
                let n = stream.read(&mut chunk).await.expect("read");
                assert!(n > 0, "complete request");
                request.extend_from_slice(&chunk[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let len = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .expect("body size");
                    if request.len() >= end + 4 + len {
                        break (end, len);
                    }
                }
            };
            let body: serde_json::Value =
                serde_json::from_slice(&request[headers_end + 4..headers_end + 4 + content_length])
                    .expect("Responses JSON");
            let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"review complete\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes()).await.expect("reply");
            body
        });
        let mut events = app.subscribe();
        app.submit(session.clone(), original.into())
            .await
            .expect("submit review");
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
                .await
                .expect("turn timeout")
                .expect("event")
            {
                CoreEvent::TurnFinished { session: id, .. } if id == session => break,
                CoreEvent::TurnFailed { error, .. } => panic!("review failed: {error}"),
                _ => {}
            }
        }
        let request = server.await.expect("fake response");
        let expected =
            include_str!("../assets/upstream/v2/review.txt").replace("$ARGUMENTS", expected_input);
        let actual = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["role"] == "user")
            .filter_map(|item| item["content"].as_array())
            .flat_map(|parts| parts.iter())
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [expected.as_str()],
            "provider receives exact expansion"
        );
        assert_eq!(
            app.read_history(session.clone()).await.unwrap()[0].text,
            original
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();

        let (reopened, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        assert_eq!(
            reopened.read_history(session).await.unwrap()[0].text,
            original
        );
        reopened.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}

#[cfg(test)]
mod file_suggestion_tests {
    use super::*;

    #[tokio::test]
    async fn burst_coalesces_to_one_pending_walk_without_overlapping_active_walk() {
        use std::sync::atomic::AtomicUsize;
        use tokio::time::{Duration, timeout};

        let queue = Arc::new(Mutex::new(SuggestionQueue::default()));
        let epoch = Arc::new(AtomicU64::new(1));
        let active = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (first_ack, first_rx) = oneshot::channel();
        let first_active = Arc::clone(&active);
        let first_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    assert_eq!(first_active.fetch_add(1, Ordering::SeqCst), 0);
                    first_entered.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    first_active.fetch_sub(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: vec!["first".into()],
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack: first_ack,
            },
        );
        timeout(Duration::from_secs(5), started_rx)
            .await
            .unwrap()
            .unwrap();

        let mut last_rx = None;
        for index in 0..100 {
            let (ack, rx) = oneshot::channel();
            let next_active = Arc::clone(&active);
            let next_entered = Arc::clone(&entered);
            enqueue_file_suggestion(
                &queue,
                &epoch,
                SuggestionRequest {
                    work: Box::new(move || {
                        assert_eq!(next_active.fetch_add(1, Ordering::SeqCst), 0);
                        next_entered.fetch_add(1, Ordering::SeqCst);
                        next_active.fetch_sub(1, Ordering::SeqCst);
                        Ok(crate::files::FileSuggestionResult {
                            paths: vec![index.to_string()],
                            truncated: false,
                        })
                    }),
                    location: "/a".into(),
                    generation: 1,
                    ack,
                },
            );
            if let Some(previous) = last_rx.replace(rx) {
                assert_eq!(
                    previous.await.unwrap(),
                    Err(app_error("file suggestions superseded"))
                );
            }
        }
        assert_eq!(entered.load(Ordering::SeqCst), 1);
        assert!(queue.lock().unwrap().pending.is_some());
        // Cancellation of the latest pending request must also skip the pool.
        drop(last_rx);
        let (ack, rx) = oneshot::channel();
        let last_entered = Arc::clone(&entered);
        let last_active = Arc::clone(&active);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    assert_eq!(last_active.fetch_add(1, Ordering::SeqCst), 0);
                    last_entered.fetch_add(1, Ordering::SeqCst);
                    last_active.fetch_sub(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: vec!["latest".into()],
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        release.send(()).unwrap();
        assert_eq!(
            timeout(Duration::from_secs(5), first_rx)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .paths,
            ["first"]
        );
        assert_eq!(
            timeout(Duration::from_secs(5), rx)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .paths,
            ["latest"]
        );
        assert_eq!(entered.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn canceled_pending_request_never_enters_blocking_pool() {
        use std::sync::atomic::AtomicUsize;
        use tokio::time::{Duration, timeout};

        let queue = Arc::new(Mutex::new(SuggestionQueue::default()));
        let epoch = Arc::new(AtomicU64::new(1));
        let entered = Arc::new(AtomicUsize::new(0));
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (ack, rx) = oneshot::channel();
        let first_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    first_entered.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    Ok(crate::files::FileSuggestionResult {
                        paths: Vec::new(),
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        timeout(Duration::from_secs(5), started_rx)
            .await
            .unwrap()
            .unwrap();

        let (ack, canceled) = oneshot::channel();
        let pending_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    pending_entered.fetch_add(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: Vec::new(),
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        drop(canceled);
        release.send(()).unwrap();
        timeout(Duration::from_secs(5), rx)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while queue.lock().unwrap().running {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(entered.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn suggestions_follow_owner_across_home_switch_and_return() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let config = r#"{"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},"models":{"m":{}}}}}"#;
        for project in [&a, &b] {
            std::fs::write(project.join("opencode.json"), config).unwrap();
        }
        std::fs::write(a.join("only-a.rs"), "a").unwrap();
        std::fs::write(b.join("only-b.rs"), "b").unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        let first = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(first.location, a.to_string_lossy());
        assert_eq!(first.paths, ["only-a.rs"]);
        assert!(!first.truncated);

        let switched = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        let second = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(second.location, b.to_string_lossy());
        assert_eq!(second.paths, ["only-b.rs"]);
        assert!(second.generation > first.generation);
        assert_eq!(second.generation, switched.generation);

        let returned = app
            .switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        let third = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(third.location, first.location);
        assert_eq!(third.paths, first.paths);
        assert!(third.generation > second.generation);
        assert_eq!(third.generation, returned.generation);
        let reloaded = app.reload_location().await.unwrap();
        assert_eq!(reloaded.location, third.location);
        assert!(reloaded.generation > third.generation);
        assert_eq!(
            app.file_suggestions("only".into(), 20)
                .await
                .unwrap()
                .generation,
            reloaded.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[test]
    fn stale_walk_is_refused_even_when_location_returns_to_same_path() {
        let epoch = AtomicU64::new(1);
        // A detached blocking walk finishes after the owner publishes A→B→A.
        epoch.fetch_add(2, Ordering::SeqCst);
        let result = file_suggestion_reply(
            &epoch,
            1,
            "/project/a".into(),
            Ok(crate::files::FileSuggestionResult {
                paths: vec!["old.rs".into()],
                truncated: false,
            }),
        );
        assert_eq!(
            result,
            Err(app_error("file suggestions belong to previous Location"))
        );
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    use oc_core::queries::SessionSelectionAction as Action;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn config(model: &str, base_url: &str, static_model: bool) -> String {
        let provider = if static_model { "fixture" } else { "ludka2" };
        let models = if static_model {
            serde_json::json!({(model): {}})
        } else {
            serde_json::json!({})
        };
        serde_json::json!({
            "model": format!("{provider}/{model}"),
            "provider": {(provider): {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": base_url, "apiKey": "dummy"},
                "models": models
            }}
        })
        .to_string()
    }

    fn env(data: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ])
    }

    #[tokio::test]
    async fn removed_selected_primary_refuses_reload_and_keeps_old_turn_usable() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let config = |include_review: bool| {
            let mut value: serde_json::Value =
                serde_json::from_str(&config("old", &base_url, true)).unwrap();
            value["agent"] = serde_json::json!({
                "build": {"mode": "primary", "prompt": "BUILD_PRIMARY"},
                "review": {"mode": "primary", "prompt": "REVIEW_PRIMARY"}
            });
            value["default_agent"] = "build".into();
            if !include_review {
                value["agent"].as_object_mut().unwrap().remove("review");
            }
            value.to_string()
        };
        let path = project.join("opencode.json");
        std::fs::write(&path, config(true)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("retained-review".into());
        let active = SessionId("active-build".into());
        app.create_session(session.clone()).await.unwrap();
        app.create_session(active.clone()).await.unwrap();
        let saved = app
            .save_tab_deck(oc_core::queries::TabDeckSnapshot {
                sessions: vec![session.clone(), active.clone()],
                active: Some(active),
                ..app.tab_deck().await.unwrap()
            })
            .await
            .unwrap();
        let chosen = app
            .session_selection(session.clone(), false, Action::Agent("review".into()))
            .await
            .unwrap();
        assert_eq!(chosen.agent_id.as_deref(), Some("review"));
        let initial = app.file_suggestions("".into(), 1).await.unwrap();
        let catalog = app.catalog().await.unwrap();

        std::fs::write(&path, config(false)).unwrap();
        assert_eq!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                detail: "retained session selection unavailable in reloaded configuration".into(),
            })
        );
        assert_eq!(app.catalog().await.unwrap(), catalog);
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            initial.generation
        );
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap(),
            chosen
        );

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
                if let Some(headers_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap();
                    if request.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }
            let body = String::from_utf8_lossy(&request);
            assert!(body.contains("REVIEW_PRIMARY"), "old agent prompt missing");
            assert!(body.contains("retained turn"), "old session prompt missing");
            let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
            stream
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes())
                .await
                .unwrap();
        });
        let mut events = app.subscribe();
        app.submit(session.clone(), "retained turn".into())
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnFinished { session: id, .. } if id == session => break,
                CoreEvent::TurnFailed { error, .. } => panic!("retained turn failed: {error}"),
                _ => {}
            }
        }
        server.await.unwrap();
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn retired_explicit_tab_model_is_not_silently_replaced_by_config_fallback() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let path = project.join("opencode.json");
        let config = |retired: bool| {
            let mut value: serde_json::Value =
                serde_json::from_str(&config("new", "http://127.0.0.1:9/v1", true)).unwrap();
            if !retired {
                value["provider"]["fixture"]["models"]["old"] = serde_json::json!({});
            }
            value.to_string()
        };
        std::fs::write(&path, config(false)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("explicit-old-model".into());
        app.create_session(session.clone()).await.unwrap();
        app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
            sessions: vec![session.clone()],
            active: Some(session.clone()),
            ..app.tab_deck().await.unwrap()
        })
        .await
        .unwrap();
        app.session_selection(session.clone(), false, Action::Model("old".into()))
            .await
            .unwrap();
        let initial = app.file_suggestions("".into(), 1).await.unwrap();
        std::fs::write(&path, config(true)).unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            })
        ));
        assert_eq!(
            app.session_selection(session, false, Action::Current)
                .await
                .unwrap()
                .model_id,
            "old"
        );
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            initial.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn current_reload_discovers_new_catalog_preserves_deck_and_rolls_back_failures() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let path = project.join("opencode.json");
        std::fs::write(&path, config("old", "http://127.0.0.1:9/v1", true)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("reload-root".into());
        app.create_session(session.clone()).await.unwrap();
        let deck = app.tab_deck().await.unwrap();
        let saved = app
            .save_tab_deck(oc_core::queries::TabDeckSnapshot {
                sessions: vec![session.clone()],
                active: Some(session.clone()),
                ..deck
            })
            .await
            .unwrap();
        let initial = app.file_suggestions("".into(), 1).await.unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (model, count) in [("new", 1), ("new", 0)] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                }
                assert!(request.starts_with(b"GET /v1/models HTTP/1.1"));
                let entries = if count == 1 {
                    vec![serde_json::json!({"id": model, "context_length": 1000})]
                } else {
                    vec![]
                };
                let body = serde_json::json!({"object":"list", "data": entries}).to_string();
                stream
                    .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
                    .await
                    .unwrap();
            }
        });
        std::fs::write(&path, config("new", &base_url, false)).unwrap();
        let reloaded = app.reload_location().await.unwrap();
        assert_eq!(reloaded.location, initial.location);
        assert!(reloaded.generation > initial.generation);
        assert_eq!(reloaded.catalog.model_id, "new");
        assert_eq!(reloaded.catalog.models.len(), 1);
        assert_eq!(reloaded.catalog.models[0].id, "new");
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap()
                .model_id,
            "new"
        );
        assert_eq!(
            app.probe_session(session.clone()).await.unwrap(),
            SessionProbe::Root
        );

        std::fs::write(&path, "{broken").unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch { .. })
        ));
        assert_eq!(app.catalog().await.unwrap(), reloaded.catalog);
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            reloaded.generation
        );

        std::fs::write(&path, config("new", &base_url, false)).unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch { .. })
        ));
        server.await.unwrap();
        assert_eq!(app.catalog().await.unwrap(), reloaded.catalog);
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.probe_session(session).await.unwrap(),
            SessionProbe::Root
        );
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            reloaded.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn reload_is_refused_by_owner_during_active_turn() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        std::fs::write(
            project.join("opencode.json"),
            config("old", &base_url, true),
        )
        .unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("active-reload".into());
        app.submit_fresh(session.clone(), "hello".into(), None)
            .await
            .unwrap();
        assert_eq!(app.reload_location().await, Err(CoreError::TurnBusy));
        app.cancel(session).await.unwrap();
        // Drop the stalled transport so the cancelled turn can terminate.
        drop(listener);
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}
