//! Single native application owner behind the core command/event interface.
//!
//! The worker owns storage, the runtime and the effective model/variant/agent
//! selection. Frontends query bounded view snapshots and send actions; they
//! never open the database or duplicate config/persistence logic (T39).
//! Review/suggestions/reload tests share application/tests.rs; fork/conversation packs stay separate.

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
    HomeLocationSnapshot, LocationSnapshot, ModelEntry, ModelSwitchNotice, ReloadLocationSnapshot,
    SelectionReadiness, SessionProbe, SkillCard, StartupNotice, ToolOpPage, ToolOpView,
    VariantEntry,
};
use oc_core::session::{CoreError, LocationSwitchFailure, MAX_QUEUE_ITEMS, MessageId, Role};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::composition::{self, Composition};
use crate::runtime::{
    Runtime, RuntimeError, SubagentAgent, SubagentCatalog, ToolCallEvent, TurnParams, TurnStatus,
};
use crate::storage::{Db, StorageError};
use crate::trace;
use crate::tui_workspace::{AgentEntry as WorkspaceAgent, WorkspaceError, WorkspaceRegistry};

#[cfg(test)]
#[path = "application_conversation_tests.rs"]
mod conversation_tests;
#[cfg(test)]
#[path = "application/fatal_tests.rs"]
mod fatal_tests;
#[cfg(test)]
#[path = "application_fork_tests.rs"]
mod fork_tests;
#[cfg(test)]
#[path = "application/inherited_tests.rs"]
mod inherited_tests;
mod mcp_lookup;
#[cfg(test)]
#[path = "application/plugin_tests.rs"]
mod plugin_tests;
mod provider_catalog;
#[cfg(test)]
#[path = "application/provider_tests.rs"]
mod provider_tests;
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

/// A normal exit may finish an already accepted title, but cannot wait for the
/// provider's full request timeout when it stalls.
const TITLE_SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

struct AutomaticTitleResult {
    session: SessionId,
    expected_event: i64,
    title: Option<String>,
    dispatch: Option<(
        String,
        oneshot::Sender<Result<(), crate::provider::ProviderError>>,
    )>,
}

// The owner aborts unfinished provider work on shutdown or Location replacement.
// A provider task has no database handle: only the owner may commit its result.
#[derive(Default)]
struct AutomaticTitles {
    pending: BTreeMap<String, i64>,
    tasks: Vec<(String, JoinHandle<()>)>,
}

impl AutomaticTitles {
    fn cancel(&mut self, session: &str) -> bool {
        if self.pending.remove(session).is_none() {
            return false;
        }
        for (owner, task) in &self.tasks {
            if owner == session {
                task.abort();
            }
        }
        true
    }
}

impl Drop for AutomaticTitles {
    fn drop(&mut self) {
        for (_, task) in &self.tasks {
            task.abort();
        }
    }
}

fn commit_automatic_title(
    db: &Db,
    events: &broadcast::Sender<CoreEvent>,
    work: &Mutex<AutomaticTitles>,
    result: AutomaticTitleResult,
) {
    if let Some((operation, ack)) = result.dispatch {
        let _ = ack.send(
            db.generation_dispatch(&result.session.0, &operation, "title")
                .map_err(|_| crate::provider::ProviderError::DispatchRefused),
        );
        return;
    }
    {
        let mut work = work.lock().expect("title work mutex");
        if work.pending.get(&result.session.0) != Some(&result.expected_event) {
            return;
        }
        work.pending.remove(&result.session.0);
    }
    if let Some(title) = result.title
        && let Ok(true) =
            db.compare_and_set_root_title(&result.session.0, None, result.expected_event, &title)
    {
        let _ = events.send(CoreEvent::SessionTitleUpdated {
            session: result.session,
            title,
        });
    }
}

async fn stop_automatic_titles(
    work: &Mutex<AutomaticTitles>,
) -> Result<(), oc_core::queries::ServiceDiagnostic> {
    let tasks = {
        let mut work = work.lock().expect("title work mutex");
        work.pending.clear();
        std::mem::take(&mut work.tasks)
    };
    for (_, task) in &tasks {
        task.abort();
    }
    let mut failed = false;
    for (_, task) in tasks {
        if let Err(error) = task.await {
            failed |= !error.is_cancelled();
        }
    }
    if failed {
        Err(title_cleanup_failure())
    } else {
        Ok(())
    }
}

fn title_cleanup_failure() -> oc_core::queries::ServiceDiagnostic {
    let mut diagnostic = crate::config::diagnostic::failure(
        "native title worker",
        &["title"],
        oc_core::queries::ServiceStage::Cleanup,
        oc_core::queries::ServiceCode::CleanupFailed,
        oc_core::queries::ServiceAction::RestartApplication,
    );
    diagnostic.kind = oc_core::queries::ServiceKind::Runtime;
    diagnostic
}

async fn drain_automatic_titles(
    db: &Db,
    events: &broadcast::Sender<CoreEvent>,
    work: &Mutex<AutomaticTitles>,
    rx: &mut mpsc::Receiver<AutomaticTitleResult>,
) -> Result<(), oc_core::queries::ServiceDiagnostic> {
    let deadline = tokio::time::Instant::now() + TITLE_SHUTDOWN_GRACE;
    while !work.lock().expect("title work mutex").pending.is_empty() {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(result)) => commit_automatic_title(db, events, work, result),
            _ => break,
        }
    }
    // Cancel all remaining provider work and join it before the owner releases
    // the database. A canceled title never becomes a late title update.
    stop_automatic_titles(work).await
}

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

/// Safe native startup cause. Category is retained for compatible coarse callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnDiagnostic {
    pub category: SpawnFailure,
    pub diagnostic: oc_core::queries::ServiceDiagnostic,
}

type SpawnIssue = SpawnDiagnostic;

impl std::fmt::Display for SpawnDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.diagnostic.fmt(f)
    }
}

impl SpawnIssue {
    fn configuration(failure: composition::LoadFailure) -> Self {
        match failure {
            composition::LoadFailure::Configuration(diagnostic) => Self {
                category: SpawnFailure::Configuration,
                diagnostic,
            },
        }
    }

