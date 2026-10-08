//! Bounded chat state over the shared `CoreApp` handle.
//!
//! The view never touches storage: history pages, catalogs, skills and DCP
//! snapshots arrive as bounded application DTOs, and user choices leave as
//! [`PanelIntent`] values that the binary applies through the application
//! API (then reports acceptance or failure). Worker event draining stays in
//! the binary; the state only applies turn-scoped events, so a late event
//! for a stale turn can never corrupt the view.
//! One state owner; implementations are in input/transcript/tabs/live.
//! Shared fixtures live in `app/tests.rs`; scenarios are grouped by input,
//! transcript and turn lifecycle there. Private tab tests live under tabs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use oc_adapters::models::ModelCatalog;
use oc_core::core_app::{
    CoreApp, CoreEvent, MAX_SESSION_TITLE_BYTES, SubmissionReceipt, WorkerTurnId,
};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, DcpSnapshot, FileSuggestionsSnapshot, HistoryPage, SkillCard,
    ToolOpView,
};
use oc_core::session::CoreError;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::commands::{CommandAction, dispatch};
use crate::dcp_panel::{DcpOutcome, DcpPanelState};
use crate::events::KeyAction;
use crate::history::{HistoryRow, HistoryWindow, ToolCard, WINDOW_BYTES, card_from_row};
use crate::messages::{AssistantMeta, ReasoningBlock};
use crate::picker::ModelPicker;
use crate::styled::Line;
use crate::theme::Theme;

/// Visible lines kept in the viewport (scroll window).
pub const VIEWPORT_LINES: usize = 20;
/// Bounded input buffer (bytes): the core input budget, so the view never
/// drops bytes the runtime would have accepted.
pub const MAX_INPUT_BYTES: usize = oc_core::session::MAX_INPUT_BYTES;
/// Max card rows retained by the Cards panel.
pub const CARDS_MAX: usize = 160;
/// Max live turn parts kept before the oldest is evicted (defensive: the
/// runtime caps rounds, so a real turn stays far below this).
pub const LIVE_PARTS_MAX: usize = 64;
/// Maximum rows fetched for one inline mention (owner traversal is separately bounded).
pub const MENTION_LIMIT: usize = 10;
/// Never send a clipboard payload larger than a bounded visible transcript.
pub const MAX_SELECTION_BYTES: usize = 64 * 1024;
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

/// Exact identity of one focused file query. A tab can park and return with
/// the same draft, so the key includes the view instance as well as its edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionRequest {
    pub query: String,
    pub location: String,
    pub view_id: u64,
    pub generation: u64,
    pub revision: u64,
    pub caret: usize,
    pub start: usize,
}

/// Provider-reported usage for the active turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TurnUsage {
    input_tokens: u64,
    output_tokens: u64,
    streamed_ms: u64,
}

/// TUI status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiStatus {
    /// Ready for input.
    Idle,
    /// Input queued; the application has not yet accepted a turn.
    PendingSubmission,
    /// Streaming a turn.
    Streaming,
    /// Last turn was cancelled.
    Cancelled,
    /// Should exit the event loop.
    Quit,
}

/// Upstream toast feedback roles (`ui/toast.tsx:7-16`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteVariant {
    Info,
    Success,
    Warning,
    Error,
}

struct ToastExpiry {
    remaining: Duration,
    started: Option<Instant>,
}

/// Open TUI panel (bounded view state; one at a time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiPanel {
    Settings,
    MessageActions {
        message: oc_core::session::MessageId,
        seq: i64,
    },
    /// Genuine native command registry.
    Commands,
    /// No panel (chat view).
    None,
    /// Model picker (UI02).
    Model,
    /// Declared variants of the effective application model, including Default.
    Variant,
    /// Primary agent selector.
    Agents,
    /// Session list with resume (UI03).
    Sessions,
    /// Focused single-line session title editor.
    Rename,
    /// Masked connection and safe account metadata, outside composer state.
    Accounts,
    /// Skill catalog (UI06).
    Skills,
    /// Current Location's actual MCP resource inventory and controls.
    Mcps,
    /// Help, optionally for one topic.
    Help(Option<String>),
    /// DCP context panel (UI04).
    Dcp,
    /// Tool cards from the runtime (newest first, paged).
    Cards,
}

