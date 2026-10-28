//! Typed vertical slice for T03: commands, queries, session worker,
//! scripted provider, cancellation and bounded channels.
//!
//! One `SessionWorker` owns all sessions and at most one active turn. Long
//! provider streams never block `CancelTurn` or read queries: the worker
//! `select!`s the inbox against the next-chunk timer. No real network or
//! SQLite happens here; persistence/resume arrive in T04/T05.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, oneshot};

use crate::domain::SessionId;
use crate::queries::{
    CatalogSnapshot, DcpSnapshot, FileSuggestionsSnapshot, HistoryPage, HomeLocationSnapshot,
    LocationSnapshot, ReloadLocationSnapshot, SessionProbe, SkillCard, TabDeckSnapshot, ToolOpPage,
};
use crate::session::{CoreError, MAX_INPUT_BYTES, MAX_QUEUE_ITEMS, Message, MessageId, Role};

/// Maximum UTF-8 bytes stored for a manually selected session title.
pub const MAX_SESSION_TITLE_BYTES: usize = 256;

// Deliberately conservative pictographic endpoints for emoji ZWJ sequences.
// Text, digits, whitespace and standalone variation selectors never qualify.
fn emoji_join_endpoint(c: char) -> bool {
    !matches!(c, '\u{1f3fb}'..='\u{1f3ff}')
        && matches!(c,
        '\u{2640}' | '\u{2642}' | '\u{2695}' | '\u{2764}'
        | '\u{1f300}'..='\u{1f5ff}' | '\u{1f600}'..='\u{1f64f}'
        | '\u{1f680}'..='\u{1f6ff}' | '\u{1f900}'..='\u{1f9ff}'
        | '\u{1fa70}'..='\u{1faff}')
}

fn emoji_joiner_context(title: &str, at: usize) -> bool {
    let mut before = title[..at].chars().rev();
    // An emoji presentation selector or skin-tone modifier belongs to the
    // preceding visible emoji, rather than being an emoji endpoint itself.
    let base = match before.next() {
        Some('\u{fe0f}' | '\u{1f3fb}'..='\u{1f3ff}') => before.next(),
        other => other,
    };
    base.is_some_and(emoji_join_endpoint)
        && title[at + '\u{200d}'.len_utf8()..]
            .chars()
            .next()
            .is_some_and(emoji_join_endpoint)
}

/// Normalize an interactive title without allowing invisible/control text.
/// Used again by the owner because inbox messages can bypass the handle.
pub fn normalized_session_title(title: &str) -> Option<&str> {
    // Unicode format (Cf) characters do not render as text. This includes
    // zero-width spaces, soft hyphens, bidi controls and tag characters.
    // ZWJ is admitted only inside a visible emoji sequence.
    // The combining grapheme joiner (Mn) also changes rendering invisibly.
    if title.char_indices().any(|(at, c)| {
        c.is_control()
            || (c == '\u{200d}' && !emoji_joiner_context(title, at))
            || matches!(c,
                '\u{00ad}' | '\u{034f}' | '\u{0600}'..='\u{0605}' | '\u{061c}' | '\u{06dd}'
                | '\u{070f}' | '\u{0890}'..='\u{0891}' | '\u{08e2}' | '\u{180e}'
                | '\u{200b}'..='\u{200c}' | '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}'
                | '\u{feff}' | '\u{fff9}'..='\u{fffb}' | '\u{110bd}'
                | '\u{110cd}' | '\u{13430}'..='\u{1343f}'
                | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}'
                | '\u{e0001}' | '\u{e0020}'..='\u{e007f}')
    }) {
        return None;
    }
    let trimmed = title.trim();
    // Variation selectors are legitimate after an emoji, but not a title on
    // their own. The same applies to blank glyphs that `str::trim` does not
    // recognize as whitespace.
    let visible = trimmed.chars().any(|c| {
        !c.is_whitespace()
            && !matches!(c,
                '\u{115f}'..='\u{1160}' | '\u{180b}'..='\u{180d}'
                | '\u{180f}' | '\u{2800}' | '\u{3164}' | '\u{fe00}'..='\u{fe0f}'
                | '\u{ffa0}' | '\u{e0100}'..='\u{e01ef}')
    });
    if !visible || trimmed.len() > MAX_SESSION_TITLE_BYTES {
        None
    } else {
        Some(trimmed)
    }
}

/// Opaque turn id for the worker (monotonic `t0001`, …).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WorkerTurnId(pub String);

/// A bounded append/read receipt for the existing shared prompt-history port.
pub struct PromptHistoryReceipt(oneshot::Receiver<Result<Vec<String>, CoreError>>);
impl PromptHistoryReceipt {
    pub fn try_result(&mut self) -> Option<Result<Vec<String>, CoreError>> {
        match crate::wake::try_reply(&mut self.0) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }
}

/// Explicit first-turn choice for a new root. `None` at the API boundary uses
/// the application's current Home choice; `Some` pins these exact values to
/// the new session and turn. The owner validates the model, variant and agent
/// against its current Location before creating anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FreshSelection {
    /// Optional composer scope; explicit programmatic callers retain admission.
    pub binding: Option<crate::queries::SelectionBinding>,
    /// Selected primary agent, if any.
    pub agent_id: Option<String>,
    /// Exact selected model id.
    pub model_id: String,
    /// Exact variant, or the explicit Default variant.
    pub variant: Option<String>,
}

/// Captured placement/selection for an explicit user command, not a model turn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UserShellSelection {
    Existing(Option<crate::queries::ModelCommit>),
    Fresh(Option<FreshSelection>),
}

/// The owner admits history and command intent before launching its supervised
/// job. Dropping a receipt does not replay or relinquish the owned operation.
pub struct UserShellReceipt {
    result: oneshot::Receiver<Result<String, CoreError>>,
    cancel: Arc<AtomicBool>,
}

impl UserShellReceipt {
    pub fn try_result(&mut self) -> Option<Result<String, CoreError>> {
        match crate::wake::try_reply(&mut self.result) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }

    /// Cancel pending admission without blocking the frontend/approval owner.
    /// After acceptance, use CoreApp::cancel_shell with the returned operation.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub async fn wait(&mut self) -> Result<String, CoreError> {
        (&mut self.result).await.map_err(|_| CoreError::Shutdown)?
    }
}

/// One enqueued submission's acceptance receipt. The application owns the
/// operation even if this receipt is dropped; cancel/shutdown it through CoreApp.
pub struct SubmissionReceipt(oneshot::Receiver<Result<WorkerTurnId, CoreError>>);

/// Exact owner acknowledgement for a commit that creates no user input/turn.
pub struct ModelCommitReceipt(oneshot::Receiver<Result<CatalogSnapshot, CoreError>>);
/// Exact acknowledgement for one already-enqueued steer/queue operation.
pub struct EnqueueReceipt(oneshot::Receiver<Result<crate::queries::InboxItem, CoreError>>);
impl EnqueueReceipt {
    pub fn try_result(&mut self) -> Option<Result<crate::queries::InboxItem, CoreError>> {
        match crate::wake::try_reply(&mut self.0) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }
}
impl ModelCommitReceipt {
    pub fn try_result(&mut self) -> Option<Result<CatalogSnapshot, CoreError>> {
        match crate::wake::try_reply(&mut self.0) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }
}

impl SubmissionReceipt {
    /// Poll without waiting for network, provider or durable acceptance.
    pub fn try_result(&mut self) -> Option<Result<WorkerTurnId, CoreError>> {
        match crate::wake::try_reply(&mut self.0) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }

    /// Wait for the owner's acceptance decision on this already-enqueued
    /// request. A closed owner is a failure, never an accepted turn.
    pub async fn wait(&mut self) -> Result<WorkerTurnId, CoreError> {
        (&mut self.0).await.map_err(|_| CoreError::Shutdown)?
    }
}

/// Typed application events (live hints + durable outcomes for T03).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoreEvent {
    /// Native TUI projection cursor. Frontends discard hints already covered by
    /// an authoritative snapshot, without dropping newer queued deltas.
    Projection {
        revision: u64,
        event: Box<CoreEvent>,
    },
    /// A committed shared deck changed. Read its current bounded snapshot.
    WorkspaceChanged {
        location: String,
    },
    /// A captured idle watermark was acknowledged, without changing the deck.
    SessionViewed {
        session: SessionId,
        viewed_seq: u64,
    },
    /// A disposable live hint was lost; replace the bounded projection.
    ProjectionGap,
    /// The owner disappeared. This is not evidence that a mutation failed.
    /// An interactive client then reattaches on its own; others stop.
    OwnerDisconnected,
    /// The same handle serves a fresh owner attachment. Nothing was resent;
    /// projections, consumer registrations and catalogs must be re-read.
    OwnerReattached,
    /// Reattachment cannot succeed (protocol/binary changed, unsafe root).
    OwnerUnrecoverable {
        reason: String,
    },
    /// The session's pending inbox changed. `delivered` names steer items
    /// that just joined the running turn as user messages.
    InboxChanged {
        session: SessionId,
        items: Vec<crate::queries::InboxItem>,
        delivered: Vec<crate::queries::InboxDelivered>,
    },
    /// Durable scoped choice changed; this is not a request/assistant identity.
    SessionModelSelected {
        session: SessionId,
        commit: crate::queries::ModelCommit,
    },
    /// A public selection notice is durable; read its current-branch projection.
    SelectionNoticeCommitted {
        session: SessionId,
        message: crate::session::MessageId,
    },
    /// Same-ID placement already committed at a durable terminal boundary.
    SessionMoved {
        operation: String,
        session: SessionId,
        directory: String,
        /// Present only when the application's current root followed the move.
        location: Option<Box<LocationSnapshot>>,
    },
    /// Automatic shell result; history delivery is already committed.
    ShellNotice(crate::queries::ShellNotice),
    ExecuteNotice(crate::queries::ExecuteNotice),
    ChildNotice(crate::queries::ChildNotice),
    /// A live root's automatic continuation was refused before turn acceptance.
    /// The durable child report remains available; this is not a replay request.
    ParentContinuationRefused {
        /// Original parent session.
        session: SessionId,
        /// Safe typed refusal without report/provider payloads.
        diagnostic: crate::queries::ServiceDiagnostic,
    },
    /// Coalesced actual supervisor drain/lifecycle change; read current facts.
    ShellChanged {
        session: SessionId,
    },
    /// Coalesced program progress or lifecycle change; read current facts.
    ExecuteChanged {
        session: SessionId,
    },
    /// Actual PTY exit/removal; frontend reads current bounded owner facts.
    TerminalChanged {
        session: SessionId,
    },
    /// Persisted before the cancellable runtime wait; addressed to one span.
    RetryScheduled {
        session: SessionId,
        turn: WorkerTurnId,
        span: String,
        retry: crate::queries::RetryFact,
    },
    /// The owner atomically completed a catalog attempt; read the current scoped catalog.
    ProviderChanged,
    /// Actual resource-owner publication, independent of an active turn.
    McpChanged(crate::queries::McpSnapshot),
    PermissionAsked(crate::approval::ApprovalRequest),
    QuestionAsked(crate::question::QuestionRequest),
    QuestionResolved {
        request: crate::question::QuestionRequest,
        decision: crate::question::QuestionDecision,
    },
    PermissionResolved {
        request: crate::approval::ApprovalRequest,
        decision: crate::approval::ApprovalDecision,
    },
    /// Session checkpoint lifecycle; summary deltas are cumulative snapshots.
    Compaction(crate::compaction::CompactionSnapshot),
    /// A root title was committed; live views can update without waiting for a turn.
    SessionTitleUpdated {
        /// Owning root session.
        session: SessionId,
        /// Committed, normalized title.
        title: String,
    },
    /// Safe durable checkpoint projection; identical identity/order on restart.
    TurnPresentation {
        /// Owning session.
        session: SessionId,
        /// Owning turn.
        turn: WorkerTurnId,
        /// Bounded metadata and public parts, never wire continuation.
        projection: crate::queries::HistoryTurn,
    },
    /// Turn was accepted and streaming started.
    TurnStarted {
        /// Session that owns the turn.
        session: SessionId,
        /// New turn id.
        turn: WorkerTurnId,
        /// Public notice accepted immediately before this turn's user row.
        model_switch: Option<crate::queries::ModelSwitchNotice>,
    },
    /// Incremental provider delta (not yet a durable message).
    TextDelta {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Chunk text.
        delta: String,
    },
    /// Incremental provider reasoning/thinking delta (not durable; rendered
    /// as the transcript's reasoning block, `session.reasoning.delta` upstream).
    ReasoningDelta {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Reasoning text chunk.
        delta: String,
    },
    /// A public reasoning item ended. Carries no provider continuation data.
    ReasoningItemEnded {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
    },
    /// Latest completed generation's measured context, independent of billed
    /// turn totals. Missing usage does not publish an estimate or erase it.
    /// Published after its local tools settle, including failed tool execution.
    TurnContextUsage {
        /// Session that owns the captured provider request.
        session: SessionId,
        /// Turn that owns the completed generation.
        turn: WorkerTurnId,
        /// Provider-reported input total (including any cached input).
        input_tokens: u64,
        /// Provider-reported output total (including any reasoning output).
        output_tokens: u64,
        /// Context limit captured by this request; zero means unknown.
        context_limit: u64,
    },
    /// Terminal provider usage for the turn (only when the provider reported
    /// it; never synthesized).
    TurnUsage {
        /// Session that owns the turn.
        session: SessionId,
        /// Turn the usage belongs to.
        turn: WorkerTurnId,
        /// Input tokens of the last reported provider round.
        input_tokens: u64,
        /// Output tokens summed over the turn's provider rounds.
        output_tokens: u64,
        /// Provider-active streaming time in milliseconds, summed over rounds
        /// (upstream `time.streamed - time.created` per assistant step).
        streamed_ms: u64,
    },
    /// Disposable provider announcement/argument prefix; never durable intent.
    ToolArgumentStream {
        /// Owning session.
        session: SessionId,
        /// Owning turn.
        turn: WorkerTurnId,
        /// Disposable presentation only.
        event: crate::tool_stream::ToolStreamEvent,
    },
    /// Durable intent notification, emitted before dispatch. Input is complete
    /// recorded arguments JSON, bounded by tool argument caps.
    ToolCallStarted {
        /// Captured safe intent label; the wire name and arguments stay intact.
        output_presentation: Option<Box<crate::tool_output::Presentation>>,
        /// Canonical validated/planned compression topic; other tools use None.
        dcp_topic: Option<String>,
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// Recorded arguments JSON.
        input: String,
    },
    /// Coalesced live facts for the existing operation, not a durable event/card.
    ToolCallProgress {
        session: SessionId,
        turn: WorkerTurnId,
        op: String,
        output_presentation: Box<crate::tool_output::Presentation>,
    },
    /// One tool call reached a terminal state (durable outcome recorded);
    /// the transcript updates the card in place.
    ToolCallFinished {
        /// Producer-owned body/guidance/capture facts, independent of raw output.
        output_presentation: Option<Box<crate::tool_output::Presentation>>,
        question: Option<crate::question::QuestionResult>,
        /// Frozen successful DCP commit, shared with durable history projection.
        dcp: Option<crate::dcp_view::DcpRunSnapshot>,
        /// Bounded confirmed mutation metadata, shared with history replay.
        patch_effects: Option<crate::patch::PatchEffects>,
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// `completed` / `failed` / `denied` / `cancelled` / `no_gain`.
        state: String,
        /// Outcome preview (bounded by the producer at the report cap).
        output: String,
        /// Full stored output size in bytes.
        output_bytes: i64,
        /// True when [`CoreEvent::ToolCallFinished::output`] is a preview.
        output_truncated: bool,
    },
    /// Turn completed; assistant message is now in history.
    TurnFinished {
        /// Session that owns the turn.
        session: SessionId,
        /// Finished turn id.
        turn: WorkerTurnId,
        /// Full assistant text.
        text: String,
        /// Turn wall time in milliseconds, accept to commit (upstream
        /// `turnDuration`: user message created → assistant completed).
        duration_ms: u64,
        /// Sanitized non-fatal notices for this turn (e.g. degraded MCP
        /// servers); never secrets, never a reason to hide the answer.
        warnings: Vec<String>,
        /// Indices in `warnings` emitted by the captured background-service
        /// owner. Other notices remain turn-specific; consumers never classify
        /// warning prose. Empty for scripted providers.
        service_warning_range: std::ops::Range<usize>,
    },
    /// Turn was cancelled; partial text was not committed as a message.
    TurnInterrupted {
        /// Session that owns the turn.
        session: SessionId,
        /// Interrupted turn id.
        turn: WorkerTurnId,
        /// Accumulated text at cancel time (not stored).
        partial: String,
        /// Turn wall time in milliseconds, accept to interrupt.
        duration_ms: u64,
    },
    /// Accepted turn failed; never present this as a completed answer.
    TurnFailed {
        /// Session that owns the turn.
        session: SessionId,
        /// Failed turn id.
        turn: WorkerTurnId,
        /// Sanitized application error.
        error: CoreError,
        /// Sanitized non-fatal notices for this turn (e.g. degraded MCP
        /// servers); never secrets, never a substitute for the error.
        warnings: Vec<String>,
        /// Background-service provenance, as in `TurnFinished`.
        service_warning_range: std::ops::Range<usize>,
    },
}