    fn new(
        category: SpawnFailure,
        source: &str,
        field: &[&str],
        code: oc_core::queries::ServiceCode,
    ) -> Self {
        use oc_core::queries::{ServiceAction, ServiceKind, ServiceStage};
        let (kind, stage, action) = match category {
            SpawnFailure::DataRootBusy => (
                ServiceKind::Storage,
                ServiceStage::Storage,
                ServiceAction::CloseOtherOwner,
            ),
            SpawnFailure::UnsafeDataRoot | SpawnFailure::DataRootUnavailable => (
                ServiceKind::Storage,
                ServiceStage::Admission,
                ServiceAction::ReviewDataDirectory,
            ),
            SpawnFailure::Storage => (
                ServiceKind::Storage,
                ServiceStage::Storage,
                ServiceAction::ReviewStorage,
            ),
            SpawnFailure::Recovery => (
                ServiceKind::Storage,
                ServiceStage::Recovery,
                ServiceAction::ReviewRecovery,
            ),
            _ => (
                ServiceKind::Runtime,
                ServiceStage::Initialize,
                ServiceAction::ReviewConfiguration,
            ),
        };
        let mut diagnostic = crate::config::diagnostic::failure(source, field, stage, code, action);
        diagnostic.kind = kind;
        Self {
            category,
            diagnostic,
        }
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

fn storage_code(error: &StorageError) -> oc_core::queries::ServiceCode {
    use oc_core::queries::ServiceCode;
    match error {
        StorageError::DataRootBusy => ServiceCode::DataRootBusy,
        StorageError::UnsafeRoot(_) => ServiceCode::UnsafeDataRoot,
        StorageError::StorageFull => ServiceCode::CapacityExceeded,
        _ => ServiceCode::StorageUnavailable,
    }
}

fn saved_selection_issue(db: &Db, field: &[&str]) -> SpawnIssue {
    let mut issue = SpawnIssue::new(
        SpawnFailure::Storage,
        &db.root().to_string_lossy(),
        field,
        oc_core::queries::ServiceCode::InvalidStoredState,
    );
    issue.diagnostic.stage = oc_core::queries::ServiceStage::Query;
    issue
}

/// Shared safe storage projection for application startup and the sessions CLI.
pub fn storage_diagnostic(
    source: &Path,
    error: &StorageError,
) -> oc_core::queries::ServiceDiagnostic {
    SpawnIssue::new(
        storage_failure(error),
        &source.to_string_lossy(),
        if matches!(error, StorageError::Sqlite(_)) {
            &["database"]
        } else {
            &["data_root"]
        },
        storage_code(error),
    )
    .diagnostic
}

fn runtime_issue(source: &str, field: &[&str], error: &RuntimeError) -> SpawnIssue {
    use oc_core::queries::{ServiceAction, ServiceCode, ServiceStage};
    if matches!(error, RuntimeError::McpAttach { .. }) {
        return SpawnIssue {
            category: SpawnFailure::Runtime,
            diagnostic: crate::runtime::mcp_diagnostic(source, error),
        };
    }
    let code = match error {
        RuntimeError::McpShutdown => ServiceCode::CleanupFailed,
        RuntimeError::Storage => ServiceCode::StorageUnavailable,
        RuntimeError::ContextOverflow { .. } | RuntimeError::McpCatalogLimit => {
            ServiceCode::CapacityExceeded
        }
        RuntimeError::Cancelled => ServiceCode::Cancelled,
        RuntimeError::ApprovalRequired { .. } => ServiceCode::ApprovalRequired,
        RuntimeError::LocationMismatch { .. } | RuntimeError::PermissionDenied { .. } => {
            ServiceCode::TrustRefused
        }
        RuntimeError::InvalidArgs(_) => ServiceCode::InvalidConfig,
        _ => ServiceCode::RuntimeFailed,
    };
    let mut issue = SpawnIssue::new(
        if matches!(error, RuntimeError::Storage) {
            SpawnFailure::Storage
        } else {
            SpawnFailure::Runtime
        },
        source,
        field,
        code,
    );
    match error {
        RuntimeError::McpShutdown => {
            issue.diagnostic.kind = oc_core::queries::ServiceKind::Mcp;
            issue.diagnostic.field = vec!["mcp".into()];
            issue.diagnostic.stage = ServiceStage::Cleanup;
            issue.diagnostic.action = ServiceAction::RestartApplication;
        }
        RuntimeError::ContextOverflow { .. } => {
            issue.diagnostic.stage = ServiceStage::Admission;
            issue.diagnostic.action = ServiceAction::ReduceCapacity;
        }
        RuntimeError::McpCatalogLimit => {
            issue.diagnostic.kind = oc_core::queries::ServiceKind::Mcp;
            issue.diagnostic.service = "native-mcp".into();
            issue.diagnostic.field = vec!["mcp".into(), "catalog".into()];
            issue.diagnostic.stage = ServiceStage::Admission;
            issue.diagnostic.action = ServiceAction::ReduceCapacity;
        }
        RuntimeError::Cancelled => issue.diagnostic.stage = ServiceStage::Call,
        RuntimeError::ApprovalRequired { .. } => {
            issue.diagnostic.field = vec!["permissions".into(), "approval".into()];
            issue.diagnostic.stage = ServiceStage::Admission;
        }
        RuntimeError::LocationMismatch { .. } | RuntimeError::PermissionDenied { .. } => {
            issue.diagnostic.stage = ServiceStage::Admission
        }
        _ => {}
    }
    issue
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
    spawn_with_diagnostic(project, data)
        .await
        .map_err(|issue| issue.to_string())
}

/// Warm headless/embedded startup with the same safe cause as interactive startup.
pub async fn spawn_with_diagnostic(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<String>), SpawnDiagnostic> {
    let env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    spawn_inner(project, data, env, false)
        .await
        .map(|(app, guard, diagnostics, _)| (app, guard, diagnostics))
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
    spawn_inner(project, data, env, false)
        .await
        .map(|(app, guard, diagnostics, _)| (app, guard, diagnostics))
        .map_err(|issue| issue.to_string())
}

/// Start the same application for a TUI, exposing only a static category on
/// failure; successful workers and their normal diagnostics are unchanged.
pub async fn spawn_diagnostic(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<StartupNotice>), SpawnFailure> {
    spawn_with_startup_diagnostic(project, data)
        .await
        .map_err(|issue| issue.category)
}

/// Structured fatal preflight for interactive callers. No raw error detail crosses it.
pub async fn spawn_with_startup_diagnostic(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<StartupNotice>), SpawnDiagnostic> {
    let env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    spawn_inner(project, data, env, true)
        .await
        .map(|(app, guard, _, notices)| (app, guard, notices))
}

async fn spawn_inner(
    project: &Path,
    data: &Path,
    env: BTreeMap<String, String>,
    defer_provider: bool,
) -> Result<(CoreApp, WorkerGuard, Vec<String>, Vec<StartupNotice>), SpawnIssue> {
    trace::log(
        "spawn.begin",
        &format!(
            "project={} data={} env={}",
            crate::config::mcp::safe_source_id(&project.to_string_lossy()),
            crate::config::mcp::safe_source_id(&data.to_string_lossy()),
            env.len()
        ),
    );
    let result = spawn_stages(project, data, env, defer_provider).await;
    if let Err(issue) = &result {
        trace::log("spawn.fail", &format!("category={:?}", issue.category));
    }
    result
}

async fn spawn_stages(
    project: &Path,
    data: &Path,
    env: BTreeMap<String, String>,
    defer_provider: bool,
) -> Result<(CoreApp, WorkerGuard, Vec<String>, Vec<StartupNotice>), SpawnIssue> {
    let mut composition = composition::load_local_with_env(project, env)
        .await
        .map_err(SpawnIssue::configuration)?;
    if !defer_provider {
        composition
            .refresh_provider()
            .await
            .map_err(SpawnIssue::configuration)?;
    }
    let mut diagnostics = composition.diagnostics.clone();
    let mut notices = composition.startup_notices.clone();
    let db = match Db::open(data) {
        Ok(db) => {
            trace::log(
                "storage.open",
                &format!(
                    "data_root={} ok",
                    crate::config::mcp::safe_source_id(&data.to_string_lossy())
                ),
            );
            db
        }
        Err(error) => {
            trace::log(
                "storage.open",
                &format!(
                    "data_root={} fail class={}",
                    crate::config::mcp::safe_source_id(&data.to_string_lossy()),
                    storage_class(&error)
                ),
            );
            return Err(SpawnIssue::new(
                storage_failure(&error),
                &data.to_string_lossy(),
                &["data_root"],
                storage_code(&error),
            ));
        }
    };
    db.recover_shell_jobs()
        .and_then(|_| db.recover_interrupted_tools())
        .map_err(|_| {
            SpawnIssue::new(
                SpawnFailure::Recovery,
                &data.to_string_lossy(),
                &["operations"],
                oc_core::queries::ServiceCode::RecoveryFailed,
            )
        })?;
    let (app, inbox, events) = CoreApp::channel(MAX_QUEUE_ITEMS);
    let (ready, ready_rx) = oneshot::channel();
    let handle = tokio::spawn(start_worker(db, composition, inbox, events, ready));
    let guard = WorkerGuard::from_diagnostic_task(handle);
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
            if let Err(diagnostic) = guard.join_diagnostic().await {
                return Err(SpawnIssue {
                    category: SpawnFailure::Runtime,
                    diagnostic,
                });
            }
            Err(match result {
                Ok(Err(issue)) => issue,
                _ => SpawnIssue::new(
                    SpawnFailure::Runtime,
                    "native application",
                    &["worker"],
                    oc_core::queries::ServiceCode::RuntimeFailed,
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
    profile_issue: Option<oc_core::queries::ServiceDiagnostic>,
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
            profile_issue: None,
            legacy_epoch: 0,
        }
    }

    /// Apply the persisted frontend model choice; retired ids stay visible
    /// and never silently fall back.
    fn apply_persisted_model(
        &mut self,
        db: &Db,
        composition: &Composition,
    ) -> Result<Vec<String>, SpawnIssue> {
        let raw = match db.get_pref(oc_core::queries::PREF_MODEL_SELECTION) {
            Ok(Some(raw)) => raw,
            Ok(None) => return Ok(Vec::new()),
            Err(error) => {
                let mut diagnostic = storage_diagnostic(db.root(), &error);
                diagnostic.stage = oc_core::queries::ServiceStage::Query;
                diagnostic.field = vec!["selection".into(), "model".into()];
                return Err(SpawnIssue {
                    category: storage_failure(&error),
                    diagnostic,
                });
            }
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Err(saved_selection_issue(db, &["selection", "model"]));
        };
        let provider = value
            .get("provider")
            .and_then(|value| value.as_str())
            .ok_or_else(|| saved_selection_issue(db, &["selection", "provider"]))?;
        if provider != composition.catalog.provider {
            return Ok(Vec::new());
        }
        let Some(id) = value.get("id").and_then(|v| v.as_str()) else {
            return Err(saved_selection_issue(db, &["selection", "model"]));
        };
        if value
            .get("variant")
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            return Err(saved_selection_issue(db, &["selection", "variant"]));
        }
        let variant = value.get("variant").and_then(|v| v.as_str());
        match crate::models::select_model(&composition.catalog, id)
            .and_then(|base| crate::models::select_variant(&base, variant))
        {
            Ok(selection) => {
                self.model_id = selection.id.clone();
                self.variant = selection.variant.map(|variant| variant.name);
                Ok(Vec::new())
            }
            Err(_) => {
                // Preserve the exact retired choice. A stale global preference
                // must not silently authorize the configured fallback either.
                self.model_id = id.to_string();
                self.variant = variant.map(str::to_string);
                Ok(vec![
                    crate::config::diagnostic::failure(
                        &composition.project.to_string_lossy(),
                        &["selection", "model"],
                        oc_core::queries::ServiceStage::Admission,
                        oc_core::queries::ServiceCode::ModelUnavailable,
                        oc_core::queries::ServiceAction::SelectModel,
                    )
                    .to_string(),
                ])
            }
        }
    }

    /// Apply the persisted primary agent for this generation.
    fn apply_persisted_agent(
        &mut self,
        db: &Db,
        composition: &Composition,
        registry: &mut WorkspaceRegistry,
    ) -> Result<Vec<String>, SpawnIssue> {
        let mut saved_id = None;
        if let Some(raw) = db
            .get_pref(oc_core::queries::PREF_PRIMARY_AGENT)
            .map_err(|error| SpawnIssue {
                category: storage_failure(&error),
                diagnostic: storage_diagnostic(db.root(), &error),
            })?
        {
            let value: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|_| saved_selection_issue(db, &["selection", "agent"]))?;
            let id = value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| saved_selection_issue(db, &["selection", "agent"]))?;
            if value
                .get("generation")
                .and_then(serde_json::Value::as_u64)
                .is_none()
            {
                return Err(saved_selection_issue(db, &["selection", "agent"]));
            }
            saved_id = Some(id.to_string());
        }
        match registry.load_primary(db) {
            Ok(id) => {
                if self.set_agent(composition, &id).is_err() {
                    self.retain_invalid_saved_agent(composition, &id);
                    return Ok(vec![
                        self.selection_issue(composition)
                            .unwrap()
                            .diagnostic
                            .to_string(),
                    ]);
                }
                Ok(Vec::new())
            }
            Err(WorkspaceError::NoPrimaryAgent) if saved_id.is_none() => Ok(Vec::new()),
            Err(WorkspaceError::NoPrimaryAgent) => {
                Err(saved_selection_issue(db, &["selection", "agent"]))
            }
            Err(_) => {
                let id =
                    saved_id.ok_or_else(|| saved_selection_issue(db, &["selection", "agent"]))?;
                self.retain_unavailable_agent(composition, &id);
                Ok(vec![
                    self.selection_issue(composition)
                        .unwrap()
                        .diagnostic
                        .to_string(),
                ])
            }
        }
    }

    fn retain_unavailable_agent(&mut self, composition: &Composition, id: &str) {
        self.agent_id = Some(id.to_string());
        self.agent_prompt = None;
        self.agent_digest = None;
        self.profile_issue = Some(selection_diagnostic(
            composition,
            &["selection", "agent"],
            oc_core::queries::ServiceCode::AgentUnavailable,
            oc_core::queries::ServiceAction::SelectAgent,
        ));
    }

    /// A saved reference to an existing primary with an unresolvable pin is
    /// a model problem. Keep its exact private identity and admitted profile
    /// constraints so an explicit scoped model replacement can repair it.
    fn retain_invalid_saved_agent(&mut self, composition: &Composition, id: &str) {
        if let Some(agent) = composition
            .agents
            .get(id)
            .filter(|agent| agent.primary_capable())
            && let Some(model) = agent.model.as_ref()
        {
            self.agent_id = Some(id.to_string());
            self.agent_prompt = Some(agent.body.clone());
            self.agent_digest = Some(crate::defs::agent_digest(agent));
            self.model_id = model.clone();
            self.variant = agent.variant.clone();
            self.profile_issue = Some(selection_diagnostic(
                composition,
                &["selection", "model"],
                oc_core::queries::ServiceCode::ModelUnavailable,
                oc_core::queries::ServiceAction::SelectModel,
            ));
        } else {
            self.retain_unavailable_agent(composition, id);
        }
    }

    fn selection_issue(&self, composition: &Composition) -> Option<SelectionReadiness> {
        use oc_core::queries::{ServiceAction as Action, ServiceCode as Code};
        let (identity, diagnostic) = if let Some(error) = &self.profile_issue {
            let identity = if error.code == Code::ModelUnavailable {
                self.model_id.as_str()
            } else {
                self.agent_id.as_deref().unwrap_or_default()
            };
            (identity, error.clone())
        } else if let Some(id) = self.agent_id.as_deref().filter(|id| {
            !composition
                .agents
                .get(*id)
                .is_some_and(|agent| agent.primary_capable())
        }) {
            (
                id,
                selection_diagnostic(
                    composition,
                    &["selection", "agent"],
                    Code::AgentUnavailable,
                    Action::SelectAgent,
                ),
            )
        } else if !composition.catalog.models.contains_key(&self.model_id)
            && composition.provider_state.catalog_status == oc_core::queries::ProviderStatus::Ready
        {
            (
                self.model_id.as_str(),
                selection_diagnostic(
                    composition,
                    &["selection", "model"],
                    Code::ModelUnavailable,
                    Action::SelectModel,
                ),
            )
        } else if composition.catalog.models.contains_key(&self.model_id)
            && self.variant.as_deref().is_some_and(|variant| {
                crate::models::select_model(&composition.catalog, &self.model_id)
                    .and_then(|base| crate::models::select_variant(&base, Some(variant)))
                    .is_err()
            })
        {
            (
                self.variant.as_deref().unwrap(),
                selection_diagnostic(
                    composition,
                    &["selection", "variant"],
                    Code::VariantUnavailable,
                    Action::SelectVariant,
                ),
            )
        } else {
            return None;
        };
        let kind = diagnostic
            .field
            .last()
            .map(String::as_str)
            .unwrap_or("model");
        Some(SelectionReadiness {
            requested: selection_identity(kind, identity),
            diagnostic,
        })
    }

