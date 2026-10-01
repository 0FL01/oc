//! Bounded query/action payloads shared by the native worker and any
//! frontend (T39).
//!
//! These are view-model DTOs: counts, ids and bounded previews only. No
//! storage handles, no transcripts beyond the requested page, no secrets.

use std::collections::BTreeMap;

use crate::domain::SessionId;
use crate::session::Role;

mod mcp_lookup;
pub use mcp_lookup::*;

/// Bounded root-session picker projection; IDs remain routing keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListEntry {
    pub id: SessionId,
    pub title: String,
    pub directory: Option<String>,
    pub created_at: String,
    /// Absent for archives predating update-time recording.
    pub updated_at: Option<String>,
    pub date_group: String,
    pub running: bool,
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPickerContext {
    pub all_projects: bool,
    pub project_name: Option<String>,
    /// Canonical owning checkout, or admitted Location directory without Git.
    pub canonical: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPickerAction {
    Rename(String),
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPickerResult {
    Renamed,
    Deleted(TabDeckSnapshot),
}

/// Published picker route. The selected root's bounded projection is prepared
/// before acceptance, so frontend refresh failures cannot undo the route.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionPickerOpen {
    pub session: SessionId,
    pub location: String,
    pub catalog: CatalogSnapshot,
    pub page: HistoryPage,
    pub deck: TabDeckSnapshot,
    pub previous_deck: TabDeckSnapshot,
}

/// Move only the durable conversation and saved provider context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationAction {
    Undo,
    Redo,
    Revert { message: crate::session::MessageId },
}

/// Result after the owner has stopped execution and committed the new point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSnapshot {
    pub session: SessionId,
    pub draft: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub reverted: Option<RevertedConversation>,
}

/// Durable staged boundary, independent of the requested history window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertedConversation {
    /// First hidden user message at the actual owner boundary.
    pub message: crate::session::MessageId,
    /// All hidden user messages in the saved active tail, including text-empty turns.
    pub user_messages: u64,
}

/// Durable independent root plus the selected user text, still unsent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkSessionSnapshot {
    pub session: SessionId,
    pub prompt: String,
}

/// One Location-verified session ID, without enumerating the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionProbe {
    /// No row and no Location binding exist for this ID.
    Absent,
    /// An existing root in the current Location.
    Root,
    /// An existing child in the current Location; never a root tab.
    Child,
}

/// Ordered root tabs in the current Location. `None` leaves Home sessionless,
/// even when other tabs are open; this is not a session creation request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabDeckSnapshot {
    /// Canonical Location served by the application at load/save time.
    pub location: String,
    /// Opaque token for the exact stored preference, or None if absent.
    /// Projected restores carry a non-writable token; carry it unchanged into
    /// a save so the owner can refuse writes that would erase hidden tabs.
    /// Adopt the returned token only after a save succeeds.
    pub revision: Option<String>,
    /// Existing Location-bound root sessions, in display order.
    pub sessions: Vec<SessionId>,
    /// Selected tab, or sessionless Home.
    pub active: Option<SessionId>,
}

impl TabDeckSnapshot {
    /// The stored preference contained tabs or an active route that could not
    /// be restored exactly. A projected deck is read-only until repaired.
    pub fn projected(&self) -> bool {
        self.revision
            .as_deref()
            .is_some_and(|revision| revision.starts_with("projected:"))
    }

    /// Mark an incomplete restore without losing the original CAS fingerprint.
    /// The revision remains opaque to consumers and cannot be used for a save.
    pub fn mark_projected(&mut self) {
        if !self.projected() {
            self.revision = Some(format!(
                "projected:{}",
                self.revision.as_deref().unwrap_or("")
            ));
        }
    }
}

/// Prefs key holding the persisted model selection JSON.
pub const PREF_MODEL_SELECTION: &str = "tui.model_selection";
/// Prefs key holding the persisted primary agent JSON.
pub const PREF_PRIMARY_AGENT: &str = "tui.primary_agent";