/// Work the panel asked the binary to apply through the application API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelIntent {
    LoadAuthMethods {
        provider: String,
        revision: u64,
    },
    BeginAuthentication(AuthRequest),
    OpenAuthorization,
    CopyAuthorization,
    Terminal {
        session: SessionId,
        action: crate::terminal_view::TerminalIntent,
        /// Lower composer activation closes/navigates before the captured action.
        close_composer: bool,
    },
    LoadProviderConnections,
    SetPermissionMode {
        auto_once: bool,
    },
    ReplyApproval(oc_core::approval::ApprovalReply),
    ReplyQuestion(oc_core::question::QuestionReply),
    LoadShells,
    LoadChildren,
    OpenChild {
        selected: oc_core::queries::ChildJob,
    },
    ReturnParent,
    BackgroundChild {
        selected: oc_core::queries::ChildJob,
    },
    InterruptChild {
        selected: oc_core::queries::ChildJob,
    },
    CancelShell {
        session: SessionId,
        shell_id: String,
    },
    BackgroundShell {
        session: SessionId,
        shell_id: String,
    },
    CompactSession,
    ChangeConversation {
        action: oc_core::queries::ConversationAction,
    },
    ForkMessage {
        message: oc_core::session::MessageId,
    },
    CopyMessage {
        message: oc_core::session::MessageId,
        seq: i64,
    },
    /// Load the model/agent catalog snapshot.
    LoadCatalog,
    ProviderAccounts {
        provider: String,
        action: Option<oc_core::queries::AccountAction>,
    },
    /// Rebuild the current Location without replacing the session or draft.
    ReloadConfiguration,
    /// Load the session list snapshot.
    LoadSessions,
    /// Load the skill card snapshot.
    LoadSkills,
    LoadMcps,
    McpControl(oc_core::queries::McpControl),
    /// Load the newest tool-card page.
    LoadCards,
    /// Continue reading one card's durable result through the owning application.
    LoadCardOutput {
        op: String,
        offset: usize,
    },
    /// Create an empty application session and attach its Home route.
    NewSession,
    /// Activate a retained real tab by its zero-based deck index.
    ActivateTab {
        index: usize,
    },
    /// Close a retained tab; `tabs.len()` denotes the synthetic Home slot.
    CloseTab {
        index: usize,
    },
    /// Apply the trimmed title to the attached session through the owner.
    RenameSession {
        title: String,
    },
    /// Apply a slash-supplied title without opening the editor; ACK clears the slash draft.
    RenameSessionDirect {
        title: String,
    },
    RenameSelectedSession {
        id: String,
        title: String,
    },
    DeleteSelectedSession {
        id: String,
    },
    /// Bare slash command: ask the owner to generate a fresh title.
    RegenerateTitle,
    /// Select a model, restoring the owner's remembered variant preference.
    SelectModel {
        /// Exact model id.
        id: String,
    },
    /// Apply an exact model + variant choice.
    ChooseModel {
        /// Exact model id.
        id: String,
        /// Optional variant name.
        variant: Option<String>,
    },
    /// Cycle from the owner's current model/variant, never a stale UI snapshot.
    CycleVariant,
    /// Apply a primary agent choice.
    SelectAgent {
        /// Agent profile id.
        id: String,
    },
    /// Resume another session.
    SwitchSession {
        /// Target session id.
        id: String,
    },
    /// Switch the whole application to another Location (project path).
    SwitchLocation {
        /// Target project path.
        path: String,
    },
    /// Load one older history page.
    LoadOlder,
    /// Load one newer history page.
    LoadNewer,
    /// Request a manual DCP compression.
    Compress {
        /// Bounded focus instruction (possibly empty).
        focus: String,
    },
}

/// One key handling result: optional status note, optional intent for the
/// binary to apply, and whether the input buffer was consumed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyOutcome {
    /// Status note for the user (error or hint).
    pub note: Option<String>,
    /// Intent the binary must apply through the application API.
    pub intent: Option<PanelIntent>,
    /// True when the key was fully handled and the input was cleared.
    pub consumed_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardMode {
    Select,
    Manual,
    /// A published Location no longer matches this view; wait for an owner
    /// catalog before allowing either automatic or explicit clipboard writes.
    Disabled,
}