    fn admit_selection(&self, composition: &Composition) -> Result<(), CoreError> {
        match self.selection_issue(composition) {
            Some(issue) => Err(CoreError::Diagnostic(issue.diagnostic)),
            None => Ok(()),
        }
    }

    /// Switch the effective agent; a pinned model must resolve exactly.
    fn set_agent(&mut self, composition: &Composition, id: &str) -> Result<(), CoreError> {
        let failed = || {
            CoreError::Diagnostic(crate::config::diagnostic::failure(
                &composition.project.to_string_lossy(),
                &["agent", "entry", "model"],
                oc_core::queries::ServiceStage::Admission,
                oc_core::queries::ServiceCode::InvalidDefinition,
                oc_core::queries::ServiceAction::ReviewConfiguration,
            ))
        };
        let agent = composition.agents.get(id).ok_or_else(failed)?;
        if !agent.primary_capable() {
            return Err(failed());
        }
        if let Some(model) = agent.model.as_deref() {
            // Retain the existing exact bare-ID alias; full profile references
            // share the child/title resolver, including IDs with slashes.
            let (model, variant) = if composition.catalog.models.contains_key(model) {
                (model.to_string(), agent.variant.clone())
            } else if let Some((provider, rest)) = model.split_once('/')
                && provider == composition.catalog.provider
                && !rest.is_empty()
            {
                // A cold/retired exact same-provider profile remains an explicit
                // choice. Execution admission owns availability; no fallback.
                let (id, variant) = rest.split_once('#').map_or((rest, None), |(id, variant)| {
                    (id, Some(variant.to_string()))
                });
                if id.is_empty() {
                    return Err(failed());
                }
                (id.to_string(), agent.variant.clone().or(variant))
            } else {
                let resolved = crate::runtime::resolve_subagent_model(&composition.catalog, model)
                    .map_err(|_| failed())?;
                (resolved.id, agent.variant.clone().or(resolved.variant))
            };
            self.model_id = model;
            self.variant = variant;
        } else if agent.variant.is_some() {
            self.variant = agent.variant.clone();
        }
        self.agent_id = Some(id.to_string());
        self.agent_prompt = Some(agent.body.clone());
        self.agent_digest = Some(crate::defs::agent_digest(agent));
        self.profile_issue = None;
        Ok(())
    }

    /// Catalog plus this effective selection.
    fn snapshot(&self, composition: &Composition) -> CatalogSnapshot {
        let issue = self.selection_issue(composition);
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
                variants: crate::models::ordered_variants(spec)
                    .into_iter()
                    .map(|(name, value)| VariantEntry {
                        name: name.to_string(),
                        disabled: value
                            .get("disabled")
                            .and_then(|flag| flag.as_bool())
                            .unwrap_or(false),
                        reasoning_effort: value
                            .get("reasoningEffort")
                            .and_then(|effort| effort.as_str())
                            .map(str::to_string),
                    })
                    .collect(),
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
            .map(|(color_index, agent)| {
                let mut profile = Self::from_composition(composition);
                let unavailable = profile.set_agent(composition, &agent.id).is_err()
                    || (composition.provider_state.catalog_status
                        == oc_core::queries::ProviderStatus::Ready
                        && profile.selection_issue(composition).is_some());
                AgentEntry {
                    id: agent.id.clone(),
                    description: agent.description.clone(),
                    model: agent.model.as_deref().map(|model| {
                        if unavailable {
                            selection_identity("model", model)
                        } else {
                            model.to_string()
                        }
                    }),
                    variant: agent.variant.as_deref().map(|variant| {
                        if unavailable {
                            selection_identity("variant", variant)
                        } else {
                            variant.to_string()
                        }
                    }),
                    color_index,
                }
            })
            .collect();
        CatalogSnapshot {
            chrome: {
                let mut chrome = composition.tui_chrome.clone();
                chrome.permissions_auto = composition.permission_preference.load(Ordering::SeqCst);
                chrome.provider = Some(composition.provider_state.for_model(
                    &self.model_id,
                    composition.catalog.models.contains_key(&self.model_id),
                ));
                chrome.service_diagnostics.retain(|diagnostic| {
                    diagnostic.kind != oc_core::queries::ServiceKind::Provider
                });
                chrome.service_diagnostics.extend(
                    chrome
                        .provider
                        .as_ref()
                        .and_then(|provider| provider.diagnostic.clone()),
                );
                chrome.selection = issue.clone();
                chrome
                    .service_diagnostics
                    .retain(|d| d.kind != oc_core::queries::ServiceKind::Selection);
                chrome
                    .service_diagnostics
                    .extend(issue.as_ref().map(|s| s.diagnostic.clone()));
                chrome
            },
            auto_accept: if composition.approval_consumer_mode.load(Ordering::SeqCst) == 2 {
                oc_core::queries::AutoAcceptState::Enabled
            } else {
                oc_core::queries::AutoAcceptState::Disabled
            },
            provider: composition.catalog.provider.clone(),
            models,
            model_id: if issue.as_ref().is_some_and(|s| {
                s.diagnostic.code == oc_core::queries::ServiceCode::ModelUnavailable
            }) || (!composition.catalog.models.contains_key(&self.model_id)
                && composition.provider_state.catalog_status
                    == oc_core::queries::ProviderStatus::Ready)
            {
                selection_identity("model", &self.model_id)
            } else {
                self.model_id.clone()
            },
            variant: if !composition.catalog.models.contains_key(&self.model_id)
                || issue
                    .as_ref()
                    .is_some_and(|s| s.diagnostic.field.last().is_some_and(|f| f == "variant"))
            {
                self.variant
                    .as_deref()
                    .map(|variant| selection_identity("variant", variant))
            } else {
                self.variant.clone()
            },
            agents,
            agent_id: if issue
                .as_ref()
                .is_some_and(|s| s.diagnostic.field.last().is_some_and(|f| f == "agent"))
            {
                Some(issue.as_ref().unwrap().requested.clone())
            } else {
                self.agent_id.clone()
            },
            commands: composition.commands.keys().cloned().collect(),
            command_descriptions: composition.command_descriptions.clone(),
        }
    }
}

fn selection_identity(kind: &str, raw: &str) -> String {
    use sha2::Digest as _;
    format!("{kind}-{:x}", sha2::Sha256::digest(raw.as_bytes()))
}

fn selection_diagnostic(
    composition: &Composition,
    field: &[&str],
    code: oc_core::queries::ServiceCode,
    action: oc_core::queries::ServiceAction,
) -> oc_core::queries::ServiceDiagnostic {
    let mut diagnostic = crate::config::diagnostic::failure(
        &composition.project.to_string_lossy(),
        field,
        oc_core::queries::ServiceStage::Admission,
        code,
        action,
    );
    diagnostic.kind = oc_core::queries::ServiceKind::Selection;
    diagnostic
}