/// Scoped frontend selection action. Selecting a model restores its preference;
/// selecting `Variant(None)` explicitly clears the overlay. Legacy headless
/// `select_model` remains an exact model/variant action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelectionAction {
    /// Commit one exact captured composer choice, with its owner scope.
    Commit(ModelCommit),
    /// Read/initialize the session's selection.
    Current,
    /// Select a model, retaining the current choice or its remembered variant.
    Model(String),
    /// Select an exact variant, including explicit Default (`None`).
    Variant(Option<String>),
    /// Select an agent and restore that session/agent's model draft.
    Agent(String),
    /// Initialize a new Home route for the current agent.
    New(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectionBinding {
    pub location: Option<String>,
    pub generation: u64,
    pub provider: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelCommit {
    /// Frontend instance correlation, not authority. Zero is the direct API.
    #[serde(default)]
    pub caller: u64,
    pub binding: SelectionBinding,
    pub model_id: String,
    pub variant: Option<String>,
    /// Local composer revision for matching receipts; never owner authority.
    pub draft_revision: u64,
}

/// One committed history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    /// Exact storage-owned message identity, independent of paging sequence.
    pub id: crate::session::MessageId,
    /// Message sequence (ordering key for paging).
    pub seq: i64,
    /// `user` / `assistant`.
    pub role: Role,
    /// Message text.
    pub text: String,
    /// Safe projection of this row's turn; absent for legacy text-only rows.
    pub turn: Option<HistoryTurn>,
    /// Public model transition committed with the following accepted prompt.
    pub model_switch: Option<ModelSwitchNotice>,
}

/// Only the validated public model identity; never provider options or credentials.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
    /// `None` is the normalized Default variant.
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSwitchNotice {
    pub previous: ModelRef,
    pub current: ModelRef,
    /// Query/live projection from the current catalog; never written to history.
    #[serde(skip_serializing, default)]
    pub display_name: Option<String>,
}

/// Safe turn metadata and ordered bounded parts, projected from durable records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryTurn {
    /// Ordered owner event watermark for snapshot/live reconciliation.
    pub revision: u64,
    /// Actual generation dispatches attributed to this owning operation.
    pub physical_requests: u64,
    /// Physical assistant spans, including settled failed continuations.
    pub spans: Vec<AssistantSpan>,
    /// Stable journal turn id.
    pub id: String,
    /// Actual terminal/runtime status.
    pub status: String,
    /// Agent selected for this turn, when recorded.
    pub agent: Option<String>,
    /// Categorical presentation slot pinned by the owning generation.
    pub agent_color_index: Option<usize>,
    /// Display name pinned at generation time.
    pub model_label: String,
    /// Last input and summed output token counts; None means unreported.
    pub usage: Option<(u64, u64)>,
    /// Latest provider-reported generation input/output pair, independent of
    /// whether billed usage for every round of this turn is known.
    pub context_usage: Option<(u64, u64)>,
    /// Measured wall time, if recorded.
    pub duration_ms: Option<u64>,
    /// Measured provider-active time, if recorded.
    pub streamed_ms: Option<u64>,
    /// Ordered parts; identity is (turn id, part index), tool ids remain durable.
    pub parts: Vec<TranscriptPart>,
    /// Identity `(id, sequence)`, original order and status for each served part.
    pub part_states: Vec<PartState>,
    /// Parts outside the bounded projection (not deleted from the archive).
    pub omitted_parts: usize,
    /// At least one served field is a preview or was omitted.
    pub truncated: bool,
    /// Older journal has no trustworthy public part ordering/summary records.
    pub legacy_text_only: bool,
}

/// Safe durable retry notice. `at` is an epoch deadline, never a dispatch command.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetryFact {
    pub attempt: u32,
    pub at: u64,
    pub safe_error: String,
}

/// Irreducible physical-span lifecycle inside the existing turn journal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssistantSpan {
    /// Actual prepared request, independent of the current composer selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestIdentity>,
    pub id: String,
    pub step: u32,
    pub status: String,
    pub started: u64,
    pub completed: Option<u64>,
    pub retry: Option<RetryFact>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub finish: Option<String>,
}

/// Bounded public receipt for one physical prepared primary attempt. Hashes are
/// input/schema provenance receipts, not promises of provider cache reuse.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestIdentity {
    pub model: ModelRef,
    pub model_label: String,
    pub span: String,
    /// First journal input owned by this attempt (including its settled results).
    pub input_start: usize,
    pub context_limit: u64,
    pub input_limit: u64,
    pub output_limit: u64,
    pub estimated_input: u64,
    pub dcp_min_context: u64,
    pub dcp_max_context: u64,
    pub tool_fingerprint: String,
    pub context_fingerprint: String,
}

/// Metadata parallel to `HistoryTurn.parts`, shared by live checkpoint events
/// and replay. Sequence never changes when an earlier part is omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartState {
    /// Actual owning request label, never the later composer choice.
    pub model_label: Option<String>,
    /// Durable display sequence within its owning turn.
    pub sequence: usize,
    /// Recorded tool outcome, or parent turn state for text/reasoning.
    pub status: String,
    /// This part is a bounded preview.
    pub truncated: bool,
    /// Structured tool input exceeded the serving budget; op id remains usable.
    pub input_omitted: bool,
}

/// Public transcript content only. Never contains opaque provider payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
// One operation is the indivisible live/history presentation unit. Its compact
// frozen DCP snapshot intentionally shares the existing tool variant; producers
// bound the number of parts and transferred metadata rather than splitting it
// into a second event/message identity.
#[allow(clippy::large_enum_variant)]
pub enum TranscriptPart {
    /// Bounded text projected from an existing wire message.
    Text(String),
    /// Public reasoning summary, not encrypted continuation.
    Reasoning {
        /// Bounded public text.
        text: String,
        /// Measured summary window, when known.
        duration_ms: Option<u64>,
    },
    /// Existing operation record, retaining exact outcome and continuation id.
    Tool(ToolOpView),
}