impl Default for ClipboardMode {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Manual
        } else {
            Self::Select
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TextPoint {
    row: usize,
    byte: usize,
}

#[derive(Clone)]
struct PaintedTranscript {
    area: Rect,
    rows: Vec<Line>,
    user_targets: Vec<Option<crate::messages::UserMessageTarget>>,
    total: usize,
    scroll: usize,
}

struct TranscriptSelection {
    anchor: TextPoint,
    focus: TextPoint,
    painted: PaintedTranscript,
    dragging: bool,
}

impl TranscriptSelection {
    fn bounds(&self) -> (TextPoint, TextPoint) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

#[derive(Clone, Copy)]
struct TranscriptClick {
    x: u16,
    y: u16,
    at: Instant,
    count: u8,
}

/// One live part of the streaming turn, in arrival order (upstream message
/// parts: reasoning, text, tool — `routes/session/index.tsx:1433-1481`).
///
/// Text/reasoning segments freeze when a tool call starts so tool cards keep
/// their upstream position between the text parts; the transient recorded
/// input stays with an in-flight card until its outcome arrives, then the
/// card is rebuilt and the input dropped (bounded live state).
#[derive(Debug, Clone, PartialEq, Eq)]
enum LivePart {
    /// Removed transient slot; keeps subsequent live reasoning identities stable.
    Vacant,
    /// Frozen assistant text segment.
    Text(String),
    /// Frozen reasoning segment with its measured window.
    Reasoning {
        text: String,
        duration_ms: Option<u64>,
    },
    /// One tool call card plus the recorded input of an in-flight call.
    Tool { card: Box<ToolCard>, input: String },
}

/// One disposable, byte-bounded tool-output page in the Cards dialog.
pub(crate) struct CardOutput {
    pub op: String,
    pub offset: usize,
    pub page: oc_core::queries::ToolOutputPage,
}

impl LivePart {
    /// Bounded bytes retained by this part (the transient in-flight input is
    /// excluded: it is dropped as soon as the outcome arrives).
    fn retained_bytes(&self) -> usize {
        match self {
            LivePart::Vacant => 0,
            LivePart::Text(text) | LivePart::Reasoning { text, .. } => text.len(),
            LivePart::Tool { card, input } => card.retained_bytes() + input.len(),
        }
    }

    /// Render this part as a transcript row.
    fn to_row(
        &self,
        agent: Option<String>,
        identity: Option<crate::messages::ReasoningIdentity>,
    ) -> HistoryRow {
        match self {
            LivePart::Vacant => unreachable!("vacant parts are never projected"),
            LivePart::Text(text) => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: text.clone(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: None,
            },
            LivePart::Reasoning { text, duration_ms } => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: String::new(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: Some(ReasoningBlock {
                    text: text.clone(),
                    duration_ms: *duration_ms,
                    running: false,
                    expanded: false,
                    toggleable: true,
                    identity,
                }),
                meta: None,
                tool: None,
            },
            LivePart::Tool { card, .. } => HistoryRow {
                message_id: None,
                seq: i64::MAX,
                role: "tool".to_string(),
                text: String::new(),
                agent,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: Some((**card).clone()),
            },
        }
    }
}

/// Numeric view-owned state sampled only by the opt-in terminal metrics loop.
/// Text/reasoning include open buffers and frozen segments; part count is the
/// frozen `live_parts` list (tool cards included), not the render projection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LiveViewMetrics {
    pub text_bytes: usize,
    pub reasoning_bytes: usize,
    pub part_count: usize,
    pub markdown_cache_retained_bytes: usize,
}

impl std::ops::AddAssign for LiveViewMetrics {
    fn add_assign(&mut self, other: Self) {
        self.text_bytes += other.text_bytes;
        self.reasoning_bytes += other.reasoning_bytes;
        self.part_count += other.part_count;
        self.markdown_cache_retained_bytes += other.markdown_cache_retained_bytes;
    }
}

/// Immutable submission identity plus the editable draft's revision at enqueue.
struct PendingSubmission {
    selection: Option<oc_core::queries::ModelCommit>,
    request_id: u64,
    generation: u64,
    session: SessionId,
    fresh: bool,
    draft: String,
    mentions: Vec<(usize, usize)>,
    revision: u64,
    receipt: SubmissionReceipt,
    agent: Option<String>,
    agent_color_index: Option<usize>,
    cancelling: bool,
    compress: bool,
}

/// Upstream Home normal-mode examples (`routes/home.tsx:19-23`).
pub(crate) const HOME_EXAMPLES: [&str; 3] = [
    "Fix a TODO in the codebase",
    "What is the tech stack of this project?",
    "Fix broken tests",
];

#[derive(Clone, Copy, PartialEq, Eq)]
struct TranscriptViewport {
    width: u16,
    terminal_width: u16,
    height: u16,
    total: usize,
    requested_scroll: usize,
    displayed_scroll: usize,
}

/// One warm screen, never another retained-history/archive cache. Selection
/// highlighting is applied after this projection on every paint.
struct VisibleTranscriptProjection {
    revision: u64,
    window_revision: u64,
    viewport: Option<TranscriptViewport>,
    live_lengths: (usize, usize),
    dimensions: (u16, u16, u16),
    requested_scroll: usize,
    chrome: oc_core::queries::TuiChrome,
    theme: crate::theme::ThemeMode,
    thinking: bool,
    reasoning: BTreeSet<crate::messages::ReasoningIdentity>,
    exploration: BTreeSet<String>,
    compaction_frame: usize,
    lines: Vec<Line>,
    total: usize,
    scroll: usize,
    targets: Vec<Option<crate::messages::UserMessageTarget>>,
}

/// Only actual visible chip cells from the latest prompt paint are actionable.
struct PaintedPrompt {
    frame: Rect,
    main: Rect,
    home: bool,
    session: Option<SessionId>,
    revision: u64,
    generation: u64,
    cursor: usize,
    anchor: Option<usize>,
    chips: Vec<(Rect, usize)>,
}

struct CompletionAnchor {
    message: std::sync::Arc<oc_core::session::MessageId>,
    part: usize,
    row: usize,
    requested_scroll: usize,
    pending: bool,
}

/// Native row-quantized presentation of a wheel target. OC2/OpenTUI 0.5.10
/// applies wheel displacement immediately (ScrollBox.onMouseEvent); it has no
/// wheel inertia. We distribute that displacement over one OC2 paint budget
/// (app.tsx targetFps: 60), never retain a velocity or replay an input queue.
#[derive(Clone, Copy)]
struct WheelMotion {
    started: Instant,
    distance: usize,
    applied: usize,
    up: bool,
}

const WHEEL_PRESENTATION: Duration = Duration::from_nanos(16_666_667);

/// Bounded chat state on the shared handle, optionally attached to a session.
pub struct TuiState {
    pub approval_roots: BTreeSet<String>,
    pub tab_attention: BTreeSet<usize>,
    pub approvals: crate::approval_view::ApprovalView,
    pub questions: crate::question_view::QuestionView,
    pub(crate) shells: crate::shell_jobs_view::ShellView,
    pub(crate) children: crate::child_view::ChildView,
    pub(crate) terminals: crate::terminal_view::TerminalView,
    pub chrome: oc_core::queries::TuiChrome,
    pub parent_id: Option<String>,
    /// New interactive launch, distinct from an explicitly attached session.
    pub home: bool,
    /// Sampled once per UI instance; never changes during a redraw.
    pub(crate) home_example: &'static str,
    viewport: std::cell::Cell<Option<TranscriptViewport>>,
    completion_anchor: std::cell::RefCell<Option<CompletionAnchor>>,
    painted_transcript: std::cell::RefCell<Option<PaintedTranscript>>,
    painted_prompt: std::cell::RefCell<Option<PaintedPrompt>>,
    prompt_width: std::cell::Cell<Option<usize>>,
    paint_generation: std::cell::Cell<u64>,
    message_down: Option<(oc_core::session::MessageId, u64, Rect)>,
    clipboard_mode: ClipboardMode,
    /// Owner availability, when known. Fresh/reopened views let the owner decide.
    conversation_available: Option<(bool, bool)>,
    reverted: Option<oc_core::queries::RevertedConversation>,
    reverted_down: Option<(oc_core::queries::RevertedConversation, u64, Rect)>,
    conversation_bindings: [Option<String>; 2],
    leader_key: String,
    /// Last successful owner projection, independent of manual/test overrides.
    owner_clipboard_mode: Option<oc_core::queries::TerminalCopyMode>,
    pending_copy: Option<String>,
    selection: Option<TranscriptSelection>,
    selection_gesture: bool,
    click: Option<TranscriptClick>,
    /// Current session's durable human title, refreshed with history.
    pub session_title: Option<String>,
    /// Session autoaccept capability supplied by the application.
    pub auto_accept: oc_core::queries::AutoAcceptState,
    app: CoreApp,
    session: Option<SessionId>,
    status: TuiStatus,
    scanner_frame: usize,
    compactions: Vec<oc_core::compaction::CompactionSnapshot>,
    compaction_at: Option<Instant>,
    compaction_frame: usize,
    compaction_turn_messages: std::collections::BTreeMap<String, String>,
    scanner_at: Option<Instant>,
    panel: TuiPanel,
    pub(crate) select: crate::dialog::SelectList,
    /// Press origin prevents drag-release across the backdrop from dismissing a dialog.
    mouse_down: Option<crate::dialog::DialogHit>,
    tab_down: Option<TabPress>,
    home_mcp_down: Option<(Rect, Rect)>,
    tab_view: std::cell::RefCell<TabView>,
    pub(crate) tab_scroll: std::cell::Cell<usize>,
    last_mouse: Option<(u16, u16, Rect)>,
    pub(crate) close_hold: Option<TabCloseHold>,
    tabs: Vec<TabPresentation>,
    active_tab: usize,
    can_add_tab: bool,
    /// Only operation IDs whose exploration headers were explicitly opened.
    exploration_expanded: BTreeSet<String>,
    exploration_down: Option<(String, u16, u16)>,
    reasoning_expanded: BTreeSet<crate::messages::ReasoningIdentity>,
    reasoning_down: Option<(
        crate::messages::ReasoningIdentity,
        u16,
        u16,
        Rect,
        usize,
        usize,
        usize,
    )>,
    /// An unfinished press/drag cannot masquerade as a standalone header UP,
    /// even when scrolling, resizing or an overlay invalidates its hit target.
    reasoning_pointer_down: bool,
    reasoning_epoch: u64,
    /// Number of evicted frozen parts in this turn; ordinals never shift.
    live_part_offset: usize,
    leader: Option<Instant>,
    input: String,
    editor: crate::editor::Editor,
    rename_input: String,
    accounts: Box<accounts::AccountsView>,
    rename_editor: crate::editor::Editor,
    rename_pending: Option<String>,
    rename_selected: Option<String>,
    session_delete_confirm: Option<String>,
    session_project_name: Option<String>,
    session_scope_pending: Option<bool>,
    rename_direct_pending: Option<(String, u64)>,
    regenerate_pending: Option<u64>,
    window: HistoryWindow,
    markdown_cache: std::cell::RefCell<crate::messages::MarkdownCache>,
    transcript_revision: u64,
    visible_projection: std::cell::RefCell<Option<VisibleTranscriptProjection>>,
    #[cfg(test)]
    transcript_row_copies: std::cell::Cell<usize>,
    #[cfg(test)]
    visible_projection_builds: std::cell::Cell<usize>,
    live_text: String,
    /// Reasoning text streamed for the active turn (never persisted).
    live_reasoning: String,
    thinking_expanded: bool,
    /// Frozen live parts (text/reasoning segments and tool cards) of the
    /// active turn, in arrival order.
    live_parts: Vec<LivePart>,
    pending_tool_seen: Vec<String>,
    pending_tool_round: u32,
    /// Explicit durable live identities; replaced at each application checkpoint.
    pub live_part_states: Vec<oc_core::queries::PartState>,
    live_agent_color_index: Option<usize>,
    live_terminal_status: Option<String>,
    live_model_label: Option<String>,
    live_mixed_models: bool,
    live_preview_truncated: bool,
    /// First reasoning delta of the active turn, for the collapsed header's
    /// duration (`part.time.created` upstream).
    reasoning_started: Option<Instant>,
    /// When text streaming followed reasoning (`part.time.completed`).
    reasoning_finished: Option<Instant>,
    /// Provider usage reported for the active turn (never synthesized).
    turn_usage: Option<TurnUsage>,
    scroll: usize,
    wheel_motion: Option<WheelMotion>,
    note: Option<(String, NoteVariant)>,
    /// Provenance of the existing toast, not a history of service failures.
    service_note: bool,
    /// A failed cause retained only while that same current source is pending.
    /// Pending is not recovery; stable status/removal/Location drops the baseline.
    service_pending_issues: Vec<services::ServiceIssue>,
    service_feedback_visible: bool,
    /// One captured pre-acceptance refusal, not the new binding's readiness or
    /// a diagnostic history. Cleared by acceptance, another refusal or Location.
    refused_submission: Option<oc_core::queries::ServiceDiagnostic>,
    toast_expiry: Option<ToastExpiry>,
    toast_down: bool,
    active_turn: Option<WorkerTurnId>,
    live_retry: Option<(String, oc_core::queries::RetryFact)>,
    live_retry_seen: Option<(WorkerTurnId, String, u32)>,
    live_span: Option<(WorkerTurnId, String)>,
    live_projection_revision: Option<(WorkerTurnId, u64)>,
    retry_due: Option<Instant>,
    /// First Esc arms the running turn for five seconds; only a second press
    /// interrupts it (pinned prompt/index.tsx:499–528).
    interrupt_armed_until: Option<Instant>,
    pending: Option<PendingSubmission>,
    compress_turn: Option<WorkerTurnId>,
    request_id: u64,
    generation: u64,
    input_revision: u64,
    slash_selected: usize,
    slash_dismissed: Option<u64>,
    view_id: u64,
    mention_selected: usize,
    mention_dismissed: Option<MentionRequest>,
    mention_result: Option<(MentionRequest, FileSuggestionsSnapshot)>,
    mention_owner_epoch: Option<u64>,
    /// Model picker (present while the Model panel lives).
    pub(crate) picker: Option<ModelPicker>,
    model_selection: model_selection::ComposerSelection,
    catalog_loaded: bool,
    /// Agent profiles from the catalog snapshot.
    pub(crate) agents: Vec<AgentEntry>,
    /// Effective agent id from the catalog snapshot (the metadata row shows
    /// the active agent, never the picker cursor).
    active_agent: Option<String>,
    /// Agents cursor.
    pub(crate) agents_cursor: usize,
    /// Session ids for the Sessions panel.
    pub(crate) sessions: Vec<String>,
    session_entries: Vec<oc_core::queries::SessionListEntry>,
    sessions_all_projects: bool,
    /// Sessions cursor.
    pub(crate) sessions_cursor: usize,
    sessions_loaded: bool,
    /// Skill catalog cards (metadata only, no bodies).
    pub(crate) skills: Vec<SkillCard>,
    /// Skills cursor.
    pub(crate) skills_cursor: usize,
    skills_loaded: bool,
    mcp_snapshot: Option<oc_core::queries::McpSnapshot>,
    mcp_detail: Option<mcp::McpDetail>,
    /// DCP panel state: snapshot in, request out, transient outcome (UI04).
    pub(crate) dcp: DcpPanelState,
    /// Command ids known to the application (templates stay there).
    pub(crate) commands: Vec<String>,
    /// Metadata for the current catalog generation only.
    command_descriptions: BTreeMap<String, String>,
    /// Newest tool cards from the runtime (bounded page).
    pub(crate) cards: Vec<HistoryRow>,
    card_ops: Vec<String>,
    /// Cards cursor.
    pub(crate) cards_cursor: usize,
    pub(crate) card_output: Option<CardOutput>,
    card_scroll: usize,
    card_seen: std::cell::Cell<usize>,
    detail_area: std::cell::Cell<ratatui::layout::Rect>,
    cards_loaded: bool,
    cards_has_older: bool,
}

/// A candidate identity exists only for an actual fresh submission, never for
/// an idle Home. The counter disambiguates submissions within a process even
/// when the clock resolution is coarse (including immediate rejected retries).
fn fresh_session_id() -> SessionId {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |now| now.as_nanos());
    SessionId(format!(
        "s-tui-{nanos:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(test)]
mod tests;

impl TuiState {
    /// Bind to a session; the session must already exist on the handle.
    pub fn new(app: CoreApp, session: SessionId) -> Self {
        Self::with_session(app, Some(session))
    }

    /// Start a Home composer without creating or retaining a session ID.
    pub fn new_home(app: CoreApp) -> Self {
        Self::with_session(app, None)
    }

    fn with_session(app: CoreApp, session: Option<SessionId>) -> Self {
        let index = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |now| now.subsec_nanos() as usize % HOME_EXAMPLES.len());
        let home = session.is_none();
        Self {
            chrome: Default::default(),
            parent_id: None,
            home,
            home_example: HOME_EXAMPLES[index],
            viewport: std::cell::Cell::new(None),
            completion_anchor: std::cell::RefCell::new(None),
            painted_transcript: std::cell::RefCell::new(None),
            painted_prompt: std::cell::RefCell::new(None),
            prompt_width: std::cell::Cell::new(None),
            paint_generation: std::cell::Cell::new(0),
            message_down: None,
            clipboard_mode: ClipboardMode::default(),
            conversation_available: None,
            reverted: None,
            reverted_down: None,
            conversation_bindings: [None, None],
            leader_key: "ctrl+x".into(),
            owner_clipboard_mode: None,
            pending_copy: None,
            selection: None,
            selection_gesture: false,
            click: None,
            session_title: None,
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            approvals: Default::default(),
            questions: Default::default(),
            shells: Default::default(),
            children: Default::default(),
            terminals: Default::default(),
            approval_roots: Default::default(),
            tab_attention: Default::default(),
            app,
            session,
            status: TuiStatus::Idle,
            scanner_frame: 0,
            compactions: Vec::new(),
            compaction_at: None,
            compaction_frame: 0,
            compaction_turn_messages: Default::default(),
            scanner_at: None,
            panel: TuiPanel::None,
            select: Default::default(),
            mouse_down: None,
            tab_down: None,
            home_mcp_down: None,
            tab_view: Default::default(),
            tab_scroll: std::cell::Cell::new(0),
            last_mouse: None,
            close_hold: None,
            tabs: Vec::new(),
            active_tab: 0,
            can_add_tab: false,
            exploration_expanded: BTreeSet::new(),
            exploration_down: None,
            reasoning_expanded: BTreeSet::new(),
            reasoning_down: None,
            reasoning_pointer_down: false,
            reasoning_epoch: 0,
            live_part_offset: 0,
            leader: None,
            input: String::new(),
            editor: Default::default(),
            rename_input: String::new(),
            accounts: Box::default(),
            rename_editor: Default::default(),
            rename_pending: None,
            rename_selected: None,
            session_delete_confirm: None,
            session_project_name: None,
            session_scope_pending: None,
            rename_direct_pending: None,
            regenerate_pending: None,
            window: HistoryWindow::new(),
            markdown_cache: std::cell::RefCell::new(Default::default()),
            transcript_revision: 0,
            visible_projection: Default::default(),
            #[cfg(test)]
            transcript_row_copies: Default::default(),
            #[cfg(test)]
            visible_projection_builds: Default::default(),
            live_text: String::new(),
            live_reasoning: String::new(),
            thinking_expanded: false,
            live_parts: Vec::new(),
            pending_tool_seen: Vec::new(),
            pending_tool_round: 0,
            live_part_states: Vec::new(),
            live_agent_color_index: None,
            live_terminal_status: None,
            live_model_label: None,
            live_mixed_models: false,
            live_preview_truncated: false,
            reasoning_started: None,
            reasoning_finished: None,
            turn_usage: None,
            scroll: 0,
            wheel_motion: None,
            note: None,
            service_note: false,
            service_pending_issues: Vec::new(),
            service_feedback_visible: true,
            refused_submission: None,
            toast_expiry: None,
            toast_down: false,
            active_turn: None,
            live_retry: None,
            live_retry_seen: None,
            live_span: None,
            live_projection_revision: None,
            retry_due: None,
            interrupt_armed_until: None,
            pending: None,
            compress_turn: None,
            request_id: 0,
            generation: 0,
            input_revision: 0,
            slash_selected: 0,
            slash_dismissed: None,
            view_id: NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed),
            mention_selected: 0,
            mention_dismissed: None,
            mention_result: None,
            mention_owner_epoch: None,
            picker: None,
            model_selection: Default::default(),
            catalog_loaded: false,
            agents: Vec::new(),
            active_agent: None,
            agents_cursor: 0,
            sessions: Vec::new(),
            session_entries: Vec::new(),
            sessions_all_projects: true,
            sessions_cursor: 0,
            sessions_loaded: false,
            skills: Vec::new(),
            skills_cursor: 0,
            skills_loaded: false,
            mcp_snapshot: None,
            mcp_detail: None,
            dcp: DcpPanelState::default(),
            commands: Vec::new(),
            command_descriptions: BTreeMap::new(),
            cards: Vec::new(),
            card_ops: Vec::new(),
            cards_cursor: 0,
            card_output: None,
            card_scroll: 0,
            card_seen: std::cell::Cell::new(0),
            detail_area: std::cell::Cell::new(ratatui::layout::Rect::default()),
            cards_loaded: false,
            cards_has_older: false,
        }
    }

    /// Attached session ID, if this view has an accepted or resumed session.
    pub fn attached_session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }

    /// Attached session ID. Only call after checking `attached_session()`.
    pub fn session(&self) -> &SessionId {
        self.attached_session()
            .expect("Home has no attached session")
    }

    /// Switch to another session after an accepted switch: clears view state
    /// and the history window; status returns to `Idle` unless quitting.
    /// Drop every generation-bound cache after a Location switch.
    ///
    /// Panel data (catalog, agents, sessions, skills, cards, DCP snapshot and
    /// workspace commands) belongs to the previous Location: the next panel
    /// open must reload from the new generation instead of showing it.
    pub fn reset_workspace(&mut self) {
        self.invalidate_transcript();
        self.close_panel();
        self.editor.forget_accepted_mentions();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.clear_mentions();
        self.tabs.clear();
        *self.tab_view.get_mut() = TabView::default();
        self.tab_scroll.set(0);
        self.tab_attention.clear();
        self.clear_mouse_position();
        self.active_tab = 0;
        self.can_add_tab = false;
        self.exploration_expanded.clear();
        self.reasoning_expanded.clear();
        self.reasoning_down = None;
        self.chrome = Default::default();
        self.parent_id = None;
        self.auto_accept = oc_core::queries::AutoAcceptState::Unsupported;
        self.session_title = None;
        self.generation += 1;
        self.invalidate_submission();
        self.picker = None;
        self.catalog_loaded = false;
        self.agents.clear();
        self.active_agent = None;
        self.agents_cursor = 0;
        self.sessions.clear();
        self.sessions_cursor = 0;
        self.sessions_loaded = false;
        self.skills.clear();
        self.skills_cursor = 0;
        self.skills_loaded = false;
        self.commands.clear();
        self.command_descriptions.clear();
        self.cards.clear();
        self.card_ops.clear();
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.detail_area.set(ratatui::layout::Rect::default());
        self.cards_cursor = 0;
        self.cards_loaded = false;
        self.cards_has_older = false;
        self.dcp = DcpPanelState::default();
    }

    pub fn set_session(&mut self, session: SessionId) {
        self.terminals = Default::default();
        self.invalidate_transcript();
        *self.tab_view.get_mut() = TabView::default();
        self.clear_mouse_position();
        self.compactions.clear();
        self.compaction_turn_messages.clear();
        self.compaction_at = None;
        self.compaction_frame = 0;
        self.completion_anchor.get_mut().take();
        self.conversation_available = None;
        self.reverted = None;
        self.close_panel();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.clear_mentions();
        self.exploration_expanded.clear();
        self.reasoning_expanded.clear();
        self.reasoning_down = None;
        self.cards.clear();
        self.card_ops.clear();
        self.cards_cursor = 0;
        self.cards_loaded = false;
        self.cards_has_older = false;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.detail_area.set(ratatui::layout::Rect::default());
        self.viewport.set(None);
        self.parent_id = None;
        self.home = false;
        self.session_title = None;
        self.generation += 1;
        self.invalidate_submission();
        self.session = Some(session);
        self.input.clear();
        self.editor.clear();
        self.window = HistoryWindow::new();
        *self.markdown_cache.get_mut() = Default::default();
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.pending_tool_seen.clear();
        self.pending_tool_round = 0;
        self.live_part_offset = 0;
        self.reasoning_down = None;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.wheel_motion = None;
        self.active_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
    }

    fn invalidate_submission(&mut self) {
        self.reset_scanner();
        self.live_part_states.clear();
        self.live_agent_color_index = None;
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_preview_truncated = false;
        // Called only after an accepted switch (binary refuses busy switches).
        // Invalidate local receipts even if a caller has an old completion queued.
        self.pending = None;
        self.active_turn = None;
        self.compress_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
    }

    /// Current status.
    pub fn status(&self) -> &TuiStatus {
        &self.status
    }

    /// Actual current-view projection loss, not producer capture completeness
    /// or a promise that bytes discarded by a tool can be recovered.
    pub(crate) fn preview_limited(&self) -> bool {
        (self.active_turn.is_some() && self.live_preview_truncated)
            || self.window.rows().iter().any(|row| {
                row.meta.as_ref().is_some_and(|meta| meta.preview_limited)
                    || row.tool.as_ref().is_some_and(ToolCard::preview_limited)
            })
            || self
                .live_parts
                .iter()
                .any(|part| matches!(part, LivePart::Tool { card, .. } if card.preview_limited()))
    }

    /// Owner receipts need reconciliation until accepted, independently from
    /// streaming and animation. A settled view has no receipt polling work.
    pub fn has_pending_submission(&self) -> bool {
        self.pending.is_some()
    }

    /// Active view deadlines only; input/provider paints never reset these
    /// clocks. Hover-paused toasts and settled wheel motion have no deadline.
    pub fn next_ui_deadline(&self) -> Option<Instant> {
        let scanner = (self.status == TuiStatus::Streaming
            && self.chrome.animations != Some(false))
        .then(|| {
            self.scanner_at
                .map(|at| at + Duration::from_millis(crate::scanner::FRAME_MS))
        })
        .flatten();
        let toast = self
            .toast_expiry
            .as_ref()
            .and_then(|expiry| expiry.started.map(|at| at + expiry.remaining));
        [
            scanner,
            self.compaction_at
                .filter(|_| self.chrome.animations != Some(false))
                .map(|at| at + Duration::from_millis(80)),
            toast,
            self.interrupt_armed_until,
            self.next_scroll_animation_deadline(),
            self.leader_deadline(),
            self.next_tab_deadline(),
            self.retry_due,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// True only when a deadline actually changes visible state.
    pub fn tick_ui(&mut self, now: Instant) -> bool {
        let retry_due = self.retry_due.is_some_and(|at| now >= at);
        if retry_due {
            self.retry_due = None;
        }
        let expired = self.interrupt_armed_until.is_some_and(|until| now >= until)
            || self.toast_expiry.as_ref().is_some_and(|expiry| {
                expiry
                    .started
                    .is_some_and(|at| now.saturating_duration_since(at) >= expiry.remaining)
            });
        self.tick_toast(now);
        let scanner = self.tick_scanner(now);
        let wheel = self.tick_scroll_animation(now);
        let compaction = self.tick_compaction(now);
        let tabs = self.tick_tabs(now);
        let leader = self.leader_deadline().is_some_and(|until| now >= until);
        if leader {
            self.leader = None;
        }
        expired || scanner || wheel || compaction || leader || tabs || retry_due
    }

    fn leader_deadline(&self) -> Option<Instant> {
        self.leader
            .and_then(|at| at.checked_add(Duration::from_millis(self.chrome.leader_timeout_ms())))
    }

    /// The composer and key resolver observe the same pending sequence.
    pub fn leader_pending(&self) -> bool {
        self.leader.is_some()
    }

    fn reset_scanner(&mut self) {
        self.scanner_frame = 0;
        self.scanner_at = None;
        self.interrupt_armed_until = None;
    }

    pub(crate) fn interrupt_armed(&self) -> bool {
        self.interrupt_armed_until
            .is_some_and(|until| Instant::now() < until)
    }

    /// Advance the running prompt scanner from a caller-supplied monotonic clock.
    /// The event loop calls this alongside its other view ticks, per view. Returns
    /// whether a frame changed and a redraw is useful; the first tick starts at 0.
    pub fn tick_scanner(&mut self, now: Instant) -> bool {
        if self.status != TuiStatus::Streaming || self.chrome.animations == Some(false) {
            return false;
        }
        let Some(at) = self.scanner_at else {
            self.scanner_at = Some(now);
            return false;
        };
        let elapsed = now.saturating_duration_since(at).as_millis();
        let steps = elapsed / u128::from(crate::scanner::FRAME_MS);
        if steps == 0 {
            return false;
        }
        let previous = self.scanner_frame;
        self.scanner_frame = (self.scanner_frame
            + (steps % crate::scanner::FRAMES as u128) as usize)
            % crate::scanner::FRAMES;
        self.scanner_at = now.checked_sub(Duration::from_millis(
            (elapsed % u128::from(crate::scanner::FRAME_MS)) as u64,
        ));
        self.scanner_frame != previous
    }

    pub(crate) fn scanner_frame(&self) -> usize {
        self.scanner_frame
    }

    /// Active turn, if any.
    pub fn active_turn(&self) -> Option<&WorkerTurnId> {
        self.active_turn.as_ref()
    }

    /// True while a submission awaits acceptance or a turn streams.
    pub fn is_busy(&self) -> bool {
        self.active_turn.is_some()
            || self.pending.is_some()
            || self.compactions.iter().any(crate::compaction::active)
    }

    /// Status note, if any (intent errors and hints; never chat history).
    pub fn note(&self) -> Option<&str> {
        self.note.as_ref().map(|(message, _)| message.as_str())
    }

    /// Semantic color of the currently displayed note.
    pub fn note_variant(&self) -> Option<NoteVariant> {
        self.note.as_ref().map(|(_, variant)| *variant)
    }

    /// Window bytes plus live text, live parts and input; bounded by the
    /// window caps.
    pub fn retained_bytes(&self) -> usize {
        self.window.retained_bytes()
            + self.markdown_cache.borrow().retained_bytes()
            + self.visible_projection.borrow().as_ref().map_or(0, |view| {
                view.lines
                    .iter()
                    .flat_map(Line::spans)
                    .map(|span| std::mem::size_of::<crate::styled::Span>() + span.content().len())
                    .sum::<usize>()
                    + view.targets.len()
                        * std::mem::size_of::<Option<crate::messages::UserMessageTarget>>()
            })
            + self.live_text.len()
            + self.live_reasoning.len()
            + self
                .pending_tool_seen
                .iter()
                .map(String::len)
                .sum::<usize>()
            + self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
            + self.input.len()
            + self.mention_result.as_ref().map_or(0, |(key, result)| {
                key.query.len()
                    + key.location.len()
                    + result.location.len()
                    + result.paths.iter().map(String::len).sum::<usize>()
            })
            + self
                .mention_dismissed
                .as_ref()
                .map_or(0, |key| key.query.len() + key.location.len())
            + self.editor.retained_bytes()
            + self
                .pending
                .as_ref()
                .map_or(0, |pending| pending.draft.len())
    }

    /// No transcript projection, Markdown parse, or history walk. The cache is
    /// persistent on each view, including parked tabs; counts are payload
    /// bytes, not String/Vec capacities or temporary frame allocations.
    pub fn live_view_metrics(&self) -> LiveViewMetrics {
        let mut result = LiveViewMetrics {
            text_bytes: self.live_text.len(),
            reasoning_bytes: self.live_reasoning.len(),
            part_count: self.live_parts.len(),
            markdown_cache_retained_bytes: self.markdown_cache.borrow().retained_bytes(),
        };
        for part in &self.live_parts {
            match part {
                LivePart::Text(text) => result.text_bytes += text.len(),
                LivePart::Reasoning { text, .. } => result.reasoning_bytes += text.len(),
                LivePart::Tool { .. } | LivePart::Vacant => {}
            }
        }
        result
    }

    /// Categorical agent color (`context/local.tsx:75-133`): the agent's index
    /// in the generation's full admitted agent list (the catalog pins the slot),
    /// and the first categorical color for an
    /// unknown/missing agent.
    pub fn agent_color(&self, agent: Option<&str>) -> ratatui::style::Color {
        if let Some(color) = agent
            .and_then(|id| self.chrome.agent_colors.get(id))
            .and_then(|hex| crate::theme::Rgba::from_hex(hex))
        {
            return ratatui::style::Color::Rgb(color.r, color.g, color.b);
        }
        let colors = Theme::dark().categorical_agents();
        let index = agent.and_then(|id| {
            self.agents
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.color_index)
        });
        match index {
            Some(index) => colors[index % colors.len()],
            None => colors[0],
        }
    }

    /// Set a persistent legacy status note until replaced or cleared.
    pub fn push_note(&mut self, note: &str) {
        self.push_note_variant(note, NoteVariant::Warning);
    }

    /// Set a typed feedback note without interpreting its free-form text.
    pub fn push_note_variant(&mut self, note: &str, variant: NoteVariant) {
        self.service_note = false;
        self.note = Some((note.to_string(), variant));
        self.toast_expiry = None;
        self.toast_down = false;
    }

    /// Timed feedback uses the upstream default five-second toast lifetime;
    /// legacy warnings continue to persist until replaced or explicitly cleared.
    pub fn push_transient_note(&mut self, note: &str, variant: NoteVariant) {
        self.push_transient_note_for(note, variant, Duration::from_secs(5));
    }

    /// Explicit duration for long-running owner operations such as reload.
    pub fn push_transient_note_for(
        &mut self,
        note: &str,
        variant: NoteVariant,
        duration: Duration,
    ) {
        self.push_transient_note_at(note, variant, duration, Instant::now());
    }

    fn push_transient_note_at(
        &mut self,
        note: &str,
        variant: NoteVariant,
        duration: Duration,
        now: Instant,
    ) {
        self.push_note_variant(note, variant);
        self.toast_expiry = Some(ToastExpiry {
            remaining: duration,
            started: Some(now),
        });
    }

    pub fn tick_toast(&mut self, now: Instant) {
        if self.interrupt_armed_until.is_some_and(|until| now >= until) {
            self.interrupt_armed_until = None;
        }
        if self.toast_expiry.as_ref().is_some_and(|expiry| {
            expiry
                .started
                .is_some_and(|started| now.saturating_duration_since(started) >= expiry.remaining)
        }) {
            self.note = None;
            self.service_note = false;
            self.toast_expiry = None;
            self.toast_down = false;
        }
    }

    fn set_toast_hover(&mut self, hovered: bool, now: Instant) {
        let Some(expiry) = &mut self.toast_expiry else {
            return;
        };
        if hovered {
            if let Some(started) = expiry.started.take() {
                expiry.remaining = expiry
                    .remaining
                    .saturating_sub(now.saturating_duration_since(started));
            }
        } else if expiry.started.is_none() {
            expiry.started = Some(now);
        }
    }

    /// Push one synthetic transcript row for a non-fatal warning, so a
    /// degraded capability stays visible after the status note is replaced.
    pub fn push_warning(&mut self, warning: &str) {
        self.window
            .push_synthetic("", &format!("(warning: {warning})"), None, None);
    }
}

mod accounts;
pub use accounts::AuthRequest;
mod input;
mod live;
mod mcp;
mod model_selection;
mod services;
mod tabs;
mod transcript;

pub use live::{PumpOutcome, ScriptDriver};
pub(crate) use tabs::{TAB_SPINNER_FRAMES, TabCloseHold, TabPulseFrame, tab_glow_intensity};
pub use tabs::{TabAttention, TabCloseSnapshot, TabPresentation};
use tabs::{TabPress, TabView};

pub(crate) fn tab_smootherstep(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}