/// Scripted provider for offline tests.
///
/// `echo` mirrors the prompt as two chunks; `fixed` replays the given chunks.
/// No network I/O happens. Delays are cooperative cancellation points.
#[derive(Debug, Clone)]
pub struct MockProvider {
    chunks: Option<Vec<String>>,
    delay_ms: u64,
}

impl MockProvider {
    /// Echo provider: `["echo: ", prompt]` with 5 ms between chunks.
    pub fn echo() -> Self {
        Self {
            chunks: None,
            delay_ms: 5,
        }
    }

    /// Fixed script with explicit per-chunk delay.
    pub fn fixed(chunks: Vec<String>, delay_ms: u64) -> Self {
        Self {
            chunks: Some(chunks),
            delay_ms,
        }
    }

    fn plan(&self, prompt: &str) -> Vec<String> {
        if let Some(chunks) = &self.chunks {
            chunks.clone()
        } else {
            vec!["echo: ".to_string(), prompt.to_string()]
        }
    }
}

/// Commands consumed by the single application owner (native or scripted).
pub enum InboxMsg {
    SessionViewed {
        session: SessionId,
        idle_seq: u64,
        ack: oneshot::Sender<Result<crate::queries::SessionActivity, CoreError>>,
    },
    Workspace {
        selected: Option<SessionId>,
        ack: oneshot::Sender<Result<crate::queries::WorkspaceSnapshot, CoreError>>,
    },
    UserShell {
        session: SessionId,
        command: String,
        selection: UserShellSelection,
        cancel: Arc<AtomicBool>,
        ack: oneshot::Sender<Result<String, CoreError>>,
    },
    PromptHistory {
        append: Option<String>,
        ack: oneshot::Sender<Result<Vec<String>, CoreError>>,
    },
    AuthMethods {
        provider: String,
        ack: oneshot::Sender<Result<Vec<crate::queries::AuthMethod>, CoreError>>,
    },
    Authenticate {
        provider: String,
        action: crate::queries::AuthAction,
        ack: oneshot::Sender<Result<crate::queries::AuthAttempt, CoreError>>,
    },
    Terminal {
        session: SessionId,
        action: crate::queries::TerminalAction,
        ack: oneshot::Sender<Result<crate::queries::TerminalReceipt, CoreError>>,
    },
    ProviderConnections {
        ack: oneshot::Sender<Result<Vec<crate::queries::ProviderConnection>, CoreError>>,
    },
    /// All admitted, already-loaded provider views; no discovery or mutation.
    ModelCatalogs {
        ack: oneshot::Sender<Result<crate::queries::ModelCatalogs, CoreError>>,
    },
    /// Read (`None`) or mutate the model dialog's favorites/recents.
    ModelPreference {
        action: Option<crate::queries::ModelPreferenceAction>,
        ack: oneshot::Sender<Result<crate::queries::ModelPreference, CoreError>>,
    },
    /// Read or explicitly save the data-root-wide MCP sidebar fold preference.
    SidebarMcpOpen {
        open: Option<bool>,
        ack: oneshot::Sender<Result<bool, CoreError>>,
    },
    CustomProvider {
        action: crate::queries::CustomProviderAction,
        ack: oneshot::Sender<Result<crate::queries::CustomProviderReply, CoreError>>,
    },
    ProviderAccounts {
        provider: String,
        action: Option<crate::queries::AccountAction>,
        ack: oneshot::Sender<Result<crate::queries::ProviderAccounts, CoreError>>,
    },
    McpStatus {
        ack: oneshot::Sender<Result<crate::queries::McpSnapshot, CoreError>>,
    },
    McpControl {
        control: crate::queries::McpControl,
        ack: oneshot::Sender<Result<crate::queries::McpSnapshot, CoreError>>,
    },
    McpLookup {
        query: crate::queries::McpLookup,
        cancel: Arc<AtomicBool>,
        ack:
            oneshot::Sender<Result<crate::queries::McpLookupReply, crate::queries::McpLookupError>>,
    },
    PendingApprovals {
        ack: oneshot::Sender<Result<Vec<crate::approval::ApprovalRequest>, CoreError>>,
    },
    PendingQuestions {
        ack: oneshot::Sender<Result<Vec<crate::question::QuestionRequest>, CoreError>>,
    },
    ReplyQuestion {
        reply: crate::question::QuestionReply,
        ack: oneshot::Sender<Result<(), crate::question::QuestionReplyError>>,
    },
    RegisterQuestionConsumer {
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    ReplyApproval {
        reply: crate::approval::ApprovalReply,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    RegisterApprovalConsumer {
        auto_once: bool,
        preference: Option<bool>,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    CompactSession {
        session: SessionId,
        ack: oneshot::Sender<Result<crate::compaction::CompactionSnapshot, CoreError>>,
    },
    CancelCompaction {
        session: SessionId,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    CompactionHistory {
        session: SessionId,
        ack: oneshot::Sender<Result<Vec<crate::compaction::CompactionSnapshot>, CoreError>>,
    },
    /// Stop execution before moving to a saved conversation point.
    ChangeConversation {
        session: SessionId,
        action: crate::queries::ConversationAction,
        ack: oneshot::Sender<Result<crate::queries::ConversationSnapshot, CoreError>>,
    },
    /// Copy settled history strictly before this exact user message.
    ForkSession {
        source: SessionId,
        before: MessageId,
        ack: oneshot::Sender<Result<crate::queries::ForkSessionSnapshot, CoreError>>,
    },
    /// Create or open a session according to the owner's storage contract.
    Create {
        /// Session id.
        id: SessionId,
        /// Acceptance after the operation succeeds.
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Persist an explicit title on an existing root in the current Location.
    RenameSession {
        session: SessionId,
        title: String,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Explicitly remove a settled root family and its saved tab membership.
    DeleteSession {
        session: SessionId,
        ack: oneshot::Sender<Result<crate::queries::TabDeckSnapshot, CoreError>>,
    },
    /// Act only on a root listed by the active Sessions scope/query. Unlike
    /// regular session APIs, placement is resolved from the owner catalog.
    PickerSessionAction {
        session: SessionId,
        search: String,
        all_projects: bool,
        action: crate::queries::SessionPickerAction,
        ack: oneshot::Sender<Result<crate::queries::SessionPickerResult, CoreError>>,
    },
    OpenPickerSession {
        session: SessionId,
        search: String,
        all_projects: bool,
        old_deck: crate::queries::TabDeckSnapshot,
        ack: oneshot::Sender<Result<crate::queries::SessionPickerOpen, CoreError>>,
    },
    SessionPickerContext {
        all_projects: Option<bool>,
        ack: oneshot::Sender<Result<crate::queries::SessionPickerContext, CoreError>>,
    },
    /// Regenerate a bound root's title with its configured title agent.
    RegenerateTitle {
        session: SessionId,
        ack: oneshot::Sender<Result<String, CoreError>>,
    },
    /// Cancel only a title request, never the owning conversational turn.
    CancelTitle {
        session: SessionId,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Accept input and start a turn.
    Submit {
        /// Owning session.
        session: SessionId,
        /// Input text.
        text: crate::prompt::PromptInput,
        /// Optional exact selection prepared in the same admission order.
        selection: Option<crate::queries::ModelCommit>,
        /// Durable acceptance or rejection.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// OC2 steer/queue: hold input for the session's running turn.
    Enqueue {
        session: SessionId,
        text: crate::prompt::PromptInput,
        delivery: crate::queries::InboxDelivery,
        selection: Option<crate::queries::ModelCommit>,
        ack: oneshot::Sender<Result<crate::queries::InboxItem, CoreError>>,
    },
    /// Explicit activation of already durable pending input. The owning
    /// application resolves the payload; callers cannot supply prepared authority.
    SubmitInbox {
        session: SessionId,
        id: String,
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Read or change the session's pending inbox.
    Inbox {
        session: SessionId,
        action: crate::queries::InboxAction,
        ack: oneshot::Sender<Result<crate::queries::InboxSnapshot, CoreError>>,
    },
    /// Atomically accept the first turn and create its Location-bound root.
    /// A refusal must not create a session; acknowledgement follows durable
    /// acceptance of the root, selection, turn and user message.
    SubmitFresh {
        /// New, unused root session id.
        session: SessionId,
        /// Input text.
        text: crate::prompt::PromptInput,
        /// Optional explicit Home selection for this root.
        selection: Option<FreshSelection>,
        /// Durable acceptance or rejection.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Cancel the active turn.
    Cancel {
        /// Owning session.
        session: SessionId,
        /// Cancellation acknowledgement.
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// List sessions.
    List {
        /// Query result; storage errors are not an empty list.
        ack: oneshot::Sender<Result<Vec<SessionId>, CoreError>>,
    },
    /// Bounded metadata query, separate from the legacy ID list.
    SessionList {
        search: String,
        all_projects: bool,
        ack: oneshot::Sender<Result<Vec<crate::queries::SessionListEntry>, CoreError>>,
    },
    /// Probe one ID through the application owner, including Location binding.
    ProbeSession {
        id: SessionId,
        ack: oneshot::Sender<Result<SessionProbe, CoreError>>,
    },
    /// Restore the current Location's ordered root tabs without opening a root.
    TabDeck {
        ack: oneshot::Sender<Result<TabDeckSnapshot, CoreError>>,
    },
    /// Persist one validated deck in the current Location.
    SaveTabDeck {
        deck: TabDeckSnapshot,
        ack: oneshot::Sender<Result<TabDeckSnapshot, CoreError>>,
    },
    /// Read committed history.
    Read {
        /// Owning session.
        session: SessionId,
        /// Query result.
        ack: oneshot::Sender<Result<Vec<Message>, CoreError>>,
    },
    /// Read one bounded history page (newest-first cursor, `before_seq`).
    History {
        /// Owning session.
        session: SessionId,
        /// Exact current-branch message lookup, independent of page cursors.
        message: Option<MessageId>,
        /// Upper bound: rows with a smaller seq (older). `None` = newest page.
        before_seq: Option<i64>,
        /// Lower bound: rows with a larger seq (newer), oldest-first. Exactly
        /// one of `before_seq`/`after_seq` is set by the caller.
        after_seq: Option<i64>,
        /// Requested rows (clamped by the owner).
        limit: usize,
        /// Bounded canonical transcript serving; None preserves message reads.
        window: Option<crate::queries::HistoryReadWindow>,
        /// Query result.
        ack: oneshot::Sender<Result<HistoryPage, CoreError>>,
    },
    /// Read one bounded tool-operation page (newest-first cursor).
    ToolOps {
        /// Owning session.
        session: SessionId,
        /// Upper bound: rows recorded before this rowid.
        before_rowid: Option<i64>,
        /// Requested rows (clamped by the owner).
        limit: usize,
        /// Query result.
        ack: oneshot::Sender<Result<ToolOpPage, CoreError>>,
    },
    /// Read a bounded continuation of one operation's result in this session.
    ToolOutput {
        session: SessionId,
        op: String,
        offset: usize,
        limit: usize,
        ack: oneshot::Sender<Result<crate::queries::ToolOutputPage, CoreError>>,
    },
    /// Read retained background output without changing the original call result.
    ShellOutput {
        session: SessionId,
        shell_id: String,
        offset: usize,
        limit: usize,
        ack: oneshot::Sender<Result<Option<crate::queries::ToolOutputPage>, CoreError>>,
    },
    /// Explicit cancellation of one job owned by this exact source session.
    CancelShell {
        session: SessionId,
        shell_id: String,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Bounded running inventory for this source family.
    ShellJobs {
        session: SessionId,
        ack: oneshot::Sender<Result<Vec<crate::queries::ShellJob>, CoreError>>,
    },
    /// Bounded running Code Mode programs in this session family.
    ExecuteJobs {
        session: SessionId,
        ack: oneshot::Sender<Result<Vec<crate::queries::ExecuteJob>, CoreError>>,
    },
    ChildJobs {
        session: SessionId,
        ack: oneshot::Sender<Result<Vec<crate::queries::ChildJob>, CoreError>>,
    },
    InterruptChild {
        session: SessionId,
        selected: crate::queries::ChildJob,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    BackgroundChild {
        session: SessionId,
        selected: crate::queries::ChildJob,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    ReadChild {
        session: SessionId,
        selected: crate::queries::ChildJob,
        window: Option<crate::queries::HistoryReadWindow>,
        ack: oneshot::Sender<Result<HistoryPage, CoreError>>,
    },
    /// Current/final recent output with the original source identity and cursors.
    ShellSnapshot {
        session: SessionId,
        shell_id: String,
        ack: oneshot::Sender<Result<crate::queries::ShellSnapshot, CoreError>>,
    },
    /// Convert an already admitted foreground operation without spawning again.
    BackgroundShell {
        session: SessionId,
        shell_id: String,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    ExecuteSnapshot {
        session: SessionId,
        execute_id: String,
        ack: oneshot::Sender<Result<crate::queries::ExecuteSnapshot, CoreError>>,
    },
    BackgroundExecute {
        session: SessionId,
        turn: String,
        execute_id: String,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Bounded real saved DCP summary, identified by operation and range index.
    DcpSummary {
        session: SessionId,
        op: String,
        block_index: usize,
        offset: i64,
        limit: usize,
        ack: oneshot::Sender<Result<Option<crate::dcp_view::DcpSummaryPage>, CoreError>>,
    },
    /// Model catalog plus the effective model/variant/agent selection.
    Catalog {
        /// Query result.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Read another admitted connection's catalog without committing a selection.
    ProviderCatalog {
        provider: String,
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Read/change a session/agent model draft in the existing application owner.
    SessionSelection {
        /// Owning session.
        session: SessionId,
        /// Home selections additionally remember the Location/agent draft.
        home: bool,
        /// Exact action; model selection is distinct from clearing a variant.
        action: crate::queries::SessionSelectionAction,
        /// Actual accepted selection and catalog.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Read/change the sessionless Home selection in the current Location.
    HomeSelection {
        /// Exact selection action; no session id or session preference exists.
        action: crate::queries::SessionSelectionAction,
        /// Actual selected choice and catalog.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Skill catalog cards (metadata only).
    Skills {
        /// Query result.
        ack: oneshot::Sender<Result<Vec<SkillCard>, CoreError>>,
    },
    /// Suggest paths from the owner's current Location; no caller-supplied root.
    FileSuggestions {
        query: String,
        limit: usize,
        ack: oneshot::Sender<Result<FileSuggestionsSnapshot, CoreError>>,
    },
    /// Select the effective model (and optional variant) for later turns.
    SelectModel {
        /// Exact model id.
        id: String,
        /// Optional variant name.
        variant: Option<String>,
        /// Resulting snapshot after acceptance.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Select the effective primary agent for later turns.
    SelectAgent {
        /// Agent profile id.
        id: String,
        /// Resulting snapshot after acceptance.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Switch the whole application to another Location (project path).
    ///
    /// The target generation is built completely before the switch is
    /// published; the current Location is kept on failure. Refused while a
    /// turn is active.
    SwitchLocation {
        /// Target project path.
        path: String,
        /// Resulting Location/session/catalog snapshot after acceptance.
        ack: oneshot::Sender<Result<LocationSnapshot, CoreError>>,
    },
    /// Publish another Location's complete generation for sessionless Home.
    SwitchLocationHome {
        /// Target project path.
        path: String,
        /// Location/catalog/diagnostics without an attached session.
        ack: oneshot::Sender<Result<HomeLocationSnapshot, CoreError>>,
    },
    /// Rebuild the current canonical Location's config/catalog/runtime between
    /// turns without switching Location, creating sessions or changing the deck.
    ReloadLocation {
        ack: oneshot::Sender<Result<ReloadLocationSnapshot, CoreError>>,
    },
    /// DCP context/stats snapshot for a session.
    Dcp {
        /// Owning session.
        session: SessionId,
        /// Query result.
        ack: oneshot::Sender<Result<DcpSnapshot, CoreError>>,
    },
    /// Manual DCP compress request (focus instruction, executed by the owner).
    Compress {
        /// Owning session.
        session: SessionId,
        /// Bounded focus instruction.
        focus: String,
        /// Accepted turn id, or a typed refusal.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Cancel/drain active work and close the owner.
    Shutdown,
}

struct SessionState {
    id: SessionId,
    messages: Vec<Message>,
    next_seq: u64,
}

impl SessionState {
    fn new(id: SessionId) -> Self {
        Self {
            id,
            messages: Vec::new(),
            next_seq: 1,
        }
    }

    fn push(&mut self, role: Role, text: String) -> Message {
        let id = MessageId(format!("m{:04}", self.next_seq));
        self.next_seq += 1;
        let msg = Message { id, role, text };
        self.messages.push(msg.clone());
        msg
    }
}

struct ActiveTurn {
    session: SessionId,
    turn: WorkerTurnId,
    chunks: Vec<String>,
    index: usize,
    accumulated: String,
    /// Wall-clock start, so terminal events report a measured duration.
    started: std::time::Instant,
}

/// Cloneable application handle over a bounded worker inbox.
#[derive(Debug, Clone)]
pub struct CoreApp {
    /// Native attachment id; a reattached interactive client gets a new one.
    attachment: Arc<AtomicU64>,
    inbox: InboxSender,
    control: Option<mpsc::Sender<NativeControl>>,
    events: broadcast::Sender<CoreEvent>,
}

/// Client-local cancellation identity exists before a native request is sent.
/// This is neither a durable receipt nor permission to retry a lost operation.
#[derive(Debug)]
pub struct NativeAdmission {
    session: SessionId,
    claimed: AtomicBool,
    pub cancelled: AtomicBool,
    pub request: AtomicU64,
}

#[derive(Debug, Clone, Default)]
pub struct NativeAdmissions(Arc<Mutex<HashMap<SessionId, Vec<Arc<NativeAdmission>>>>>);

impl NativeAdmissions {
    pub fn claim(&self, session: &SessionId) -> Option<Arc<NativeAdmission>> {
        self.0
            .lock()
            .expect("native admissions")
            .get(session)?
            .iter()
            .find(|entry| !entry.claimed.swap(true, Ordering::AcqRel))
            .cloned()
    }
    pub fn complete(&self, entry: &Arc<NativeAdmission>) {
        let mut entries = self.0.lock().expect("native admissions");
        if let Some(pending) = entries.get_mut(&entry.session) {
            pending.retain(|item| !Arc::ptr_eq(item, entry));
            if pending.is_empty() {
                entries.remove(&entry.session);
            }
        }
    }
    fn cancel(&self, session: &SessionId) -> Option<Arc<NativeAdmission>> {
        let entries = self.0.lock().expect("native admissions");
        let entry = entries.get(session)?.first()?.clone();
        entry.cancelled.store(true, Ordering::Release);
        Some(entry)
    }
}

pub enum NativeControl {
    Cancel {
        session: SessionId,
        turn: Option<WorkerTurnId>,
        admission: Option<Arc<NativeAdmission>>,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    Shutdown,
}

/// The existing bounded inbox records only native execution cancellation
/// metadata at enqueue, closing the cancel-before-bridge-poll race.
#[derive(Debug, Clone)]
struct InboxSender {
    sender: mpsc::Sender<InboxMsg>,
    admissions: Option<NativeAdmissions>,
}
impl InboxSender {
    fn enqueue(&self, permit: mpsc::Permit<'_, InboxMsg>, message: InboxMsg) {
        let entries = self
            .admissions
            .as_ref()
            .map(|entries| entries.0.lock().expect("native admissions"));
        if let Some(mut entries) = entries {
            let session = match &message {
                InboxMsg::Submit { session, .. }
                | InboxMsg::SubmitInbox { session, .. }
                | InboxMsg::SubmitFresh { session, .. }
                | InboxMsg::UserShell { session, .. }
                | InboxMsg::Compress { session, .. }
                | InboxMsg::CompactSession { session, .. }
                | InboxMsg::RegenerateTitle { session, .. } => Some(session),
                _ => None,
            };
            if let Some(session) = session {
                entries
                    .entry(session.clone())
                    .or_default()
                    .push(Arc::new(NativeAdmission {
                        session: session.clone(),
                        claimed: AtomicBool::new(false),
                        cancelled: AtomicBool::new(false),
                        request: AtomicU64::new(0),
                    }));
            }
            permit.send(message);
        } else {
            permit.send(message);
        }
    }
    async fn send(&self, message: InboxMsg) -> Result<(), mpsc::error::SendError<InboxMsg>> {
        match self.sender.reserve().await {
            Ok(permit) => {
                self.enqueue(permit, message);
                Ok(())
            }
            Err(_) => Err(mpsc::error::SendError(message)),
        }
    }
    fn try_send(&self, message: InboxMsg) -> Result<(), mpsc::error::TrySendError<()>> {
        match self.sender.try_reserve() {
            Ok(permit) => {
                self.enqueue(permit, message);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(())) => Err(mpsc::error::TrySendError::Full(())),
            Err(mpsc::error::TrySendError::Closed(())) => {
                Err(mpsc::error::TrySendError::Closed(()))
            }
        }
    }
}

/// Worker join guard; await after [`CoreApp::shutdown`].
///
/// The worker reports a typed failure string (for example an owned MCP
/// resource that could not be closed), so callers can exit non-zero instead
/// of claiming a clean shutdown.
pub struct WorkerGuard {
    handle: Option<WorkerTask>,
}

enum WorkerTask {
    Legacy(tokio::task::JoinHandle<Result<(), String>>),
    Diagnostic(tokio::task::JoinHandle<Result<(), crate::queries::ServiceDiagnostic>>),
}

impl CoreApp {
    pub async fn session_viewed(
        &self,
        session: SessionId,
        idle_seq: u64,
    ) -> Result<crate::queries::SessionActivity, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SessionViewed {
                session,
                idle_seq,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn workspace(
        &self,
        selected: Option<SessionId>,
    ) -> Result<crate::queries::WorkspaceSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Workspace { selected, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn mcp_status(&self) -> Result<crate::queries::McpSnapshot, CoreError> {
        let (ack, reply) = oneshot::channel();
        self.inbox
            .send(InboxMsg::McpStatus { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        reply.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Caller-only lookup; dropping this future cancels its owned request.
    pub async fn mcp_lookup(
        &self,
        query: crate::queries::McpLookup,
    ) -> Result<crate::queries::McpLookupReply, crate::queries::McpLookupError> {
        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let _guard = CancelOnDrop(cancel.clone());
        let (ack, reply) = oneshot::channel();
        self.inbox
            .send(InboxMsg::McpLookup { query, cancel, ack })
            .await
            .map_err(|_| crate::queries::McpLookupError::Unavailable)?;
        reply
            .await
            .map_err(|_| crate::queries::McpLookupError::Unavailable)?
    }

    /// Acknowledges the owner's admitted/coalesced action, not its eventual
    /// network outcome. Completion arrives through McpChanged/status reads.
    pub async fn mcp_control(
        &self,
        control: crate::queries::McpControl,
    ) -> Result<crate::queries::McpSnapshot, CoreError> {
        let (ack, reply) = oneshot::channel();
        self.inbox
            .send(InboxMsg::McpControl { control, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        reply.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn pending_approvals(
        &self,
    ) -> Result<Vec<crate::approval::ApprovalRequest>, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::PendingApprovals { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn pending_questions(
        &self,
    ) -> Result<Vec<crate::question::QuestionRequest>, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::PendingQuestions { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn reply_question(
        &self,
        reply: crate::question::QuestionReply,
    ) -> Result<(), crate::question::QuestionReplyError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ReplyQuestion { reply, ack })
            .await
            .map_err(|_| crate::question::QuestionReplyError::Unavailable)?;
        rx.await
            .map_err(|_| crate::question::QuestionReplyError::Unavailable)?
    }
    pub async fn register_question_consumer(&self) -> Result<(), CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::RegisterQuestionConsumer { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn reply_approval(
        &self,
        reply: crate::approval::ApprovalReply,
    ) -> Result<(), CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ReplyApproval { reply, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn register_approval_consumer(&self, auto_once: bool) -> Result<(), CoreError> {
        self.approval_consumer(auto_once, None).await
    }
    /// Persist a consumer mode preference through the application owner.
    pub async fn set_permission_mode(&self, auto_once: bool) -> Result<(), CoreError> {
        self.set_permission_mode_with_cli_auto(auto_once, false)
            .await
    }
    /// Save the preference and apply its effective process-only CLI override
    /// in one owner command, without briefly registering a Prompt consumer.
    pub async fn set_permission_mode_with_cli_auto(
        &self,
        auto_once: bool,
        cli_auto: bool,
    ) -> Result<(), CoreError> {
        self.approval_consumer(auto_once || cli_auto, Some(auto_once))
            .await
    }
    /// Shared bounded input history. Appending is independent of generation,
    /// while normal input is recorded atomically by durable turn acceptance.
    pub async fn prompt_history(&self, append: Option<String>) -> Result<Vec<String>, CoreError> {
        self.request_prompt_history(append)?
            .0
            .await
            .map_err(|_| CoreError::Shutdown)?
    }

    /// Enqueue the same history operation without awaiting owner persistence.
    pub fn request_prompt_history(
        &self,
        append: Option<String>,
    ) -> Result<PromptHistoryReceipt, CoreError> {
        if append
            .as_ref()
            .is_some_and(|text| text.len() > MAX_INPUT_BYTES)
        {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, rx) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::PromptHistory { append, ack })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(PromptHistoryReceipt(rx))
    }
    async fn approval_consumer(
        &self,
        auto_once: bool,
        preference: Option<bool>,
    ) -> Result<(), CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::RegisterApprovalConsumer {
                auto_once,
                preference,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    /// Admit/coalesce a checkpoint operation without accepting a user message.
    pub async fn compact_session(
        &self,
        session: SessionId,
    ) -> Result<crate::compaction::CompactionSnapshot, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CompactSession { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    /// Cancel only compaction, never a title request or running tool.
    pub async fn cancel_compaction(&self, session: SessionId) -> Result<(), CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CancelCompaction { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    pub async fn compaction_history(
        &self,
        session: SessionId,
    ) -> Result<Vec<crate::compaction::CompactionSnapshot>, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CompactionHistory { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }
    /// Restore a saved conversation point without replaying external effects.
    pub async fn change_conversation(
        &self,
        session: SessionId,
        action: crate::queries::ConversationAction,
    ) -> Result<crate::queries::ConversationSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ChangeConversation {
                session,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }
    /// Create an independently routable root without submitting its restored draft.
    pub async fn fork_session(
        &self,
        source: SessionId,
        before: MessageId,
    ) -> Result<crate::queries::ForkSessionSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ForkSession {
                source,
                before,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }
    /// Build the existing application boundary for an adapter-owned worker.
    pub fn channel(
        capacity: usize,
    ) -> (Self, mpsc::Receiver<InboxMsg>, broadcast::Sender<CoreEvent>) {
        let (events, _) = broadcast::channel(256);
        let (app, receiver) = Self::channel_with_events(capacity, events.clone());
        (app, receiver, events)
    }

    /// Adapter-owned session runners publish onto their authority's common bus.
    pub fn channel_with_events(
        capacity: usize,
        events: broadcast::Sender<CoreEvent>,
    ) -> (Self, mpsc::Receiver<InboxMsg>) {
        let (inbox, receiver) = mpsc::channel(capacity);
        (
            Self {
                attachment: Arc::default(),
                inbox: InboxSender {
                    sender: inbox,
                    admissions: None,
                },
                control: None,
                events,
            },
            receiver,
        )
    }

    /// Native clients keep cancellation/release independent of a full work inbox.
    pub fn native_channel(
        capacity: usize,
        attachment: u64,
    ) -> (
        Self,
        mpsc::Receiver<InboxMsg>,
        mpsc::Receiver<NativeControl>,
        broadcast::Sender<CoreEvent>,
        NativeAdmissions,
    ) {
        let (mut app, inbox, events) = Self::channel(capacity);
        let (control, receiver) = mpsc::channel(16);
        let admissions = NativeAdmissions::default();
        app.inbox.admissions = Some(admissions.clone());
        app.control = Some(control);
        app.attachment.store(attachment, Ordering::Release);
        (app, inbox, receiver, events, admissions)
    }

    /// The worker or native owner bridge that served this handle is gone; every
    /// later request fails with [`CoreError::Shutdown`] until a new attachment.
    pub fn is_closed(&self) -> bool {
        self.inbox.sender.is_closed()
    }

    /// Native frontends mirror the authority's workspace, not a private deck.
    pub fn shares_workspace(&self) -> bool {
        self.control.is_some()
    }

    pub fn attachment(&self) -> u64 {
        self.attachment.load(Ordering::Acquire)
    }

    /// Shared by every clone: the native transport publishes a reattachment.
    pub fn attachment_handle(&self) -> Arc<AtomicU64> {
        self.attachment.clone()
    }

    /// Forward an already typed command across the native authority's bounded
    /// routing boundary. Acknowledgements retain their original caller owner.
    pub async fn dispatch(&self, message: InboxMsg) -> Result<(), CoreError> {
        if let Some(control) = &self.control {
            let message = match message {
                InboxMsg::Cancel { session, ack } => NativeControl::Cancel {
                    admission: self
                        .inbox
                        .admissions
                        .as_ref()
                        .and_then(|entries| entries.cancel(&session)),
                    session,
                    turn: None,
                    ack,
                },
                InboxMsg::Shutdown => NativeControl::Shutdown,
                other => {
                    return self
                        .inbox
                        .send(other)
                        .await
                        .map_err(|_| CoreError::Shutdown);
                }
            };
            return control.send(message).await.map_err(|_| CoreError::Shutdown);
        }
        self.inbox
            .send(message)
            .await
            .map_err(|_| CoreError::Shutdown)
    }

    /// Spawn a worker with default bounded capacity.
    pub fn spawn(provider: MockProvider) -> (Self, WorkerGuard) {
        Self::spawn_with_capacity(provider, MAX_QUEUE_ITEMS)
    }

    /// Spawn a worker with explicit inbox capacity (tests use small caps).
    pub fn spawn_with_capacity(provider: MockProvider, capacity: usize) -> (Self, WorkerGuard) {
        let (app, inbox_rx, event_tx) = Self::channel(capacity);
        let handle = tokio::spawn(async move {
            worker_loop(provider, inbox_rx, event_tx).await;
            Ok(())
        });
        (
            app,
            WorkerGuard {
                handle: Some(WorkerTask::Legacy(handle)),
            },
        )
    }

    /// Subscribe to live turn events.
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    /// Create a session; duplicate ids fail.
    pub async fn create_session(&self, id: SessionId) -> Result<(), CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Create { id, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Rename a Location-bound root; succeeds after the title is durable.
    pub async fn rename_session(&self, session: SessionId, title: String) -> Result<(), CoreError> {
        let title = normalized_session_title(&title)
            .ok_or_else(|| CoreError::Application("invalid session title".into()))?
            .to_string();
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::RenameSession {
                session,
                title,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn delete_session(
        &self,
        session: SessionId,
    ) -> Result<crate::queries::TabDeckSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::DeleteSession { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn picker_session_action(
        &self,
        session: SessionId,
        search: String,
        all_projects: bool,
        action: crate::queries::SessionPickerAction,
    ) -> Result<crate::queries::SessionPickerResult, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::PickerSessionAction {
                session,
                search,
                all_projects,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn open_picker_session(
        &self,
        session: SessionId,
        search: String,
        all_projects: bool,
        old_deck: crate::queries::TabDeckSnapshot,
    ) -> Result<crate::queries::SessionPickerOpen, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::OpenPickerSession {
                session,
                search,
                all_projects,
                old_deck,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn session_picker_context(
        &self,
        all_projects: Option<bool>,
    ) -> Result<crate::queries::SessionPickerContext, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SessionPickerContext { all_projects, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Recompute a root title. The returned title is durable; no turn is added.
    pub async fn regenerate_title(&self, session: SessionId) -> Result<String, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::RegenerateTitle { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Cancel the current title request for this session without cancelling a turn.
    pub async fn cancel_title(&self, session: SessionId) -> Result<(), CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CancelTitle { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Submit input; fails fast on oversize/unknown/busy without queueing.
    pub async fn submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Submit {
                session,
                text: text.into(),
                selection: None,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Non-waiting submit: `QueueFull` when the bounded inbox is full.
    pub async fn try_submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Submit {
                session,
                text: text.into(),
                selection: None,
                ack: ack_tx,
            })
            .map_err(|_| CoreError::QueueFull)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Enqueue once without awaiting acceptance. A subsequent session cancel is
    /// ordered after this request in the same inbox; no detached task is needed.
    pub fn request_submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<SubmissionReceipt, CoreError> {
        self.request_submit_selected(session, text, None)
    }

    /// Queue an explicit command through the same bounded application owner.
    /// No model request, interactive-terminal byte inference, or optimistic
    /// input-history write occurs at this boundary.
    pub fn request_user_shell(
        &self,
        session: SessionId,
        command: String,
        selection: UserShellSelection,
    ) -> Result<UserShellReceipt, CoreError> {
        if command.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, result) = oneshot::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.inbox
            .try_send(InboxMsg::UserShell {
                session,
                command,
                selection,
                cancel: cancel.clone(),
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(UserShellReceipt { result, cancel })
    }

    /// Capture selection now; the single inbox owner commits it at preparation.
    pub fn request_submit_selected(
        &self,
        session: SessionId,
        text: String,
        selection: Option<crate::queries::ModelCommit>,
    ) -> Result<SubmissionReceipt, CoreError> {
        self.request_submit_input_selected(session, text.into(), selection)
    }

    pub fn request_submit_input_selected(
        &self,
        session: SessionId,
        text: crate::prompt::PromptInput,
        selection: Option<crate::queries::ModelCommit>,
    ) -> Result<SubmissionReceipt, CoreError> {
        text.check_budget()?;
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Submit {
                session,
                text,
                selection,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Bounded, nonwaiting busy commit, using the existing session selection port.
    pub fn request_model_commit(
        &self,
        session: SessionId,
        commit: crate::queries::ModelCommit,
    ) -> Result<ModelCommitReceipt, CoreError> {
        self.request_session_selection(
            session,
            false,
            crate::queries::SessionSelectionAction::Commit(commit),
        )
    }

    /// Bounded preparation using the existing selection message and owner ACK.
    pub fn request_session_selection(
        &self,
        session: SessionId,
        home: bool,
        action: crate::queries::SessionSelectionAction,
    ) -> Result<ModelCommitReceipt, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::SessionSelection {
                session,
                home,
                action,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(ModelCommitReceipt(receipt))
    }

    /// Local enqueue acceptance is not evidence of durable inbox acceptance.
    pub fn request_enqueue_selected(
        &self,
        session: SessionId,
        text: crate::prompt::PromptInput,
        delivery: crate::queries::InboxDelivery,
        selection: Option<crate::queries::ModelCommit>,
    ) -> Result<EnqueueReceipt, CoreError> {
        text.check_budget()?;
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Enqueue {
                session,
                text,
                delivery,
                selection,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(EnqueueReceipt(receipt))
    }

    /// Enqueue a new root's first turn without waiting for durable acceptance.
    /// Rejection leaves no session; the receipt resolves only from the owner.
    pub fn request_submit_fresh(
        &self,
        session: SessionId,
        text: String,
        selection: Option<FreshSelection>,
    ) -> Result<SubmissionReceipt, CoreError> {
        self.request_submit_input_fresh(session, text.into(), selection)
    }

    pub fn request_submit_input_fresh(
        &self,
        session: SessionId,
        text: crate::prompt::PromptInput,
        selection: Option<FreshSelection>,
    ) -> Result<SubmissionReceipt, CoreError> {
        text.check_budget()?;
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::SubmitFresh {
                session,
                text,
                selection,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Create a new root and submit its first turn, awaiting the owner's
    /// acceptance acknowledgement. A rejected input never creates a root.
    pub async fn submit_fresh(
        &self,
        session: SessionId,
        text: String,
        selection: Option<FreshSelection>,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SubmitFresh {
                session,
                text: text.into(),
                selection,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Cancel the active turn and owned background work for this session.
    pub async fn cancel(&self, session: SessionId) -> Result<(), CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.dispatch(InboxMsg::Cancel {
            session,
            ack: ack_tx,
        })
        .await?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// A painted T1 control must never be reinterpreted as cancellation of T2.
    pub async fn cancel_turn(
        &self,
        session: SessionId,
        turn: WorkerTurnId,
    ) -> Result<(), CoreError> {
        let Some(control) = &self.control else {
            return self.cancel(session).await;
        };
        let (ack, result) = oneshot::channel();
        control
            .send(NativeControl::Cancel {
                session,
                turn: Some(turn),
                admission: None,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Cancel one owned background shell; a foreign session/job pairing is refused.
    pub async fn cancel_shell(
        &self,
        session: SessionId,
        shell_id: String,
    ) -> Result<(), CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CancelShell {
                session,
                shell_id,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Current running jobs from the existing supervisor; includes descendants.
    pub async fn shell_jobs(
        &self,
        session: SessionId,
    ) -> Result<Vec<crate::queries::ShellJob>, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ShellJobs { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Current owned Code Mode programs, including descendants.
    pub async fn execute_jobs(
        &self,
        session: SessionId,
    ) -> Result<Vec<crate::queries::ExecuteJob>, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ExecuteJobs { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Bounded actual child family generations, including current terminal state.
    pub async fn child_jobs(
        &self,
        session: SessionId,
    ) -> Result<Vec<crate::queries::ChildJob>, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ChildJobs { session, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Interrupt precisely the selected child generation owned by this parent.
    pub async fn interrupt_child(
        &self,
        session: SessionId,
        selected: crate::queries::ChildJob,
    ) -> Result<(), CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::InterruptChild {
                session,
                selected,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn background_child(
        &self,
        session: SessionId,
        selected: crate::queries::ChildJob,
    ) -> Result<(), CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::BackgroundChild {
                session,
                selected,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read-only linked navigation; does not select a Location or grant a turn.
    pub async fn read_child(
        &self,
        session: SessionId,
        selected: crate::queries::ChildJob,
    ) -> Result<HistoryPage, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ReadChild {
                session,
                selected,
                window: None,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Canonical linked history; every continuation retains the original fence.
    pub async fn read_child_transcript(
        &self,
        session: SessionId,
        selected: crate::queries::ChildJob,
    ) -> Result<HistoryPage, CoreError> {
        self.read_child_window(session, selected, Default::default())
            .await
    }

    pub async fn read_child_window(
        &self,
        session: SessionId,
        selected: crate::queries::ChildJob,
        window: crate::queries::HistoryReadWindow,
    ) -> Result<HistoryPage, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ReadChild {
                session,
                selected,
                window: Some(window),
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Current admitted methods, without initiating authorization or reading secrets.
    pub async fn auth_methods(
        &self,
        provider: String,
    ) -> Result<Vec<crate::queries::AuthMethod>, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::AuthMethods { provider, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Explicit built-in OpenAI login/status/cancel through the native owner.
    pub async fn authenticate(
        &self,
        provider: String,
        action: crate::queries::AuthAction,
    ) -> Result<crate::queries::AuthAttempt, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Authenticate {
                provider,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Native session terminal command/query with an acknowledged owner receipt.
    pub async fn terminal(
        &self,
        session: SessionId,
        action: crate::queries::TerminalAction,
    ) -> Result<crate::queries::TerminalReceipt, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Terminal {
                session,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Current/final bounded recent capture; exact source pairing is required.
    pub async fn shell_snapshot(
        &self,
        session: SessionId,
        shell_id: String,
    ) -> Result<crate::queries::ShellSnapshot, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ShellSnapshot {
                session,
                shell_id,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Release this exact foreground wait into owned background execution.
    pub async fn background_shell(
        &self,
        session: SessionId,
        shell_id: String,
    ) -> Result<(), CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::BackgroundShell {
                session,
                shell_id,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Current or frozen Code Mode facts for the exact operation.
    pub async fn execute_snapshot(
        &self,
        session: SessionId,
        execute_id: String,
    ) -> Result<crate::queries::ExecuteSnapshot, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ExecuteSnapshot {
                session,
                execute_id,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Release the exact foreground program without restarting it.
    pub async fn background_execute(
        &self,
        session: SessionId,
        turn: String,
        execute_id: String,
    ) -> Result<(), CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::BackgroundExecute {
                session,
                turn,
                execute_id,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Bounded real live capture or retained terminal output of an owned shell.
    /// `None` means the operation has no shell capture.
    pub async fn shell_output(
        &self,
        session: SessionId,
        shell_id: String,
        offset: usize,
        limit: usize,
    ) -> Result<Option<crate::queries::ToolOutputPage>, CoreError> {
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ShellOutput {
                session,
                shell_id,
                offset,
                limit,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// List known sessions.
    pub async fn list_sessions(&self) -> Result<Vec<SessionId>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::List { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn session_list(
        &self,
        search: String,
        all_projects: bool,
    ) -> Result<Vec<crate::queries::SessionListEntry>, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SessionList {
                search,
                all_projects,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Check one Location-bound ID without listing any other sessions.
    pub async fn probe_session(&self, id: SessionId) -> Result<SessionProbe, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ProbeSession { id, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read the current Location's bounded deck (no root or provider work).
    pub async fn tab_deck(&self) -> Result<TabDeckSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::TabDeck { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Persist an ordered deck for the current Location after owner validation.
    pub async fn save_tab_deck(&self, deck: TabDeckSnapshot) -> Result<TabDeckSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SaveTabDeck { deck, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read committed history (user + finished assistant messages only).
    pub async fn read_history(&self, session: SessionId) -> Result<Vec<Message>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Read {
                session,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read one bounded history page (newest-first cursor).
    pub async fn history_page(
        &self,
        session: SessionId,
        before_seq: Option<i64>,
        after_seq: Option<i64>,
        limit: usize,
    ) -> Result<HistoryPage, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::History {
                session,
                message: None,
                before_seq,
                after_seq,
                limit,
                window: None,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Bounded canonical transcript, with intra-message continuation when needed.
    pub async fn history_window(
        &self,
        session: SessionId,
        window: crate::queries::HistoryReadWindow,
        limit: usize,
    ) -> Result<HistoryPage, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::History {
                session,
                message: window.part.as_ref().map(|part| part.message.clone()),
                before_seq: window.before_seq,
                after_seq: window.after_seq,
                limit,
                window: Some(window),
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    pub async fn transcript_page(
        &self,
        session: SessionId,
        before_seq: Option<i64>,
        after_seq: Option<i64>,
        limit: usize,
    ) -> Result<HistoryPage, CoreError> {
        self.history_window(
            session,
            crate::queries::HistoryReadWindow {
                before_seq,
                after_seq,
                part: None,
            },
            limit,
        )
        .await
    }

    /// Resolve one exact committed message on the current conversation branch.
    /// A hidden or foreign message produces an empty bounded page.
    pub async fn history_message(
        &self,
        session: SessionId,
        message: MessageId,
    ) -> Result<HistoryPage, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::History {
                session,
                message: Some(message),
                before_seq: None,
                after_seq: None,
                limit: 1,
                window: None,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read one bounded tool-operation page (newest-first cursor).
    pub async fn tool_ops_page(
        &self,
        session: SessionId,
        before_rowid: Option<i64>,
        limit: usize,
    ) -> Result<ToolOpPage, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ToolOps {
                session,
                before_rowid,
                limit,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Fetch the durable output beyond a tool-card preview, scoped to its
    /// session. The owner caps each request and returns a byte continuation.
    pub async fn tool_output_page(
        &self,
        session: SessionId,
        op: String,
        offset: usize,
        limit: usize,
    ) -> Result<crate::queries::ToolOutputPage, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ToolOutput {
                session,
                op,
                offset,
                limit,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Summary bodies are fetched explicitly; snapshots never embed an archive.
    pub async fn dcp_summary_page(
        &self,
        session: SessionId,
        op: String,
        block_index: usize,
        offset: i64,
        limit: usize,
    ) -> Result<Option<crate::dcp_view::DcpSummaryPage>, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::DcpSummary {
                session,
                op,
                block_index,
                offset,
                limit,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Model catalog plus the effective model/variant/agent selection.
    pub async fn catalog(&self) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Catalog { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read an admitted provider view without selecting or dispatching a model.
    pub async fn provider_catalog(&self, provider: String) -> Result<CatalogSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ProviderCatalog { provider, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// List admitted connection identities without exposing transport or auth inputs.
    pub async fn provider_connections(
        &self,
    ) -> Result<Vec<crate::queries::ProviderConnection>, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ProviderConnections { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Model dialog view of every admitted provider whose catalog is already
    /// loaded. Never starts discovery, selects a model or reads credentials' values.
    pub async fn model_catalogs(&self) -> Result<crate::queries::ModelCatalogs, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ModelCatalogs { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read the model dialog favorites/recents, or apply one owner-side mutation
    /// and return the stored result. Identity only; never a selection.
    pub async fn model_preference(
        &self,
        action: Option<crate::queries::ModelPreferenceAction>,
    ) -> Result<crate::queries::ModelPreference, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ModelPreference { action, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read (`None`) or save the shared MCP sidebar presentation preference.
    pub async fn sidebar_mcp_open(&self, open: Option<bool>) -> Result<bool, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SidebarMcpOpen { open, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Probe, confirm or refresh one custom connection without frontend persistence.
    pub async fn custom_provider(
        &self,
        action: crate::queries::CustomProviderAction,
    ) -> Result<crate::queries::CustomProviderReply, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::CustomProvider { action, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read or mutate scoped accounts through the existing application owner.
    pub async fn provider_accounts(
        &self,
        provider: String,
        action: Option<crate::queries::AccountAction>,
    ) -> Result<crate::queries::ProviderAccounts, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ProviderAccounts {
                provider,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// OC2 steer/queue input for a session whose turn is running.
    pub async fn enqueue(
        &self,
        session: SessionId,
        text: crate::prompt::PromptInput,
        delivery: crate::queries::InboxDelivery,
    ) -> Result<crate::queries::InboxItem, CoreError> {
        self.enqueue_selected(session, text, delivery, None).await
    }

    pub async fn enqueue_selected(
        &self,
        session: SessionId,
        text: crate::prompt::PromptInput,
        delivery: crate::queries::InboxDelivery,
        selection: Option<crate::queries::ModelCommit>,
    ) -> Result<crate::queries::InboxItem, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Enqueue {
                session,
                text,
                delivery,
                selection,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read or change the session's pending inbox.
    pub async fn inbox(
        &self,
        session: SessionId,
        action: crate::queries::InboxAction,
    ) -> Result<crate::queries::InboxSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Inbox {
                session,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read/change the durable session/agent selection. Home model choices also
    /// remember the Location/agent draft; no transcript is copied or modified.
    pub async fn session_selection(
        &self,
        session: SessionId,
        home: bool,
        action: crate::queries::SessionSelectionAction,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SessionSelection {
                session,
                home,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read/change the application's Home choice without creating a session.
    pub async fn home_selection(
        &self,
        action: crate::queries::SessionSelectionAction,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::HomeSelection { action, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Switch the running application to another Location (project path).
    pub async fn switch_location(&self, path: String) -> Result<LocationSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SwitchLocation { path, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Switch a sessionless Home composer without minting a root. A failed
    /// target validation leaves the currently published generation untouched.
    pub async fn switch_location_home(
        &self,
        path: String,
    ) -> Result<HomeLocationSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SwitchLocationHome { path, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Atomically reload the current canonical Location for subsequent turns.
    /// Refused by the owner while a turn or title request is in progress. A
    /// failed rebuild leaves the published generation and selection effective.
    pub async fn reload_location(&self) -> Result<ReloadLocationSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ReloadLocation { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Skill catalog cards (metadata only).
    pub async fn skills(&self) -> Result<Vec<SkillCard>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Skills { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Suggest bounded Location-relative file paths without reading file contents.
    /// Responses are tagged with the Location and owner epoch: discard a late
    /// response when either differs from the currently displayed route.
    pub async fn file_suggestions(
        &self,
        query: String,
        limit: usize,
    ) -> Result<FileSuggestionsSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::FileSuggestions { query, limit, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Select the effective model (and optional variant) for later turns.
    pub async fn select_model(
        &self,
        id: String,
        variant: Option<String>,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SelectModel {
                id,
                variant,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Select the effective primary agent for later turns.
    pub async fn select_agent(&self, id: String) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SelectAgent { id, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// DCP context/stats snapshot for a session.
    pub async fn dcp_snapshot(&self, session: SessionId) -> Result<DcpSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Dcp {
                session,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Enqueue manual compression without waiting for durable acceptance.
    pub fn request_compress(
        &self,
        session: SessionId,
        focus: String,
    ) -> Result<SubmissionReceipt, CoreError> {
        if focus.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Compress {
                session,
                focus,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Manual DCP compress request; waits for the accepted turn id.
    pub async fn compress(
        &self,
        session: SessionId,
        focus: String,
    ) -> Result<WorkerTurnId, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Compress {
                session,
                focus,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Request clean shutdown; worker finishes without orphan tasks.
    pub async fn shutdown(&self) -> Result<(), CoreError> {
        self.dispatch(InboxMsg::Shutdown).await
    }
}

impl WorkerGuard {
    /// Own the native application task using the same shutdown/join contract.
    pub fn from_task(handle: tokio::task::JoinHandle<Result<(), String>>) -> Self {
        Self {
            handle: Some(WorkerTask::Legacy(handle)),
        }
    }

    /// Native workers preserve their safe typed cause through shutdown/join.
    pub fn from_diagnostic_task(
        handle: tokio::task::JoinHandle<Result<(), crate::queries::ServiceDiagnostic>>,
    ) -> Self {
        Self {
            handle: Some(WorkerTask::Diagnostic(handle)),
        }
    }

    /// Wait for the worker task to finish and surface its cleanup result.
    pub async fn join(self) -> Result<(), String> {
        self.join_diagnostic()
            .await
            .map_err(|diagnostic| diagnostic.to_string())
    }

    /// No panic payload or legacy raw error is a diagnostic value.
    pub async fn join_diagnostic(mut self) -> Result<(), crate::queries::ServiceDiagnostic> {
        use crate::queries::{
            ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
        };
        let failed = |cancelled| ServiceDiagnostic {
            kind: ServiceKind::Runtime,
            service: "worker".into(),
            source: "source-native/worker".into(),
            field: vec!["worker".into()],
            stage: ServiceStage::Cleanup,
            code: if cancelled {
                ServiceCode::Cancelled
            } else {
                ServiceCode::RuntimeFailed
            },
            action: ServiceAction::RestartApplication,
            details: None,
        };
        match self.handle.take() {
            Some(WorkerTask::Diagnostic(task)) => {
                task.await.map_err(|error| failed(error.is_cancelled()))?
            }
            Some(WorkerTask::Legacy(task)) => task
                .await
                .map_err(|error| failed(error.is_cancelled()))?
                .map_err(|_| failed(false)),
            None => Ok(()),
        }
    }
}

async fn worker_loop(
    provider: MockProvider,
    mut inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
) {
    let mut sessions: HashMap<String, SessionState> = HashMap::new();
    let mut active: Option<ActiveTurn> = None;
    let mut turn_counter: u64 = 0;
    let mut prompt_history = Vec::new();

    loop {
        if let Some(turn) = active.as_mut() {
            let delay = Duration::from_millis(provider.delay_ms);
            tokio::select! {
                msg = inbox.recv() => {
                    match msg {
                        None => break,
                        Some(InboxMsg::Shutdown) => {
                            let partial = turn.accumulated.clone();
                            let _ = events.send(CoreEvent::TurnInterrupted {
                                session: turn.session.clone(),
                                turn: turn.turn.clone(),
                                partial,
                                duration_ms: elapsed_ms(turn.started),
                            });
                            break;
                        }
                        Some(InboxMsg::Create { id, ack }) => {
                            use std::collections::hash_map::Entry;
                            let res = match sessions.entry(id.0.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(SessionState::new(id));
                                    Ok(())
                                }
                                Entry::Occupied(_) => Err(CoreError::SessionAlreadyExists),
                            };
                            let _ = ack.send(res);
                        }
                        Some(InboxMsg::Submit { session: _, text: _, ack, .. }) => {
                            let _ = ack.send(Err(CoreError::TurnBusy));
                        }
                        Some(InboxMsg::SubmitFresh { ack, .. }) => {
                            let _ = ack.send(Err(CoreError::TurnBusy));
                        }
                        Some(InboxMsg::Cancel { session, ack }) => {
                            if session == turn.session {
                                let partial = turn.accumulated.clone();
                                let done = CoreEvent::TurnInterrupted {
                                    session: turn.session.clone(),
                                    turn: turn.turn.clone(),
                                    partial,
                                    duration_ms: elapsed_ms(turn.started),
                                };
                                active = None;
                                let _ = events.send(done);
                                let _ = ack.send(Ok(()));
                            } else {
                                let _ = ack.send(Err(CoreError::TurnNotActive));
                            }
                        }
                        Some(InboxMsg::List { ack }) => {
                            let mut ids: Vec<SessionId> =
                                sessions.values().map(|s| s.id.clone()).collect();
                            ids.sort_by(|a, b| a.0.cmp(&b.0));
                            let _ = ack.send(Ok(ids));
                        }
                        Some(InboxMsg::ProbeSession { id, ack }) => {
                            let kind = sessions.get(&id.0).map_or(SessionProbe::Absent, |_| SessionProbe::Root);
                            let _ = ack.send(Ok(kind));
                        }
                        Some(InboxMsg::Read { session, ack }) => {
                            let res = sessions
                                .get(&session.0)
                                .map(|s| s.messages.clone())
                                .ok_or(CoreError::SessionNotFound);
                            let _ = ack.send(res);
                        }
                        Some(InboxMsg::PromptHistory { append, ack }) => {
                            let result = append.as_deref().map_or(Ok(()), |text| crate::queries::append_prompt_history(&mut prompt_history, text));
                            let _ = ack.send(result.map(|()| prompt_history.clone()));
                        }
                        Some(other) => scripted_unsupported(other),
                    }
                }
                _ = tokio::time::sleep(delay) => {
                    if turn.index < turn.chunks.len() {
                        let delta = turn.chunks[turn.index].clone();
                        turn.accumulated.push_str(&delta);
                        turn.index += 1;
                        let _ = events.send(CoreEvent::TextDelta {
                            session: turn.session.clone(),
                            turn: turn.turn.clone(),
                            delta,
                        });
                        if turn.index == turn.chunks.len() {
                            let text = turn.accumulated.clone();
                            if let Some(sess) = sessions.get_mut(&turn.session.0) {
                                sess.push(Role::Assistant, text.clone());
                            }
                            let done = CoreEvent::TurnFinished {
                                session: turn.session.clone(),
                                turn: turn.turn.clone(),
                                text,
                                duration_ms: elapsed_ms(turn.started),
                                warnings: Vec::new(),
                                service_warning_range: 0..0,
                            };
                            active = None;
                            let _ = events.send(done);
                        }
                    }
                }
            }
        } else {
            match inbox.recv().await {
                None => break,
                Some(InboxMsg::Shutdown) => break,
                Some(InboxMsg::Create { id, ack }) => {
                    use std::collections::hash_map::Entry;
                    let res = match sessions.entry(id.0.clone()) {
                        Entry::Vacant(entry) => {
                            entry.insert(SessionState::new(id));
                            Ok(())
                        }
                        Entry::Occupied(_) => Err(CoreError::SessionAlreadyExists),
                    };
                    let _ = ack.send(res);
                }
                Some(InboxMsg::Submit {
                    session, text, ack, ..
                }) => {
                    scripted_accept_submit(
                        &provider,
                        &events,
                        &mut sessions,
                        &mut active,
                        &mut turn_counter,
                        &mut prompt_history,
                        session,
                        text,
                        false,
                        ack,
                    );
                }
                Some(InboxMsg::SubmitFresh {
                    session,
                    text,
                    selection: _,
                    ack,
                }) => {
                    scripted_accept_submit(
                        &provider,
                        &events,
                        &mut sessions,
                        &mut active,
                        &mut turn_counter,
                        &mut prompt_history,
                        session,
                        text,
                        true,
                        ack,
                    );
                }
                Some(InboxMsg::Cancel { session: _, ack }) => {
                    let _ = ack.send(Err(CoreError::TurnNotActive));
                }
                Some(InboxMsg::List { ack }) => {
                    let mut ids: Vec<SessionId> = sessions.values().map(|s| s.id.clone()).collect();
                    ids.sort_by(|a, b| a.0.cmp(&b.0));
                    let _ = ack.send(Ok(ids));
                }
                Some(InboxMsg::ProbeSession { id, ack }) => {
                    let kind = sessions
                        .get(&id.0)
                        .map_or(SessionProbe::Absent, |_| SessionProbe::Root);
                    let _ = ack.send(Ok(kind));
                }
                Some(InboxMsg::Read { session, ack }) => {
                    let res = sessions
                        .get(&session.0)
                        .map(|s| s.messages.clone())
                        .ok_or(CoreError::SessionNotFound);
                    let _ = ack.send(res);
                }
                Some(InboxMsg::PromptHistory { append, ack }) => {
                    let result = append.as_deref().map_or(Ok(()), |text| {
                        crate::queries::append_prompt_history(&mut prompt_history, text)
                    });
                    let _ = ack.send(result.map(|()| prompt_history.clone()));
                }
                Some(other) => scripted_unsupported(other),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scripted_accept_submit(
    provider: &MockProvider,
    events: &broadcast::Sender<CoreEvent>,
    sessions: &mut HashMap<String, SessionState>,
    active: &mut Option<ActiveTurn>,
    turn_counter: &mut u64,
    prompt_history: &mut Vec<String>,
    session: SessionId,
    text: crate::prompt::PromptInput,
    fresh: bool,
    ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
) {
    if !text.images.is_empty() {
        let _ = ack.send(Err(CoreError::Application(
            "image input unavailable in mock runtime".into(),
        )));
        return;
    }
    let text = text.text;
    if text.len() > MAX_INPUT_BYTES {
        let _ = ack.send(Err(CoreError::InputTooLarge));
        return;
    }
    if fresh {
        if text.trim().is_empty() {
            let _ = ack.send(Err(CoreError::Application("empty prompt".to_string())));
            return;
        }
        if sessions.contains_key(&session.0) {
            let _ = ack.send(Err(CoreError::SessionAlreadyExists));
            return;
        }
        sessions.insert(session.0.clone(), SessionState::new(session.clone()));
    }
    let Some(sess) = sessions.get_mut(&session.0) else {
        let _ = ack.send(Err(CoreError::SessionNotFound));
        return;
    };
    *turn_counter += 1;
    let turn_id = WorkerTurnId(format!("t{:04}", turn_counter));
    sess.push(Role::User, text.clone());
    crate::queries::append_prompt_history(prompt_history, &text).expect("input already bounded");
    *active = Some(ActiveTurn {
        session: session.clone(),
        turn: turn_id.clone(),
        chunks: provider.plan(&text),
        index: 0,
        accumulated: String::new(),
        started: std::time::Instant::now(),
    });
    let _ = ack.send(Ok(turn_id.clone()));
    let _ = events.send(CoreEvent::TurnStarted {
        session,
        turn: turn_id,
        model_switch: None,
    });
}

/// Whole milliseconds since `started`, saturating at `u64::MAX`.
fn elapsed_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Scripted worker answer for owner-only queries: typed, never silent.
fn scripted_unsupported(message: InboxMsg) {
    let error = || CoreError::Application("query unsupported by scripted worker".to_string());
    match message {
        InboxMsg::SessionViewed { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Workspace { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SubmitInbox { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::UserShell { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::PromptHistory { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::McpLookup { ack, .. } => {
            let _ = ack.send(Err(crate::queries::McpLookupError::Unavailable));
        }
        InboxMsg::McpStatus { ack } | InboxMsg::McpControl { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::PendingApprovals { ack } => {
            let _ = ack.send(Ok(Vec::new()));
        }
        InboxMsg::PendingQuestions { ack } => {
            let _ = ack.send(Ok(Vec::new()));
        }
        InboxMsg::ReplyQuestion { ack, .. } => {
            let _ = ack.send(Err(crate::question::QuestionReplyError::Unavailable));
        }
        InboxMsg::RegisterQuestionConsumer { ack } => {
            let _ = ack.send(Ok(()));
        }
        InboxMsg::ReplyApproval { ack, .. } | InboxMsg::RegisterApprovalConsumer { ack, .. } => {
            let _ = ack.send(Err(CoreError::Application(
                "approval unavailable in scripted runtime".into(),
            )));
        }
        InboxMsg::CompactSession { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::CancelCompaction { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::CompactionHistory { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ChangeConversation { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ForkSession { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::History { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ToolOps { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ToolOutput { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ShellOutput { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::CancelShell { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::BackgroundShell { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::BackgroundExecute { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ShellJobs { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ExecuteJobs { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::AuthMethods { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Authenticate { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Terminal { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ChildJobs { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::InterruptChild { ack, .. } | InboxMsg::BackgroundChild { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ReadChild { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ShellSnapshot { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ExecuteSnapshot { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::DcpSummary { ack, .. } => {
            let _ = ack.send(Err(CoreError::Shutdown));
        }
        InboxMsg::Catalog { ack }
        | InboxMsg::ProviderCatalog { ack, .. }
        | InboxMsg::SessionSelection { ack, .. }
        | InboxMsg::HomeSelection { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ProviderAccounts { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::CustomProvider { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ProviderConnections { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ModelCatalogs { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ModelPreference { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SidebarMcpOpen { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Skills { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::FileSuggestions { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::TabDeck { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SaveTabDeck { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::RenameSession { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::DeleteSession { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::PickerSessionAction { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::OpenPickerSession { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SessionPickerContext { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::RegenerateTitle { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::CancelTitle { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SelectModel { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SelectAgent { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Enqueue { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Inbox { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SwitchLocation { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SwitchLocationHome { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ReloadLocation { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Dcp { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SessionList { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Compress { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Create { .. }
        | InboxMsg::Submit { .. }
        | InboxMsg::SubmitFresh { .. }
        | InboxMsg::Cancel { .. }
        | InboxMsg::List { .. }
        | InboxMsg::ProbeSession { .. }
        | InboxMsg::Read { .. }
        | InboxMsg::Shutdown => {}
    }
}

#[cfg(test)]
mod fork_tests {
    use super::*;

    #[tokio::test]
    async fn scripted_fork_is_explicitly_unsupported_and_creates_nothing() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        assert!(
            matches!(app.fork_session(SessionId("source".into()), MessageId("m0001".into())).await,
            Err(CoreError::Application(reason)) if reason.contains("unsupported"))
        );
        assert!(app.list_sessions().await.unwrap().is_empty());
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}

#[cfg(test)]
#[path = "core_app/diagnostic_tests.rs"]
mod diagnostic_tests;

#[cfg(test)]
#[path = "core_app/prompt_history_tests.rs"]
mod prompt_history_tests;

#[cfg(test)]
#[path = "core_app/native_tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::{
        CoreApp, CoreEvent, FreshSelection, InboxMsg, MAX_SESSION_TITLE_BYTES, MockProvider,
        WorkerGuard, WorkerTurnId, normalized_session_title,
    };
    use crate::domain::SessionId;
    use crate::session::{CoreError, Role};
    use std::time::Duration;

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("valid session id")
    }

    async fn collect_until_finished(
        rx: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
        timeout: Duration,
    ) -> (Vec<String>, String) {
        let mut deltas = Vec::new();
        loop {
            let ev = tokio::time::timeout(timeout, rx.recv())
                .await
                .expect("event timeout")
                .expect("event channel");
            match ev {
                CoreEvent::SessionModelSelected { .. }
                | CoreEvent::SelectionNoticeCommitted { .. }
                | CoreEvent::InboxChanged { .. }
                | CoreEvent::RetryScheduled { .. }
                | CoreEvent::WorkspaceChanged { .. }
                | CoreEvent::SessionViewed { .. }
                | CoreEvent::ProjectionGap
                | CoreEvent::OwnerDisconnected
                | CoreEvent::OwnerReattached
                | CoreEvent::OwnerUnrecoverable { .. }
                | CoreEvent::Projection { .. } => {}
                CoreEvent::TextDelta { delta, .. } => deltas.push(delta),
                CoreEvent::TurnFinished { text, .. } => return (deltas, text),
                CoreEvent::TurnInterrupted { partial, .. } => {
                    panic!("unexpected interrupt partial={partial}")
                }
                CoreEvent::TurnStarted { .. }
                | CoreEvent::SessionTitleUpdated { .. }
                | CoreEvent::SessionMoved { .. }
                | CoreEvent::TurnPresentation { .. }
                | CoreEvent::ReasoningDelta { .. }
                | CoreEvent::ReasoningItemEnded { .. }
                | CoreEvent::TurnUsage { .. }
                | CoreEvent::TurnContextUsage { .. }
                | CoreEvent::ToolCallStarted { .. }
                | CoreEvent::ToolArgumentStream { .. }
                | CoreEvent::ToolCallProgress { .. }
                | CoreEvent::ToolCallFinished { .. } => {}
                CoreEvent::Compaction(_) => panic!("unexpected compaction"),
                CoreEvent::ChildNotice(_)
                | CoreEvent::ShellNotice(_)
                | CoreEvent::ExecuteNotice(_)
                | CoreEvent::ShellChanged { .. }
                | CoreEvent::ExecuteChanged { .. }
                | CoreEvent::TerminalChanged { .. } => {
                    panic!("unexpected shell in scripted runtime")
                }
                CoreEvent::McpChanged(_) => panic!("unexpected MCP in scripted runtime"),
                CoreEvent::ProviderChanged => {
                    panic!("unexpected native provider in scripted runtime")
                }
                CoreEvent::PermissionAsked(_) | CoreEvent::PermissionResolved { .. } => {
                    panic!("unexpected permission request")
                }
                CoreEvent::QuestionAsked(_) | CoreEvent::QuestionResolved { .. } => {
                    panic!("unexpected question request")
                }
                CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
                CoreEvent::ParentContinuationRefused { diagnostic, .. } => {
                    panic!("unexpected continuation refusal: {diagnostic}")
                }
            }
        }
    }

    #[tokio::test]
    async fn worker_guard_surfaces_cleanup_failure() {
        let handle = tokio::spawn(async { Err("mcp shutdown failed".to_string()) });
        let error = WorkerGuard::from_task(handle)
            .join()
            .await
            .expect_err("cleanup failure must not be reported as success");
        assert!(error.contains("cleanup: runtime_failed"), "{error}");
        assert!(error.contains("source-native/worker"), "{error}");
        assert!(error.contains("restart application"), "{error}");
        assert!(!error.contains("mcp shutdown failed"), "{error}");
        let panicked = tokio::spawn(async { panic!("worker panic probe") });
        let error = WorkerGuard::from_task(panicked)
            .join()
            .await
            .expect_err("a panicked worker is a join failure");
        assert!(error.contains("cleanup: runtime_failed"), "{error}");
        assert!(!error.contains("worker panic probe"), "{error}");
    }

    #[test]
    fn session_title_normalization_requires_visible_text_and_limits_utf8_bytes() {
        for invalid in [
            "\u{200b}",
            " \u{200b} ",
            "hello\u{200b}world",
            "a\u{200d}b",
            "\u{200d}",
            "\u{200d}👩",
            "👩\u{200d}",
            "👩\u{200d} 👩",
            "👩 \u{200d}👩",
            "👩\u{200d}\u{200d}💻",
            "👩\u{301}\u{200d}💻",
            "👩\u{200d}a",
            "a\u{200d}👩",
            "♩\u{200d}♩",
            "👩\u{200d}\u{fe0f}💻",
            "👩\u{200d}🏽",
            "👩\u{fe0f}\u{fe0f}\u{200d}💻",
            "👩\u{200d}💻\u{200b}",
            "👩\u{200d}💻\u{202e}",
            "a\u{00ad}b",
            "a\u{034f}b",
            "a\u{202e}b",
            "a\u{2066}b",
            "a\u{e0020}b",
            "a\nb",
            "\u{fe0f}",
            "\u{034f}\u{fe0f}",
            "\u{2800}",
            " \u{3164} ",
            " \t ",
        ] {
            assert_eq!(normalized_session_title(invalid), None, "{invalid:?}");
        }
        assert_eq!(normalized_session_title("  Café 🦀  "), Some("Café 🦀"));
        for valid in [
            "👩\u{200d}💻",
            "👩🏽\u{200d}💻",
            "👩\u{200d}⚕️",
            "👩\u{200d}❤️\u{200d}👩",
            "  Pair 👩\u{200d}💻  ",
        ] {
            assert_eq!(normalized_session_title(valid), Some(valid.trim()));
        }
        assert_eq!(
            normalized_session_title(" e\u{301} 👩️ "),
            Some("e\u{301} 👩️")
        );
        let max_ascii = "a".repeat(MAX_SESSION_TITLE_BYTES);
        let max_unicode = "é".repeat(MAX_SESSION_TITLE_BYTES / 2);
        let max_emoji = "🦀".repeat(MAX_SESSION_TITLE_BYTES / 4);
        for valid in [&max_ascii, &max_unicode, &max_emoji] {
            assert_eq!(normalized_session_title(valid), Some(valid.as_str()));
            assert_eq!(
                normalized_session_title(&format!(" {valid} ")),
                Some(valid.as_str())
            );
            assert_eq!(normalized_session_title(&format!("{valid}a")), None);
        }
        let joined = "👩\u{200d}💻"; // 11 UTF-8 bytes, including the joiner.
        let within_limit = format!("{}{}", "a".repeat(245), joined);
        assert_eq!(within_limit.len(), MAX_SESSION_TITLE_BYTES);
        assert_eq!(
            normalized_session_title(&within_limit),
            Some(within_limit.as_str())
        );
        assert_eq!(normalized_session_title(&format!("{within_limit}a")), None);
    }

    #[tokio::test]
    async fn scripted_worker_explicitly_refuses_rename() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        app.create_session(sid("root")).await.unwrap();
        assert_eq!(
            app.rename_session(sid("root"), "New title".into()).await,
            Err(CoreError::Application(
                "query unsupported by scripted worker".into()
            ))
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn vertical_input_to_response() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut rx = app.subscribe();
        app.create_session(sid("s-1")).await.expect("create");
        let turn = app
            .submit(sid("s-1"), "hello".to_string())
            .await
            .expect("submit");
        assert_eq!(turn.0, "t0001");

        let (deltas, text) = collect_until_finished(&mut rx, Duration::from_secs(5)).await;
        assert_eq!(deltas, vec!["echo: ".to_string(), "hello".to_string()]);
        assert_eq!(text, "echo: hello");

        let history = app.read_history(sid("s-1")).await.expect("history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, Role::User);
        assert_eq!(history[0].text, "hello");
        assert_eq!(history[1].role, Role::Assistant);
        assert_eq!(history[1].text, "echo: hello");

        let sessions = app.list_sessions().await.expect("list");
        assert_eq!(sessions, vec![sid("s-1")]);

        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn fresh_receipt_waits_for_owner_ack_and_preserves_explicit_selection() {
        let (app, mut inbox, _) = CoreApp::channel(1);
        let selected = FreshSelection {
            binding: None,
            agent_id: Some("build".into()),
            model_id: "model-x".into(),
            variant: None,
        };
        let mut receipt = app
            .request_submit_fresh(sid("fresh"), "prompt".into(), Some(selected.clone()))
            .expect("enqueued");
        assert!(receipt.try_result().is_none(), "enqueue is not acceptance");
        let err = app
            .request_submit_fresh(sid("other"), "prompt".into(), None)
            .err()
            .expect("bounded queue must refuse");
        assert_eq!(err, CoreError::QueueFull);
        let Some(InboxMsg::SubmitFresh {
            session,
            text,
            selection,
            ack,
        }) = inbox.recv().await
        else {
            panic!("expected fresh submit");
        };
        assert_eq!(session, sid("fresh"));
        assert_eq!(text.text, "prompt");
        assert_eq!(selection, Some(selected));
        assert!(receipt.try_result().is_none());
        ack.send(Ok(WorkerTurnId("durable-turn".into())))
            .expect("ack");
        assert_eq!(
            receipt.try_result(),
            Some(Ok(WorkerTurnId("durable-turn".into())))
        );
        assert_eq!(
            app.request_submit_fresh(
                sid("large"),
                "x".repeat(crate::session::MAX_INPUT_BYTES + 1),
                None
            )
            .err(),
            Some(CoreError::InputTooLarge)
        );
        drop(inbox);
        assert_eq!(
            app.request_submit_fresh(sid("closed"), "prompt".into(), None)
                .err(),
            Some(CoreError::Shutdown)
        );
    }

    #[tokio::test]
    async fn admission_selection_receipt_is_bounded_and_owner_acknowledged() {
        let (app, mut inbox, _) = CoreApp::channel(1);
        let mut receipt = app
            .request_session_selection(
                sid("selection"),
                true,
                crate::queries::SessionSelectionAction::Agent("build".into()),
            )
            .unwrap();
        assert!(receipt.try_result().is_none());
        assert_eq!(
            app.request_session_selection(
                sid("other"),
                false,
                crate::queries::SessionSelectionAction::Agent("plan".into())
            )
            .err(),
            Some(CoreError::QueueFull)
        );
        let InboxMsg::SessionSelection {
            session,
            home,
            action,
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("selection")
        };
        assert_eq!(session, sid("selection"));
        assert!(home);
        assert_eq!(
            action,
            crate::queries::SessionSelectionAction::Agent("build".into())
        );
        ack.send(Err(CoreError::TurnBusy)).unwrap();
        assert_eq!(receipt.try_result(), Some(Err(CoreError::TurnBusy)));
        let mut dropped = app
            .request_session_selection(
                sid("selection"),
                false,
                crate::queries::SessionSelectionAction::Agent("build".into()),
            )
            .unwrap();
        drop(inbox.recv().await);
        assert_eq!(dropped.try_result(), Some(Err(CoreError::Shutdown)));
        drop(inbox);
        assert_eq!(
            app.request_session_selection(
                sid("closed"),
                false,
                crate::queries::SessionSelectionAction::Agent("build".into())
            )
            .err(),
            Some(CoreError::Shutdown)
        );
    }

    #[tokio::test]
    async fn admission_enqueue_receipt_preserves_capture_and_never_retries() {
        use crate::queries::{InboxDelivery, InboxItem, ModelCommit, SelectionBinding};
        let (app, mut inbox, _) = CoreApp::channel(1);
        let selected = ModelCommit {
            attachment: app.attachment(),
            caller: 27,
            draft_revision: 4,
            binding: SelectionBinding {
                location: Some("/fixture".into()),
                generation: 9,
                provider: "chosen-provider".into(),
                agent_id: Some("build".into()),
            },
            model_id: "exact/model".into(),
            variant: Some("low".into()),
        };
        let mut receipt = app
            .request_enqueue_selected(
                sid("inbox"),
                "Ω prompt".into(),
                InboxDelivery::Queue,
                Some(selected.clone()),
            )
            .unwrap();
        assert!(receipt.try_result().is_none());
        assert_eq!(
            app.request_enqueue_selected(sid("full"), "other".into(), InboxDelivery::Steer, None)
                .err(),
            Some(CoreError::QueueFull)
        );
        let InboxMsg::Enqueue {
            session,
            text,
            delivery,
            selection,
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("enqueue")
        };
        assert_eq!(session, sid("inbox"));
        assert_eq!(text.text, "Ω prompt");
        assert_eq!(delivery, InboxDelivery::Queue);
        assert_eq!(selection, Some(selected));
        let item = InboxItem {
            id: "one".into(),
            text: text.text,
            images: 0,
            delivery,
            error: None,
        };
        ack.send(Ok(item.clone())).unwrap();
        assert_eq!(receipt.try_result(), Some(Ok(item)));
        assert_eq!(
            app.request_enqueue_selected(
                sid("large"),
                "x".repeat(crate::session::MAX_INPUT_BYTES + 1).into(),
                InboxDelivery::Queue,
                None
            )
            .err(),
            Some(CoreError::InputTooLarge)
        );
        let mut unknown = app
            .request_enqueue_selected(sid("unknown"), "once".into(), InboxDelivery::Steer, None)
            .unwrap();
        drop(inbox.recv().await);
        assert_eq!(unknown.try_result(), Some(Err(CoreError::Shutdown)));
        assert!(
            inbox.try_recv().is_err(),
            "receipt polling cannot replay a mutation"
        );
        drop(inbox);
        assert_eq!(
            app.request_enqueue_selected(sid("closed"), "once".into(), InboxDelivery::Queue, None)
                .err(),
            Some(CoreError::Shutdown)
        );
    }

    #[tokio::test]
    async fn mock_fresh_rejects_without_creating_and_keeps_existing_submit_path() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["reply".into()], 100));
        assert_eq!(
            app.submit_fresh(sid("fresh"), "  \n ".into(), None).await,
            Err(CoreError::Application("empty prompt".into()))
        );
        assert!(app.list_sessions().await.expect("list").is_empty());
        app.create_session(sid("taken")).await.expect("create");
        assert_eq!(
            app.submit_fresh(sid("taken"), "prompt".into(), None).await,
            Err(CoreError::SessionAlreadyExists)
        );
        assert!(
            app.read_history(sid("taken"))
                .await
                .expect("history")
                .is_empty()
        );
        let turn = app
            .submit_fresh(sid("fresh"), "first".into(), None)
            .await
            .expect("accepted");
        assert_eq!(turn, WorkerTurnId("t0001".into()));
        assert_eq!(
            app.list_sessions().await.expect("list"),
            [sid("fresh"), sid("taken")]
        );
        assert_eq!(
            app.read_history(sid("fresh")).await.expect("history")[0].text,
            "first"
        );
        assert_eq!(
            app.submit_fresh(sid("busy"), "second".into(), None).await,
            Err(CoreError::TurnBusy)
        );
        assert_eq!(
            app.read_history(sid("busy")).await,
            Err(CoreError::SessionNotFound)
        );
        app.cancel(sid("fresh")).await.expect("cancel");
        assert_eq!(
            app.submit_fresh(sid("fresh"), "again".into(), None).await,
            Err(CoreError::SessionAlreadyExists)
        );
        assert_eq!(
            app.submit(sid("fresh"), "continued".into()).await,
            Ok(WorkerTurnId("t0002".into()))
        );
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn cancel_long_stream_interrupts() {
        let chunks = (0..10).map(|i| format!("c{i} ")).collect::<Vec<_>>();
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(chunks, 20));
        let mut rx = app.subscribe();
        app.create_session(sid("s-cancel")).await.expect("create");
        app.submit(sid("s-cancel"), "long".to_string())
            .await
            .expect("submit");

        // First delta must arrive before cancel.
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("event");
        match first {
            CoreEvent::TurnStarted { .. } => {
                let second = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .expect("timeout")
                    .expect("event");
                assert!(matches!(second, CoreEvent::TextDelta { .. }));
            }
            CoreEvent::TextDelta { .. } => {}
            other => panic!("unexpected first event: {other:?}"),
        }

        app.cancel(sid("s-cancel")).await.expect("cancel");
        // Next terminal event must be Interrupted, never Finished.
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("timeout")
                .expect("event");
            match ev {
                CoreEvent::SessionModelSelected { .. }
                | CoreEvent::SelectionNoticeCommitted { .. }
                | CoreEvent::InboxChanged { .. }
                | CoreEvent::RetryScheduled { .. }
                | CoreEvent::WorkspaceChanged { .. }
                | CoreEvent::SessionViewed { .. }
                | CoreEvent::ProjectionGap
                | CoreEvent::OwnerDisconnected
                | CoreEvent::OwnerReattached
                | CoreEvent::OwnerUnrecoverable { .. }
                | CoreEvent::Projection { .. } => {}
                CoreEvent::TurnInterrupted { partial, .. } => {
                    assert!(partial.contains("c0 "), "partial={partial}");
                    assert!(
                        !partial.contains("c9 "),
                        "partial must be partial: {partial}"
                    );
                    break;
                }
                CoreEvent::TurnFinished { text, .. } => {
                    panic!("cancel must not finish text={text}")
                }
                CoreEvent::PermissionAsked(_) | CoreEvent::PermissionResolved { .. } => {
                    panic!("unexpected permission request")
                }
                CoreEvent::QuestionAsked(_) | CoreEvent::QuestionResolved { .. } => {
                    panic!("unexpected question request")
                }
                CoreEvent::TextDelta { .. }
                | CoreEvent::SessionTitleUpdated { .. }
                | CoreEvent::SessionMoved { .. }
                | CoreEvent::TurnPresentation { .. }
                | CoreEvent::TurnStarted { .. }
                | CoreEvent::ReasoningDelta { .. }
                | CoreEvent::ReasoningItemEnded { .. }
                | CoreEvent::TurnUsage { .. }
                | CoreEvent::TurnContextUsage { .. }
                | CoreEvent::ToolCallStarted { .. }
                | CoreEvent::ToolArgumentStream { .. }
                | CoreEvent::ToolCallProgress { .. }
                | CoreEvent::ToolCallFinished { .. } => {}
                CoreEvent::Compaction(_) => panic!("unexpected compaction"),
                CoreEvent::ChildNotice(_)
                | CoreEvent::ShellNotice(_)
                | CoreEvent::ExecuteNotice(_)
                | CoreEvent::ShellChanged { .. }
                | CoreEvent::ExecuteChanged { .. }
                | CoreEvent::TerminalChanged { .. } => {
                    panic!("unexpected shell in scripted runtime")
                }
                CoreEvent::McpChanged(_) => panic!("unexpected MCP in scripted runtime"),
                CoreEvent::ProviderChanged => {
                    panic!("unexpected native provider in scripted runtime")
                }
                CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
                CoreEvent::ParentContinuationRefused { diagnostic, .. } => {
                    panic!("unexpected continuation refusal: {diagnostic}")
                }
            }
        }

        // Interrupted assistant text is not committed; only user remains.
        let history = app.read_history(sid("s-cancel")).await.expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, Role::User);

        // Worker stays usable after cancel.
        let turn = app
            .submit(sid("s-cancel"), "again".to_string())
            .await
            .expect("resubmit");
        assert_eq!(turn.0, "t0002");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn second_submit_while_active_is_busy() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            vec!["a".to_string(), "b".to_string()],
            50,
        ));
        app.create_session(sid("s-busy")).await.expect("create");
        app.submit(sid("s-busy"), "first".to_string())
            .await
            .expect("first");
        let err = app
            .submit(sid("s-busy"), "second".to_string())
            .await
            .expect_err("busy");
        assert_eq!(err, crate::session::CoreError::TurnBusy);
        app.cancel(sid("s-busy")).await.expect("cancel");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn rejects_unknown_session_and_oversize_and_stale_cancel() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let err = app
            .submit(sid("missing"), "hi".to_string())
            .await
            .expect_err("missing");
        assert_eq!(err, crate::session::CoreError::SessionNotFound);
        app.create_session(sid("s-err")).await.expect("create");
        let dup = app.create_session(sid("s-err")).await.expect_err("dup");
        assert_eq!(dup, crate::session::CoreError::SessionAlreadyExists);
        let big = "x".repeat(crate::session::MAX_INPUT_BYTES + 1);
        let err = app.submit(sid("s-err"), big).await.expect_err("big");
        assert_eq!(err, crate::session::CoreError::InputTooLarge);
        let err = app.cancel(sid("s-err")).await.expect_err("no turn");
        assert_eq!(err, crate::session::CoreError::TurnNotActive);
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn queries_stay_responsive_during_stream() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
            50,
        ));
        app.create_session(sid("s-q")).await.expect("create");
        app.submit(sid("s-q"), "prompt".to_string())
            .await
            .expect("submit");
        // List must answer while streaming (worker select, not blocked).
        let sessions = tokio::time::timeout(Duration::from_secs(5), app.list_sessions())
            .await
            .expect("timeout")
            .expect("list");
        assert_eq!(sessions, vec![sid("s-q")]);
        app.cancel(sid("s-q")).await.expect("cancel");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn scripted_conversation_is_explicitly_unsupported() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["answer".into()], 1));
        app.create_session(sid("conversation")).await.unwrap();
        for action in [
            crate::queries::ConversationAction::Undo,
            crate::queries::ConversationAction::Redo,
        ] {
            assert!(
                matches!(app.change_conversation(sid("conversation"), action).await,
                Err(CoreError::Application(reason)) if reason.contains("unsupported"))
            );
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}