/// One contiguous, bounded history page (oldest-first for rendering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPage {
    pub reverted: Option<RevertedConversation>,
    /// Durable hierarchy; child sessions suppress the automatic sidebar.
    pub parent_id: Option<String>,
    /// Existing session metadata; None honestly denotes an untitled session.
    pub title: Option<String>,
    /// Rows in render order.
    pub rows: Vec<HistoryMessage>,
    /// Total committed messages in the session.
    pub total: usize,
    /// Older rows exist before the first row of this page.
    pub has_older: bool,
    /// Newer rows exist after the last row of this page.
    pub has_newer: bool,
}

/// Bounded byte window of an existing durable tool result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputPage {
    /// Complete UTF-8 characters in this window.
    pub text: String,
    /// Complete stored result size in bytes.
    pub total_bytes: i64,
    /// Next byte boundary, absent when the result is exhausted.
    pub next_offset: Option<i64>,
}

/// One selectable model with its bounded variant list and limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelEntry {
    /// Exact model id (no provider prefix).
    pub id: String,
    /// Configured/discovered human name; exact id is the fallback.
    pub display_name: String,
    /// Selected provider's configured name, falling back to its id.
    pub provider_name: String,
    /// Explicit input/output tariffs. None is unknown, never free.
    pub price: Option<ModelPrice>,
    /// Declared variants in stable effective effort order, including disabled
    /// and reserved metadata. Available consumers omit those entries.
    pub variants: Vec<VariantEntry>,
    /// Context limit (0 when undeclared).
    pub context: u64,
    /// Distinguishes an explicit context value from the legacy zero fallback.
    pub context_known: bool,
    /// Output limit (0 when undeclared).
    pub output: u64,
    /// Distinguishes an explicit output value from the legacy zero fallback.
    pub output_known: bool,
}

/// Decimal tariffs per million tokens (strings preserve configured precision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPrice {
    /// Input tariff.
    pub input: String,
    /// Output tariff.
    pub output: String,
}

impl ModelPrice {
    /// Both explicitly declared tariffs must be zero.
    pub fn is_free(&self) -> bool {
        self.input.parse::<f64>() == Ok(0.0) && self.output.parse::<f64>() == Ok(0.0)
    }
}

/// One declared model variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEntry {
    /// Variant name.
    pub name: String,
    /// True when the variant is declared but disabled.
    pub disabled: bool,
    /// Declared reasoning effort, if any.
    pub reasoning_effort: Option<String>,
}

/// One selectable agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    /// Generation categorical slot over all admitted profiles, including children.
    pub color_index: usize,
    /// Profile id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Pinned model id, if any.
    pub model: Option<String>,
    /// Pinned variant, if any.
    pub variant: Option<String>,
}

/// Catalog plus the effective selection for the next turn.
/// Session autoaccept is independent of agent tool allow/deny rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutoAcceptState {
    /// Backend has no interactive permission-request/reply capability.
    #[default]
    Unsupported,
    /// Capability exists, but approvals are prompted.
    Disabled,
    /// Pending permission requests are automatically approved.
    Enabled,
}

/// Catalog plus effective next-turn and session capability state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Safe presentation settings and canonical admitted Location.
    pub chrome: TuiChrome,
    /// Honest autoaccept capability/state, never inferred from permission rules.
    pub auto_accept: AutoAcceptState,
    /// Selected provider id.
    pub provider: String,
    /// Sorted models.
    pub models: Vec<ModelEntry>,
    /// Effective model id.
    pub model_id: String,
    /// Effective variant.
    pub variant: Option<String>,
    /// Sorted agent profiles.
    pub agents: Vec<AgentEntry>,
    /// Effective primary agent id, if resolved.
    pub agent_id: Option<String>,
    /// Admitted command ids (templates stay in the application).
    pub commands: Vec<String>,
    /// Descriptions for the admitted commands in this generation; no templates.
    pub command_descriptions: BTreeMap<String, String>,
}