/// Build one complete runtime for a composition (no publication yet).
fn build_runtime<'a>(db: &'a Db, composition: &Composition) -> Result<Runtime<'a>, SpawnIssue> {
    let source = composition.project.to_string_lossy();
    let files = crate::files::Files::new(&composition.project, db.root()).map_err(|_| {
        SpawnIssue::new(
            SpawnFailure::Runtime,
            &source,
            &["files"],
            oc_core::queries::ServiceCode::TrustRefused,
        )
    })?;
    let shell = crate::shell::Shell::new(&composition.project).map_err(|_| {
        SpawnIssue::new(
            SpawnFailure::Runtime,
            &source,
            &["shell"],
            oc_core::queries::ServiceCode::RuntimeFailed,
        )
    })?;
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
    .map_err(|error| runtime_issue(&source, &["runtime"], &error))?;
    runtime.set_mcp_activation(composition.mcp_activation.clone());
    runtime
        .publish_provider_state(composition.provider_state.clone())
        .map_err(|error| runtime_issue(&source, &["provider"], &error))?;
    runtime
        .publish_dcp_protection(composition.dcp_protected.clone())
        .map_err(|error| runtime_issue(&source, &["dcp"], &error))?;
    runtime
        .publish_subagents(subagent_catalog(composition))
        .map_err(|error| runtime_issue(&source, &["agent"], &error))?;
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
    ProviderCatalog(
        Result<crate::discovery::DiscoveryOutcome, oc_core::queries::ServiceDiagnostic>,
    ),
    PickerOpen {
        path: String,
        session: SessionId,
        old_deck: oc_core::queries::TabDeckSnapshot,
        ack: oneshot::Sender<Result<oc_core::queries::SessionPickerOpen, CoreError>>,
    },
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
) -> Result<(), oc_core::queries::ServiceDiagnostic> {
    let mut runtime = match build_runtime(&db, &composition) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    let mut effective = Effective::from_composition(&composition);
    effective.legacy_epoch = match selection::legacy_epoch(&db, &composition) {
        Ok(epoch) => epoch,
        Err(error) => {
            let mut issue = saved_selection_issue(&db, &["selection"]);
            if let CoreError::Diagnostic(diagnostic) = error {
                issue.diagnostic = diagnostic;
            }
            let _ = ready.send(Err(issue));
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
    let diagnostics =
        match effective
            .apply_persisted_model(&db, &composition)
            .and_then(|mut diagnostics| {
                diagnostics.extend(effective.apply_persisted_agent(
                    &db,
                    &composition,
                    &mut registry,
                )?);
                Ok(diagnostics)
            }) {
            Ok(diagnostics) => diagnostics,
            Err(issue) => {
                let _ = ready.send(Err(issue));
                return Ok(());
            }
        };
    if let Err(error) = publish_workspace(&runtime, &composition, &effective) {
        let _ = ready.send(Err(runtime_issue(
            runtime.location(),
            &["workspace"],
            &error,
        )));
        return Ok(());
    }
    runtime.set_approval_events(&events);
    if let Err(error) = runtime.start_mcp() {
        let mut issue = runtime_issue(runtime.location(), &["mcp"], &error);
        if composition
            .generation
            .mcp
            .values()
            .filter(|entry| entry.enabled)
            .count()
            > crate::runtime::MAX_MCP_SERVERS
        {
            issue.diagnostic.code = oc_core::queries::ServiceCode::CapacityExceeded;
            issue.diagnostic.stage = oc_core::queries::ServiceStage::Admission;
            issue.diagnostic.action = oc_core::queries::ServiceAction::ReduceCapacity;
        }
        let _ = ready.send(Err(issue));
        runtime
            .shutdown_mcp()
            .await
            .map_err(|error| runtime_issue(runtime.location(), &["mcp"], &error).diagnostic)?;
        return Ok(());
    }
    let mut provider_work = provider_catalog::ProviderWork::start(&composition);
    if ready.send(Ok(diagnostics)).is_err() {
        let provider_stop = provider_work.stop().await;
        runtime
            .shutdown_mcp()
            .await
            .map_err(|error| runtime_issue(runtime.location(), &["mcp"], &error).diagnostic)?;
        provider_stop?;
        return Ok(());
    }
    // This worker, not any one Location runtime, owns unresolved remote calls.
    // No endpoint identity or credential leaves the runtime/application boundary.
    let mut remote_retry_quarantined = false;
    // Runtime publication ids restart at 1 after a Location rebuild. A
    // worker-wide epoch distinguishes even a return to the same Location.
    let location_epoch = Arc::new(AtomicU64::new(1));
    let suggestion_queue = Arc::new(Mutex::new(SuggestionQueue::default()));
    // Title work belongs to the application owner, not a single invocation of
    // the command loop: a rejected switch resumes that same Location.
    let (title_tx, mut title_rx) = mpsc::channel::<AutomaticTitleResult>(32);
    let title_work = Mutex::new(AutomaticTitles::default());
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
            &title_tx,
            &mut title_rx,
            &title_work,
            &mut provider_work,
        )
        .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let provider_stop = provider_work.stop().await;
                let title_stop = stop_automatic_titles(&title_work).await;
                runtime.shutdown_mcp().await.map_err(|error| {
                    runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                })?;
                title_stop?;
                provider_stop?;
                return Err(error);
            }
        };
        match outcome {
            WorkerOutcome::ProviderCatalog(outcome) => {
                let outcome = match outcome {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let title_stop = stop_automatic_titles(&title_work).await;
                        runtime.shutdown_mcp().await.map_err(|error| {
                            runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                        })?;
                        title_stop?;
                        return Err(error);
                    }
                };
                composition.accept_provider_catalog(outcome);
                runtime
                    .publish_provider_state(composition.provider_state.clone())
                    .map_err(|error| {
                        runtime_issue(runtime.location(), &["provider"], &error).diagnostic
                    })?;
                let _ = events.send(CoreEvent::ProviderChanged);
            }
            WorkerOutcome::PickerOpen {
                path,
                session,
                old_deck,
                ack,
            } => {
                if path == runtime.location() {
                    let receipt = prepare_picker_open(
                        &db,
                        &runtime,
                        &composition,
                        &mut effective,
                        &mut registry,
                        &mut sessions,
                        &mut home_choices,
                        &location_epoch,
                        &suggestion_queue,
                        &title_work,
                        session,
                        old_deck,
                    );
                    let _ = ack.send(receipt);
                    continue;
                }
                let next = switch_target(
                    &db,
                    &path,
                    &mut sessions,
                    composition.parent_env.clone(),
                    true,
                )
                .await;
                let (next, next_composition, mut next_effective, mut next_registry, _, _) =
                    match next {
                        Ok(next) => next,
                        Err(issue) => {
                            let _ = ack.send(Err(CoreError::LocationSwitch {
                                category: match issue.category {
                                    SpawnFailure::Configuration
                                    | SpawnFailure::MissingCredential => {
                                        LocationSwitchFailure::Configuration
                                    }
                                    SpawnFailure::Storage => LocationSwitchFailure::Storage,
                                    _ => LocationSwitchFailure::Runtime,
                                },
                                detail: issue.to_string(),
                                diagnostic: Some(issue.diagnostic),
                            }));
                            continue;
                        }
                    };
                let mode = composition.approval_consumer_mode.load(Ordering::SeqCst);
                next_composition
                    .approval_consumer_mode
                    .store(mode, Ordering::SeqCst);
                if mode > 0 {
                    next.register_approval_consumer(mode == 2);
                }
                let receipt = prepare_picker_open(
                    &db,
                    &next,
                    &next_composition,
                    &mut next_effective,
                    &mut next_registry,
                    &mut sessions,
                    &mut home_choices,
                    &location_epoch,
                    &suggestion_queue,
                    &title_work,
                    session.clone(),
                    old_deck,
                );
                let receipt = match receipt {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        next.shutdown_mcp().await.map_err(|error| {
                            runtime_issue(next.location(), &["mcp"], &error).diagnostic
                        })?;
                        let _ = ack.send(Err(error));
                        continue;
                    }
                };
                // The route and both decks have committed. Cleanup cannot turn
                // that accepted route back into a refusal or an old view.
                let provider_stop = provider_work.stop().await;
                let mcp_stop = runtime.shutdown_mcp().await.map_err(|error| {
                    runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                });
                while let Ok(result) = title_rx.try_recv() {
                    commit_automatic_title(&db, &events, &title_work, result);
                }
                let title_stop = stop_automatic_titles(&title_work).await;
                mcp_stop?;
                title_stop?;
                provider_stop?;
                remote_retry_quarantined |= runtime.remote_retry_quarantined();
                if remote_retry_quarantined {
                    next.quarantine_remote_retries();
                }
                next.set_approval_events(&events);
                next.start_mcp()
                    .map_err(|error| runtime_issue(next.location(), &["mcp"], &error).diagnostic)?;
                let mut next = next;
                next.shell_jobs = runtime.shell_jobs.clone();
                runtime = next;
                composition = next_composition;
                effective = next_effective;
                registry = next_registry;
                sessions.insert(path, session.0);
                location_epoch.fetch_add(1, Ordering::SeqCst);
                let _ = ack.send(Ok(receipt));
            }
            WorkerOutcome::Stop => {
                runtime.shell_jobs.shutdown().await.map_err(|_| {
                    runtime_issue(
                        runtime.location(),
                        &["shell", "cleanup"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                })?;
                runtime.shell_jobs.deliver(&events).map_err(|_| {
                    runtime_issue(
                        runtime.location(),
                        &["shell", "delivery"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                })?;
                let provider_stop = provider_work.stop().await;
                let title_stop =
                    drain_automatic_titles(&db, &events, &title_work, &mut title_rx).await;
                runtime.shutdown_mcp().await.map_err(|error| {
                    runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                })?;
                title_stop?;
                provider_stop?;
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
                        if matches!(ack, SwitchAck::Reload(_))
                            && next_composition.provider_state.catalog_status
                                == oc_core::queries::ProviderStatus::Failed
                        {
                            let error = next_composition
                                .provider_state
                                .for_model(&next_composition.model_id, false)
                                .diagnostic
                                .expect("failed refresh has a cause");
                            next.shutdown_mcp().await.map_err(|error| {
                                runtime_issue(next.location(), &["mcp"], &error).diagnostic
                            })?;
                            if next_composition.catalog.provider == composition.catalog.provider {
                                // Keep the complete healthy catalog/policy/credentials. Only the
                                // transient latest-attempt fact changes, as with MCP status.
                                let same_binding = composition
                                    .provider
                                    .same_request_binding(&next_composition.provider)
                                    .map_err(|_| {
                                        crate::config::diagnostic::failure(
                                            "native provider binding",
                                            &["provider", "options", "headers"],
                                            oc_core::queries::ServiceStage::Config,
                                            oc_core::queries::ServiceCode::InvalidHeader,
                                            oc_core::queries::ServiceAction::ReviewConfiguration,
                                        )
                                    })?;
                                composition.provider_state.retain_failed_attempt(
                                    &next_composition.provider_state,
                                    same_binding,
                                );
                                runtime
                                    .publish_provider_state(composition.provider_state.clone())
                                    .map_err(|error| {
                                        runtime_issue(runtime.location(), &["provider"], &error)
                                            .diagnostic
                                    })?;
                                let _ = events.send(CoreEvent::ProviderChanged);
                            }
                            if let SwitchAck::Reload(ack) = ack {
                                let _ = ack.send(Err(CoreError::ProviderUnavailable(error)));
                            }
                            continue;
                        }
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
                                next.shutdown_mcp().await.map_err(|error| {
                                    runtime_issue(next.location(), &["mcp"], &error).diagnostic
                                })?;
                                if let SwitchAck::Reload(ack) = ack {
                                    let _ = ack.send(Err(error));
                                }
                                continue;
                            }
                        }
                        next_composition.approval_consumer_mode.store(
                            composition.approval_consumer_mode.load(Ordering::SeqCst),
                            Ordering::SeqCst,
                        );
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
                                    next.shutdown_mcp().await.map_err(|error| {
                                        runtime_issue(next.location(), &["mcp"], &error).diagnostic
                                    })?;
                                    if let SwitchAck::Home(ack) = ack {
                                        let _ = ack.send(Err(CoreError::LocationSwitch {
                                            category: LocationSwitchFailure::Storage,
                                            detail: error.to_string(),
                                            diagnostic: Some(SpawnIssue::new(SpawnFailure::Storage, &db.root().to_string_lossy(), &["selection"], oc_core::queries::ServiceCode::StorageUnavailable).diagnostic),
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
                        let provider_stop = provider_work.stop().await;
                        let mcp_stop = runtime.shutdown_mcp().await.map_err(|error| {
                            runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                        });
                        // A title that actually completed while target
                        // validation was in progress still belongs to the old
                        // accepted prompt. Commit it before retiring that
                        // Location; only unfinished work is cancelled.
                        while let Ok(result) = title_rx.try_recv() {
                            commit_automatic_title(&db, &events, &title_work, result);
                        }
                        // All target validation has succeeded. Retire and join
                        // old provider work before publishing the new Location;
                        // queued results lose their pending stamp as well.
                        let title_stop = stop_automatic_titles(&title_work).await;
                        mcp_stop?;
                        title_stop?;
                        provider_stop?;
                        remote_retry_quarantined |= runtime.remote_retry_quarantined();
                        if remote_retry_quarantined {
                            next.quarantine_remote_retries();
                        }
                        next.set_approval_events(&events);
                        next.start_mcp().map_err(|error| {
                            runtime_issue(next.location(), &["mcp"], &error).diagnostic
                        })?;
                        let mode = composition.approval_consumer_mode.load(Ordering::SeqCst);
                        if mode > 0 {
                            next.register_approval_consumer(mode == 2);
                        }
                        let mut next = next;
                        next.shell_jobs = runtime.shell_jobs.clone();
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
                            detail: issue.to_string(),
                            diagnostic: Some(issue.diagnostic),
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
        diagnostic: Some(
            SpawnIssue::new(
                SpawnFailure::Storage,
                &db.root().to_string_lossy(),
                &["tab_deck"],
                oc_core::queries::ServiceCode::StorageUnavailable,
            )
            .diagnostic,
        ),
    };
    let selection_error = || CoreError::LocationSwitch {
        category: LocationSwitchFailure::Configuration,
        detail: "retained session selection unavailable in reloaded configuration".into(),
        diagnostic: Some(crate::config::diagnostic::failure(
            &composition.project.to_string_lossy(),
            &["selection"],
            oc_core::queries::ServiceStage::Admission,
            oc_core::queries::ServiceCode::ModelUnavailable,
            oc_core::queries::ServiceAction::SelectModel,
        )),
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
            if selected.selection_issue(composition).is_some() {
                return Err(selection_error());
            }
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
    let composition = composition::load_with_env_diagnostic(Path::new(path), env)
        .await
        .map_err(SpawnIssue::configuration)?;
    let runtime = build_runtime(db, &composition)?;
    let mut effective = Effective::from_composition(&composition);
    effective.legacy_epoch = selection::legacy_epoch(db, &composition).map_err(|_| {
        SpawnIssue::new(
            SpawnFailure::Storage,
            &db.root().to_string_lossy(),
            &["selection"],
            oc_core::queries::ServiceCode::StorageUnavailable,
        )
    })?;
    let mut registry = WorkspaceRegistry::bind(
        runtime.generation_id(),
        runtime.location(),
        &composition.generation,
        workspace_agents(&composition),
        skill_metas(&composition),
    );
    let mut notes = composition.diagnostics.clone();
    notes.extend(effective.apply_persisted_model(db, &composition)?);
    notes.extend(effective.apply_persisted_agent(db, &composition, &mut registry)?);
    publish_workspace(&runtime, &composition, &effective)
        .map_err(|error| runtime_issue(path, &["workspace"], &error))?;
    // Home publishes only the target generation. Attached switches retain
    // their existing Location-bound reopen/create behavior.
    let location = runtime.location().to_string();
    let session = if home {
        None
    } else {
        Some(match sessions.get(&location).cloned() {
            Some(id) => {
                runtime.open_session(&id).map_err(|error| {
                    runtime_issue(&db.root().to_string_lossy(), &["session"], &error)
                })?;
                SessionId(id)
            }
            None => {
                let id = format!("s-loc-{}", nanos());
                runtime.create_session(&id).map_err(|error| {
                    runtime_issue(&db.root().to_string_lossy(), &["session"], &error)
                })?;
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

fn runtime_error(error: RuntimeError) -> CoreError {
    match error {
        RuntimeError::ProviderUnavailable(diagnostic) => CoreError::ProviderUnavailable(diagnostic),
        other => {
            CoreError::Diagnostic(runtime_issue("native runtime", &["request"], &other).diagnostic)
        }
    }
}

fn query_storage_error(db: &Db, error: StorageError) -> CoreError {
    let mut diagnostic = storage_diagnostic(db.root(), &error);
    diagnostic.stage = oc_core::queries::ServiceStage::Query;
    diagnostic.field = vec!["database".into()];
    CoreError::Diagnostic(diagnostic)
}

fn runtime_query_error(db: &Db, error: RuntimeError) -> CoreError {
    if let RuntimeError::ProviderUnavailable(diagnostic) = error {
        return CoreError::ProviderUnavailable(diagnostic);
    }
    let mut issue = runtime_issue(&db.root().to_string_lossy(), &["session"], &error);
    if !matches!(
        error,
        RuntimeError::LocationMismatch { .. } | RuntimeError::PermissionDenied { .. }
    ) {
        issue.diagnostic.stage = oc_core::queries::ServiceStage::Query;
    }
    if matches!(error, RuntimeError::SessionNotFound) {
        issue.diagnostic.code = oc_core::queries::ServiceCode::SourceUnavailable;
    }
    CoreError::Diagnostic(issue.diagnostic)
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
        FileToolError::Cancelled => "file suggestions cancelled",
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

/// Resolve only the current catalog's matching public display name. Durable
/// notices carry refs alone; a rename/removal takes effect on the next query.
fn project_model_switch(
    mut notice: ModelSwitchNotice,
    catalog: &crate::models::ModelCatalog,
) -> ModelSwitchNotice {
    notice.display_name = (notice.current.provider == catalog.provider)
        .then(|| catalog.models.get(&notice.current.id))
        .flatten()
        .and_then(|entry| entry.get("name"))
        .and_then(serde_json::Value::as_str)
        .map(|name| name.chars().filter(|c| !c.is_control()).take(128).collect());
    notice
}

/// Prepare the selected root and atomically save the old and target decks.
/// All fallible target reads precede the commit and Location publication.
#[allow(clippy::too_many_arguments)]
fn prepare_picker_open(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    sessions: &mut BTreeMap<String, String>,
    home_choices: &mut BTreeMap<String, Effective>,
    location_epoch: &Arc<AtomicU64>,
    suggestion_queue: &Arc<Mutex<SuggestionQueue>>,
    title_work: &Mutex<AutomaticTitles>,
    session: SessionId,
    old_deck: oc_core::queries::TabDeckSnapshot,
) -> Result<oc_core::queries::SessionPickerOpen, CoreError> {
    runtime
        .open_session(&session.0)
        .map_err(|_| CoreError::SessionNotFound)?;
    if db
        .session_meta(&session.0)
        .map_err(app_error)?
        .parent_id
        .is_some()
    {
        return Err(CoreError::SessionNotFound);
    }
    let selected = selection::apply(
        db,
        composition,
        effective,
        &session.0,
        false,
        oc_core::queries::SessionSelectionAction::Current,
    )?;
    // Current is a read projection: existing retired selections must remain
    // visible and editable without rewriting preferences. Execution admission
    // belongs to for_turn; the target workspace itself is already validated.
    // Reuse the exact bounded owner history projection, including immutable
    // message identities, conversation branches and turn/tool metadata.
    let (ack, mut result) = oneshot::channel();
    query(
        db,
        runtime,
        composition,
        effective,
        registry,
        sessions,
        home_choices,
        location_epoch,
        suggestion_queue,
        title_work,
        InboxMsg::History {
            session: session.clone(),
            before_seq: None,
            after_seq: None,
            limit: HISTORY_PAGE_LIMIT,
            ack,
        },
    );
    let page = result
        .try_recv()
        .map_err(|_| app_error("picker history unavailable"))??;
    let mut target = if old_deck.location == runtime.location() {
        old_deck.clone()
    } else {
        tab_deck::load(db, runtime)?
    };
    if target.projected() {
        return Err(CoreError::StoredTabDeck);
    }
    if !target.sessions.contains(&session) {
        if target.sessions.len() >= crate::storage::MAX_TABS {
            return Err(CoreError::InvalidTabDeck);
        }
        target.sessions.push(session.clone());
    }
    target.active = Some(session.clone());
    let mut decks = if old_deck.location == runtime.location() {
        vec![target]
    } else {
        vec![old_deck, target]
    };
    db.save_picker_decks(&mut decks)?;
    sessions.insert(runtime.location().to_owned(), session.0.clone());
    let deck = decks.last().expect("target").clone();
    let previous_deck = decks.first().expect("old").clone();
    Ok(oc_core::queries::SessionPickerOpen {
        session,
        location: runtime.location().into(),
        catalog: selected.snapshot(composition),
        page,
        deck,
        previous_deck,
    })
}

fn picker_target(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    session: &SessionId,
    search: &str,
    all_projects: bool,
) -> Result<String, CoreError> {
    if runtime.turn_active() {
        return Err(CoreError::TurnBusy);
    }
    let scope = match db
        .get_pref("tui.session-list.allProjects")
        .map_err(app_error)?
        .as_deref()
    {
        Some("true") => true,
        Some("false") => false,
        _ => composition.tui_chrome.sessions_all_projects,
    };
    if scope != all_projects {
        return Err(app_error("Sessions scope changed; reopen the picker"));
    }
    let location = db
        .session_list(search, (!scope).then_some(runtime.location()))
        .map_err(app_error)?
        .into_iter()
        .find(|entry| &entry.id == session)
        .and_then(|entry| entry.directory)
        .ok_or(CoreError::SessionNotFound)?;
    if db.session_family_running(&session.0).map_err(app_error)? {
        return Err(CoreError::TurnBusy);
    }
    Ok(location)
}

#[allow(clippy::too_many_arguments)]
/// Handle one owner-only query or action; streaming admits read snapshots.
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
    title_work: &Mutex<AutomaticTitles>,
    message: InboxMsg,
) {
    match message {
        InboxMsg::McpLookup { query, cancel, ack } => {
            match mcp_lookup::admit(db, runtime, composition, effective, &query) {
                Ok(()) => runtime.enqueue_mcp_lookup(query, cancel, ack),
                Err(error) => {
                    let _ = ack.send(Err(error));
                }
            }
        }
        InboxMsg::McpStatus { ack } => {
            let _ = ack.send(Ok(runtime.mcp_status()));
        }
        InboxMsg::McpControl { control, ack } => {
            match runtime.mcp_control_server(&control) {
                Ok(_) => {}
                Err(error) => {
                    let _ = ack.send(Err(app_error(error)));
                    return;
                }
            }
            runtime.enqueue_mcp_control(control, ack);
        }
        InboxMsg::PendingApprovals { ack } => {
            let _ = ack.send(Ok(runtime.pending_approvals()));
        }
        InboxMsg::ReplyApproval { reply, ack } => {
            let _ = ack.send(runtime.reply_approval(reply).map_err(app_error));
        }
        InboxMsg::RegisterApprovalConsumer {
            auto_once,
            persist,
            ack,
        } => {
            if persist
                && let Err(error) = crate::composition::save_permission_mode(composition, auto_once)
            {
                let _ = ack.send(Err(app_error(error.to_string())));
                return;
            }
            runtime.register_approval_consumer(auto_once);
            if persist {
                composition
                    .permission_preference
                    .store(auto_once, Ordering::SeqCst);
            }
            composition
                .approval_consumer_mode
                .store(if auto_once { 2 } else { 1 }, Ordering::SeqCst);
            if auto_once {
                for request in runtime.pending_approvals() {
                    if let Err(error) = runtime.reply_approval(oc_core::approval::ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: oc_core::approval::ApprovalDecision::Once,
                    }) {
                        let _ = ack.send(Err(app_error(error)));
                        return;
                    }
                }
            }
            let _ = ack.send(Ok(()));
        }
        InboxMsg::OpenPickerSession { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::ChangeConversation {
            session,
            action,
            ack,
        } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                runtime
                    .open_session(&session.0)
                    .map_err(|_| CoreError::SessionNotFound)?;
                let snapshot = db.change_conversation(&session.0, action)?;
                runtime.conversation_changed(&session.0);
                Ok(snapshot)
            })();
            let _ = ack.send(result);
        }
        InboxMsg::ForkSession {
            source,
            before,
            ack,
        } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                runtime
                    .open_session(&source.0)
                    .map_err(|_| CoreError::SessionNotFound)?;
                let choice = selection::fork_choice(db, composition, effective, &source.0)?;
                db.fork_session(
                    &source.0,
                    &before.0,
                    runtime.location(),
                    &composition.catalog.provider,
                    &choice,
                )
            })();
            let _ = ack.send(result);
        }
        InboxMsg::Create { id, ack } => {
            let result = runtime
                .create_session(&id.0)
                .map_err(|error| runtime_query_error(db, error));
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
                if runtime.turn_active()
                    || db.session_family_running(&session.0).map_err(app_error)?
                {
                    return Err(CoreError::TurnBusy);
                }
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
        InboxMsg::DeleteSession { session, ack } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                runtime
                    .open_session(&session.0)
                    .map_err(|_| CoreError::SessionNotFound)?;
                if db
                    .session_meta(&session.0)
                    .map_err(app_error)?
                    .parent_id
                    .is_some()
                {
                    return Err(CoreError::SessionNotFound);
                }
                if tab_deck::load(db, runtime)?.projected() {
                    return Err(CoreError::StoredTabDeck);
                }
                if db.session_family_running(&session.0).map_err(app_error)? {
                    return Err(CoreError::TurnBusy);
                }
                let accepted = db
                    .delete_root_family(&session.0, runtime.location())
                    .map_err(app_error)?;
                title_work
                    .lock()
                    .expect("title work mutex")
                    .cancel(&session.0);
                runtime.conversation_changed(&session.0);
                sessions.retain(|_, id| id != &session.0);
                // No fallible read after commit: success carries the prepared
                // deck and the exact token written in the deletion transaction.
                Ok(accepted)
            })();
            let _ = ack.send(result);
        }
        InboxMsg::CancelTitle { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::PickerSessionAction {
            session,
            search,
            all_projects,
            action,
            ack,
        } => {
            let result = (|| {
                if runtime.turn_active() {
                    return Err(CoreError::TurnBusy);
                }
                let scope = match db
                    .get_pref("tui.session-list.allProjects")
                    .map_err(app_error)?
                    .as_deref()
                {
                    Some("true") => true,
                    Some("false") => false,
                    _ => composition.tui_chrome.sessions_all_projects,
                };
                if scope != all_projects {
                    return Err(app_error("Sessions scope changed; reopen the picker"));
                }
                let listed = db
                    .session_list(&search, (!scope).then_some(runtime.location()))
                    .map_err(app_error)?;
                let location = listed
                    .into_iter()
                    .find(|entry| entry.id == session)
                    .and_then(|entry| entry.directory)
                    .ok_or(CoreError::SessionNotFound)?;
                if db.session_family_running(&session.0).map_err(app_error)? {
                    return Err(CoreError::TurnBusy);
                }
                match action {
                    oc_core::queries::SessionPickerAction::Rename(title) => {
                        let title = normalized_session_title(&title)
                            .ok_or_else(|| app_error("invalid session title"))?;
                        db.rename_root_session(&session.0, title)
                            .map_err(app_error)?;
                        Ok(oc_core::queries::SessionPickerResult::Renamed)
                    }
                    oc_core::queries::SessionPickerAction::Delete => {
                        let accepted = db
                            .delete_root_family(&session.0, &location)
                            .map_err(app_error)?;
                        title_work
                            .lock()
                            .expect("title work mutex")
                            .cancel(&session.0);
                        runtime.conversation_changed(&session.0);
                        sessions.retain(|_, id| id != &session.0);
                        Ok(oc_core::queries::SessionPickerResult::Deleted(accepted))
                    }
                }
            })();
            let _ = ack.send(result);
        }
        InboxMsg::List { ack } => {
            let _ = ack.send(
                db.list_sessions()
                    .map(|ids| ids.into_iter().map(SessionId).collect())
                    .map_err(|error| query_storage_error(db, error)),
            );
        }
        InboxMsg::SessionList {
            search,
            all_projects,
            ack,
        } => {
            let _ = ack.send(
                db.session_list(&search, (!all_projects).then_some(runtime.location()))
                    .map_err(|error| query_storage_error(db, error)),
            );
        }
        InboxMsg::SessionPickerContext { all_projects, ack } => {
            let result = (|| {
                if let Some(value) = all_projects {
                    db.set_pref(
                        "tui.session-list.allProjects",
                        if value { "true" } else { "false" },
                    )
                    .map_err(|error| query_storage_error(db, error))?;
                }
                let all_projects = match db
                    .get_pref("tui.session-list.allProjects")
                    .map_err(|error| query_storage_error(db, error))?
                    .as_deref()
                {
                    Some("true") => true,
                    Some("false") => false,
                    _ => composition.tui_chrome.sessions_all_projects,
                };
                let canonical =
                    crate::storage::session_project_root(std::path::Path::new(runtime.location()))
                        .or_else(|| std::fs::canonicalize(runtime.location()).ok());
                Ok(oc_core::queries::SessionPickerContext {
                    all_projects,
                    project_name: canonical
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned()),
                    canonical: canonical.map(|path| path.to_string_lossy().into_owned()),
                })
            })();
            let _ = ack.send(result);
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
                    .map_err(|error| query_storage_error(db, error)),
                Err(RuntimeError::SessionNotFound) => match db.session_meta(&id.0) {
                    Err(StorageError::SessionNotFound) => Ok(SessionProbe::Absent),
                    Ok(_) => Err(CoreError::Diagnostic(
                        saved_selection_issue(db, &["session", "binding"]).diagnostic,
                    )),
                    Err(error) => Err(query_storage_error(db, error)),
                },
                Err(error) => Err(runtime_query_error(db, error)),
            };
            let _ = ack.send(result);
        }
        InboxMsg::TabDeck { ack } => {
            let _ = ack.send(tab_deck::load(db, runtime).map_err(|error| match error {
                CoreError::TabDeckStorage => {
                    let mut issue = saved_selection_issue(db, &["tab_deck"]);
                    issue.diagnostic.code = oc_core::queries::ServiceCode::StorageUnavailable;
                    CoreError::Diagnostic(issue.diagnostic)
                }
                other => other,
            }));
        }
        InboxMsg::SaveTabDeck { deck, ack } => {
            let result = tab_deck::save(db, runtime, &deck).map_err(|error| match error {
                CoreError::TabDeckStorage => CoreError::Diagnostic(
                    SpawnIssue::new(
                        SpawnFailure::Storage,
                        &db.root().to_string_lossy(),
                        &["tab_deck"],
                        oc_core::queries::ServiceCode::StorageUnavailable,
                    )
                    .diagnostic,
                ),
                other => other,
            });
            let _ = ack.send(result);
        }
        InboxMsg::Read { session, ack } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(|error| runtime_query_error(db, error))
                .and_then(|()| {
                    db.conversation_history_full(&session.0)
                        .map_err(|error| query_storage_error(db, error))
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
                runtime
                    .open_session(&session.0)
                    .map_err(|error| runtime_query_error(db, error))?;
                let (min, max) = db
                    .history_bounds(&session.0)
                    .map_err(|error| query_storage_error(db, error))?;
                let total = db
                    .history_len(&session.0)
                    .map_err(|error| query_storage_error(db, error))?;
                let limit = limit.min(HISTORY_PAGE_LIMIT);
                let (mut page, ascending) = match after_seq {
                    Some(after) => (
                        db.read_history_after_typed(&session.0, limit, after)
                            .map_err(|error| query_storage_error(db, error))?,
                        true,
                    ),
                    None => (
                        db.read_history_page_typed(&session.0, limit, before_seq)
                            .map_err(|error| query_storage_error(db, error))?,
                        false,
                    ),
                };
                let has_newer = if ascending {
                    matches!((page.last(), max), (Some(row), Some(max)) if row.seq < max)
                } else {
                    matches!((page.first(), max), (Some(row), Some(max)) if row.seq < max)
                };
                let has_older = if ascending {
                    matches!((page.first(), min), (Some(row), Some(min)) if row.seq > min)
                } else {
                    matches!((page.last(), min), (Some(row), Some(min)) if row.seq > min)
                };
                if !ascending {
                    page.reverse();
                }
                let rows = page
                    .into_iter()
                    .map(
                        |crate::storage::HistoryPageRow {
                             id,
                             seq,
                             role,
                             text,
                         }| {
                            let model_switch = if role == "model_switch" {
                                let notice = serde_json::from_str(&text).map_err(|_| {
                                    CoreError::Diagnostic(
                                        saved_selection_issue(db, &["history", "model"]).diagnostic,
                                    )
                                })?;
                                Some(project_model_switch(notice, &composition.catalog))
                            } else {
                                None
                            };
                            Ok(HistoryMessage {
                                id,
                                turn: if model_switch.is_some() {
                                    None
                                } else {
                                    db.history_turn(&session.0, seq)
                                        .map_err(|error| query_storage_error(db, error))?
                                        .or_else(|| {
                                            (role == "assistant").then(|| {
                                                oc_core::queries::HistoryTurn {
                                                    legacy_text_only: true,
                                                    ..Default::default()
                                                }
                                            })
                                        })
                                },
                                model_switch,
                                seq,
                                role: if role == "user" {
                                    Role::User
                                } else {
                                    Role::Assistant
                                },
                                text: if role == "model_switch" {
                                    String::new()
                                } else {
                                    text
                                },
                            })
                        },
                    )
                    .collect::<Result<Vec<_>, CoreError>>()?;
                Ok(HistoryPage {
                    reverted: db
                        .reverted_conversation(&session.0)
                        .map_err(|error| query_storage_error(db, error))?,
                    parent_id: db
                        .session_meta(&session.0)
                        .map_err(|error| query_storage_error(db, error))?
                        .parent_id,
                    title: db
                        .session_meta(&session.0)
                        .map_err(|error| query_storage_error(db, error))?
                        .title,
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
                runtime
                    .open_session(&session.0)
                    .map_err(|error| runtime_query_error(db, error))?;
                let total = db
                    .tool_ops_len(&session.0)
                    .map_err(|error| query_storage_error(db, error))?;
                let page = db
                    .list_tool_ops_page(&session.0, limit.min(TOOL_OPS_PAGE_LIMIT), before_rowid)
                    .map_err(|error| query_storage_error(db, error))?;
                let has_older = match (
                    page.last(),
                    db.tool_ops_bounds(&session.0)
                        .map_err(|error| query_storage_error(db, error))?
                        .0,
                ) {
                    (Some(row), Some(min)) => row.rowid > min,
                    _ => false,
                };
                let rows = page
                    .into_iter()
                    .map(|row| ToolOpView {
                        dcp_topic: row.dcp_topic,
                        dcp: row.dcp,
                        patch_effects: row.patch_effects,
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
        InboxMsg::DcpSummary {
            session,
            op,
            block_index,
            offset,
            limit,
            ack,
        } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(|error| runtime_query_error(db, error))
                .and_then(|_| {
                    db.dcp_summary_page(&session.0, &op, block_index, offset, limit)
                        .map_err(|error| query_storage_error(db, error))
                });
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
                .map_err(|error| runtime_query_error(db, error))
                .and_then(|_| {
                    db.read_session_tool_output(&session.0, &op, offset, limit)
                        .map(
                            |(text, total_bytes, next_offset)| oc_core::queries::ToolOutputPage {
                                text,
                                total_bytes,
                                next_offset,
                            },
                        )
                        .map_err(|error| query_storage_error(db, error))
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
                runtime
                    .open_session(&session.0)
                    .map_err(|error| runtime_query_error(db, error))?;
                selection::apply(db, composition, effective, &session.0, home, action)
                    .map(|selected| selected.snapshot(composition))
            })();
            let _ = ack.send(result);
        }
        InboxMsg::HomeSelection { action, ack } => {
            let result = (|| {
                // Preparing a sessionless Home choice does not select/open a runtime
                // session or mutate the immutable configuration of the active turn.
                if runtime.turn_active()
                    && !matches!(action, oc_core::queries::SessionSelectionAction::New(_))
                {
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
                let mut selected = effective.clone();
                selected.set_agent(composition, &id)?;
                selected.admit_selection(composition)?;
                registry
                    .select_primary(&id, runtime.generation_id(), db)
                    .map_err(|error| app_error(error.to_string()))?;
                *effective = selected;
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
                runtime
                    .open_session(&session.0)
                    .map_err(|error| runtime_query_error(db, error))?;
                let accounting = db
                    .dcp_accounting(&session.0)
                    .map_err(|error| query_storage_error(db, error))?;
                let blocks = db
                    .dcp_block_count(&session.0)
                    .map_err(|error| query_storage_error(db, error))?;
                let turns_since_compress = runtime
                    .dcp_turn_state(&session.0)
                    .map(|state| state.turns_since_compress)
                    .unwrap_or(0);
                // Estimate from the active projection, not the archive: the
                // DCP panel must not materialise pruned/covered history.
                let after_seq = db
                    .prune_bound(&session.0)
                    .map_err(|error| query_storage_error(db, error))?
                    .map(|(_, seq)| seq)
                    .unwrap_or(0)
                    .max(
                        db.session_checkpoint(&session.0)
                            .map_err(|error| query_storage_error(db, error))?
                            .map(|(seq, _)| seq)
                            .unwrap_or(0),
                    );
                let active = db
                    .active_history(
                        &session.0,
                        after_seq,
                        crate::runtime::ACTIVE_CONTEXT_BYTES_CAP,
                    )
                    .map_err(|error| query_storage_error(db, error))?;
                let estimated_tokens = if active.overflow {
                    None
                } else {
                    let positions = db
                        .block_positions(&session.0, after_seq)
                        .map_err(|error| query_storage_error(db, error))?;
                    let saved_blocks = db
                        .active_compression_graph(&session.0, after_seq)
                        .map_err(|error| query_storage_error(db, error))?;
                    let projected =
                        crate::dcp::project_active_rows(&active.rows, &saved_blocks, &positions)
                            .map_err(|_| {
                                CoreError::Diagnostic(
                                    saved_selection_issue(db, &["history", "dcp"]).diagnostic,
                                )
                            })?;
                    runtime
                        .dcp_projection_estimate(&session.0, &projected, &saved_blocks, after_seq)
                        .map_err(|error| runtime_query_error(db, error))?
                };
                let checkpoint_tokens = db
                    .session_checkpoint(&session.0)
                    .map_err(|error| query_storage_error(db, error))?
                    .map(|(_, summary)| oc_core::dcp_view::estimate_content(&summary))
                    .unwrap_or(0);
                let estimated_tokens =
                    estimated_tokens.map(|tokens| tokens.saturating_add(checkpoint_tokens));
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
                    estimated_tokens_available: estimated_tokens.is_some(),
                    estimate_method: Default::default(),
                    accounting: accounting.clone(),
                    estimated_tokens: estimated_tokens.unwrap_or(0),
                    max_context: thresholds.max_context,
                    turns_since_compress,
                    blocks,
                    compressions: accounting.as_ref().map_or(0, |a| a.compressions),
                    nudges: db
                        .dcp_nudges(&session.0)
                        .map_err(|error| query_storage_error(db, error))?,
                    prunes: accounting.as_ref().map_or(0, |a| a.prunes),
                })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::ShellOutput {
            session,
            shell_id,
            offset,
            limit,
            ack,
        } => {
            let result = db
                .shell_output(&session.0, &shell_id, offset, limit)
                .map(|result| {
                    result.map(
                        |(text, bytes, next_offset)| oc_core::queries::ToolOutputPage {
                            text,
                            total_bytes: bytes,
                            next_offset,
                        },
                    )
                })
                .map_err(|error| query_storage_error(db, error));
            let _ = ack.send(result);
        }
        InboxMsg::CancelShell {
            session,
            shell_id,
            ack,
        } => {
            let result = if runtime.shell_jobs.cancel_job(&session.0, &shell_id) {
                Ok(())
            } else {
                Err(CoreError::TurnNotActive)
            };
            let _ = ack.send(result);
        }
        InboxMsg::Cancel { session, ack } => {
            let result = if runtime.shell_jobs.cancel_session(&session.0) {
                Ok(())
            } else {
                Err(CoreError::TurnNotActive)
            };
            let _ = ack.send(result);
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
        InboxMsg::CompactSession { session, ack } => {
            let result =
                selection::for_turn(db, composition, effective, &session.0).and_then(|selected| {
                    selected.admit_selection(composition)?;
                    runtime
                        .admit_provider(
                            &composition.catalog,
                            &selected.model_id,
                            &composition.provider,
                        )
                        .map_err(runtime_error)?;
                    runtime
                        .queue_compaction(&session.0, oc_core::compaction::CompactionReason::Manual)
                        .map_err(|error| runtime_query_error(db, error))
                });
            let _ = ack.send(result);
        }
        InboxMsg::CancelCompaction { session, ack } => {
            let result = runtime
                .open_session(&session.0)
                .and_then(|_| runtime.cancel_compaction(&session.0))
                .map_err(|error| runtime_query_error(db, error));
            let _ = ack.send(result);
        }
        InboxMsg::CompactionHistory { session, ack } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(|error| runtime_query_error(db, error))
                .and_then(|_| {
                    db.compaction_history(&session.0)
                        .map_err(|error| query_storage_error(db, error))
                });
            let _ = ack.send(result);
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
    title_tx: &mpsc::Sender<AutomaticTitleResult>,
    title_rx: &mut mpsc::Receiver<AutomaticTitleResult>,
    title_work: &Mutex<AutomaticTitles>,
    provider_work: &mut provider_catalog::ProviderWork,
) -> Result<WorkerOutcome, oc_core::queries::ServiceDiagnostic> {
    runtime.set_compaction_events(events);
    let mut pending_inputs = std::collections::VecDeque::new();
    'worker: loop {
        runtime.shell_jobs.deliver(events).map_err(|_| {
            runtime_issue(
                runtime.location(),
                &["shell", "delivery"],
                &RuntimeError::Storage,
            )
            .diagnostic
        })?;
        if let Some(session) = runtime.pending_compaction() {
            let selected = match selection::for_turn(db, composition, effective, &session) {
                Ok(selected) => selected,
                Err(_) => {
                    runtime.refuse_compaction(&session).map_err(|error| {
                        runtime_issue(runtime.location(), &["compaction"], &error).diagnostic
                    })?;
                    continue;
                }
            };
            let operation = runtime.deliver_compaction(
                &session,
                &composition.catalog,
                &selected.model_id,
                selected.variant.as_deref(),
                &composition.provider,
            );
            tokio::pin!(operation);
            let mut shutdown = false;
            let mut deferred = None;
            loop {
                tokio::select! {
                    result = &mut operation => {
                        if let Err(error) = result {
                            // Delivery publishes a Failed lifecycle on recoverable
                            // publication errors and releases ownership. Keep the
                            // owner serving queries/new work only with that proof.
                            let failed = db.compaction_history(&session)
                                .map_err(|error| storage_diagnostic(db.root(), &error))?
                                .first().is_some_and(|snapshot| snapshot.state == oc_core::compaction::CompactionState::Failed);
                            if !failed { return Err(runtime_issue(runtime.location(), &["compaction"], &error).diagnostic); }
                        }
                        break;
                    },
                    command = inbox.recv(), if !shutdown => match command {
                        None | Some(InboxMsg::Shutdown) => { shutdown=true; runtime.cancel_all_compactions(); }
                        Some(command @ InboxMsg::ChangeConversation { .. }) => {
                            runtime.cancel_all_compactions();
                            if deferred.is_none() { deferred=Some(command); }
                            else if let InboxMsg::ChangeConversation { ack,.. } = command { let _=ack.send(Err(CoreError::TurnBusy)); }
                        }
                        Some(command @ (InboxMsg::Submit { .. } | InboxMsg::SubmitFresh { .. })) if pending_inputs.len()<MAX_QUEUE_ITEMS => pending_inputs.push_back(command),
                        Some(command) => query(db,runtime,composition,effective,registry,sessions,home_choices,location_epoch,suggestion_queue,title_work,command),
                    }
                }
            }
            if let Some(command) = deferred {
                query(
                    db,
                    runtime,
                    composition,
                    effective,
                    registry,
                    sessions,
                    home_choices,
                    location_epoch,
                    suggestion_queue,
                    title_work,
                    command,
                );
            }
            if shutdown {
                break 'worker;
            }
            continue;
        }
        let message = if let Some(message) = pending_inputs.pop_front() {
            message
        } else {
            tokio::select! {
                biased;
                error = runtime.wait_mcp_failure() => return Err(runtime_issue(runtime.location(), &["mcp"], &error).diagnostic),
                () = runtime.shell_jobs.changed() => { continue; }
                Some(result) = title_rx.recv() => {
                    commit_automatic_title(db, events, title_work, result);
                    continue;
                }
                result = provider_work.wait(), if provider_work.pending() => return Ok(WorkerOutcome::ProviderCatalog(result)),
                message = inbox.recv() => match message {
                    Some(message) => message,
                    None => break,
                },
            }
        };
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
        if let InboxMsg::ChangeConversation { session, .. } = &message {
            title_work
                .lock()
                .expect("title work mutex")
                .cancel(&session.0);
        }
        match message {
            InboxMsg::OpenPickerSession {
                session,
                search,
                all_projects,
                old_deck,
                ack,
            } => {
                let target =
                    picker_target(db, runtime, composition, &session, &search, all_projects);
                match target {
                    Ok(path) if old_deck.location == runtime.location() => {
                        return Ok(WorkerOutcome::PickerOpen {
                            path,
                            session,
                            old_deck,
                            ack,
                        });
                    }
                    Ok(_) => {
                        let _ = ack.send(Err(CoreError::TabDeckConflict));
                    }
                    Err(error) => {
                        let _ = ack.send(Err(error));
                    }
                }
            }
            InboxMsg::Shutdown => {
                runtime.cancel_all_compactions();
                break 'worker;
            }
            InboxMsg::CancelTitle { session, ack } => {
                let result = if title_work
                    .lock()
                    .expect("title work mutex")
                    .cancel(&session.0)
                {
                    Ok(())
                } else {
                    Err(CoreError::TurnBusy)
                };
                let _ = ack.send(result);
            }
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
                    selected.admit_selection(composition)?;
                    runtime
                        .admit_provider(
                            &composition.catalog,
                            &selected.model_id,
                            &composition.provider,
                        )
                        .map_err(runtime_error)?;
                    // The exact primary choice was admitted before this title
                    // agent may use its own pinned model.
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
                    runtime
                        .admit_provider(&composition.catalog, &selection.id, &composition.provider)
                        .map_err(runtime_error)?;
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
                let title_operation = crate::runtime::next_turn_id(
                    "title",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                );
                let cancel = AtomicBool::new(false);
                let operation = async {
                    let generation = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        crate::provider::stream_input_counted(
                            &composition.provider,
                            &selection.id,
                            selection.variant.as_ref(),
                            &input,
                            &[],
                            output,
                            &cancel,
                            &mut |_| {},
                            &mut || async {
                                db.generation_dispatch(&session.0, &title_operation, "title")
                                    .map_err(|_| crate::provider::ProviderError::DispatchRefused)
                            },
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
                let mut conversation_change = None;
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
                            Some(command @ InboxMsg::ChangeConversation { .. }) => {
                                if conversation_change.is_none() {
                                    cancel.store(true, Ordering::Relaxed);
                                    if let InboxMsg::ChangeConversation { session: target, .. } = &command {
                                        title_work.lock().expect("title work mutex").cancel(&target.0);
                                    }
                                    conversation_change = Some(command);
                                } else if let InboxMsg::ChangeConversation { ack, .. } = command {
                                    let _ = ack.send(Err(CoreError::TurnBusy));
                                }
                            }
                            Some(command) => {
                                query(db, runtime, composition, effective, registry, sessions, home_choices, location_epoch, suggestion_queue, title_work, command);
                                if matches!(db.session_meta(&session.0), Err(StorageError::SessionNotFound)) {
                                    cancel.store(true,Ordering::Relaxed);
                                    // Dropping the pinned provider operation closes the
                                    // held stream before processing another owner command.
                                    break Err(app_error("title session deleted"));
                                }
                            },
                        },
                    }
                };
                if let Some(command) = conversation_change {
                    query(
                        db,
                        runtime,
                        composition,
                        effective,
                        registry,
                        sessions,
                        home_choices,
                        location_epoch,
                        suggestion_queue,
                        title_work,
                        command,
                    );
                }
                let _ = ack.send(result);
                if shutdown {
                    break 'worker;
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
                if provider_work.pending() {
                    let diagnostic = composition
                        .provider_state
                        .for_model(&effective.model_id, false)
                        .diagnostic
                        .expect("pending catalog has a cause");
                    let _ = ack.send(Err(CoreError::ProviderUnavailable(diagnostic)));
                    continue;
                }
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
                    selected.admit_selection(composition)?;
                    runtime
                        .admit_provider(
                            &composition.catalog,
                            &selected.model_id,
                            &composition.provider,
                        )
                        .map_err(runtime_error)?;
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
                let mut conversation_change = None;
                let result;
                {
                    let operation = async {
                        let mut title_warnings = Vec::new();
                        let on_accept = |id: &str,
                                         model_switch: Option<
                            &oc_core::queries::ModelSwitchNotice,
                        >| {
                            let id = WorkerTurnId(id.to_string());
                            turn = Some(id.clone());
                            if let Some(ack) = ack.take() {
                                let _ = ack.send(Ok(id.clone()));
                            }
                            let _ = events.send(CoreEvent::TurnStarted {
                                session: session.clone(),
                                turn: id.clone(),
                                model_switch: model_switch.cloned().map(|notice| {
                                    project_model_switch(notice, &composition.catalog)
                                }),
                            });
                            // Acceptance is durable here. Do not launch a title
                            // request for a rejected prompt, an already titled
                            // root, or one with an in-flight title request.
                            let stamp = db.root_title_stamp(&session.0).ok().flatten();
                            if let Some((None, expected_event)) = stamp {
                                let mut work = title_work.lock().expect("title work mutex");
                                if !work.pending.contains_key(&session.0) {
                                    work.pending.insert(session.0.clone(), expected_event);
                                    work.tasks.retain(|(_, task)| !task.is_finished());
                                    let session = session.clone();
                                    let title_operation = id.0.clone();
                                    let sender = (*title_tx).clone();
                                    let provider = composition.provider.clone();
                                    let selection = title_selection.clone();
                                    let prompt = title_prompt.clone();
                                    let instructions = title_agent
                                        .map(|agent| agent.body.clone())
                                        .unwrap_or_else(|| "Generate a short session title from the user's request. Output only the title, in at most 100 characters.".into());
                                    let fallback = composition
                                        .generation
                                        .providers
                                        .get(&composition.catalog.provider)
                                        .map(|provider| provider.options.native_fallback_limits)
                                        .unwrap_or_default();
                                    let budget = crate::models::budget(&selection, 256, fallback);
                                    if let Some(warning) = &budget.warning {
                                        title_warnings.push(format!("title generation: {warning}"));
                                    }
                                    let input = vec![
                                        crate::provider::InputItem::message(
                                            crate::provider::InputRole::Developer,
                                            &instructions,
                                        ),
                                        crate::provider::InputItem::message(
                                            crate::provider::InputRole::User,
                                            &prompt[..prompt
                                                .floor_char_boundary(prompt.len().min(8192))],
                                        ),
                                    ];
                                    let admitted = serde_json::to_string(&(
                                        &input,
                                        &[] as &[crate::provider::ToolDef],
                                    ))
                                    .map(|json| crate::runtime::estimate_tokens(&json))
                                    .map_err(|_| "invalid title request".to_string())
                                    .and_then(|tokens| {
                                        crate::models::admit_budget(&selection, tokens, &budget)
                                            .map_err(|error| error.to_string())
                                    });
                                    if let Err(error) = admitted {
                                        title_warnings
                                            .push(format!("title generation skipped: {error}"));
                                        work.pending.remove(&session.0);
                                    } else {
                                        work.tasks.push((
                                            session.0.clone(),
                                            tokio::spawn(async move {
                                                let title = async {
                                                    let cancel = AtomicBool::new(false);
                                                    let generation = tokio::time::timeout(
                                                        std::time::Duration::from_secs(10),
                                                        crate::provider::stream_input_counted(
                                                            &provider,
                                                            &selection.id,
                                                            selection.variant.as_ref(),
                                                            &input,
                                                            &[],
                                                            budget.output,
                                                            &cancel,
                                                            &mut |_| {},
                                                            &mut || {
                                                                let sender = sender.clone();
                                                                let session = session.clone();
                                                                let operation = title_operation.clone();
                                                                async move {
                                                                    let (ack, receipt) = oneshot::channel();
                                                                    sender.send(AutomaticTitleResult {session,expected_event,title:None,dispatch:Some((operation,ack))}).await
                                                                        .map_err(|_|crate::provider::ProviderError::DispatchRefused)?;
                                                                    receipt.await.map_err(|_|crate::provider::ProviderError::DispatchRefused)?
                                                                }
                                                            },
                                                        ),
                                                    )
                                                    .await
                                                    .ok()?
                                                    .ok()?;
                                                    let canonical = generation
                                                        .output
                                                        .iter()
                                                        .filter(|item| {
                                                            item["type"] == "message"
                                                                && item["role"] == "assistant"
                                                        })
                                                        .filter_map(|item| {
                                                            item.get("content")
                                                                .and_then(|value| value.as_array())
                                                        })
                                                        .flatten()
                                                        .filter(|part| {
                                                            part["type"] == "output_text"
                                                        })
                                                        .filter_map(|part| part["text"].as_str())
                                                        .collect::<Vec<_>>()
                                                        .join("");
                                                    let text = if generation.text.is_empty() {
                                                        &canonical
                                                    } else {
                                                        &generation.text
                                                    };
                                                    let line = text
                                                        .lines()
                                                        .find(|line| !line.trim().is_empty())
                                                        .unwrap_or_default()
                                                        .trim()
                                                        .trim_matches('"');
                                                    let title: String = line
                                                        .chars()
                                                        .filter(|c| !c.is_control())
                                                        .take(100)
                                                        .collect();
                                                    normalized_session_title(&title)
                                                        .map(str::to_string)
                                                }
                                                .await;
                                                let _ = sender
                                                    .send(AutomaticTitleResult {
                                                        dispatch: None,
                                                        session,
                                                        expected_event,
                                                        title,
                                                    })
                                                    .await;
                                            }),
                                        ));
                                    }
                                }
                            }
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
                        let on_reasoning_end = |id: &str| {
                            let _ = events.send(CoreEvent::ReasoningItemEnded {
                                session: session.clone(),
                                turn: WorkerTurnId(id.to_string()),
                            });
                        };
                        let on_tool = |id: &str, event: &ToolCallEvent| {
                            let turn = WorkerTurnId(id.to_string());
                            let _ = events.send(match event {
                                ToolCallEvent::ArgumentStream(event) => {
                                    CoreEvent::ToolArgumentStream {
                                        session: session.clone(),
                                        turn,
                                        event: event.clone(),
                                    }
                                }
                                ToolCallEvent::Started {
                                    dcp_topic,
                                    op,
                                    name,
                                    input,
                                } => CoreEvent::ToolCallStarted {
                                    dcp_topic: dcp_topic.clone(),
                                    session: session.clone(),
                                    turn,
                                    op: op.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                },
                                ToolCallEvent::Finished {
                                    dcp,
                                    patch_effects,
                                    op,
                                    name,
                                    state,
                                    output,
                                    output_bytes,
                                    output_truncated,
                                } => CoreEvent::ToolCallFinished {
                                    dcp: dcp.clone(),
                                    patch_effects: patch_effects.clone(),
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
                            if !matches!(event, ToolCallEvent::ArgumentStream(_))
                                && let Ok(Some(projection)) = db.turn_presentation(&session.0, id)
                            {
                                let _ = events.send(CoreEvent::TurnPresentation {
                                    session: session.clone(),
                                    turn: WorkerTurnId(id.to_string()),
                                    projection,
                                });
                            }
                        };
                        let mut report = if is_fresh {
                            runtime
                                .run_fresh_turn_with_reasoning_items_and_notice(
                                    params,
                                    initial_selection
                                        .as_ref()
                                        .map(|(key, value)| (key.as_str(), value.as_str())),
                                    on_accept,
                                    on_text,
                                    on_reasoning,
                                    on_reasoning_end,
                                    on_tool,
                                )
                                .await?
                        } else {
                            runtime
                                .run_turn_with_reasoning_items_and_notice(
                                    params,
                                    on_accept,
                                    on_text,
                                    on_reasoning,
                                    on_reasoning_end,
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
                        report.warnings.extend(title_warnings);
                        Ok::<_, RuntimeError>(report)
                    };
                    tokio::pin!(operation);
                    result = loop {
                        tokio::select! {
                            result = &mut operation => break result,
                            () = runtime.shell_jobs.changed() => {
                                runtime.shell_jobs.deliver(events).map_err(|_| runtime_issue(runtime.location(), &["shell", "delivery"], &RuntimeError::Storage).diagnostic)?;
                            }
                            _ = runtime.wait_mcp_failure(), if !shutdown => {
                                shutdown = true;
                                cancel.store(true, Ordering::Relaxed);
                                runtime.cancel_pending_approvals();
                                runtime.cancel_all_compactions();
                            },
                            Some(result) = title_rx.recv(), if !shutdown => {
                                commit_automatic_title(db, events, title_work, result);
                            }
                            command = inbox.recv(), if !shutdown => match command {
                                None | Some(InboxMsg::Shutdown) => {
                                    shutdown = true;
                                    cancel.store(true, Ordering::Relaxed);
                                    runtime.cancel_pending_approvals();
                                    runtime.cancel_all_compactions();
                                }
                                Some(InboxMsg::CancelTitle { session: target, ack }) => {
                                    let result = if title_work.lock().expect("title work mutex").cancel(&target.0) {
                                        Ok(())
                                    } else {
                                        Err(CoreError::TurnBusy)
                                    };
                                    let _ = ack.send(result);
                                }
                                Some(InboxMsg::Cancel { session: target, ack }) if target == session => {
                                    cancel.store(true, Ordering::Relaxed);
                                    runtime.cancel_pending_approvals();
                                    runtime.shell_jobs.cancel_session(&target.0);
                                    let _ = ack.send(Ok(()));
                                }
                                Some(command @ InboxMsg::ChangeConversation { .. }) => {
                                    if conversation_change.is_none() {
                                        if let InboxMsg::ChangeConversation { session: target, .. } = &command {
                                            title_work.lock().expect("title work mutex").cancel(&target.0);
                                        }
                                        cancel.store(true, Ordering::Relaxed);
                                        runtime.cancel_pending_approvals();
                                        runtime.cancel_all_compactions();
                                        conversation_change = Some(command);
                                    } else if let InboxMsg::ChangeConversation { ack, .. } = command {
                                        let _ = ack.send(Err(CoreError::TurnBusy));
                                    }
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
                                    title_work,
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
                    let _ = ack.send(Err(runtime_error(error)));
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
                            error: runtime_error(error),
                            warnings,
                        },
                    };
                    let _ = events.send(event);
                }
                if let Some(command) = conversation_change {
                    query(
                        db,
                        runtime,
                        composition,
                        effective,
                        registry,
                        sessions,
                        home_choices,
                        location_epoch,
                        suggestion_queue,
                        title_work,
                        command,
                    );
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
                title_work,
                message,
            ),
        }
    }
    runtime.cancel_all_compactions();
    for pending in pending_inputs {
        match pending {
            InboxMsg::Submit { ack, .. } | InboxMsg::SubmitFresh { ack, .. } => {
                let _ = ack.send(Err(CoreError::Shutdown));
            }
            _ => unreachable!(),
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
mod tests;

#[cfg(test)]
#[path = "application/mcp_tests.rs"]
mod mcp_tests;
