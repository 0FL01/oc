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

mod accounts;
mod authentication;
#[cfg(test)]
mod authentication_tests;
#[path = "application/commands.rs"]
mod commands;
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
#[cfg(test)]
mod terminal_tests;
mod terminals;
mod user_shell;
pub(crate) use selection::request_choice;
pub(crate) mod session_move;
#[path = "application_tab_deck.rs"]
mod tab_deck;

/// One event projection for root and child runtime callbacks.
pub(crate) fn publish_tool_event(
    db: &Db,
    events: &broadcast::Sender<CoreEvent>,
    session: &SessionId,
    id: &str,
    event: &ToolCallEvent,
) {
    let turn = WorkerTurnId(id.into());
    let _ = events.send(match event {
        ToolCallEvent::ArgumentStream(event) => CoreEvent::ToolArgumentStream {
            session: session.clone(),
            turn,
            event: event.clone(),
        },
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
            output_presentation,
            question,
            dcp,
            patch_effects,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
        } => CoreEvent::ToolCallFinished {
            output_presentation: output_presentation.clone(),
            question: question.clone(),
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
            turn: WorkerTurnId(id.into()),
            projection,
        });
    }
}

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

/// Private direct seam for an already admitted native invocation. No Inbox
/// request: the application worker is currently awaiting this same turn.
pub(crate) fn commit_session_rename(
    db: &Db,
    events: Option<&broadcast::Sender<CoreEvent>>,
    session: &str,
    title: &str,
    root_only: bool,
) -> Result<(), StorageError> {
    db.rename_session(session, title, root_only)?;
    if let Some(events) = events {
        let _ = events.send(CoreEvent::SessionTitleUpdated {
            session: oc_core::domain::SessionId(session.into()),
            title: title.into(),
        });
    }
    Ok(())
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
        RuntimeError::QuestionRequired => ServiceCode::QuestionRequired,
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
        RuntimeError::QuestionRequired => {
            issue.diagnostic.field = vec!["question".into()];
            issue.diagnostic.stage = ServiceStage::Call;
            issue.diagnostic.action = ServiceAction::UseInteractiveTui;
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
        StorageError::InvalidCredential => "InvalidCredential",
        StorageError::CredentialNotFound => "CredentialNotFound",
        StorageError::CredentialStorage => "CredentialStorage",
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
    composition
        .resolve_credentials(&db)
        .await
        .map_err(SpawnIssue::configuration)?;
    composition.attach_public_catalog(&db).await;
    if !defer_provider {
        composition.refresh_public_catalog(&db, false).await;
        composition
            .refresh_provider()
            .await
            .map_err(SpawnIssue::configuration)?;
    }
    db.recover_shell_jobs()
        .and_then(|_| db.recover_child_jobs())
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
    provider_id: String,
    model_id: String,
    variant: Option<String>,
    agent_id: Option<String>,
    agent_prompt: Option<String>,
    agent_digest: Option<String>,
    profile_issue: Option<oc_core::queries::ServiceDiagnostic>,
    legacy_epoch: u64,
    inline_command: Option<String>,
    command_parents: Vec<String>,
}

impl Effective {
    fn request<'a>(
        &self,
        composition: &'a Composition,
    ) -> Result<
        (
            &'a crate::models::ModelCatalog,
            crate::provider::ResponsesConfig,
        ),
        CoreError,
    > {
        let catalog = composition
            .catalog_for(&self.provider_id)
            .ok_or_else(|| app_error("selected provider unavailable"))?;
        let provider = composition
            .request_provider(&self.provider_id)
            .ok_or_else(|| app_error("selected provider unavailable"))?;
        Ok((catalog, provider))
    }
    fn from_composition(composition: &Composition) -> Self {
        Self {
            provider_id: composition.catalog.provider.clone(),
            model_id: composition.model_id.clone(),
            variant: composition.variant.clone(),
            agent_id: composition.default_agent.clone(),
            agent_prompt: composition.agent_prompt.clone(),
            agent_digest: composition.agent_digest.clone(),
            profile_issue: None,
            legacy_epoch: 0,
            inline_command: None,
            command_parents: Vec::new(),
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
        let empty = crate::models::ModelCatalog {
            provider: self.provider_id.clone(),
            models: BTreeMap::new(),
        };
        let catalog = composition.catalog_for(&self.provider_id).unwrap_or(&empty);
        let status = composition
            .readiness_for(&self.provider_id, &self.model_id, self.variant.as_deref())
            .catalog_status;
        let (identity, diagnostic) = if let Some(id) = self
            .command_parents
            .iter()
            .find(|id| !composition.agents.contains_key(*id))
        {
            (
                id.as_str(),
                selection_diagnostic(
                    composition,
                    &["selection", "agent"],
                    Code::AgentUnavailable,
                    Action::SelectAgent,
                ),
            )
        } else if let Some(error) = &self.profile_issue {
            let identity = if error.code == Code::ModelUnavailable {
                self.model_id.as_str()
            } else {
                self.agent_id.as_deref().unwrap_or_default()
            };
            (identity, error.clone())
        } else if let Some(id) = self.agent_id.as_deref().filter(|id| {
            !composition.agents.get(*id).is_some_and(|agent| {
                agent.primary_capable()
                    || commands::inline_eligible(composition, self.inline_command.as_deref(), id)
            })
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
        } else if self.model_id.is_empty()
            || (!catalog.models.contains_key(&self.model_id)
                && (status == oc_core::queries::ProviderStatus::Ready
                    || !matches!(
                        catalog.provider.as_str(),
                        crate::discovery::PROVIDER_ID | crate::models_dev::PROVIDER
                    )))
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
        } else if catalog.models.contains_key(&self.model_id)
            && self.variant.as_deref().is_some_and(|variant| {
                crate::models::select_model(catalog, &self.model_id)
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
        self.set_agent_inner(composition, id, false)
    }

    fn set_agent_inner(
        &mut self,
        composition: &Composition,
        id: &str,
        command_inline: bool,
    ) -> Result<(), CoreError> {
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
        if !agent.primary_capable() && !command_inline {
            return Err(failed());
        }
        if let Some(model) = agent.model.as_deref() {
            // Retain the existing exact bare-ID alias; full profile references
            // share the child/title resolver, including IDs with slashes.
            let resolved = composition
                .model_reference(&composition.catalog.provider, model)
                .map_err(|_| failed())?;
            self.provider_id = resolved.provider;
            self.model_id = resolved.id;
            self.variant = resolved.variant.or(agent.variant.clone());
        } else if agent.variant.is_some() {
            self.variant = agent.variant.clone();
        }
        self.agent_id = Some(id.to_string());
        self.agent_prompt = Some(agent.body.clone());
        self.agent_digest = Some(crate::defs::agent_digest(agent));
        self.profile_issue = None;
        if !command_inline {
            self.inline_command = None;
            self.command_parents.clear();
        }
        Ok(())
    }

    /// Catalog plus this effective selection.
    fn snapshot(&self, composition: &Composition, generation: u64) -> CatalogSnapshot {
        let empty = crate::models::ModelCatalog {
            provider: self.provider_id.clone(),
            models: BTreeMap::new(),
        };
        self.snapshot_catalog(
            composition,
            generation,
            composition.catalog_for(&self.provider_id).unwrap_or(&empty),
            composition.readiness_for(&self.provider_id, &self.model_id, self.variant.as_deref()),
        )
    }

    fn snapshot_catalog(
        &self,
        composition: &Composition,
        generation: u64,
        catalog: &crate::models::ModelCatalog,
        readiness: oc_core::queries::ProviderReadiness,
    ) -> CatalogSnapshot {
        let issue = self.selection_issue(composition);
        let mut models: Vec<ModelEntry> = catalog
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
                    .get(&catalog.provider)
                    .and_then(|p| p.name.clone())
                    .unwrap_or_else(|| catalog.provider.clone()),
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
                            .get("settings")
                            .unwrap_or(value)
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
            .default_agent
            .iter()
            .chain(
                composition
                    .agent_order
                    .iter()
                    .filter(|id| Some(id.as_str()) != composition.default_agent.as_deref()),
            )
            .filter_map(|id| composition.agents.get(id))
            .enumerate()
            .filter(|(_, agent)| agent.primary_visible())
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
                chrome.selection_generation = generation;
                chrome.terminal_generation = generation;
                chrome.agent_colors = composition
                    .agents
                    .values()
                    .filter_map(|agent| Some((agent.id.clone(), agent.color.clone()?)))
                    .collect();
                chrome.permissions_auto = composition.permission_preference.load(Ordering::SeqCst);
                chrome.provider = Some(readiness);
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
            provider: catalog.provider.clone(),
            models,
            model_id: if self.model_id.is_empty() {
                String::new()
            } else if issue.as_ref().is_some_and(|s| {
                s.diagnostic.code == oc_core::queries::ServiceCode::ModelUnavailable
            }) || (!catalog.models.contains_key(&self.model_id)
                && composition
                    .readiness_for(&self.provider_id, &self.model_id, self.variant.as_deref())
                    .catalog_status
                    == oc_core::queries::ProviderStatus::Ready)
            {
                selection_identity("model", &self.model_id)
            } else {
                self.model_id.clone()
            },
            variant: if !catalog.models.contains_key(&self.model_id)
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
    crate::models::unavailable_selection_identity(kind, raw)
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
        composition
            .parent_env
            .get("OC_TEST_WEBFETCH_ALLOW_LOOPBACK")
            .map(String::as_str)
            == Some("1"),
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
    runtime
        .publish_command_digest(crate::compaction::fingerprint(
            &composition
                .command_defs
                .values()
                .map(|def| {
                    (
                        &def.id,
                        &def.body,
                        &def.description,
                        &def.agent,
                        &def.model,
                        def.subagent,
                        def.subtask,
                    )
                })
                .collect::<Vec<_>>(),
        ))
        .map_err(|error| runtime_issue(&source, &["command"], &error))?;
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
                    request: agent.request.clone(),
                    color: agent.color.clone(),
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
    Accounts {
        provider: String,
        action: Option<oc_core::queries::AccountAction>,
        ack: oneshot::Sender<Result<oc_core::queries::ProviderAccounts, CoreError>>,
    },
    Move(Box<session_move::Prepared>),
    /// Inbox closed or an explicit shutdown was requested.
    Stop,
    ProviderCatalog(Result<provider_catalog::CatalogOutcome, oc_core::queries::ServiceDiagnostic>),
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
    composition: Composition,
    inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
    ready: oneshot::Sender<Result<Vec<String>, SpawnIssue>>,
) -> Result<(), oc_core::queries::ServiceDiagnostic> {
    let children = crate::runtime::children::Jobs::new(&db);
    let root = db.root().to_owned();
    let authentication = authentication::new(&db);
    let response_channels = db.response_channels.clone();
    let authentication_location = composition.project.to_string_lossy().into_owned();
    let result = start_worker_inner(
        db,
        composition,
        inbox,
        events.clone(),
        ready,
        children.clone(),
        &authentication,
    )
    .await;
    // Covers every return/failed publication, including errors after a move.
    let authentication_stop = authentication::shutdown(&authentication).await;
    let joined = children.shutdown().await;
    let delivered = children.deliver(Some(&events));
    let channel_stop = response_channels.shutdown().await;
    authentication_stop.map_err(|_| {
        runtime_issue(
            &authentication_location,
            &["authentication", "cleanup"],
            &RuntimeError::Storage,
        )
        .diagnostic
    })?;
    joined.map_err(|error| storage_diagnostic(&root, &error))?;
    delivered.map_err(|error| storage_diagnostic(&root, &error))?;
    channel_stop.map_err(|_| {
        runtime_issue(
            &authentication_location,
            &["provider", "openai", "cleanup"],
            &RuntimeError::Provider,
        )
        .diagnostic
    })?;
    result
}

async fn start_worker_inner(
    db: Db,
    mut composition: Composition,
    mut inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
    ready: oneshot::Sender<Result<Vec<String>, SpawnIssue>>,
    children: Arc<crate::runtime::children::Jobs>,
    authentication: &authentication::Owner,
) -> Result<(), oc_core::queries::ServiceDiagnostic> {
    let mut runtime = match build_runtime(&db, &composition) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(error));
            return Ok(());
        }
    };
    runtime.child_jobs = children;
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
    runtime
        .recover_background_children(&composition.catalog, &composition.provider)
        .await
        .map_err(|error| {
            runtime_issue(runtime.location(), &["child_recovery"], &error).diagnostic
        })?;
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
    let mut provider_work = provider_catalog::ProviderWork::start(&composition, &db);
    // Failed terminal recovery keeps local UI/controls available, but refuses
    // every terminal action and is never reported as successful cleanup.
    let terminals = Mutex::new(crate::terminals::Terminals::new(&db).map(|mut owner| {
        owner.set_events(&events);
        owner
    }));
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
            &terminals,
            authentication,
        )
        .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let terminal_stop = terminals::shutdown(&terminals).map_err(|_| {
                    runtime_issue(
                        runtime.location(),
                        &["terminals", "cleanup"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                });
                let child_stop = runtime
                    .child_jobs
                    .shutdown()
                    .await
                    .map_err(|e| storage_diagnostic(db.root(), &e));
                let child_delivery = runtime
                    .child_jobs
                    .deliver(Some(&events))
                    .map_err(|e| storage_diagnostic(db.root(), &e));
                let shell_stop = runtime
                    .shell_jobs
                    .shutdown()
                    .await
                    .map_err(|e| storage_diagnostic(db.root(), &e));
                let shell_delivery = runtime
                    .shell_jobs
                    .deliver(&events)
                    .map_err(|e| storage_diagnostic(db.root(), &e));
                let provider_stop = provider_work.stop().await;
                let title_stop = stop_automatic_titles(&title_work).await;
                runtime.shutdown_mcp().await.map_err(|error| {
                    runtime_issue(runtime.location(), &["mcp"], &error).diagnostic
                })?;
                title_stop?;
                provider_stop?;
                terminal_stop?;
                shell_stop?;
                shell_delivery?;
                child_stop?;
                child_delivery?;
                return Err(error);
            }
        };
        match outcome {
            WorkerOutcome::Accounts {
                provider,
                action,
                ack,
            } => {
                let changed = action.is_some() || provider == "openai";
                let result =
                    accounts::apply(&db, &runtime, &mut composition, provider, action).await;
                if changed && result.is_ok() {
                    let _ = events.send(CoreEvent::ProviderChanged);
                }
                let _ = ack.send(result);
            }
            WorkerOutcome::Move(prepared) => {
                let prepared = *prepared;
                let moved: Result<(), oc_core::queries::ServiceDiagnostic> = async {
                    prepared.recheck(&db).map_err(|error| {
                        runtime_issue(
                            runtime.location(),
                            &["session_move"],
                            &RuntimeError::InvalidArgs(error),
                        )
                        .diagnostic
                    })?;
                    let record = prepared.record.clone();
                    let current = (sessions.get(runtime.location()) == Some(&record.session)
                        || (!sessions.contains_key(runtime.location())
                            && db
                                .move_source_is_target(&record)
                                .map_err(|e| storage_diagnostic(db.root(), &e))?))
                        && db
                            .session_meta(&record.session)
                            .map_err(|e| storage_diagnostic(db.root(), &e))?
                            .parent_id
                            .is_none();
                    let next_composition = prepared.composition;
                    let mut next =
                        build_runtime(&db, &next_composition).map_err(|e| e.diagnostic)?;
                    let next_effective = prepared.effective;
                    publish_workspace(&next, &next_composition, &next_effective).map_err(|e| {
                        runtime_issue(&record.directory, &["workspace"], &e).diagnostic
                    })?;
                    let next_registry = prepared.registry;
                    if current {
                        let provider_stop = provider_work.stop().await;
                        let mcp_stop = runtime.retire_or_shutdown_mcp().await.map_err(|e| {
                            runtime_issue(runtime.location(), &["mcp"], &e).diagnostic
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
                        next.set_compaction_events(&events);
                        next.start_mcp()
                            .map_err(|e| runtime_issue(next.location(), &["mcp"], &e).diagnostic)?;
                    }
                    if let Err(error) = session_move::Prepared::recheck_parts(
                        &db,
                        &record,
                        &prepared.directory,
                        prepared.identity,
                    ) {
                        next.shutdown_mcp()
                            .await
                            .map_err(|e| runtime_issue(next.location(), &["mcp"], &e).diagnostic)?;
                        return Err(runtime_issue(
                            runtime.location(),
                            &["session_move"],
                            &RuntimeError::InvalidArgs(error),
                        )
                        .diagnostic);
                    }
                    if let Err(error) = db.apply_session_move(&record) {
                        next.shutdown_mcp()
                            .await
                            .map_err(|e| runtime_issue(next.location(), &["mcp"], &e).diagnostic)?;
                        return Err(storage_diagnostic(db.root(), &error));
                    }
                    let mut location = None;
                    if current {
                        let mode = composition.approval_consumer_mode.load(Ordering::SeqCst);
                        next_composition
                            .approval_consumer_mode
                            .store(mode, Ordering::SeqCst);
                        if mode > 0 {
                            next.register_approval_consumer(mode == 2);
                        }
                        let consumer = composition.question_consumer.load(Ordering::SeqCst);
                        next_composition
                            .question_consumer
                            .store(consumer, Ordering::SeqCst);
                        if consumer {
                            next.register_question_consumer();
                        }
                        next.inherit_application_owners(&runtime);
                        sessions.remove(&record.source);
                        sessions.insert(record.directory.clone(), record.session.clone());
                        runtime = next;
                        composition = next_composition;
                        effective = next_effective;
                        registry = next_registry;
                        location_epoch.fetch_add(1, Ordering::SeqCst);
                        location = Some(Box::new(LocationSnapshot {
                            location: record.directory.clone(),
                            generation: location_epoch.load(Ordering::SeqCst),
                            session: record.session.clone(),
                            catalog: effective
                                .snapshot(&composition, location_epoch.load(Ordering::SeqCst)),
                            diagnostics: composition.diagnostics.clone(),
                            notices: composition.startup_notices.clone(),
                        }));
                    }
                    let _ = events.send(CoreEvent::SessionMoved {
                        operation: record.operation,
                        session: SessionId(record.session),
                        directory: record.directory,
                        location,
                    });
                    Ok(())
                }
                .await;
                if let Err(error) = moved {
                    let shell_stop = runtime
                        .shell_jobs
                        .shutdown()
                        .await
                        .map_err(|e| storage_diagnostic(db.root(), &e));
                    let shell_delivery = runtime
                        .shell_jobs
                        .deliver(&events)
                        .map_err(|e| storage_diagnostic(db.root(), &e));
                    let provider_stop = provider_work.stop().await;
                    let title_stop = stop_automatic_titles(&title_work).await;
                    let mcp_stop = runtime
                        .shutdown_mcp()
                        .await
                        .map_err(|e| runtime_issue(runtime.location(), &["mcp"], &e).diagnostic);
                    shell_stop?;
                    shell_delivery?;
                    provider_stop?;
                    title_stop?;
                    mcp_stop?;
                    return Err(error);
                }
            }
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
                let mut changed = match outcome {
                    provider_catalog::CatalogOutcome::Selected(outcome) => {
                        let previous = (
                            composition.catalog.models.clone(),
                            composition.provider_state.clone(),
                        );
                        composition.accept_provider_catalog(outcome);
                        previous
                            != (
                                composition.catalog.models.clone(),
                                composition.provider_state.clone(),
                            )
                    }
                    provider_catalog::CatalogOutcome::Public(outcome) => {
                        composition.accept_public_view(outcome)
                    }
                    provider_catalog::CatalogOutcome::OpenAi(outcome) => {
                        composition.accept_public_view_for(crate::models_dev::OPENAI, outcome)
                    }
                };
                // The sole public source fetch contains both slices. Publish
                // current cache projections without creating another GET.
                changed |= composition.attach_public_catalog(&db).await;
                runtime
                    .publish_provider_state(composition.provider_state.clone())
                    .map_err(|error| {
                        runtime_issue(runtime.location(), &["provider"], &error).diagnostic
                    })?;
                if changed {
                    let _ = events.send(CoreEvent::ProviderChanged);
                }
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
                        &terminals,
                        authentication,
                        session,
                        old_deck,
                    )
                    .await;
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
                let consumer = composition.question_consumer.load(Ordering::SeqCst);
                next_composition
                    .question_consumer
                    .store(consumer, Ordering::SeqCst);
                if consumer {
                    next.register_question_consumer();
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
                    &terminals,
                    authentication,
                    session.clone(),
                    old_deck,
                )
                .await;
                let mut receipt = match receipt {
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
                let mcp_stop = runtime.retire_or_shutdown_mcp().await.map_err(|error| {
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
                next.inherit_application_owners(&runtime);
                runtime = next;
                composition = next_composition;
                effective = next_effective;
                registry = next_registry;
                sessions.insert(path, session.0);
                receipt.catalog.chrome.selection_generation =
                    location_epoch.fetch_add(1, Ordering::SeqCst) + 1;
                receipt.catalog.chrome.terminal_generation =
                    receipt.catalog.chrome.selection_generation;
                let _ = ack.send(Ok(receipt));
            }
            WorkerOutcome::Stop => {
                let terminal_stop = terminals::shutdown(&terminals).map_err(|_| {
                    runtime_issue(
                        runtime.location(),
                        &["terminals", "cleanup"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                });
                runtime
                    .child_jobs
                    .shutdown()
                    .await
                    .map_err(|e| storage_diagnostic(db.root(), &e))?;
                runtime
                    .child_jobs
                    .deliver(Some(&events))
                    .map_err(|e| storage_diagnostic(db.root(), &e))?;
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
                terminal_stop?;
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
                        next_composition.question_consumer.store(
                            composition.question_consumer.load(Ordering::SeqCst),
                            Ordering::SeqCst,
                        );
                        if next_composition.question_consumer.load(Ordering::SeqCst) {
                            next.register_question_consumer();
                        }
                        let home_catalog = if matches!(ack, SwitchAck::Home(_)) {
                            let selected = match home_choices.get(next.location()) {
                                Some(selected) => Ok(selected.clone()),
                                None => {
                                    selection::home_current(&db, &next_composition, &next_effective)
                                }
                            };
                            match selected {
                                Ok(selected) => Some(selected.snapshot(
                                    &next_composition,
                                    location_epoch.load(Ordering::SeqCst) + 1,
                                )),
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
                        let mcp_stop = runtime.retire_or_shutdown_mcp().await.map_err(|error| {
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
                        next.inherit_application_owners(&runtime);
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
                                    catalog: effective.snapshot(
                                        &composition,
                                        location_epoch.load(Ordering::SeqCst),
                                    ),
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
                                    catalog: effective.snapshot(
                                        &composition,
                                        location_epoch.load(Ordering::SeqCst),
                                    ),
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
/// generation. Missing model/provider/variant availability is optional: retain
/// the exact choice as unavailable, not the old executable configuration or a
/// substitute model. Profile/policy and storage admission remain mandatory.
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
            next.generation_id(),
        )
        .map_err(|_| selection_error())?;
        let turn = selection::for_turn(db, composition, effective, &session.0)
            .map_err(|_| selection_error())?;
        for selected in [&selected, &turn] {
            if selected.profile_issue.is_some() {
                return Err(selection_error());
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
    let mut composition = composition::load_local_with_env(Path::new(path), env)
        .await
        .map_err(SpawnIssue::configuration)?;
    composition
        .resolve_credentials(db)
        .await
        .map_err(SpawnIssue::configuration)?;
    composition.attach_public_catalog(db).await;
    composition.refresh_public_catalog(db, true).await;
    composition
        .refresh_provider()
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
    runtime.publish_instruction_roots(composition.instruction_roots.clone())?;
    runtime.publish_workspace(
        effective.agent_prompt.as_deref(),
        &composition.instructions,
        composition.skills.clone(),
        composition.skill_errors.clone(),
        effective.agent_digest.clone(),
        effective.agent_id.clone(),
        // An explicit profile color is resolved by id, not by a pinned slot.
        effective
            .agent_id
            .as_ref()
            .filter(|_| agent.is_none_or(|agent| agent.color.is_none()))
            .and_then(|id| composition.agents.values().position(|a| &a.id == id)),
        agent
            .map(|agent| agent.permissions.clone())
            .unwrap_or_default(),
        commands::permission_rules(composition, effective, agent),
        agent.map(|agent| agent.request.clone()).unwrap_or_default(),
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
async fn prepare_picker_open(
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
    terminals: &terminals::Owner,
    authentication: &authentication::Owner,
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
        runtime.generation_id(),
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
        terminals,
        authentication,
        InboxMsg::History {
            session: session.clone(),
            message: None,
            before_seq: None,
            after_seq: None,
            limit: HISTORY_PAGE_LIMIT,
            ack,
        },
    )
    .await;
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
        if !target.new_session_titles.is_empty() {
            target.new_session_titles.push(false);
        }
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
        catalog: selected.snapshot(composition, location_epoch.load(Ordering::SeqCst)),
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
async fn query(
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
    terminals: &terminals::Owner,
    authentication: &authentication::Owner,
    message: InboxMsg,
) {
    match message {
        InboxMsg::UserShell { ack, .. } => {
            // Active operations keep one admission owner. Ask must be pumped
            // by the idle worker, never awaited inside this query handler.
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::PromptHistory { append, ack } => {
            let result = if append
                .as_ref()
                .is_some_and(|text| text.len() > oc_core::session::MAX_INPUT_BYTES)
            {
                Err(CoreError::InputTooLarge)
            } else {
                db.prompt_history(append.as_deref()).map_err(app_error)
            };
            let _ = ack.send(result);
        }
        InboxMsg::AuthMethods { provider, ack } => {
            let _ = ack.send(authentication::methods(composition, &provider));
        }
        InboxMsg::Authenticate {
            provider,
            action,
            ack,
        } => {
            let _ = ack
                .send(authentication::action(authentication, composition, &provider, action).await);
        }
        InboxMsg::Terminal {
            session,
            action,
            ack,
        } => {
            let _ = ack.send(terminals::action(
                terminals,
                runtime,
                location_epoch.load(Ordering::SeqCst),
                session,
                action,
            ));
        }
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
        InboxMsg::PendingQuestions { ack } => {
            let _ = ack.send(Ok(runtime.pending_questions()));
        }
        InboxMsg::ReplyQuestion { reply, ack } => {
            let _ = ack.send(runtime.reply_question(reply));
        }
        InboxMsg::RegisterQuestionConsumer { ack } => {
            runtime.register_question_consumer();
            composition.question_consumer.store(true, Ordering::SeqCst);
            let _ = ack.send(Ok(()));
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
                let providers = composition
                    .provider_views
                    .keys()
                    .cloned()
                    .chain(std::iter::once(composition.catalog.provider.clone()))
                    .collect();
                db.fork_session_admitted(
                    &source.0,
                    &before.0,
                    runtime.location(),
                    &composition.catalog.provider,
                    &providers,
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
            message,
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
                let (mut page, ascending) = if let Some(message) = message {
                    (
                        db.read_history_message_typed(&session.0, &message)
                            .map_err(|error| query_storage_error(db, error))?
                            .into_iter()
                            .collect(),
                        false,
                    )
                } else {
                    match after_seq {
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
                    }
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
                            let user_shell = db
                                .user_shell_result(&session.0, &id)
                                .map_err(|error| query_storage_error(db, error))?;
                            let child = db
                                .child_history(&session.0, &id)
                                .map_err(|error| query_storage_error(db, error))?;
                            let shell_notice = db
                                .model_shell_notice(&session.0, &id)
                                .map_err(|error| query_storage_error(db, error))?;
                            Ok(HistoryMessage {
                                id,
                                user_shell,
                                child,
                                shell_notice,
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
                    child_job: None,
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
                    .map(|row| {
                        Ok(ToolOpView {
                            child_job: if row.name == "subagent" {
                                db.child_tool_job(&session.0, &row.op)
                                    .map_err(|error| query_storage_error(db, error))?
                                    .map(Box::new)
                            } else {
                                None
                            },
                            output_presentation: row.output_presentation,
                            question: row.question,
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
                    })
                    .collect::<Result<Vec<_>, CoreError>>()?;
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
            let snapshot = effective.snapshot(composition, location_epoch.load(Ordering::SeqCst));
            let _ = ack.send(Ok(snapshot));
        }
        InboxMsg::ProviderCatalog { provider, ack } => {
            let preview = composition
                .auth_catalog_preview(
                    db,
                    &provider,
                    if provider == composition.catalog.provider {
                        &effective.model_id
                    } else {
                        ""
                    },
                )
                .await;
            let result = if let Ok(Some((catalog, readiness))) = &preview {
                let mut unchosen = effective.clone();
                if provider != composition.catalog.provider {
                    unchosen.model_id.clear();
                    unchosen.variant = None;
                }
                Ok(unchosen.snapshot_catalog(
                    composition,
                    location_epoch.load(Ordering::SeqCst),
                    catalog,
                    readiness.clone(),
                ))
            } else if let Err(composition::LoadFailure::Configuration(diagnostic)) = preview {
                Err(CoreError::Diagnostic(diagnostic))
            } else if provider == composition.catalog.provider {
                Ok(effective.snapshot(composition, location_epoch.load(Ordering::SeqCst)))
            } else {
                composition
                    .provider_views
                    .get(&provider)
                    .map(|view| {
                        let mut unchosen = Effective::from_composition(composition);
                        unchosen.model_id.clear();
                        unchosen.variant = None;
                        unchosen.snapshot_catalog(
                            composition,
                            location_epoch.load(Ordering::SeqCst),
                            &view.catalog,
                            view.readiness("", None),
                        )
                    })
                    .ok_or_else(|| app_error("provider catalog unavailable"))
            };
            let _ = ack.send(result);
        }
        InboxMsg::ProviderAccounts {
            provider,
            action,
            ack,
        } => {
            let result = if action.is_some() {
                Err(CoreError::TurnBusy)
            } else {
                accounts::read(db, composition, provider)
            };
            let _ = ack.send(result);
        }
        InboxMsg::ProviderConnections { ack } => {
            let _ = ack.send(Ok(accounts::connections(composition)));
        }
        InboxMsg::SessionSelection {
            session,
            home,
            action,
            ack,
        } => {
            let result = (|| {
                if runtime.turn_active()
                    && !matches!(
                        action,
                        oc_core::queries::SessionSelectionAction::Current
                            | oc_core::queries::SessionSelectionAction::Model(_)
                            | oc_core::queries::SessionSelectionAction::Variant(_)
                            | oc_core::queries::SessionSelectionAction::Commit(_)
                    )
                {
                    return Err(CoreError::TurnBusy);
                }
                runtime
                    .open_session(&session.0)
                    .map_err(|error| runtime_query_error(db, error))?;
                if runtime.turn_active()
                    && matches!(
                        action,
                        oc_core::queries::SessionSelectionAction::Model(_)
                            | oc_core::queries::SessionSelectionAction::Variant(_)
                    )
                    && db
                        .session_meta(&session.0)
                        .map_err(|error| query_storage_error(db, error))?
                        .parent_id
                        .is_some()
                {
                    return Err(CoreError::TurnBusy);
                }
                let previous = selection::for_turn(db, composition, effective, &session.0)?;
                let selected = selection::apply(
                    db,
                    composition,
                    effective,
                    &session.0,
                    home,
                    action.clone(),
                    location_epoch.load(Ordering::SeqCst),
                )?;
                if matches!(
                    action,
                    oc_core::queries::SessionSelectionAction::Model(_)
                        | oc_core::queries::SessionSelectionAction::Variant(_)
                        | oc_core::queries::SessionSelectionAction::Commit(_)
                ) && (previous.provider_id != selected.provider_id
                    || previous.model_id != selected.model_id
                    || previous.variant.as_deref().filter(|v| *v != "default")
                        != selected.variant.as_deref().filter(|v| *v != "default"))
                {
                    runtime.publish_model_selection(
                        session.clone(),
                        selection::publication(
                            composition,
                            &selected,
                            location_epoch.load(Ordering::SeqCst),
                            &action,
                        ),
                    );
                }
                let snapshot =
                    selected.snapshot(composition, location_epoch.load(Ordering::SeqCst));
                Ok(snapshot)
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
                if let oc_core::queries::SessionSelectionAction::Commit(commit) = &action
                    && commit.binding.generation != location_epoch.load(Ordering::SeqCst)
                {
                    return Err(app_error("stale model commit scope"));
                }
                let selected = selection::home(db, composition, effective, &current, action)?;
                let snapshot =
                    selected.snapshot(composition, location_epoch.load(Ordering::SeqCst));
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
                Ok(effective.snapshot(composition, location_epoch.load(Ordering::SeqCst)))
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
                let session = sessions.get(runtime.location());
                let previous = session
                    .map(|session| selection::for_turn(db, composition, effective, session))
                    .transpose()?
                    .unwrap_or_else(|| effective.clone());
                let reminder = crate::plan::switched(
                    previous.agent_id.as_deref(),
                    selected.agent_id.as_deref(),
                    composition.parent_env.get("HOME").map(String::as_str),
                );
                let (epoch_key, epoch_value) = selection::next_legacy_epoch(db, composition)?;
                registry
                    .select_primary_with_reminder(
                        &id,
                        runtime.generation_id(),
                        db,
                        &[(epoch_key, epoch_value)],
                        session.and_then(|session| {
                            reminder.as_deref().map(|text| (session.as_str(), text))
                        }),
                    )
                    .map_err(|error| app_error(error.to_string()))?;
                *effective = selected;
                effective.legacy_epoch += 1;
                publish_workspace(runtime, composition, effective).map_err(app_error)?;
                Ok(effective.snapshot(composition, location_epoch.load(Ordering::SeqCst)))
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
                let catalog = composition
                    .catalog_for(&selected.provider_id)
                    .ok_or_else(|| app_error("selected provider unavailable"))?;
                let selection = crate::models::select_model(catalog, &selected.model_id)
                    .and_then(|base| {
                        crate::models::select_variant(&base, selected.variant.as_deref())
                    })
                    .map_err(app_error)?;
                let fallback = composition
                    .generation
                    .providers
                    .get(&catalog.provider)
                    .map(|provider| provider.options.native_fallback_limits)
                    .unwrap_or_default();
                let budget = crate::models::budget(&selection, 0, fallback);
                let reminders = composition
                    .dcp_config
                    .reminder_facts(&catalog.provider, &selection, &budget)
                    .map_err(app_error)?;
                Ok(DcpSnapshot {
                    availability: runtime
                        .compression_availability_for_profile(
                            &session.0,
                            selected
                                .agent_id
                                .as_ref()
                                .and_then(|id| composition.agents.get(id))
                                .map(|agent| (&agent.permissions, &agent.permission_rules)),
                        )
                        .map_err(runtime_error)?,
                    estimated_tokens_available: estimated_tokens.is_some(),
                    estimate_method: Default::default(),
                    accounting: accounting.clone(),
                    estimated_tokens: estimated_tokens.unwrap_or(0),
                    max_context: reminders.max_context,
                    reminders: Some(Box::new(reminders)),
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
            let result = runtime
                .shell_jobs
                .output(&session.0, &shell_id, offset, limit)
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
        InboxMsg::ShellJobs { session, ack } => {
            let result = runtime
                .shell_jobs
                .running(&session.0)
                .map_err(|error| query_storage_error(db, error));
            let _ = ack.send(result);
        }
        InboxMsg::ChildJobs { session, ack } => {
            let result = db
                .child_tool_jobs(&session.0)
                .map_err(|error| query_storage_error(db, error));
            let _ = ack.send(result);
        }
        InboxMsg::InterruptChild {
            session,
            selected,
            ack,
        } => {
            let result = if runtime.child_jobs.interrupt(&session.0, &selected) {
                Ok(())
            } else {
                Err(CoreError::TurnBusy)
            };
            let _ = ack.send(result);
        }
        InboxMsg::BackgroundChild {
            session,
            selected,
            ack,
        } => {
            let result = if runtime.child_jobs.background(&session.0, &selected) {
                Ok(())
            } else {
                Err(CoreError::TurnNotActive)
            };
            let _ = ack.send(result);
        }
        InboxMsg::ReadChild {
            session,
            selected,
            ack,
        } => {
            let result = (|| {
                let job = db
                    .child_job(&session.0, &selected.operation)
                    .map_err(|e| query_storage_error(db, e))?
                    .filter(|job| {
                        job.parent == session
                            && job.parent == selected.parent
                            && job.child == selected.child
                            && job.operation == selected.operation
                            && job.generation == selected.generation
                            && job.location == selected.location
                            && job.delivery_id == selected.delivery_id
                    })
                    .ok_or(CoreError::SessionNotFound)?;
                let child = &job.child.0;
                let total = db
                    .history_len(child)
                    .map_err(|e| query_storage_error(db, e))?;
                let mut rows = db
                    .read_history_page_typed(child, HISTORY_PAGE_LIMIT, None)
                    .map_err(|e| query_storage_error(db, e))?;
                rows.reverse();
                let rows = rows
                    .into_iter()
                    .map(|row| {
                        let child_history = db
                            .child_history(child, &row.id)
                            .map_err(|error| query_storage_error(db, error))?;
                        let shell_notice = db
                            .model_shell_notice(child, &row.id)
                            .map_err(|error| query_storage_error(db, error))?;
                        Ok(HistoryMessage {
                            id: row.id,
                            child: child_history,
                            shell_notice,
                            seq: row.seq,
                            role: if row.role == "user" {
                                Role::User
                            } else {
                                Role::Assistant
                            },
                            text: row.text,
                            user_shell: None,
                            model_switch: None,
                            turn: db
                                .history_turn(child, row.seq)
                                .map_err(|e| query_storage_error(db, e))?,
                        })
                    })
                    .collect::<Result<Vec<_>, CoreError>>()?;
                let reverted = db
                    .reverted_conversation(child)
                    .map_err(|e| query_storage_error(db, e))?;
                let child_job = db
                    .child_tool_job(&session.0, &job.operation)
                    .map_err(|e| query_storage_error(db, e))?
                    .map(Box::new);
                let title = job.description[..job
                    .description
                    .floor_char_boundary(oc_core::tool_output::PREVIEW_BYTES)]
                    .to_owned();
                Ok(HistoryPage {
                    parent_id: Some(job.parent.0.clone()),
                    title: Some(title),
                    child_job,
                    has_older: total > rows.len(),
                    has_newer: false,
                    total,
                    rows,
                    reverted,
                })
            })();
            let _ = ack.send(result);
        }
        InboxMsg::ShellSnapshot {
            session,
            shell_id,
            ack,
        } => {
            let result = runtime
                .shell_jobs
                .snapshot(&session.0, &shell_id)
                .map_err(|error| query_storage_error(db, error));
            let _ = ack.send(result);
        }
        InboxMsg::BackgroundShell {
            session,
            shell_id,
            ack,
        } => {
            let result = runtime
                .shell_jobs
                .background(&session.0, &shell_id)
                .map_err(|error| query_storage_error(db, error))
                .and_then(|changed| {
                    if changed {
                        Ok(())
                    } else {
                        Err(CoreError::TurnNotActive)
                    }
                });
            let _ = ack.send(result);
        }
        InboxMsg::Cancel { session, ack } => {
            let children = runtime.child_jobs.cancel_session(&session.0);
            let result = if runtime.shell_jobs.cancel_session(&session.0) || children {
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
                    let (catalog, provider) = selected.request(composition)?;
                    runtime
                        .admit_provider_variant(
                            catalog,
                            &selected.model_id,
                            selected.variant.as_deref(),
                            &provider,
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
    terminals: &terminals::Owner,
    authentication: &authentication::Owner,
) -> Result<WorkerOutcome, oc_core::queries::ServiceDiagnostic> {
    runtime.set_compaction_events(events);
    let mut pending_inputs = std::collections::VecDeque::new();
    'worker: loop {
        if let Some(prepared) = runtime.ready_session_move().await.map_err(|error| {
            runtime_issue(
                runtime.location(),
                &["session_move"],
                &RuntimeError::InvalidArgs(error),
            )
            .diagnostic
        })? {
            return Ok(WorkerOutcome::Move(Box::new(prepared)));
        }
        runtime.shell_jobs.deliver(events).map_err(|_| {
            runtime_issue(
                runtime.location(),
                &["shell", "delivery"],
                &RuntimeError::Storage,
            )
            .diagnostic
        })?;
        runtime
            .child_jobs
            .reap()
            .await
            .map_err(|e| storage_diagnostic(db.root(), &e))?;
        runtime
            .child_jobs
            .deliver(Some(events))
            .map_err(|e| storage_diagnostic(db.root(), &e))?;
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
            let (catalog, provider) = selected.request(composition).map_err(|_| {
                selection_diagnostic(
                    composition,
                    &["selection", "provider"],
                    oc_core::queries::ServiceCode::ModelUnavailable,
                    oc_core::queries::ServiceAction::SelectModel,
                )
            })?;
            let operation = runtime.deliver_compaction(
                &session,
                catalog,
                &selected.model_id,
                selected.variant.as_deref(),
                &provider,
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
                        Some(command) => query(db,runtime,composition,effective,registry,sessions,home_choices,location_epoch,suggestion_queue,title_work,terminals,authentication,command).await,
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
                    terminals,
                    authentication,
                    command,
                )
                .await;
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
                () = runtime.child_jobs.changed() => { continue; }
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
        if (matches!(&message, InboxMsg::Catalog { .. }) && composition.go_catalog.is_some()
            || matches!(&message, InboxMsg::ProviderCatalog { provider, .. } if composition.public_provider(provider)))
            && !provider_work.pending()
        {
            let requested = match &message {
                InboxMsg::ProviderCatalog { provider, .. } => Some(provider.as_str()),
                _ => None,
            };
            *provider_work =
                provider_catalog::ProviderWork::start_requested(composition, db, requested);
        }
        // Only this typed command creates an explicit bounded manual trigger.
        // Refusal precedes Submit, turn acceptance and provider dispatch.
        let mut manual_trigger = None;
        let message = match message {
            InboxMsg::Compress {
                session,
                focus,
                ack,
            } => {
                let admitted = (|| -> Result<_, CoreError> {
                    let selected = selection::for_turn(db, composition, effective, &session.0)?;
                    selected.admit_selection(composition)?;
                    let (catalog, provider) = selected.request(composition)?;
                    publish_workspace(runtime, composition, &selected).map_err(app_error)?;
                    runtime
                        .admit_provider_variant(
                            catalog,
                            &selected.model_id,
                            selected.variant.as_deref(),
                            &provider,
                        )
                        .map_err(runtime_error)?;
                    runtime
                        .admit_manual_compression(&session.0)
                        .map_err(app_error)
                })();
                match admitted {
                    Ok(trigger) => manual_trigger = Some(trigger),
                    Err(error) => {
                        let _ = ack.send(Err(error));
                        continue;
                    }
                }
                InboxMsg::Submit {
                    session,
                    text: compress_prompt(&focus),
                    selection: None,
                    ack,
                }
            }
            other => other,
        };
        if let InboxMsg::ChangeConversation { session, .. } = &message {
            title_work
                .lock()
                .expect("title work mutex")
                .cancel(&session.0);
        }
        match message {
            InboxMsg::ProviderAccounts {
                provider,
                action,
                ack,
            } => {
                return Ok(WorkerOutcome::Accounts {
                    provider,
                    action,
                    ack,
                });
            }
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
                    let (catalog, provider) = selected.request(composition)?;
                    runtime
                        .admit_provider_variant(
                            catalog,
                            &selected.model_id,
                            selected.variant.as_deref(),
                            &provider,
                        )
                        .map_err(runtime_error)?;
                    // The exact primary choice was admitted before this title
                    // agent may use its own pinned model.
                    let agent = composition.agents.get("title");
                    let (provider_id, id, variant) =
                        if let Some(raw) = agent.and_then(|a| a.model.as_deref()) {
                            let resolved = composition
                                .model_reference(&catalog.provider, raw)
                                .map_err(|_| app_error("title agent model unavailable"))?;
                            (
                                resolved.provider,
                                resolved.id,
                                agent.and_then(|a| a.variant.clone()).or(resolved.variant),
                            )
                        } else {
                            (
                                selected.provider_id.clone(),
                                selected.model_id,
                                agent.and_then(|a| a.variant.clone()).or(selected.variant),
                            )
                        };
                    let catalog = composition
                        .catalog_for(&provider_id)
                        .ok_or_else(|| app_error("title provider unavailable"))?;
                    let provider = composition
                        .request_provider(&provider_id)
                        .ok_or_else(|| app_error("title provider unavailable"))?;
                    let selection = crate::models::select_model(catalog, &id)
                        .and_then(|base| crate::models::select_variant(&base, variant.as_deref()))
                        .map_err(|_| app_error("title agent model/variant unavailable"))?;
                    runtime
                        .admit_provider_variant(
                            catalog,
                            &selection.id,
                            selection.variant.as_ref().map(|v| v.name.as_str()),
                            &provider,
                        )
                        .map_err(runtime_error)?;
                    let fallback = composition
                        .generation
                        .providers
                        .get(&catalog.provider)
                        .map(|p| p.options.native_fallback_limits)
                        .unwrap_or_default();
                    let budget = crate::models::budget(&selection, 256, fallback);
                    let input = vec![
                        crate::provider::InputItem::message(crate::provider::InputRole::Developer,
                            agent.map(|a| a.body.as_str()).unwrap_or("Generate a short session title from the user's request. Output only the title, in at most 100 characters.")),
                        runtime.environment_input(),
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
                        provider,
                        provider_id,
                    ))
                })();
                let (expected, expected_event, selection, input, output, provider, provider_id) =
                    match prepared {
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
                let provider = match crate::provider::context::RequestContext::capture(
                    db,
                    &composition.project,
                    &session.0,
                ) {
                    Ok(context) => provider.with_context(context.for_operation(&title_operation)),
                    Err(error) => {
                        let _ = ack.send(Err(runtime_error(error)));
                        continue;
                    }
                };
                let title_env = composition.parent_env.clone();
                let operation = async {
                    let generation =
                        tokio::time::timeout(std::time::Duration::from_secs(10), async {
                            let provider = crate::auth::prepare_request(
                                &provider,
                                db,
                                &title_env,
                                &provider_id,
                                &selection.id,
                                selection.variant.as_ref().map(|v| v.name.as_str()),
                                &cancel,
                            )
                            .await
                            .map_err(|_| crate::provider::ProviderError::InvalidConfig)?;
                            crate::provider::stream_input_counted(
                                &provider,
                                &selection.id,
                                selection.variant.as_ref(),
                                &input,
                                &[],
                                output,
                                &cancel,
                                &mut |_| {},
                                &mut || async {
                                    db.generation_dispatch(&session.0, &title_operation, "title")
                                        .map_err(|_| {
                                            crate::provider::ProviderError::DispatchRefused
                                        })
                                },
                            )
                            .await
                        })
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
                                query(db, runtime, composition, effective, registry, sessions, home_choices, location_epoch, suggestion_queue, title_work, terminals, authentication, command).await;
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
                        terminals,
                        authentication,
                        command,
                    )
                    .await;
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
                if provider_work.selected_pending() && composition.go_catalog.is_none() {
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
            message @ InboxMsg::UserShell { .. } => {
                if user_shell::run(
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
                    terminals,
                    authentication,
                    events,
                    inbox,
                    &mut pending_inputs,
                    title_rx,
                    message,
                )
                .await?
                {
                    break 'worker;
                }
            }
            message @ (InboxMsg::Submit { .. } | InboxMsg::SubmitFresh { .. }) => {
                let (session, text, fresh, captured_selection, ack) = match message {
                    InboxMsg::Submit {
                        session,
                        text,
                        selection,
                        ack,
                    } => (session, text, None, selection, ack),
                    InboxMsg::SubmitFresh {
                        session,
                        text,
                        selection,
                        ack,
                    } => (session, text, Some(selection), None, ack),
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
                let command_def = commands::definition(composition, invocation.as_deref());
                let cancel = AtomicBool::new(false);
                let is_fresh = fresh.is_some();
                // Resolve from the owning session, never whichever TUI tab was
                // most recently viewed. Title and inherited child requests use
                // this same selection and agent workspace.
                let (turn_selection, initial_selection, command_route) =
                    match (|| -> Result<_, CoreError> {
                        if is_fresh {
                            match db.session_meta(&session.0) {
                                Ok(_) => return Err(CoreError::SessionAlreadyExists),
                                Err(StorageError::SessionNotFound) => {}
                                Err(error) => return Err(app_error(error)),
                            }
                        }
                        let (selected, initial_selection) = match fresh {
                            Some(choice) => {
                                let (selected, record) = selection::fresh_captured(
                                    db,
                                    composition,
                                    effective,
                                    &session.0,
                                    choice,
                                    home_choices,
                                    location_epoch.load(Ordering::SeqCst),
                                )?;
                                (selected, Some(record))
                            }
                            None => {
                                let selected = if let Some(commit) = captured_selection {
                                    if command_def.is_some() {
                                        let previous = selection::for_turn(
                                            db,
                                            composition,
                                            effective,
                                            &session.0,
                                        )?;
                                        selection::command_commit(
                                            db,
                                            composition,
                                            &previous,
                                            &session.0,
                                            location_epoch.load(Ordering::SeqCst),
                                            &commit,
                                        )?
                                    } else {
                                        let previous = selection::for_turn(
                                            db,
                                            composition,
                                            effective,
                                            &session.0,
                                        )?;
                                        let action =
                                            oc_core::queries::SessionSelectionAction::Commit(
                                                commit,
                                            );
                                        let selected = selection::apply(
                                            db,
                                            composition,
                                            effective,
                                            &session.0,
                                            false,
                                            action.clone(),
                                            location_epoch.load(Ordering::SeqCst),
                                        )?;
                                        if previous.provider_id != selected.provider_id
                                            || previous.model_id != selected.model_id
                                            || previous
                                                .variant
                                                .as_deref()
                                                .filter(|v| *v != "default")
                                                != selected
                                                    .variant
                                                    .as_deref()
                                                    .filter(|v| *v != "default")
                                        {
                                            runtime.publish_model_selection(
                                                session.clone(),
                                                selection::publication(
                                                    composition,
                                                    &selected,
                                                    location_epoch.load(Ordering::SeqCst),
                                                    &action,
                                                ),
                                            );
                                        }
                                        selected
                                    }
                                } else {
                                    selection::for_turn(db, composition, effective, &session.0)?
                                };
                                (selected, None)
                            }
                        };
                        let (selected, command_route) = if let Some(def) = command_def {
                            let (selected, mut route) =
                                commands::prepare(composition, &selected, def)?;
                            if route.child.is_none() {
                                route.selection = Some(selection::command_record(
                                    db,
                                    composition,
                                    &session.0,
                                    &selected,
                                )?);
                            }
                            (selected, Some(route))
                        } else {
                            (selected, None)
                        };
                        selected.admit_selection(composition)?;
                        let (catalog, provider) = selected.request(composition)?;
                        runtime
                            .admit_provider_variant(
                                catalog,
                                &selected.model_id,
                                selected.variant.as_deref(),
                                &provider,
                            )
                            .map_err(runtime_error)?;
                        publish_workspace(runtime, composition, &selected).map_err(app_error)?;
                        Ok((selected, initial_selection, command_route))
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
                let (turn_catalog, turn_provider) = match turn_selection.request(composition) {
                    Ok(request) => request,
                    Err(error) => {
                        let _ = ack.send(Err(error));
                        continue;
                    }
                };
                let title_selection = (|| -> Result<_, String> {
                    let (provider_id, id, variant) = if let Some(raw) =
                        title_agent.and_then(|a| a.model.as_deref())
                    {
                        let resolved = composition.model_reference(&turn_catalog.provider, raw)?;
                        (
                            resolved.provider,
                            resolved.id,
                            title_agent
                                .and_then(|a| a.variant.clone())
                                .or(resolved.variant),
                        )
                    } else {
                        (
                            turn_selection.provider_id.clone(),
                            turn_selection.model_id.clone(),
                            title_agent
                                .and_then(|a| a.variant.clone())
                                .or(turn_selection.variant.clone()),
                        )
                    };
                    let catalog = composition
                        .catalog_for(&provider_id)
                        .ok_or("title provider unavailable")?;
                    let provider = composition
                        .request_provider(&provider_id)
                        .ok_or("title provider unavailable")?;
                    crate::models::select_model(catalog, &id)
                        .and_then(|base| crate::models::select_variant(&base, variant.as_deref()))
                        .map(|selection| (selection, provider, provider_id))
                        .map_err(|e| e.to_string())
                })();
                let (title_selection, title_provider, title_provider_id) = match title_selection {
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
                    catalog: turn_catalog,
                    model_id: turn_selection.model_id.clone(),
                    variant: turn_selection.variant.clone(),
                    // The runtime resolves its native default against known
                    // metadata and fallback caps; capacity is not a request.
                    max_output: 0,
                    provider: turn_provider.clone(),
                    cancel: &cancel,
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
                                    let provider =
                                        crate::provider::context::RequestContext::capture(
                                            db,
                                            &composition.project,
                                            &session.0,
                                        )
                                        .map(|context| {
                                            title_provider.with_context(
                                                context.for_operation(&title_operation),
                                            )
                                        });
                                    let selection = title_selection.clone();
                                    let prompt = title_prompt.clone();
                                    let instructions = title_agent
                                        .map(|agent| agent.body.clone())
                                        .unwrap_or_else(|| "Generate a short session title from the user's request. Output only the title, in at most 100 characters.".into());
                                    let fallback = composition
                                        .generation
                                        .providers
                                        .get(&title_provider_id)
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
                                        runtime.environment_input(),
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
                                        let title_db = db.shared_handle();
                                        let title_env = composition.parent_env.clone();
                                        let provider_id = title_provider_id.clone();
                                        work.tasks.push((
                                            session.0.clone(),
                                            tokio::spawn(async move {
                                                let title = async {
                                                    let provider = provider.ok()?;
                                                    let cancel = AtomicBool::new(false);
                                                    let generation = tokio::time::timeout(
                                                        std::time::Duration::from_secs(10),
                                                        async {
                                                            let provider = crate::auth::prepare_request(
                                                                &provider,
                                                                &title_db,
                                                                &title_env,
                                                                &provider_id,
                                                                &selection.id,
                                                                selection.variant.as_ref().map(|v| v.name.as_str()),
                                                                &cancel,
                                                            )
                                                            .await
                                                            .map_err(|_| crate::provider::ProviderError::InvalidConfig)?;
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
                                                        ).await
                                                        },
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
                            publish_tool_event(db, events, &session, id, event);
                        };
                        let mut report = if is_fresh {
                            runtime
                                .run_fresh_turn_with_reasoning_items_and_notice(
                                    params,
                                    initial_selection
                                        .as_ref()
                                        .map(|(key, value)| (key.as_str(), value.as_str())),
                                    command_route.as_ref(),
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
                                    manual_trigger,
                                    command_route.as_ref(),
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
                            () = runtime.child_jobs.changed() => {
                                runtime.child_jobs.reap().await.map_err(|e| storage_diagnostic(db.root(), &e))?;
                                runtime.child_jobs.deliver(Some(events)).map_err(|e| storage_diagnostic(db.root(), &e))?;
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
                                    runtime.child_jobs.cancel_session(&target.0);
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
                                    terminals,
                                    authentication,
                                    command,
                                ).await,
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
                    let service_warning_range = match &result {
                        Ok(report) => report.service_warning_range.clone(),
                        Err(_) => 0..0,
                    };
                    let event =
                        match result {
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
                                    service_warning_range,
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
                                        "turn incomplete: provider response ended early",
                                    )),
                                    warnings,
                                    service_warning_range,
                                }
                            }
                            Ok(report) => CoreEvent::TurnFailed {
                                session: session.clone(),
                                turn,
                                error: app_error(
                                    report.diagnostic.as_deref().unwrap_or("provider error"),
                                ),
                                warnings,
                                service_warning_range,
                            },
                            Err(error) => CoreEvent::TurnFailed {
                                session: session.clone(),
                                turn,
                                error: runtime_error(error),
                                warnings,
                                service_warning_range,
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
                        terminals,
                        authentication,
                        command,
                    )
                    .await;
                }
                if shutdown {
                    break;
                }
            }
            message => {
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
                    terminals,
                    authentication,
                    message,
                )
                .await
            }
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
    if !composition.dcp_config.commands_enabled && matches!(id, "dcp" | "dcp-compress") {
        return Err(RuntimeError::Compress("DCP commands disabled".into()));
    }
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
mod profile_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "application/command_tests.rs"]
mod command_tests;

#[cfg(test)]
#[path = "application/plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "application/dcp_controls_tests.rs"]
mod dcp_controls_tests;

#[cfg(test)]
#[path = "application/mcp_tests.rs"]
mod mcp_tests;

#[cfg(test)]
#[path = "application/user_shell_tests.rs"]
mod user_shell_tests;