/// Presentation-only settings; no runtime policy or credentials.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuiChrome {
    /// Actual configuration generation used to authorize captured selections.
    pub selection_generation: u64,
    /// Independent, effective DCP transcript display controls.
    pub dcp: crate::dcp_view::DcpDisplayConfig,
    /// Admitted CLI session permission preference. Consumer registration is explicit.
    pub permissions_auto: bool,
    pub permission_shortcuts: PermissionShortcuts,
    /// Ordered, value-free diagnostics from admitted configuration sources.
    pub config_diagnostics: Vec<ConfigDiagnostic>,
    pub service_diagnostics: Vec<ServiceDiagnostic>,
    /// Additional admitted failures/notes omitted only from the bounded view.
    pub service_diagnostics_omitted: usize,
    /// Current generation's compiled-plugin requests; contains no executable identities.
    pub plugins: PluginInventory,
    /// Owner facts for this exact selection; ready means request-admissible, not connected.
    pub provider: Option<ProviderReadiness>,
    /// Unavailable saved agent/model/variant; absent when the exact choice is admitted.
    pub selection: Option<SelectionReadiness>,
    /// Canonical application Location, unknown in mock workers.
    pub location: Option<String>,
    /// Explicit debug.devtools override; absence uses the build channel.
    pub devtools: Option<bool>,
    /// Effective native build channel, projected by the composition owner.
    pub build_channel: TuiBuildChannel,
    /// session.sidebar == hide.
    pub sidebar_hidden: bool,
    /// Width occupied by configured vertical tabs (zero for horizontal).
    pub vertical_tabs_width: u16,
    /// Upstream tab indicator presentation; status is the default.
    pub tab_indicators: TabIndicators,
    /// tabs.scope == global; upstream defaults to cwd.
    pub sessions_all_projects: bool,
    /// Explicit terminal.copy selection; absence uses the UI's platform default.
    pub terminal_copy: Option<TerminalCopyMode>,
    /// Explicit config.animations; absence enables interface animations.
    pub animations: Option<bool>,
    /// Explicit admitted CLI session.tps; absence uses the pinned default true.
    pub session_tps: Option<bool>,
    /// Effective CLI diff layout and wrapping, presentation only.
    pub diffs: DiffSettings,
    /// Effective conversation bindings from the admitted Location configuration.
    pub conversation_shortcuts: ConversationShortcuts,
    /// Admitted leader.timeout (preferred over legacy leader_timeout), in milliseconds.
    pub leader_timeout_ms: Option<u64>,
    /// Admitted command.palette.show binding; None keeps the native default.
    pub command_palette_shortcut: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffSettings {
    pub view: DiffView,
    pub wrap: DiffWrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionShortcuts {
    pub fullscreen: String,
    pub exit: String,
}
impl Default for PermissionShortcuts {
    fn default() -> Self {
        Self {
            fullscreen: "ctrl+f".into(),
            exit: "ctrl+c,ctrl+d,ctrl+x q".into(),
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffView {
    #[default]
    Auto,
    Unified,
    Split,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffWrap {
    #[default]
    Word,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConfigDiagnostic {
    pub source: String,
    pub field: Vec<String>,
    pub kind: ConfigDiagnosticKind,
    pub action: ConfigDiagnosticAction,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDiagnosticKind {
    Unsupported,
    Invalid,
    Conflict,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDiagnosticAction {
    Skip,
    RetainNative,
}
impl ConfigDiagnostic {
    pub fn message(&self) -> &'static str {
        match self.kind {
            ConfigDiagnosticKind::Unsupported => "omitted unsupported legacy setting",
            ConfigDiagnosticKind::Invalid => "skipped malformed recognized value",
            ConfigDiagnosticKind::Conflict => "retained native value over legacy value",
        }
    }
}
impl std::fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}: {:?}/{:?}: {}",
            self.source,
            self.field.join("."),
            self.kind,
            self.action,
            self.message()
        )
    }
}

/// Payload-free service or fatal admission failure, shared with startup consumers.
/// Producers bound/sanitize identities; no remote exceptions or expanded values.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServiceDiagnostic {
    #[serde(default)]
    pub kind: ServiceKind,
    pub service: String,
    pub source: String,
    pub field: Vec<String>,
    pub stage: ServiceStage,
    pub code: ServiceCode,
    pub action: ServiceAction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    #[default]
    Mcp,
    Plugin,
    Provider,
    Configuration,
    Definition,
    Selection,
    Storage,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStage {
    Config,
    Capability,
    Admission,
    Dns,
    Connection,
    Initialize,
    Catalog,
    ModelCatalog,
    Cleanup,
    Call,
    Storage,
    Recovery,
    Query,
}

impl ServiceStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Capability => "capability",
            Self::Admission => "admission",
            Self::Dns => "DNS",
            Self::Connection => "connect",
            Self::Initialize => "initialize",
            Self::Catalog => "tools-list",
            Self::ModelCatalog => "models-list",
            Self::Cleanup => "cleanup",
            Self::Call => "call",
            Self::Storage => "storage",
            Self::Recovery => "recovery",
            Self::Query => "query",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCode {
    InvalidConfig,
    InvalidDocument,
    InvalidDefinition,
    MissingConfiguration,
    TrustRefused,
    SourceUnavailable,
    CapacityExceeded,
    DataRootBusy,
    UnsafeDataRoot,
    StorageUnavailable,
    RecoveryFailed,
    RuntimeFailed,
    QueryFailed,
    InvalidStoredState,
    AgentUnavailable,
    VariantUnavailable,
    ApprovalRequired,
    QuestionRequired,
    IgnoredSetting,
    UnsupportedPlugin,
    UnsupportedCapability,
    UnsupportedProtocol,
    MissingCredential,
    ProviderPending,
    ModelUnavailable,
    HttpFailure,
    InvalidHeader,
    AuthorizationHeaderConflict,
    HeaderConflict,
    InvalidCwd,
    PrivateHost,
    ResolutionFailed,
    ConnectionFailed,
    SpawnFailed,
    Deadline,
    Transport,
    Unauthorized,
    Forbidden,
    ProtocolMismatch,
    CatalogLimit,
    InvalidCatalog,
    CleanupFailed,
    Cancelled,
    UnsafeRetry,
}

impl ServiceCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfig => "invalid_config",
            Self::InvalidDocument => "invalid_document",
            Self::InvalidDefinition => "invalid_definition",
            Self::MissingConfiguration => "missing_configuration",
            Self::TrustRefused => "trust_refused",
            Self::SourceUnavailable => "source_unavailable",
            Self::CapacityExceeded => "capacity_exceeded",
            Self::DataRootBusy => "data_root_busy",
            Self::UnsafeDataRoot => "unsafe_data_root",
            Self::StorageUnavailable => "storage_unavailable",
            Self::RecoveryFailed => "recovery_failed",
            Self::RuntimeFailed => "runtime_failed",
            Self::QueryFailed => "query_failed",
            Self::InvalidStoredState => "invalid_stored_state",
            Self::AgentUnavailable => "agent_unavailable",
            Self::VariantUnavailable => "variant_unavailable",
            Self::ApprovalRequired => "approval_required",
            Self::QuestionRequired => "question_required",
            Self::IgnoredSetting => "ignored_setting",
            Self::UnsupportedPlugin => "unsupported_plugin",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::MissingCredential => "missing_credential",
            Self::ProviderPending => "provider_pending",
            Self::ModelUnavailable => "model_unavailable",
            Self::HttpFailure => "http_failure",
            Self::InvalidHeader => "invalid_header",
            Self::AuthorizationHeaderConflict => "authorization_header_conflict",
            Self::HeaderConflict => "header_conflict",
            Self::InvalidCwd => "invalid_cwd",
            Self::PrivateHost => "private_host",
            Self::ResolutionFailed => "resolution_failed",
            Self::ConnectionFailed => "connection_failed",
            Self::SpawnFailed => "spawn_failed",
            Self::Deadline => "deadline",
            Self::Transport => "transport",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::ProtocolMismatch => "protocol_mismatch",
            Self::CatalogLimit => "catalog_limit",
            Self::InvalidCatalog => "invalid_catalog",
            Self::CleanupFailed => "cleanup_failed",
            Self::Cancelled => "cancelled",
            Self::UnsafeRetry => "unsafe_retry",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    ReviewConfiguration,
    RetryConnection,
    SignInUnsupported,
    RestartApplication,
    RefreshCatalog,
    SelectModel,
    SelectAgent,
    SelectVariant,
    WaitForProvider,
    CloseOtherOwner,
    ReviewDataDirectory,
    ReviewStorage,
    ReviewRecovery,
    ReduceCapacity,
    UseInteractiveTui,
}

impl std::fmt::Display for ServiceDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}: {} (retryable={}); {} {}: {}",
            match self.kind {
                ServiceKind::Mcp => "mcp",
                ServiceKind::Plugin => "plugin",
                ServiceKind::Provider => "provider",
                ServiceKind::Configuration => "configuration",
                ServiceKind::Definition => "definition",
                ServiceKind::Selection => "selection",
                ServiceKind::Storage => "storage",
                ServiceKind::Runtime => "runtime",
            },
            self.service,
            self.stage.as_str(),
            self.code.as_str(),
            matches!(
                self.action,
                ServiceAction::RetryConnection | ServiceAction::RefreshCatalog
            ),
            self.source,
            self.field.join("."),
            match self.action {
                ServiceAction::ReviewConfiguration => "review configuration",
                ServiceAction::RetryConnection => "retry connection",
                ServiceAction::SignInUnsupported =>
                    "native OAuth sign-in unsupported; review credentials",
                ServiceAction::RestartApplication => "restart application; retry unsafe",
                ServiceAction::RefreshCatalog => "reload configuration to refresh catalog",
                ServiceAction::SelectModel => "select an admitted model",
                ServiceAction::SelectAgent => "select an admitted primary agent",
                ServiceAction::SelectVariant => "select an admitted model variant or Default",
                ServiceAction::WaitForProvider => "wait for catalog; selection remains unchanged",
                ServiceAction::CloseOtherOwner =>
                    "close the other oc process using this data directory",
                ServiceAction::ReviewDataDirectory =>
                    "choose a private, owned data directory; check access",
                ServiceAction::ReviewStorage =>
                    "check storage integrity, access and available space",
                ServiceAction::ReviewRecovery =>
                    "repair interrupted-operation storage before restarting; do not replay unknown effects",
                ServiceAction::ReduceCapacity =>
                    "reduce the admitted input or resource size; limits remain enforced",
                ServiceAction::UseInteractiveTui =>
                    "use the interactive TUI to answer; --auto only accepts permissions",
            }
        )
    }
}

