//! Single native application owner behind the core command/event interface.
//!
//! The worker owns storage, the runtime and the effective model/variant/agent
//! selection. Frontends query bounded view snapshots and send actions; they
//! never open the database or duplicate config/persistence logic (T39).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use oc_core::core_app::{CoreApp, CoreEvent, InboxMsg, WorkerGuard, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, DcpSnapshot, HistoryMessage, HistoryPage, LocationSnapshot,
    ModelEntry, SkillCard, StartupNotice, ToolOpPage, ToolOpView, VariantEntry,
};
use oc_core::session::{CoreError, LocationSwitchFailure, MAX_QUEUE_ITEMS, MessageId, Role};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::composition::{self, Composition};
use crate::runtime::{
    Runtime, RuntimeError, SubagentAgent, SubagentCatalog, ToolCallEvent, TurnParams, TurnStatus,
};
use crate::storage::{Db, StorageError};
use crate::tui_workspace::{AgentEntry as WorkspaceAgent, WorkspaceError, WorkspaceRegistry};

#[path = "application_selection.rs"]
mod selection;

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
    let db = Db::open(data)
        .map_err(|e| SpawnIssue::new(storage_failure(&e), format!("storage: {e}")))?;
    db.recover_interrupted_tools()
        .map_err(|e| SpawnIssue::new(SpawnFailure::Recovery, format!("recovery: {e}")))?;
    let (app, inbox, events) = CoreApp::channel(MAX_QUEUE_ITEMS);
    let (ready, ready_rx) = oneshot::channel();
    let handle = tokio::spawn(start_worker(db, composition, inbox, events, ready));
    let guard = WorkerGuard::from_task(handle);
    match ready_rx.await {
        Ok(Ok(worker_diagnostics)) => {
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
        ack: oneshot::Sender<Result<LocationSnapshot, CoreError>>,
    },
}

/// Own the whole application task: build the runtime, publish the workspace
/// exactly once, then run the command loop and close owned MCP resources.
///
/// A Location switch builds the complete target generation (config, catalog,
/// agents/skills/commands, MCP resources, runtime, session) before it is
/// published; a failure keeps the current Location untouched.
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
                match switch_target(&db, &path, &mut sessions, composition.parent_env.clone()).await
                {
                    Ok((next, next_composition, next_effective, next_registry, session, notes)) => {
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
                        let location = runtime.location().to_string();
                        let catalog = next_effective.snapshot(&next_composition);
                        let mut notices = next_composition.startup_notices.clone();
                        if notes.len() > next_composition.diagnostics.len() {
                            notices.push(StartupNotice::SavedSelection);
                        }
                        composition = next_composition;
                        effective = next_effective;
                        registry = next_registry;
                        let _ = ack.send(Ok(LocationSnapshot {
                            location,
                            session: session.0,
                            catalog,
                            diagnostics: notes,
                            notices,
                        }));
                    }
                    Err(issue) => {
                        let category = match issue.category {
                            SpawnFailure::Configuration | SpawnFailure::MissingCredential => {
                                LocationSwitchFailure::Configuration
                            }
                            SpawnFailure::Storage => LocationSwitchFailure::Storage,
                            _ => LocationSwitchFailure::Runtime,
                        };
                        let _ = ack.send(Err(CoreError::LocationSwitch {
                            category,
                            detail: issue.detail,
                        }));
                    }
                }
            }
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
) -> Result<
    (
        Runtime<'a>,
        Composition,
        Effective,
        WorkspaceRegistry,
        SessionId,
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
    // Sessions stay Location-bound: a return to a visited Location reopens
    // its recorded session, a first visit mints one for the new Location.
    let location = runtime.location().to_string();
    let session = match sessions.get(&location).cloned() {
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
fn query(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    sessions: &mut BTreeMap<String, String>,
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
        InboxMsg::List { ack } => {
            let _ = ack.send(
                db.list_sessions()
                    .map(|ids| ids.into_iter().map(SessionId).collect())
                    .map_err(app_error),
            );
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
        InboxMsg::Skills { ack } => {
            let _ = ack.send(Ok(skill_cards(composition)));
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
            InboxMsg::SwitchLocation { path, ack } => {
                return Ok(WorkerOutcome::Switch { path, ack });
            }
            InboxMsg::Submit { session, text, ack } => {
                sessions.insert(runtime.location().to_string(), session.0.clone());
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
                // Resolve from the owning session, never whichever TUI tab was
                // most recently viewed. Title and inherited child requests use
                // this same selection and agent workspace.
                let turn_selection =
                    match selection::for_turn(db, composition, effective, &session.0).and_then(
                        |selected| {
                            let base = crate::models::select_model(&composition.catalog, &selected.model_id)
                                .and_then(|base| crate::models::select_variant(&base, selected.variant.as_deref()))
                                .map_err(|e| app_error(format!("selected model/variant unavailable; select an admitted replacement or Default: {e}")))?;
                            let _ = base;
                            publish_workspace(runtime, composition, &selected)
                                .map_err(app_error)?;
                            Ok(selected)
                        },
                    ) {
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
                        let mut report = runtime
                            .run_turn_with_tool_events(
                                params,
                                |id| {
                                    let id = WorkerTurnId(id.to_string());
                                    turn = Some(id.clone());
                                    if let Some(ack) = ack.take() {
                                        let _ = ack.send(Ok(id.clone()));
                                    }
                                    let _ = events.send(CoreEvent::TurnStarted {
                                        session: session.clone(),
                                        turn: id.clone(),
                                    });
                                },
                                |id, delta| {
                                    let _ = events.send(CoreEvent::TextDelta {
                                        session: session.clone(),
                                        turn: WorkerTurnId(id.to_string()),
                                        delta: delta.to_string(),
                                    });
                                },
                                |id, delta| {
                                    let _ = events.send(CoreEvent::ReasoningDelta {
                                        session: session.clone(),
                                        turn: WorkerTurnId(id.to_string()),
                                        delta: delta.to_string(),
                                    });
                                },
                                |id, event| {
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
                                    if let Ok(Some(projection)) =
                                        db.turn_presentation(&session.0, id)
                                    {
                                        let _ = events.send(CoreEvent::TurnPresentation {
                                            session: session.clone(),
                                            turn: WorkerTurnId(id.to_string()),
                                            projection,
                                        });
                                    }
                                },
                            )
                            .await?;
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
                                    command,
                                ),
                            }
                        }
                    };
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
    let args = remainder
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let expanded = crate::runtime::expand_command(template, &args)?;
    Ok((expanded, Some(text)))
}