impl ServiceDiagnostic {
    /// An unsent investigation payload built only from the safe owner DTO.
    pub fn investigation_draft(&self) -> String {
        format!(
            "Investigate this admitted failure without changing current policy or selection. Explain the recorded cause and allowed next action.\n{self}"
        )
    }
}

/// Opaque presentation identity for an exact owner-held but unavailable choice.
/// The raw saved value remains in the application/store, never in this view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionReadiness {
    pub requested: String,
    pub diagnostic: ServiceDiagnostic,
}

impl std::fmt::Display for SelectionReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Saved selection {} unavailable: {}",
            self.requested, self.diagnostic
        )
    }
}

/// Transient provider/catalog facts, owned by the existing application generation.
/// No endpoint, credential, raw exception or untrusted identity crosses this DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReadiness {
    pub service: String,
    /// Opaque exact model identity; raw unavailable IDs may be URLs or secrets.
    pub model: String,
    pub status: ProviderStatus,
    pub catalog_status: ProviderStatus,
    pub diagnostic: Option<ServiceDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStatus {
    /// Credential and exact model metadata are admitted; no network health claim.
    Ready,
    Pending,
    Unavailable,
    Failed,
}

impl ProviderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Pending => "pending",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ProviderReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Provider request — {}; catalog — {}",
            self.status.as_str(),
            self.catalog_status.as_str()
        )?;
        if self.status != ProviderStatus::Ready {
            write!(f, "; selection={}", self.model)?;
        }
        if let Some(diagnostic) = &self.diagnostic {
            write!(
                f,
                "; {}: {diagnostic}",
                if self.status == ProviderStatus::Ready {
                    "catalog attempt"
                } else {
                    "request refusal"
                }
            )?;
        }
        Ok(())
    }
}

/// A bounded presentation window, not an admission/activation limit. The owner
/// classifies every request and counts omitted rows without retaining raw text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginInventory {
    pub entries: Vec<PluginEntry>,
    pub omitted: usize,
    pub omitted_failed: usize,
    /// Deduplicated compiled capabilities bound by this complete generation.
    pub active_modules: Vec<NativePlugin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativePlugin {
    Dcp,
    OpenProxyModels,
}

impl NativePlugin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dcp => "DCP",
            Self::OpenProxyModels => "OpenProxy models",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    Active,
    Failed,
    Ignored,
}

impl PluginStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Ignored => "ignored",
        }
    }
}

/// One exact requested identity maps to a compiled identity only on successful
/// admission. Requested ids are opaque hashes, never package/URL/path fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    pub requested: String,
    pub current: Option<String>,
    pub module: Option<NativePlugin>,
    pub status: PluginStatus,
    pub source: String,
    pub field: Vec<String>,
    pub diagnostic: Option<ServiceDiagnostic>,
}

impl PluginEntry {
    pub fn label(&self) -> &'static str {
        self.module.map_or_else(
            || match self.status {
                PluginStatus::Ignored => "Authoring-only marker",
                _ => "Unsupported plugin",
            },
            NativePlugin::label,
        )
    }
}

impl std::fmt::Display for PluginEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plugin {}: {}; requested={}; current={}; {} {}",
            self.label(),
            self.status.as_str(),
            self.requested,
            self.current.as_deref().unwrap_or("none"),
            self.source,
            self.field.join(".")
        )?;
        if let Some(diagnostic) = &self.diagnostic {
            write!(f, "; {diagnostic}")?;
        } else if self.status == PluginStatus::Ignored {
            write!(
                f,
                "; authoring-only plugin ignored; no package code was loaded"
            )?;
        }
        Ok(())
    }
}

/// Exact ephemeral resource scope. An instance changes even on a return to the
/// same Location; neither connected labels nor controls are restored from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpBinding {
    pub location: String,
    pub generation: u64,
    pub instance: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpStatus {
    Pending,
    Connected,
    Disabled,
    Failed,
    NeedsAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAction {
    Connect,
    Disconnect,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpControl {
    pub binding: McpBinding,
    /// Opaque entry identity from the snapshot, never an endpoint/command.
    pub server: String,
    pub action: McpAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerSnapshot {
    pub id: String,
    pub name: String,
    pub configured_enabled: bool,
    pub status: McpStatus,
    pub pending_action: Option<McpAction>,
    pub tools: usize,
    pub diagnostic: Option<ServiceDiagnostic>,
    pub actions: Vec<McpAction>,
}

/// Bounded by admitted inventory (64); tool/resource payloads never enter UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSnapshot {
    pub binding: McpBinding,
    pub revision: u64,
    pub servers: Vec<McpServerSnapshot>,
}

/// Presentation-only key strings, ready for the UI's existing key parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationShortcuts {
    /// Effective leader; an empty string disables it.
    pub leader: String,
    /// Resolved undo binding; comma-separated alternatives, empty means disabled.
    pub undo: String,
    /// Resolved redo binding; comma-separated alternatives, empty means disabled.
    pub redo: String,
}

impl Default for ConversationShortcuts {
    fn default() -> Self {
        Self {
            leader: "ctrl+x".into(),
            undo: "ctrl+x u".into(),
            redo: "ctrl+x r".into(),
        }
    }
}

/// Mouse text selection/copy behavior from the effective Location config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCopyMode {
    /// Copy selected text on a dragging mouse-up.
    Select,
    /// Copy an existing selection on right mouse-down.
    Manual,
}

/// Selected session tab indicator presentation (upstream `tabs.indicators`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TabIndicators {
    /// Blank while idle; busy state displays the first spinner frame.
    #[default]
    Status,
    /// Always display the selected tab number.
    Numbers,
}

/// Native debug builds map to upstream's local channel; release builds to packaged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TuiBuildChannel {
    /// Locally built debug executable.
    Local,
    /// Packaged/release executable (also the neutral mock default).
    #[default]
    Packaged,
}

impl TuiChrome {
    pub fn leader_timeout_ms(&self) -> u64 {
        self.leader_timeout_ms.unwrap_or(2000)
    }

    /// Match upstream `debug.devtools ?? channel == local` without fake controls.
    pub fn devtools_visible(&self) -> bool {
        self.devtools
            .unwrap_or(self.build_channel == TuiBuildChannel::Local)
    }

    /// Match upstream `config.animations ?? true`.
    pub fn animations_enabled(&self) -> bool {
        self.animations.unwrap_or(true)
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;

    #[test]
    fn v03_devtools_override_and_channel_default() {
        for channel in [TuiBuildChannel::Local, TuiBuildChannel::Packaged] {
            let mut chrome = TuiChrome {
                build_channel: channel,
                ..Default::default()
            };
            assert_eq!(chrome.devtools_visible(), channel == TuiBuildChannel::Local);
            chrome.devtools = Some(false);
            assert!(!chrome.devtools_visible());
            chrome.devtools = Some(true);
            assert!(chrome.devtools_visible());
        }
    }
}

/// Result of one successful Location switch inside a running application.
///
/// The target generation was built completely before this value is produced:
/// the frontend adopts the returned session and catalog together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationSnapshot {
    /// Canonical Location id (project path) now served.
    pub location: String,
    /// Monotonic owner Location epoch; compare with async file suggestions.
    pub generation: u64,
    /// Session bound to that Location (new, or the recorded one on return).
    pub session: String,
    /// Catalog for the new generation.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for existing application API callers. May contain
    /// configured paths/keys; frontends must use `notices` instead.
    pub diagnostics: Vec<String>,
    /// Source-based, allowlisted warnings for interactive presentation.
    pub notices: Vec<StartupNotice>,
}

/// Published Location generation for a sessionless Home composer.
/// No root session is created by this switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeLocationSnapshot {
    /// Canonical Location id now served.
    pub location: String,
    /// Monotonic owner Location epoch; compare with async file suggestions.
    pub generation: u64,
    /// Current Home choice and catalog in the target Location.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for application callers; frontends display `notices`.
    pub diagnostics: Vec<String>,
    /// Allowlisted interactive warnings.
    pub notices: Vec<StartupNotice>,
}

/// Rebuilt configuration for the current canonical Location. Existing roots,
/// selected tab and stored deck are untouched; the catalog is for the next turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadLocationSnapshot {
    /// Canonical Location still served by the application.
    pub location: String,
    /// Monotonic owner epoch, including reloads and A→B→A switches.
    pub generation: u64,
    /// Fresh catalog and effective selection for this generation.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for application callers; UIs display `notices`.
    pub diagnostics: Vec<String>,
    /// Allowlisted interactive warnings.
    pub notices: Vec<StartupNotice>,
}

/// Bounded file candidates from the application owner's current Location.
/// Consumers must compare both `location` and `generation` with their current
/// route before showing an asynchronously delivered response (including A→B→A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSuggestionsSnapshot {
    /// Canonical Location that owned the file walk.
    pub location: String,
    /// Monotonic owner Location epoch; changes on every successful switch.
    pub generation: u64,
    /// Sorted Location-relative paths; no file contents.
    pub paths: Vec<String>,
    /// More matches exist or the traversal budget was reached.
    pub truncated: bool,
}

/// Source of non-fatal composition/persisted-selection diagnostics. Never
/// contains user input or text from the underlying error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupNotice {
    /// Unsupported, invalid, or conflicting compaction settings were normalized.
    CompactionConfig,
    /// Individual MCP config/capability entries failed admission.
    McpConfig,
    /// Agent, skill or command definition could not be admitted.
    Definitions,
    /// Plugin requests failed admission or were ignored without execution.
    Plugin,
    /// Native DCP settings include ignored or unsupported options.
    Dcp,
    /// Ordered instruction sources produced diagnostics.
    Instructions,
    /// A stored model/agent selection could not be applied.
    SavedSelection,
}

/// Skill catalog card: metadata only, never bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCard {
    /// Directory id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
}

/// DCP context/stats snapshot (counts only, no transcript).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcpSnapshot {
    /// False if the bounded active projection cannot be hydrated. Numeric zero
    /// then denotes unavailable, never a fabricated UTF-8/serialized estimate.
    pub estimated_tokens_available: bool,
    pub estimate_method: crate::dcp_view::DcpEstimateMethod,
    /// Durable branch/revision accounting; absent for unmeasured legacy state.
    pub accounting: Option<crate::dcp_view::DcpAccounting>,
    /// Estimated context tokens.
    pub estimated_tokens: u64,
    /// Effective max context tokens.
    pub max_context: u64,
    /// Turns since the last successful compression.
    pub turns_since_compress: u64,
    /// Stored compression blocks for the session.
    pub blocks: usize,
    /// Successful compressions.
    pub compressions: u64,
    /// Emitted nudges.
    pub nudges: u64,
    /// Recorded prune marks.
    pub prunes: u64,
}

/// One tool operation as recorded durably.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOpView {
    /// Bounded, terminal question presentation. Never restores an active form.
    pub question: Option<crate::question::QuestionResult>,
    /// Owner-validated compression topic; never extracted from raw JSON text.
    /// Pending streams/unavailable arguments and other tools have no topic.
    pub dcp_topic: Option<String>,
    /// Frozen successful DCP run; None honestly denotes unavailable metadata.
    pub dcp: Option<crate::dcp_view::DcpRunSnapshot>,
    /// Confirmed mutation metadata; absent for legacy records and other tools.
    pub patch_effects: Option<crate::patch::PatchEffects>,
    /// Operation id.
    pub op: String,
    /// Insertion-order cursor for newest-first paging.
    pub rowid: i64,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `denied` / `cancelled` / `unknown`.
    pub state: String,
    /// Recorded input JSON (bounded preview by the consumer).
    pub input: Option<String>,
    /// Recorded outcome preview (bounded; never the whole result).
    pub output: Option<String>,
    /// Full stored output size in bytes.
    pub output_bytes: i64,
    /// Whether [`ToolOpView::output`] is a truncated preview.
    pub output_truncated: bool,
}

/// One bounded, newest-first tool operation page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOpPage {
    /// Rows newest-first.
    pub rows: Vec<ToolOpView>,
    /// Total recorded operations for the session.
    pub total: usize,
    /// Older operations exist before the last row of this page.
    pub has_older: bool,
}
/// Committed automatic result, addressed to its immutable source session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellNotice {
    /// Owning session, independent of the current UI Location/tab.
    pub session: crate::domain::SessionId,
    /// Existing operation identity also identifies the owned shell job.
    pub shell_id: String,
    /// Stable deduplication identity.
    pub delivery_id: String,
    /// Immutable history message committed with delivery.
    pub message_id: String,
    /// Durable terminal state.
    pub state: String,
    /// Bounded readable facts identical to provider/history input.
    pub text: String,
}

/// Current supervisor-owned running job, independent of immutable tool results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellJob {
    /// Exact source session, including child-origin work.
    pub session: crate::domain::SessionId,
    /// Original tool operation / capture identity.
    pub shell_id: String,
    /// Original admitted execution Location.
    pub location: String,
    /// Original configuration generation.
    pub generation: u64,
    /// Original turn identity.
    pub turn: String,
    /// Actual issuing-request model.
    pub model: String,
    /// Actual issuing-request provider.
    pub provider: String,
    /// Validated admitted command (legacy argv stays literal).
    pub command: String,
    /// Verified process-group leader, never a signal capability for callers.
    pub pid: Option<i32>,
    /// Whether the original foreground await has been converted/released.
    pub background: bool,
}

/// Bounded recent snapshot from actual drains or the frozen terminal capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSnapshot {
    /// Original source/operation/execution identity, even after list removal.
    pub job: ShellJob,
    /// Current owner state; never the original running result.
    pub state: String,
    /// Monotonic actual drained stdout byte cursor.
    pub stdout_cursor: u64,
    /// Monotonic actual drained stderr byte cursor.
    pub stderr_cursor: u64,
    /// Either bounded capture window discarded earlier bytes.
    pub truncated: bool,
    /// Recent text, bounded independently of stream retention.
    pub text: String,
}
