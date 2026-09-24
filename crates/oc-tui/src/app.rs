//! Bounded chat state over the shared `CoreApp` handle.
//!
//! The view never touches storage: history pages, catalogs, skills and DCP
//! snapshots arrive as bounded application DTOs, and user choices leave as
//! [`PanelIntent`] values that the binary applies through the application
//! API (then reports acceptance or failure). Worker event draining stays in
//! the binary; the state only applies turn-scoped events, so a late event
//! for a stale turn can never corrupt the view.

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
    AgentEntry, CatalogSnapshot, DcpSnapshot, HistoryPage, SkillCard, ToolOpView,
};
use oc_core::session::CoreError;
use ratatui::layout::Rect;

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

/// Open TUI panel (bounded view state; one at a time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiPanel {
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
    /// Skill catalog (UI06).
    Skills,
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
    /// Load the model/agent catalog snapshot.
    LoadCatalog,
    /// Load the session list snapshot.
    LoadSessions,
    /// Load the skill card snapshot.
    LoadSkills,
    /// Load the newest tool-card page.
    LoadCards,
    /// Continue reading one card's durable result through the owning application.
    LoadCardOutput { op: String, offset: usize },
    /// Create an empty application session and attach its Home route.
    NewSession,
    /// Activate a retained real tab by its zero-based deck index.
    ActivateTab { index: usize },
    /// Close a retained tab; `tabs.len()` denotes the synthetic Home slot.
    CloseTab { index: usize },
    /// Apply the trimmed title to the attached session through the owner.
    RenameSession { title: String },
    /// Apply a slash-supplied title without opening the editor; ACK clears the slash draft.
    RenameSessionDirect { title: String },
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

/// Bounded, application-supplied presentation for one retained real tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabPresentation {
    pub title: Option<String>,
    pub home: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabPress {
    Add,
    Tab(usize),
    Close(usize),
}

/// Geometry and pointer from a genuine mouse release on a painted close cell.
/// Kept across the binary's replacement of the active view only on success.
pub struct TabCloseSnapshot {
    area: Rect,
    strip: crate::layout::HorizontalTabStrip,
    tabs: Vec<TabPresentation>,
    home: bool,
    closed: usize,
    pointer: (u16, u16),
}

pub(crate) struct TabCloseHold {
    pub(crate) area: Rect,
    pub(crate) strip: crate::layout::HorizontalTabStrip,
    pub(crate) until: Instant,
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

/// One live part of the streaming turn, in arrival order (upstream message
/// parts: reasoning, text, tool — `routes/session/index.tsx:1433-1481`).
///
/// Text/reasoning segments freeze when a tool call starts so tool cards keep
/// their upstream position between the text parts; the transient recorded
/// input stays with an in-flight card until its outcome arrives, then the
/// card is rebuilt and the input dropped (bounded live state).
#[derive(Debug, Clone, PartialEq, Eq)]
enum LivePart {
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
            LivePart::Text(text) | LivePart::Reasoning { text, .. } => text.len(),
            LivePart::Tool { card, input } => card.retained_bytes() + input.len(),
        }
    }

    /// Render this part as a transcript row.
    fn to_row(&self, agent: Option<String>) -> HistoryRow {
        match self {
            LivePart::Text(text) => HistoryRow {
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
                }),
                meta: None,
                tool: None,
            },
            LivePart::Tool { card, .. } => HistoryRow {
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

/// Immutable submission identity plus the editable draft's revision at enqueue.
struct PendingSubmission {
    request_id: u64,
    generation: u64,
    session: SessionId,
    fresh: bool,
    draft: String,
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

#[derive(Clone, Copy)]
struct TranscriptViewport {
    width: u16,
    terminal_width: u16,
    height: u16,
    total: usize,
    requested_scroll: usize,
    displayed_scroll: usize,
}

/// Bounded chat state on the shared handle, optionally attached to a session.
pub struct TuiState {
    pub chrome: oc_core::queries::TuiChrome,
    pub parent_id: Option<String>,
    /// New interactive launch, distinct from an explicitly attached session.
    pub home: bool,
    /// Sampled once per UI instance; never changes during a redraw.
    pub(crate) home_example: &'static str,
    viewport: std::cell::Cell<Option<TranscriptViewport>>,
    /// Current session's durable human title, refreshed with history.
    pub session_title: Option<String>,
    /// Session autoaccept capability supplied by the application.
    pub auto_accept: oc_core::queries::AutoAcceptState,
    app: CoreApp,
    session: Option<SessionId>,
    status: TuiStatus,
    panel: TuiPanel,
    pub(crate) select: crate::dialog::SelectList,
    /// Press origin prevents drag-release across the backdrop from dismissing a dialog.
    mouse_down: Option<crate::dialog::DialogHit>,
    tab_down: Option<TabPress>,
    hovered_tab: std::cell::Cell<Option<(usize, Rect)>>,
    last_mouse: Option<(u16, u16, Rect)>,
    pub(crate) close_hold: Option<TabCloseHold>,
    tabs: Vec<TabPresentation>,
    active_tab: usize,
    can_add_tab: bool,
    /// Only operation IDs whose exploration headers were explicitly opened.
    exploration_expanded: BTreeSet<String>,
    exploration_down: Option<(String, u16, u16)>,
    leader: Option<Instant>,
    input: String,
    editor: crate::editor::Editor,
    rename_input: String,
    rename_editor: crate::editor::Editor,
    rename_pending: Option<String>,
    rename_direct_pending: Option<(String, u64)>,
    regenerate_pending: Option<u64>,
    window: HistoryWindow,
    markdown_cache: std::cell::RefCell<crate::messages::MarkdownCache>,
    live_text: String,
    /// Reasoning text streamed for the active turn (never persisted).
    live_reasoning: String,
    thinking_expanded: bool,
    /// Frozen live parts (text/reasoning segments and tool cards) of the
    /// active turn, in arrival order.
    live_parts: Vec<LivePart>,
    /// Explicit durable live identities; replaced at each application checkpoint.
    pub live_part_states: Vec<oc_core::queries::PartState>,
    live_agent_color_index: Option<usize>,
    live_terminal_status: Option<String>,
    live_model_label: Option<String>,
    live_preview_truncated: bool,
    /// First reasoning delta of the active turn, for the collapsed header's
    /// duration (`part.time.created` upstream).
    reasoning_started: Option<Instant>,
    /// When text streaming followed reasoning (`part.time.completed`).
    reasoning_finished: Option<Instant>,
    /// Provider usage reported for the active turn (never synthesized).
    turn_usage: Option<TurnUsage>,
    scroll: usize,
    note: Option<String>,
    active_turn: Option<WorkerTurnId>,
    pending: Option<PendingSubmission>,
    compress_turn: Option<WorkerTurnId>,
    request_id: u64,
    generation: u64,
    input_revision: u64,
    slash_selected: usize,
    slash_dismissed: Option<u64>,
    /// Model picker (present while the Model panel lives).
    pub(crate) picker: Option<ModelPicker>,
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
    /// Sessions cursor.
    pub(crate) sessions_cursor: usize,
    sessions_loaded: bool,
    /// Skill catalog cards (metadata only, no bodies).
    pub(crate) skills: Vec<SkillCard>,
    /// Skills cursor.
    pub(crate) skills_cursor: usize,
    skills_loaded: bool,
    /// DCP panel state: snapshot in, request out, transient outcome (UI04).
    pub(crate) dcp: DcpPanelState,
    /// Workspace command ids known to the application (templates stay there).
    pub(crate) commands: Vec<String>,
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
            session_title: None,
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            app,
            session,
            status: TuiStatus::Idle,
            panel: TuiPanel::None,
            select: Default::default(),
            mouse_down: None,
            tab_down: None,
            hovered_tab: std::cell::Cell::new(None),
            last_mouse: None,
            close_hold: None,
            tabs: Vec::new(),
            active_tab: 0,
            can_add_tab: false,
            exploration_expanded: BTreeSet::new(),
            exploration_down: None,
            leader: None,
            input: String::new(),
            editor: Default::default(),
            rename_input: String::new(),
            rename_editor: Default::default(),
            rename_pending: None,
            rename_direct_pending: None,
            regenerate_pending: None,
            window: HistoryWindow::new(),
            markdown_cache: std::cell::RefCell::new(Default::default()),
            live_text: String::new(),
            live_reasoning: String::new(),
            thinking_expanded: false,
            live_parts: Vec::new(),
            live_part_states: Vec::new(),
            live_agent_color_index: None,
            live_terminal_status: None,
            live_model_label: None,
            live_preview_truncated: false,
            reasoning_started: None,
            reasoning_finished: None,
            turn_usage: None,
            scroll: 0,
            note: None,
            active_turn: None,
            pending: None,
            compress_turn: None,
            request_id: 0,
            generation: 0,
            input_revision: 0,
            slash_selected: 0,
            slash_dismissed: None,
            picker: None,
            catalog_loaded: false,
            agents: Vec::new(),
            active_agent: None,
            agents_cursor: 0,
            sessions: Vec::new(),
            sessions_cursor: 0,
            sessions_loaded: false,
            skills: Vec::new(),
            skills_cursor: 0,
            skills_loaded: false,
            dcp: DcpPanelState::default(),
            commands: Vec::new(),
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

    /// Real tabs, selected deck index and application-owned add capability.
    /// On Home with retained tabs, the selected synthetic slot is at `tabs.len()`.
    /// An empty slice means the legacy single-session/stripless-Home view.
    pub fn tab_presentation(&self) -> (&[TabPresentation], usize, bool) {
        (
            &self.tabs,
            if self.home && !self.tabs.is_empty() {
                self.tabs.len()
            } else {
                self.active_tab
            },
            self.can_add_tab,
        )
    }

    /// Refresh the retained deck. The binary owns route selection and the add
    /// action; Home itself becomes a synthetic final slot only in the renderer.
    pub fn set_tab_strip(&mut self, tabs: Vec<TabPresentation>, active: usize, can_add: bool) {
        self.tab_down = None;
        let count = tabs.len().min(16);
        if self.tabs.len() != count
            || self.active_tab != active.min(count.saturating_sub(1))
            || self.can_add_tab != (can_add && count > 0)
        {
            self.hovered_tab.set(None);
            self.close_hold = None;
        }
        self.tabs = tabs.into_iter().take(16).collect();
        self.active_tab = active.min(self.tabs.len().saturating_sub(1));
        self.can_add_tab = can_add && !self.tabs.is_empty();
    }

    /// Clear the recorded SGR coordinate when a resize invalidates its frame.
    pub fn clear_mouse_position(&mut self) {
        self.last_mouse = None;
        self.hovered_tab.set(None);
        self.close_hold = None;
        self.tab_down = None;
    }

    /// Recover hover after a successful mouse tab activation (never on keys).
    pub fn restore_mouse_hover(&mut self, pointer: (u16, u16, Rect)) {
        let (x, y, area) = pointer;
        if self.panel != TuiPanel::None || self.is_busy() {
            return;
        }
        self.last_mouse = Some(pointer);
        self.hovered_tab.set(
            crate::shell::tab_strip(self, area)
                .and_then(|strip| strip.hit_test(x, y))
                .map(|index| (index, area)),
        );
    }

    /// A keyboard close can replace the view that received the last real mouse
    /// event. Only restore the add hover if that same pointer hits the newly
    /// painted, owner-enabled control; keys never create a mouse close hold.
    pub fn restore_tab_hover_at(&mut self, pointer: (u16, u16, Rect)) {
        let (x, y, area) = pointer;
        if self.panel != TuiPanel::None || self.is_busy() {
            return;
        }
        if crate::shell::tab_strip(self, area)
            .and_then(|strip| strip.add)
            .is_some_and(|add| add.width == 3 && add.contains((x, y).into()))
        {
            self.last_mouse = Some(pointer);
            self.hovered_tab.set(None);
        }
    }

    pub(crate) fn tab_add_hovered(&self, area: Rect, add: Rect) -> bool {
        self.panel == TuiPanel::None
            && !self.is_busy()
            && add.width == 3
            && self.last_mouse.is_some_and(|(x, y, pointer_area)| {
                pointer_area == area && add.contains((x, y).into())
            })
    }

    pub fn mouse_position(&self) -> Option<(u16, u16, Rect)> {
        self.last_mouse
    }

    /// Snapshot must be taken before the owner mutates the deck. A plain
    /// CloseTab intent (e.g. keyboard) cannot create a mouse hold.
    pub fn mouse_close_snapshot(&self, index: usize) -> Option<TabCloseSnapshot> {
        let (x, y, area) = self.last_mouse?;
        let region = crate::shell::tab_region(self, area);
        if self.panel != TuiPanel::None
            || self.is_busy()
            || region.height != 1
            || region.width != area.width
            || self.tab_hit(area, x, y) != Some(TabPress::Close(index))
        {
            return None;
        }
        Some(TabCloseSnapshot {
            area,
            strip: crate::shell::tab_strip(self, area)?,
            tabs: self.tabs.clone(),
            home: self.home,
            closed: index,
            pointer: (x, y),
        })
    }

    /// `session-tabs.tsx:237-274,1446-1467`: hold the adjacent visible
    /// survivor under the release column for up to five seconds. Reject a
    /// changed deck or a clipped cell; re-hit-test exactly what is painted.
    pub fn restore_mouse_close(&mut self, snapshot: TabCloseSnapshot) {
        let old_count = snapshot.tabs.len() + usize::from(snapshot.home);
        let new_count = self.tabs.len() + usize::from(self.home);
        if self.panel != TuiPanel::None || self.is_busy() || old_count != new_count + 1 {
            return;
        }
        let old_tabs = snapshot.tabs.len();
        let mut old = snapshot.tabs;
        if snapshot.closed < old.len() {
            old.remove(snapshot.closed);
        }
        if old != self.tabs || (snapshot.closed != old_tabs && snapshot.home != self.home) {
            return;
        }
        let target = if snapshot.closed < old_count - 1 {
            snapshot.closed
        } else if snapshot.closed > 0 {
            snapshot.closed - 1
        } else {
            return;
        };
        let region = crate::shell::tab_region(self, snapshot.area);
        if region.height != 1 || region.width != snapshot.area.width {
            return; // The surviving route changed orientation or terminal geometry.
        }
        if let Some(strip) = crate::layout::held_after_close(
            &snapshot.strip,
            region,
            snapshot.closed,
            new_count,
            target,
            snapshot.pointer.0,
            !self.home && self.can_add_tab,
        ) {
            self.close_hold = Some(TabCloseHold {
                area: snapshot.area,
                strip,
                until: Instant::now() + std::time::Duration::from_secs(5),
            });
        }
        self.restore_mouse_hover((snapshot.pointer.0, snapshot.pointer.1, snapshot.area));
    }

    pub(crate) fn set_detail_area(&self, area: ratatui::layout::Rect) {
        self.detail_area.set(area);
    }

    pub(crate) fn detail_area(&self) -> ratatui::layout::Rect {
        self.detail_area.get()
    }

    pub(crate) fn card_scroll(&self) -> usize {
        self.card_scroll
    }

    pub(crate) fn card_seen(&self) -> usize {
        self.card_seen.get()
    }

    /// Only rows actually painted in a frame count as accessible. An End key
    /// or repeated key events without drawing cannot skip a result window.
    pub(crate) fn card_rows_painted(&self, start: usize, end: usize) {
        if start <= self.card_seen.get() {
            self.card_seen.set(self.card_seen.get().max(end));
        }
    }

    /// Switch to another session after an accepted switch: clears view state
    /// and the history window; status returns to `Idle` unless quitting.
    /// Drop every generation-bound cache after a Location switch.
    ///
    /// Panel data (catalog, agents, sessions, skills, cards, DCP snapshot and
    /// workspace commands) belongs to the previous Location: the next panel
    /// open must reload from the new generation instead of showing it.
    pub fn reset_workspace(&mut self) {
        self.close_panel();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.tabs.clear();
        self.active_tab = 0;
        self.can_add_tab = false;
        self.exploration_expanded.clear();
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
        self.close_panel();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.exploration_expanded.clear();
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
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.active_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
    }

    fn invalidate_submission(&mut self) {
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

    /// Open panel, if any.
    pub fn panel(&self) -> &TuiPanel {
        &self.panel
    }

    /// Safe options from actual snapshots. Filtering never changes runtime selection.
    pub fn modal_options(&self) -> std::rc::Rc<Vec<crate::dialog::SelectOption>> {
        use crate::dialog::SelectOption;
        let item =
            |value: String, title: String, category: &str, footer: String, current| SelectOption {
                value,
                title,
                category: category.into(),
                footer,
                current,
            };
        let options = match &self.panel {
            TuiPanel::Commands => {
                let mut options = Vec::new();
                // Use the last rendered terminal width and the same rail allocation
                // and auto breakpoint as shell::shell_regions/session_main.
                let sidebar_visible = !self.home
                    && self.parent_id.is_none()
                    && !self.chrome.sidebar_hidden
                    && self.viewport.get().is_some_and(|view| {
                        let session = crate::layout::configured_shell_regions(
                            Rect::new(0, 0, view.terminal_width, view.height),
                            self.chrome.devtools_visible(),
                            self.chrome.vertical_tabs_width,
                        )
                        .session;
                        crate::layout::sidebar_auto(session.width)
                    });
                if self.select.query.is_empty() {
                    options.extend(
                        crate::commands::REGISTRY
                            .iter()
                            .filter(|c| {
                                c.id == "model.list"
                                    || ((c.id == "session.list" || c.id == "session.new")
                                        && !self.home)
                            })
                            .map(|c| {
                                item(
                                    c.id.into(),
                                    c.title.into(),
                                    "Suggested",
                                    self.command_footer(c),
                                    false,
                                )
                            }),
                    );
                }
                options.extend(
                    crate::commands::REGISTRY
                        .iter()
                        .filter(|c| {
                            c.in_palette(self.picker.as_ref().is_some_and(|p| p.has_variants()))
                                && (c.action != CommandAction::CloseTab || !self.tabs.is_empty())
                                && (!matches!(c.action, CommandAction::RenameSession { .. })
                                    || self.command_unavailable(&c.action).is_none())
                        })
                        .map(|c| {
                            item(
                                c.id.into(),
                                if c.id == "session.toggle.thinking" && self.thinking_expanded {
                                    "Collapse thinking".into()
                                } else if c.id == "session.sidebar.toggle" {
                                    if sidebar_visible {
                                        "Hide sidebar".into()
                                    } else {
                                        "Show sidebar".into()
                                    }
                                } else {
                                    c.title.into()
                                },
                                c.group,
                                self.command_footer(c),
                                false,
                            )
                        }),
                );
                options
            }
            TuiPanel::Model => {
                return self.select.filter_for(
                    self.picker
                        .as_ref()
                        .map(|p| p.options())
                        .unwrap_or_default(),
                    &self.panel,
                );
            }
            TuiPanel::Variant => self
                .picker
                .as_ref()
                .map(|p| p.variant_options())
                .unwrap_or_default(),
            TuiPanel::Agents => self
                .agents
                .iter()
                .map(|a| {
                    item(
                        a.id.clone(),
                        a.id.clone(),
                        "Agents",
                        a.description.clone(),
                        self.active_agent.as_ref() == Some(&a.id),
                    )
                })
                .collect(),
            TuiPanel::Sessions => self
                .sessions
                .iter()
                .map(|s| {
                    item(
                        s.clone(),
                        s.clone(),
                        "Sessions",
                        String::new(),
                        self.attached_session().is_some_and(|id| s == &id.0),
                    )
                })
                .collect(),
            TuiPanel::Skills => self
                .skills
                .iter()
                .map(|s| {
                    item(
                        s.id.clone(),
                        format!("{} — {}", s.id, s.name),
                        "Skills",
                        s.description.clone(),
                        false,
                    )
                })
                .collect(),
            TuiPanel::Rename => Vec::new(),
            TuiPanel::None => Vec::new(),
            _ => crate::views::panel_lines(self)
                .into_iter()
                .enumerate()
                .map(|(i, s)| item(i.to_string(), s, "", String::new(), false))
                .collect(),
        };
        self.select
            .filter_for(std::rc::Rc::new(options), &self.panel)
    }

    pub fn command_unavailable(&self, action: &CommandAction) -> Option<&'static str> {
        if *action == CommandAction::CloseTab {
            let (tabs, index, _) = self.tab_presentation();
            if tabs.is_empty() || (index >= tabs.len() && !self.home) {
                return Some("no tab to close");
            }
            if tabs.get(index).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if matches!(action, CommandAction::RenameSession { .. }) {
            if self.home || self.session.is_none() {
                return Some("no session yet");
            }
            if self.parent_id.is_some() {
                return Some("child session is read-only");
            }
            if self.tabs.get(self.active_tab).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if self.session.is_none()
            && matches!(
                action,
                CommandAction::OpenCards | CommandAction::DcpCompress { .. }
            )
        {
            return Some("no session yet");
        }
        crate::commands::spec(action).unavailable(
            self.is_busy(),
            self.picker.as_ref().is_some_and(|p| p.has_variants()),
        )
    }

    fn command_footer(&self, command: &crate::commands::CommandSpec) -> String {
        self.command_unavailable(&command.action)
            .map(str::to_string)
            .unwrap_or_else(|| command.shortcuts.join(" "))
    }

    /// Called only after a successful application selection. The original applies
    /// the model before replacing its dialog; Escape here never rolls it back.
    pub fn model_choice_applied(&mut self, snapshot: CatalogSnapshot) {
        let selecting_model = self.panel == TuiPanel::Model;
        self.apply_catalog(snapshot);
        if selecting_model
            && self.picker.as_ref().is_some_and(|p| {
                p.has_variants() && p.selection().is_some_and(|s| s.variant.is_none())
            })
        {
            self.open_variants();
        } else {
            self.close_panel();
        }
    }

    fn open_variants(&mut self) {
        self.panel = TuiPanel::Variant;
        self.hovered_tab.set(None);
        self.close_hold = None;
        self.last_mouse = None;
        // A press belongs to the dialog where it began, not the replacement.
        self.mouse_down = None;
        self.select.reset();
        self.select.cursor = self
            .modal_options()
            .iter()
            .position(|o| o.current)
            .unwrap_or(0);
    }

    fn changed_modal_query(&mut self) {
        self.select.changed_query();
        if self.panel == TuiPanel::Variant && self.select.query.is_empty() {
            self.select.cursor = self
                .modal_options()
                .iter()
                .position(|o| o.current)
                .unwrap_or(0);
        }
        self.sync_modal_cursor();
    }

    fn sync_modal_cursor(&mut self) {
        let options = self.modal_options();
        self.select.cursor = self.select.cursor.min(options.len().saturating_sub(1));
        let Some(option) = options.get(self.select.cursor) else {
            return;
        };
        match self.panel {
            TuiPanel::Model => {
                if let Some(p) = &mut self.picker {
                    p.focus_id(&option.value);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = self
                    .agents
                    .iter()
                    .position(|a| a.id == option.value)
                    .unwrap_or(0)
            }
            TuiPanel::Sessions => {
                self.sessions_cursor = self
                    .sessions
                    .iter()
                    .position(|s| s == &option.value)
                    .unwrap_or(0)
            }
            TuiPanel::Skills => {
                self.skills_cursor = self
                    .skills
                    .iter()
                    .position(|s| s.id == option.value)
                    .unwrap_or(0)
            }
            _ => {}
        }
    }

    /// Current input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Prompt layout and the insertion caret share the same grapheme/cell model.
    pub fn prompt_layout(&self, width: usize) -> (Vec<crate::editor::PromptRow>, (usize, usize)) {
        self.editor.layout(&self.input, width)
    }

    /// Only the focused prompt, not a dialog or a dismissed revision, owns the overlay.
    pub(crate) fn slash_options(&self) -> Option<Vec<crate::autocomplete::SlashOption>> {
        if self.panel != TuiPanel::None || self.slash_dismissed == Some(self.input_revision) {
            return None;
        }
        let filter = crate::autocomplete::query(&self.input, self.editor.cursor)?;
        Some(crate::autocomplete::options(filter, &self.commands))
    }

    /// Keep selection and activation aligned when caret movement or a catalog
    /// refresh shrinks the filtered list without an intervening text edit.
    pub(crate) fn slash_selected(&self, count: usize) -> usize {
        self.slash_selected.min(count.saturating_sub(1))
    }

    fn replace_slash(&mut self, name: &str, trailing_space: bool) {
        let cursor = self.editor.cursor;
        self.editor.move_to(0, false);
        self.editor.move_to(cursor, true);
        let replacement = format!("/{name}{}", if trailing_space { " " } else { "" });
        if self
            .editor
            .replace(&mut self.input, &replacement, MAX_INPUT_BYTES)
            > 0
        {
            self.input_revision += 1;
        }
        self.slash_selected = 0;
        self.slash_dismissed = Some(self.input_revision);
    }

    async fn select_slash(&mut self, enter: bool) -> KeyOutcome {
        let Some(options) = self.slash_options() else {
            return KeyOutcome::default();
        };
        let selected = self.slash_selected(options.len());
        let Some(option) = options.into_iter().nth(selected) else {
            return KeyOutcome::default();
        };
        if !enter || option.arguments || option.action.is_none() {
            self.replace_slash(&option.name, true);
            return KeyOutcome::default();
        }
        let action = option.action.expect("argument-free built-in");
        if matches!(action, CommandAction::NewSession | CommandAction::CloseTab) {
            // The binary clears these drafts only after its owner accepts the
            // intent. An optimistic removal would lose `/new` on refusal.
            if self.input != format!("/{}", option.name) {
                self.replace_slash(&option.name, false);
            }
            return self.handle_enter().await;
        }
        // The existing owner path checks availability and returns actual intents;
        // a refused command leaves the editable slash text untouched.
        let result = self.run_command(action);
        if result.note.is_none() {
            let cursor = self.editor.cursor;
            self.editor.move_to(0, false);
            self.editor.move_to(cursor, true);
            if self.editor.delete(&mut self.input, true, false) {
                self.input_revision += 1;
            }
            self.slash_selected = 0;
        }
        result
    }

    /// Active turn, if any.
    pub fn active_turn(&self) -> Option<&WorkerTurnId> {
        self.active_turn.as_ref()
    }

    /// Transcript scroll offset in rows (0 = pinned to the newest row).
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Effective offset in the last drawn viewport; requested offset survives resize.
    pub fn display_scroll(&self) -> usize {
        self.viewport
            .get()
            .filter(|view| view.requested_scroll == self.scroll)
            .map_or_else(
                || self.scroll.min(self.max_scroll()),
                |view| view.displayed_scroll,
            )
    }

    /// True while a submission awaits acceptance or a turn streams.
    pub fn is_busy(&self) -> bool {
        self.active_turn.is_some() || self.pending.is_some()
    }

    /// Status note, if any (intent errors and hints; never chat history).
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Newest page becomes the whole window; scroll pins to the newest row.
    pub fn attach_page(&mut self, page: &HistoryPage) {
        self.exploration_down = None;
        self.exploration_expanded.clear();
        self.viewport.set(None);
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.window.reset(page);
        self.scroll = 0;
    }

    /// Add an older page at the front of the window.
    pub fn prepend_page(&mut self, page: &HistoryPage) {
        self.viewport.set(None);
        self.window.prepend_older(page);
    }

    /// Add a newer page at the back of the window.
    pub fn append_page(&mut self, page: &HistoryPage) {
        self.viewport.set(None);
        self.window.append_newer(page);
    }

    /// Older committed rows exist before the loaded window.
    pub fn needs_older(&self) -> bool {
        self.window.has_older()
    }

    /// Newer committed rows exist after the loaded window.
    pub fn needs_newer(&self) -> bool {
        self.window.has_newer()
    }

    /// Bounded history window (rows, caps and paging flags).
    pub fn history(&self) -> &HistoryWindow {
        &self.window
    }

    /// Close any open panel (chat view).
    pub fn close_panel(&mut self) {
        self.panel = TuiPanel::None;
        self.rename_input.clear();
        self.rename_editor.clear();
        self.rename_pending = None;
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.mouse_down = None;
        self.tab_down = None;
        self.hovered_tab.set(None);
        self.close_hold = None;
        self.last_mouse = None;
        self.exploration_down = None;
        self.select.reset();
    }

    /// The active dialog exclusively owns search/cursor input; when it is
    /// replaced (Model → Variant) the former owner is destroyed, and closing
    /// the replacement restores the original prompt draft, selection and caret.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        use crate::dialog::DialogHit;
        self.last_mouse = (self.panel == TuiPanel::None).then_some((event.column, event.row, area));
        if self.close_hold.as_ref().is_some_and(|hold| {
            hold.area != area
                || hold.until <= Instant::now()
                || event.row != hold.strip.tabs.first().map_or(u16::MAX, |tab| tab.rect.y)
                || !area.contains((event.column, event.row).into())
        }) || matches!(event.kind, MouseEventKind::Down(_))
        {
            self.close_hold = None;
        }
        if self.panel == TuiPanel::None {
            match event.kind {
                MouseEventKind::Moved => {
                    self.hovered_tab.set(
                        crate::shell::tab_strip(self, area)
                            .and_then(|strip| strip.hit_test(event.column, event.row))
                            .map(|index| (index, area)),
                    );
                    self.exploration_down = None;
                }
                MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                    self.tab_down = self.tab_hit(area, event.column, event.row);
                    self.exploration_down = self
                        .exploration_hit(area, event.column, event.row)
                        .map(|op| (op, event.column, event.row));
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let pressed_tab = self.tab_down.take();
                    let pressed = self.exploration_down.take();
                    if event.modifiers.is_empty()
                        && let Some(tab) = pressed_tab
                        && self.tab_hit(area, event.column, event.row) == Some(tab)
                    {
                        return KeyOutcome {
                            intent: Some(match tab {
                                TabPress::Add => PanelIntent::NewSession,
                                TabPress::Tab(index) => PanelIntent::ActivateTab { index },
                                TabPress::Close(index) => PanelIntent::CloseTab { index },
                            }),
                            ..KeyOutcome::default()
                        };
                    }
                    if event.modifiers.is_empty()
                        && let Some((op, x, y)) = pressed
                        && (x, y) == (event.column, event.row)
                        && self
                            .exploration_hit(area, event.column, event.row)
                            .as_deref()
                            == Some(&op)
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let height = rect.height as usize;
                        let (_, before, displayed) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        // Keep the clicked header at its painted row by anchoring
                        // the first visible row, rather than the bottom offset.
                        let first = before.saturating_sub(height).saturating_sub(displayed);
                        let rows = self.transcript_rows();
                        self.exploration_expanded.retain(|id| {
                            rows.iter()
                                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
                        });
                        if !self.exploration_expanded.insert(op.clone()) {
                            self.exploration_expanded.remove(&op);
                        }
                        let (_, after) =
                            self.visible_transcript(rect.width, area.width, rect.height);
                        self.scroll = after.saturating_sub(height).saturating_sub(first);
                        self.observe_transcript_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                            after,
                            self.scroll,
                        );
                    }
                }
                MouseEventKind::Drag(_)
                | MouseEventKind::Down(_)
                | MouseEventKind::Up(_)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown => {
                    self.exploration_down = None;
                    self.tab_down = None;
                    if matches!(event.kind, MouseEventKind::Drag(_)) {
                        self.hovered_tab.set(None);
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        self.exploration_down = None;
        self.tab_down = None;
        self.hovered_tab.set(None);
        self.close_hold = None;
        if self.panel == TuiPanel::Rename {
            let rect = crate::dialog::rename_geometry(area);
            let hit = if !rect.contains((event.column, event.row).into()) {
                DialogHit::Backdrop
            } else if event.row == rect.y + 1
                && event.column >= rect.right().saturating_sub(7)
                && event.column < rect.right().saturating_sub(4)
            {
                DialogHit::Close
            } else {
                DialogHit::Surface
            };
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => self.mouse_down = Some(hit),
                MouseEventKind::Up(MouseButton::Left) => {
                    let pressed = self.mouse_down.take();
                    if self.rename_pending.is_none()
                        && ((pressed == Some(DialogHit::Backdrop) && hit == DialogHit::Backdrop)
                            || (pressed == Some(DialogHit::Close) && hit == DialogHit::Close))
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (rect, _, _) = crate::dialog::card_geometry(area);
            let inside = rect.contains((event.column, event.row).into());
            match event.kind {
                MouseEventKind::ScrollUp if inside => {
                    self.handle_panel_key(KeyAction::Up);
                }
                MouseEventKind::ScrollDown if inside => {
                    self.handle_panel_key(KeyAction::Down);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    self.mouse_down = Some(if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    });
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let hit = if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    };
                    if self.mouse_down.take() == Some(DialogHit::Backdrop)
                        && hit == DialogHit::Backdrop
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        let options = self.modal_options();
        let size = crate::dialog::size_for(&self.panel);
        let hit = self
            .select
            .hit(area, size, &options, event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_down = Some(hit);
                if let DialogHit::Option(index) = hit {
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                if let DialogHit::Option(index) = hit
                    && self.select.cursor != index
                {
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if hit != DialogHit::Backdrop {
                    self.select.scroll_rows(
                        if event.kind == MouseEventKind::ScrollUp {
                            -3
                        } else {
                            3
                        },
                        area,
                        size,
                        &options,
                    );
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.mouse_down.take();
                if pressed == Some(hit) {
                    match hit {
                        DialogHit::Backdrop | DialogHit::Close => self.close_panel(),
                        DialogHit::Option(index)
                            if !matches!(
                                self.panel,
                                TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                            ) =>
                        {
                            self.select.cursor = index;
                            self.sync_modal_cursor();
                            return self.panel_enter();
                        }
                        DialogHit::Option(_)
                        | DialogHit::Search
                        | DialogHit::Surface
                        | DialogHit::List => {}
                    }
                }
            }
            _ => {}
        }
        KeyOutcome::default()
    }

    fn tab_hit(&self, area: Rect, x: u16, y: u16) -> Option<TabPress> {
        let strip = crate::shell::tab_strip(self, area)?;
        if strip
            .add
            .is_some_and(|rect| rect.width == 3 && rect.contains((x, y).into()))
        {
            return Some(TabPress::Add);
        }
        let index = strip.hit_test(x, y)?;
        if strip
            .tabs
            .iter()
            .find(|tab| tab.index == index)
            .and_then(|tab| self.tab_close_cell(area, tab.index, tab.rect))
            == Some(x)
        {
            return Some(TabPress::Close(index));
        }
        // The promoted Home slot is already selected; its click is not an
        // application action. Only retained real tabs have activation intents.
        (index < self.tabs.len()).then_some(TabPress::Tab(index))
    }

    /// Hover is tied to the frame geometry that actually received a motion event.
    pub(crate) fn hovered_tab(&self, area: Rect) -> Option<usize> {
        if self.panel != TuiPanel::None {
            return None;
        }
        match self.hovered_tab.get() {
            Some((index, painted))
                if painted == area
                    && self.last_mouse.is_some_and(|(x, y, pointer_area)| {
                        pointer_area == area
                            && crate::shell::tab_strip(self, area)
                                .and_then(|strip| strip.hit_test(x, y))
                                == Some(index)
                    }) =>
            {
                Some(index)
            }
            Some(_) => {
                self.hovered_tab.set(None);
                None
            }
            None => None,
        }
    }

    /// Same eligibility and cell for the painted overlay and mouse action.
    pub fn tab_close_cell(&self, area: Rect, index: usize, rect: Rect) -> Option<u16> {
        (!self.tabs.is_empty()
            && (index < self.tabs.len() || (self.home && index == self.tabs.len()))
            && self.hovered_tab(area) == Some(index)
            && !self.is_busy()
            && !self.tabs.get(index).is_some_and(|tab| tab.busy))
        .then(|| crate::layout::tab_close_cell(rect))
        .flatten()
    }

    fn exploration_hit(&self, area: Rect, x: u16, y: u16) -> Option<String> {
        let rect = crate::shell::transcript_area(self, area);
        if rect.width == 0 || rect.height == 0 || !rect.contains((x, y).into()) {
            return None;
        }
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len() - 1 - usize::from(self.active_turn.is_some() && self.live_preview_truncated)
        });
        crate::messages::exploration_header_at(
            &rows,
            Theme::dark(),
            (rect.width, area.width),
            (
                rect.height as usize,
                self.scroll_for_current_view(),
                live_row,
            ),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
    }

    /// Window bytes plus live text, live parts and input; bounded by the
    /// window caps.
    pub fn retained_bytes(&self) -> usize {
        self.window.retained_bytes()
            + self.markdown_cache.borrow().retained_bytes()
            + self.live_text.len()
            + self.live_reasoning.len()
            + self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
            + self.input.len()
            + self.editor.retained_bytes()
            + self
                .pending
                .as_ref()
                .map_or(0, |pending| pending.draft.len())
    }

    /// Visible viewport lines (bounded, scroll-aware, live answer last).
    ///
    /// Unstyled and unwrapped; [`TuiState::transcript_lines`] is the styled
    /// renderer the shell uses.
    pub fn viewport(&self) -> Vec<String> {
        let lines = self.transcript_lines(0, u16::MAX);
        let texts: Vec<String> = lines
            .iter()
            .map(|line| line.plain_text().trim_end().to_string())
            .collect();
        let total = texts.len();
        let max_scroll = total.saturating_sub(VIEWPORT_LINES);
        let scroll = self.scroll.min(max_scroll);
        let end = total - scroll;
        let start = end.saturating_sub(VIEWPORT_LINES);
        texts[start..end].to_vec()
    }

    /// Render rows of the transcript: the bounded window plus the live parts
    /// (frozen text/reasoning segments and tool cards) and the open live
    /// answer (reasoning block and streaming text) while a turn is active.
    pub fn transcript_rows(&self) -> Vec<HistoryRow> {
        let mut rows = self.window.rows().to_vec();
        for part in &self.live_parts {
            rows.push(part.to_row(self.active_agent.clone()));
        }
        let live = !self.live_text.is_empty() || !self.live_reasoning.is_empty();
        if live {
            rows.push(HistoryRow {
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: self.live_text.clone(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning: (!self.live_reasoning.is_empty()).then(|| ReasoningBlock {
                    text: self.live_reasoning.clone(),
                    duration_ms: None,
                    running: true,
                    expanded: false,
                }),
                meta: None,
                tool: None,
            });
        }
        if self.active_turn.is_some() && self.live_preview_truncated {
            rows.push(HistoryRow {
                seq:i64::MAX,role:"assistant".into(),text:"[Live preview truncated; durable parts remain available through history and /cards]".into(),
                agent:None,agent_color_index:None,chips:Vec::new(),reasoning:None,meta:None,tool:None,
            });
        }
        for row in &mut rows {
            if let Some(reasoning) = &mut row.reasoning {
                reasoning.expanded = self.thinking_expanded;
            }
        }
        rows
    }

    /// Styled transcript lines wrapped to the content-box `width`
    /// (upstream row model: user block, assistant markdown, collapsed
    /// reasoning, assistant footer). `width == 0` is the unbounded text
    /// projection used for scroll metrics and plain-text assertions.
    pub fn transcript_lines(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let theme = Theme::dark();
        crate::messages::transcript_with_expansion(
            &self.transcript_rows(),
            theme,
            width,
            terminal_width,
            |agent| self.agent_color(agent),
            Some(&self.markdown_cache),
            &|op| self.exploration_expanded.contains(op),
        )
    }

    /// Bounded wrapped content, including the scrollbox's top padding. Padding
    /// scrolls away with long history; short history starts below that one row.
    pub fn rendered_transcript(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let mut lines = vec![Line::plain("")];
        lines.extend(crate::styled::wrap_lines(
            &self.transcript_lines(width, terminal_width),
            width as usize,
        ));
        lines
    }

    /// Materialize no more than the visible viewport, counting bounded parts
    /// through the per-session Markdown cache instead of building all rows.
    pub fn visible_transcript(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize) {
        let (lines, total, _) = self.visible_transcript_at_viewport(width, terminal_width, height);
        (lines, total)
    }

    /// Resolve a resize against the last painted top row without changing
    /// the input-owned scroll request or materializing the whole transcript.
    pub(crate) fn visible_transcript_at_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize, usize) {
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len() - 1 - usize::from(self.active_turn.is_some() && self.live_preview_truncated)
        });
        let render = |height, scroll| {
            crate::messages::visible_transcript_expanded(
                &rows,
                Theme::dark(),
                (width, terminal_width),
                (height, scroll, live_row),
                |agent| self.agent_color(agent),
                &self.markdown_cache,
                &|op| self.exploration_expanded.contains(op),
            )
        };
        let previous = self.viewport.get();
        let resized = previous.is_some_and(|view| {
            (view.width, view.terminal_width, view.height) != (width, terminal_width, height)
        });
        let scroll = if self.scroll == 0 {
            0
        } else if resized && previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            let view = previous.expect("resized viewport");
            // Count-only indexing is needed on resize, not on every draw.
            let (_, total) = render(0, 0);
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            total.saturating_sub(height as usize).saturating_sub(top)
        } else if previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            self.display_scroll()
        } else {
            self.scroll
        };
        let (lines, total) = render(height as usize, scroll);
        (
            lines,
            total,
            scroll.min(total.saturating_sub(height as usize)),
        )
    }

    /// Categorical agent color (`context/local.tsx:75-133`): the agent's index
    /// in the generation's full admitted agent list (the catalog pins the slot),
    /// and the first categorical color for an
    /// unknown/missing agent.
    pub fn agent_color(&self, agent: Option<&str>) -> ratatui::style::Color {
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

    // ---- snapshots from the binary -------------------------------------

    /// Apply a catalog snapshot: picker, agents and the effective selection.
    pub fn apply_catalog(&mut self, snapshot: CatalogSnapshot) {
        self.chrome = snapshot.chrome.clone();
        self.auto_accept = snapshot.auto_accept;
        let mut picker = ModelPicker::new(catalog_from_snapshot(&snapshot));
        if !snapshot.model_id.is_empty() {
            let record = serde_json::json!({
                "provider": snapshot.provider,
                "id": snapshot.model_id,
                "variant": snapshot.variant,
            });
            picker.load_persisted_raw(Some(&record.to_string()));
            picker.focus_id(&snapshot.model_id);
        }
        if let Some(error) = picker.last_error() {
            self.push_note(error);
        }
        self.picker = Some(picker);
        self.commands = snapshot.commands;
        self.active_agent = snapshot.agent_id.clone();
        self.agents = snapshot.agents;
        self.agents_cursor = snapshot
            .agent_id
            .as_ref()
            .and_then(|id| self.agents.iter().position(|agent| &agent.id == id))
            .unwrap_or(0);
        self.catalog_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply the session list snapshot.
    pub fn apply_sessions(&mut self, sessions: Vec<String>) {
        self.sessions = sessions;
        self.sessions_cursor = 0;
        self.sessions_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply the skill card snapshot (bodies never reach the view).
    pub fn apply_skills(&mut self, cards: Vec<SkillCard>) {
        self.skills = cards;
        self.skills_cursor = 0;
        self.skills_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply a DCP context/stats snapshot.
    pub fn apply_dcp_snapshot(&mut self, snapshot: DcpSnapshot) {
        self.dcp.set_snapshot(snapshot);
    }

    /// Apply a newest-first tool-card page; rendered as bounded rows.
    pub fn apply_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.card_ops = cards.iter().map(|card| card.op.clone()).collect();
        self.cards = cards.iter().map(card_row).collect();
        self.cards_cursor = 0;
        self.cards_loaded = true;
        self.cards_has_older = has_older;
    }

    /// Prepend an older tool-card page (paging up in the Cards panel).
    pub fn prepend_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        let mut ops: Vec<String> = cards.iter().map(|card| card.op.clone()).collect();
        ops.append(&mut self.card_ops);
        ops.truncate(CARDS_MAX);
        self.card_ops = ops;
        let mut rows: Vec<HistoryRow> = cards.iter().map(card_row).collect();
        rows.append(&mut self.cards);
        rows.truncate(CARDS_MAX);
        self.cards = rows;
        self.cards_has_older = has_older;
    }

    /// True when older tool cards exist before the loaded page.
    pub fn cards_need_older(&self) -> bool {
        self.cards_has_older
    }

    /// Replace a single bounded output page. The text remains application-owned;
    /// this preview disappears when the panel/session is closed.
    pub fn apply_card_output(
        &mut self,
        op: String,
        offset: usize,
        page: oc_core::queries::ToolOutputPage,
    ) {
        if self.panel == TuiPanel::Cards && self.card_ops.contains(&op) {
            self.card_output = Some(CardOutput { op, offset, page });
            self.card_scroll = 0;
            self.card_seen.set(0);
            self.select.reset();
        }
    }

    /// Handle a bracketed paste as one bounded event (never per-char).
    pub fn handle_paste(&mut self, text: &str) -> KeyOutcome {
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.panel != TuiPanel::None {
            if self.panel == TuiPanel::Rename {
                return self.paste_rename(text);
            }
            let room = 512_usize.saturating_sub(self.select.query.len());
            let mut kept = 0;
            for grapheme in text.graphemes(true) {
                if kept + grapheme.len() > room {
                    break;
                }
                if !grapheme.chars().any(char::is_control) {
                    self.select.query.push_str(grapheme);
                    kept += grapheme.len();
                }
            }
            self.changed_modal_query();
            return KeyOutcome {
                note: (text.len() > kept).then(|| "modal search truncated at 512 bytes".into()),
                ..KeyOutcome::default()
            };
        }
        let mut clean = String::with_capacity(text.len().min(MAX_INPUT_BYTES));
        let mut exceeded = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\r" => "\n",
                "\t" => " ",
                _ if grapheme.chars().any(char::is_control) && grapheme != "\n" => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_INPUT_BYTES {
                exceeded = true;
                break;
            }
            clean.push_str(safe);
        }
        let chip_count = self.editor.chip_count();
        let paste = self.editor.paste(&mut self.input, &clean, MAX_INPUT_BYTES);
        let dropped = text.len().saturating_sub(paste.inserted);
        if paste.inserted > 0 {
            self.input_revision += 1;
            self.slash_selected = 0;
        }
        let mut notes = Vec::new();
        if exceeded || clean.len() > paste.inserted + paste.trimmed {
            notes.push(format!(
                "paste truncated: {dropped} bytes dropped at the {MAX_INPUT_BYTES} byte input limit"
            ));
        } else if text.len() > clean.len() {
            notes.push(format!(
                "paste filtered: {} control/newline-normalization bytes",
                text.len() - clean.len()
            ));
        }
        if paste.trimmed > 0 {
            notes.push(format!(
                "paste chip trimmed: {} surrounding whitespace bytes removed",
                paste.trimmed
            ));
        }
        if paste.inserted > 0
            && chip_count == crate::editor::MAX_PASTE_CHIPS
            && crate::editor::chip_worthy(&clean[..paste.inserted + paste.trimmed]).is_some()
        {
            notes
                .push("paste display chip limit reached; full pasted text remains in draft".into());
        }
        KeyOutcome {
            note: (!notes.is_empty()).then(|| notes.join("; ")),
            ..KeyOutcome::default()
        }
    }

    /// Report a runtime DCP outcome: transient notice, never chat history.
    pub fn notify_dcp(&mut self, outcome: DcpOutcome) {
        self.dcp.set_outcome(outcome);
    }

    /// Report that an intent could not be applied; the input is kept so the
    /// user can retry or edit it.
    pub fn apply_intent_error(&mut self, message: String) {
        self.note = Some(message);
    }

    /// Current modal value, distinct from the prompt draft and its caret.
    pub fn rename_title(&self) -> Option<&str> {
        (self.panel == TuiPanel::Rename).then_some(self.rename_input.as_str())
    }

    pub(crate) fn rename_cursor(&self) -> usize {
        self.rename_editor.cursor
    }

    /// Call only after the application owner has accepted the title update.
    /// This also updates the active tab until the next owner deck snapshot.
    pub fn rename_session_applied(&mut self, title: String) {
        if self.panel != TuiPanel::Rename || self.rename_pending.as_deref() != Some(&title) {
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        self.close_panel();
        self.note = None;
    }

    /// Owner refusal retains the focused title and prompt draft for retry.
    pub fn rename_session_rejected(&mut self, message: String) {
        if self.panel == TuiPanel::Rename {
            self.rename_pending = None;
            self.apply_intent_error(message);
        }
    }

    /// ACK a direct `/rename <title>` only when its matching owner request completed.
    /// Preserve edits made to the composer while the owner was working.
    pub fn rename_session_direct_applied(&mut self, title: String) {
        let Some((pending, revision)) = self.rename_direct_pending.take() else {
            return;
        };
        if pending != title {
            self.rename_direct_pending = Some((pending, revision));
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        if self.input_revision == revision {
            self.accept_intent();
            self.input_revision += 1;
        }
        self.note = None;
    }

    /// A failed direct request leaves the original slash draft available for correction.
    pub fn rename_session_direct_rejected(&mut self, message: String) {
        if self.rename_direct_pending.take().is_some() {
            self.apply_intent_error(message);
        }
    }

    /// Resolve only the matching slash request, preserving edits typed while
    /// the provider was running. A completion on another tab is not applied.
    pub fn regenerated_title(&mut self, result: Result<String, String>) {
        let Some(revision) = self.regenerate_pending.take() else {
            return;
        };
        match result {
            Ok(title) => {
                self.session_title = Some(title.clone());
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.title = Some(title);
                }
                if self.input_revision == revision {
                    self.accept_intent();
                    self.input_revision += 1;
                }
                self.note = None;
            }
            Err(message) => self.apply_intent_error(message),
        }
    }

    fn paste_rename(&mut self, text: &str) -> KeyOutcome {
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        let mut clean = String::new();
        let mut clipped = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\n" | "\r" | "\t" => " ",
                _ if grapheme.chars().any(char::is_control) => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_SESSION_TITLE_BYTES {
                clipped = true;
                break;
            }
            clean.push_str(safe);
        }
        let inserted =
            self.rename_editor
                .replace(&mut self.rename_input, &clean, MAX_SESSION_TITLE_BYTES);
        KeyOutcome {
            note: (clipped || inserted < clean.len()).then(|| self.rename_limit_note()),
            ..KeyOutcome::default()
        }
    }

    fn rename_limit_note(&self) -> String {
        if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
            format!(
                "session title exceeds {MAX_SESSION_TITLE_BYTES} bytes; shorten it or select all to replace"
            )
        } else {
            format!("session title truncated at {MAX_SESSION_TITLE_BYTES} bytes")
        }
    }

    /// Report that an intent was accepted and applied; clears the input.
    pub fn accept_intent(&mut self) {
        self.input.clear();
        self.editor.clear();
    }

    /// The accepted compress turn starts streaming: status, turn, DCP panel.
    pub fn begin_compress_turn(&mut self, turn: WorkerTurnId) {
        self.live_preview_truncated = false;
        self.live_part_states.clear();
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_agent_color_index = None;
        self.compress_turn = Some(turn.clone());
        self.active_turn = Some(turn);
        self.status = TuiStatus::Streaming;
        self.panel = TuiPanel::Dcp;
        self.clear_mouse_position();
        self.input.clear();
        self.editor.clear();
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.push_note("dcp: compressing…");
    }

    /// Whether the active turn was accepted from a manual compression request.
    pub fn is_compress_turn(&self, turn: &WorkerTurnId) -> bool {
        self.compress_turn.as_ref() == Some(turn) && self.active_turn.as_ref() == Some(turn)
    }

    /// Enqueue manual compression using the same draft/receipt lifecycle as text.
    pub fn request_compress(&mut self, focus: String) -> Result<(), CoreError> {
        if self.is_busy() {
            return Err(CoreError::TurnBusy);
        }
        let session = self.session.clone().ok_or(CoreError::SessionNotFound)?;
        let receipt = self.app.request_compress(session.clone(), focus)?;
        self.begin_submission(receipt, session, false, true);
        Ok(())
    }

    fn begin_submission(
        &mut self,
        receipt: SubmissionReceipt,
        session: SessionId,
        fresh: bool,
        compress: bool,
    ) {
        self.request_id += 1;
        let agent = self.active_agent.clone();
        let agent_color_index = agent.as_deref().and_then(|agent| {
            self.agents
                .iter()
                .find(|entry| entry.id == agent)
                .map(|entry| entry.color_index)
        });
        self.pending = Some(PendingSubmission {
            request_id: self.request_id,
            generation: self.generation,
            session,
            fresh,
            draft: self.input.clone(),
            revision: self.input_revision,
            receipt,
            agent,
            agent_color_index,
            cancelling: false,
            compress,
        });
        self.status = TuiStatus::PendingSubmission;
        self.note = Some("submission pending; Esc to cancel".into());
    }

    /// Set the transient status note.
    pub fn push_note(&mut self, note: &str) {
        self.note = Some(note.to_string());
    }

    /// Push one synthetic transcript row for a non-fatal warning, so a
    /// degraded capability stays visible after the status note is replaced.
    pub fn push_warning(&mut self, warning: &str) {
        self.window
            .push_synthetic("", &format!("(warning: {warning})"), None, None);
    }

    /// Model under the picker cursor; the active variant is preserved when
    /// the cursor still points at the selected model.
    pub fn picker_selection(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        let id = picker.cursor_id()?;
        let variant = picker
            .selection()
            .filter(|selection| selection.id == id)
            .and_then(|selection| selection.variant.as_ref())
            .map(|variant| variant.name.clone());
        Some((id, variant))
    }

    /// Effective model id and variant name from the catalog snapshot (the
    /// resolved selection, never the picker cursor).
    pub fn active_model_label(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        if let crate::picker::PickerState::Retired { wanted, .. } = picker.state() {
            return Some((format!("{wanted} (unavailable)"), None));
        }
        let selection = picker.selection()?;
        Some((
            selection
                .entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&selection.id)
                .to_string(),
            selection
                .variant
                .as_ref()
                .map(|variant| variant.name.clone())
                .or_else(|| {
                    picker
                        .retired_variant()
                        .map(|name| format!("{name} (unavailable)"))
                }),
        ))
    }

    /// Provider id of the loaded catalog, if any.
    pub fn active_provider(&self) -> Option<&str> {
        let picker = self.picker.as_ref()?;
        Some(
            picker
                .selection()
                .and_then(|s| s.entry.get("provider_name"))
                .and_then(|v| v.as_str())
                .unwrap_or(picker.provider()),
        )
    }

    /// Effective agent id from the catalog snapshot, if any.
    pub fn active_agent(&self) -> Option<&str> {
        self.active_agent.as_deref()
    }

    /// True when the input names a workspace command (template expanded by
    /// the application, never by the view).
    pub fn is_workspace_command(&self, text: &str) -> bool {
        let Some(rest) = text.strip_prefix('/') else {
            return false;
        };
        let name = rest.split_whitespace().next().unwrap_or_default();
        self.commands.iter().any(|id| id == name)
    }

    /// Agent id under the Agents cursor, if any.
    pub fn selected_agent(&self) -> Option<String> {
        self.agents
            .get(self.agents_cursor)
            .map(|agent| agent.id.clone())
    }

    /// Sessions cursor position.
    pub fn sessions_cursor(&self) -> usize {
        self.sessions_cursor
    }

    // ---- key handling ---------------------------------------------------

    /// Handle one key action. The returned [`KeyOutcome`] tells the binary
    /// whether to display a note, apply an intent, or treat the input as
    /// consumed.
    pub async fn handle_key(&mut self, action: KeyAction) -> KeyOutcome {
        self.poll_submission();
        if self.panel != TuiPanel::None {
            return self.handle_panel_key(action);
        }
        if let Some(options) = self.slash_options() {
            match action {
                KeyAction::Up | KeyAction::Commands => {
                    if !options.is_empty() {
                        self.slash_selected = (self.slash_selected(options.len()) + options.len()
                            - 1)
                            % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Down => {
                    if !options.is_empty() {
                        self.slash_selected =
                            (self.slash_selected(options.len()) + 1) % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Tab => return self.select_slash(false).await,
                KeyAction::Enter => return self.select_slash(true).await,
                KeyAction::Cancel => {
                    self.slash_dismissed = Some(self.input_revision);
                    return KeyOutcome::default();
                }
                _ => {}
            }
        }
        if matches!(
            action,
            KeyAction::Cancel
                | KeyAction::Interrupt
                | KeyAction::Quit
                | KeyAction::Commands
                | KeyAction::Agents
                | KeyAction::Rename
        ) {
            self.leader = None;
        } else if let Some(start) = self.leader.take()
            && start.elapsed() < std::time::Duration::from_secs(2)
        {
            let command = if let KeyAction::Char(key) = action {
                crate::commands::REGISTRY
                    .iter()
                    .find(|c| c.shortcuts.contains(&format!("ctrl+x {key}").as_str()))
                    .map(|c| c.action.clone())
            } else {
                None
            };
            return command.map_or_else(KeyOutcome::default, |c| self.run_command(c));
        }
        match action {
            KeyAction::Commands => self.run_command(CommandAction::OpenCommands),
            KeyAction::Agents => self.run_command(CommandAction::OpenAgents),
            KeyAction::Rename => self.run_command(CommandAction::RenameSession { title: None }),
            KeyAction::Leader => {
                self.leader = Some(Instant::now());
                KeyOutcome::default()
            }
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                let right = matches!(
                    action,
                    KeyAction::Right
                        | KeyAction::WordRight
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordRight
                );
                let word = matches!(
                    action,
                    KeyAction::WordLeft
                        | KeyAction::WordRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                let select = matches!(
                    action,
                    KeyAction::SelectLeft
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                self.editor.horizontal(&self.input, right, word, select);
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::End => {
                self.editor
                    .line_edge(&self.input, action == KeyAction::End, false);
                KeyOutcome::default()
            }
            KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.editor.move_to(
                    if action == KeyAction::SelectHome {
                        0
                    } else {
                        self.input.len()
                    },
                    true,
                );
                KeyOutcome::default()
            }
            KeyAction::SelectUp | KeyAction::SelectDown => {
                self.editor
                    .vertical(&self.input, action == KeyAction::SelectDown, true);
                KeyOutcome::default()
            }
            KeyAction::PageUp | KeyAction::PageDown => KeyOutcome::default(),
            KeyAction::Char(c) => {
                if self
                    .editor
                    .replace(&mut self.input, &c.to_string(), MAX_INPUT_BYTES)
                    > 0
                {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    KeyOutcome::default()
                } else {
                    KeyOutcome {
                        note: Some(format!(
                            "input limit {MAX_INPUT_BYTES} bytes reached; the key was not added"
                        )),
                        ..KeyOutcome::default()
                    }
                }
            }
            KeyAction::Backspace => {
                if self.editor.delete(&mut self.input, true, false) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::DeleteOrQuit if self.input.is_empty() && !self.is_busy() => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Delete
            | KeyAction::DeleteOrQuit
            | KeyAction::WordBackspace
            | KeyAction::WordDelete => {
                if self.editor.delete(
                    &mut self.input,
                    action == KeyAction::WordBackspace,
                    action != KeyAction::Delete,
                ) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Newline => {
                if self.editor.replace(&mut self.input, "\n", MAX_INPUT_BYTES) > 0 {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                if self.editor.undo(&mut self.input, action == KeyAction::Redo) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Up => {
                if self.editor.vertical(&self.input, false, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(true) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(true)
            }
            KeyAction::Down => {
                if self.editor.vertical(&self.input, true, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(false) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(false)
            }
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Interrupt => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.is_busy() {
                    let session = self
                        .pending
                        .as_ref()
                        .map(|p| &p.session)
                        .or(self.session.as_ref())
                        .cloned();
                    if let Some(pending) = &mut self.pending {
                        pending.cancelling = true;
                    }
                    let Some(session) = session else {
                        return KeyOutcome {
                            note: Some("no session yet".into()),
                            ..KeyOutcome::default()
                        };
                    };
                    match self.app.cancel(session).await {
                        Ok(()) => KeyOutcome::default(),
                        Err(error) => KeyOutcome {
                            note: Some(format!("cancel: {error}")),
                            ..KeyOutcome::default()
                        },
                    }
                } else {
                    self.status = TuiStatus::Quit;
                    KeyOutcome::default()
                }
            }
            KeyAction::Enter => self.handle_enter().await,
            KeyAction::Tab => KeyOutcome::default(),
        }
    }

    fn recall_history(&mut self, previous: bool) -> bool {
        let entries: Vec<String> = self
            .window
            .rows()
            .iter()
            .filter(|row| row.role == "user")
            .map(|row| row.text.clone())
            .collect();
        let changed = self.editor.recall(&mut self.input, previous, entries);
        if changed {
            self.input_revision += 1;
        }
        changed
    }

    /// Wheel/scrollbox navigation never changes the focused editor, even
    /// when keyboard Up/Down would move its caret or recall prompt history.
    pub fn scroll_transcript(&mut self, up: bool) -> KeyOutcome {
        self.poll_submission();
        if self.panel != TuiPanel::None {
            return KeyOutcome::default();
        }
        if up {
            let max_scroll = self.max_scroll();
            self.scroll = self.display_scroll();
            if self.scroll < max_scroll {
                self.scroll += 1;
            }
            KeyOutcome {
                intent: (self.window.has_older() && self.scroll >= self.max_scroll())
                    .then_some(PanelIntent::LoadOlder),
                ..KeyOutcome::default()
            }
        } else if self.scroll > 0 {
            // Resize can clamp the displayed position below the retained
            // request. The first Down must move from that visible row.
            self.scroll = self.display_scroll().saturating_sub(1);
            KeyOutcome::default()
        } else {
            KeyOutcome {
                intent: self.window.has_newer().then_some(PanelIntent::LoadNewer),
                ..KeyOutcome::default()
            }
        }
    }

    async fn handle_enter(&mut self) -> KeyOutcome {
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.pending.is_some() {
            if let Some(action) = dispatch(self.input.trim())
                && let Some(reason) = self.command_unavailable(&action)
            {
                return KeyOutcome {
                    note: Some(reason.into()),
                    ..KeyOutcome::default()
                };
            }
            return KeyOutcome {
                note: Some("submission pending; Esc to cancel".into()),
                ..KeyOutcome::default()
            };
        }
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return KeyOutcome::default();
        }
        if let Some(action) = dispatch(&text) {
            // Workspace commands reach the application, which owns their
            // templates; the built-in table only routes known commands.
            if !matches!(action, CommandAction::Help(None)) || !self.is_workspace_command(&text) {
                if matches!(action, CommandAction::RenameSession { title: None }) {
                    if let Some(reason) = self.command_unavailable(&action) {
                        return KeyOutcome {
                            note: Some(reason.into()),
                            ..KeyOutcome::default()
                        };
                    }
                    if self.regenerate_pending.is_some() {
                        return KeyOutcome {
                            note: Some("title generation pending".into()),
                            ..KeyOutcome::default()
                        };
                    }
                    self.regenerate_pending = Some(self.input_revision);
                    return KeyOutcome {
                        intent: Some(PanelIntent::RegenerateTitle),
                        ..KeyOutcome::default()
                    };
                }
                let outcome = self.run_command(action);
                if outcome.consumed_input
                    || matches!(
                        self.panel,
                        TuiPanel::Model
                            | TuiPanel::Variant
                            | TuiPanel::Agents
                            | TuiPanel::Sessions
                            | TuiPanel::Skills
                            | TuiPanel::Commands
                            | TuiPanel::Cards
                            | TuiPanel::Help(_)
                    )
                {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                }
                return outcome;
            }
        }
        if self.active_turn.is_some() {
            return KeyOutcome {
                note: Some("turn busy".into()),
                ..KeyOutcome::default()
            };
        }
        // The application owns Home selection and validates it on acceptance;
        // `None` resolves the current Home choice without session preferences.
        let fresh = self.session.is_none();
        let session = self.session.clone().unwrap_or_else(fresh_session_id);
        let result = if fresh {
            self.app.request_submit_fresh(session.clone(), text, None)
        } else {
            self.app.request_submit(session.clone(), text)
        };
        match result {
            Ok(receipt) => {
                self.begin_submission(receipt, session, fresh, false);
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(format!("submit: {error}")),
                ..KeyOutcome::default()
            },
        }
    }

    /// Reconcile the unique acceptance receipt before applying queued turn
    /// events. Failure leaves the editable draft intact. No worker is spawned.
    pub fn poll_submission(&mut self) {
        let Some(result) = self.pending.as_mut().and_then(|p| p.receipt.try_result()) else {
            return;
        };
        let pending = self.pending.take().expect("polled receipt");
        self.reconcile_submission(pending, result, false);
    }

    /// Quit must not discard an in-flight Home root after the owner commits
    /// it. Cancel through the owner first (so tool work is stopped safely),
    /// then resolve the *same* receipt before the application shuts down.
    /// Existing-session turns already have a durable tab and need no wait.
    pub async fn reconcile_fresh_quit(&mut self) -> Result<(), CoreError> {
        let Some(pending) = self.pending.as_mut().filter(|p| p.fresh) else {
            return Ok(());
        };
        pending.cancelling = true;
        let session = pending.session.clone();
        // Cancel follows SubmitFresh in the owner's inbox. A rejected or
        // already-finished turn has nothing left to cancel.
        match self.app.cancel(session).await {
            Ok(()) | Err(CoreError::TurnNotActive) => {}
            Err(error) => return Err(error),
        }
        let result = self
            .pending
            .as_mut()
            .expect("fresh receipt still pending")
            .receipt
            .wait()
            .await;
        let pending = self.pending.take().expect("fresh receipt still pending");
        if result == Err(CoreError::Shutdown) {
            return Err(CoreError::Shutdown);
        }
        self.reconcile_submission(pending, result, true);
        Ok(())
    }

    fn reconcile_submission(
        &mut self,
        pending: PendingSubmission,
        result: Result<WorkerTurnId, CoreError>,
        exiting: bool,
    ) {
        if (self.status == TuiStatus::Quit && !exiting)
            || pending.request_id != self.request_id
            || pending.generation != self.generation
            || (pending.fresh != self.session.is_none())
            || (!pending.fresh && self.session.as_ref() != Some(&pending.session))
        {
            return;
        }
        match result {
            Ok(turn) => {
                if pending.fresh {
                    self.session = Some(pending.session);
                }
                self.live_preview_truncated = false;
                self.live_part_states.clear();
                self.live_terminal_status = None;
                self.live_model_label = None;
                self.live_agent_color_index = None;
                if !pending.compress {
                    self.home = false;
                    self.window.push_synthetic(
                        "user",
                        pending.draft.trim(),
                        pending.agent,
                        pending.agent_color_index,
                    );
                }
                self.compress_turn = pending.compress.then(|| turn.clone());
                self.live_text.clear();
                self.live_reasoning.clear();
                self.live_parts.clear();
                self.reasoning_started = None;
                self.reasoning_finished = None;
                self.turn_usage = None;
                self.active_turn = Some(turn);
                if !exiting {
                    self.status = TuiStatus::Streaming;
                }
                self.scroll = 0;
                if self.input_revision == pending.revision && !pending.cancelling {
                    self.input.clear();
                    self.editor.clear();
                }
                self.dcp.clear_notice();
                self.note = None;
            }
            Err(error) => {
                if !exiting {
                    self.status = TuiStatus::Idle;
                }
                self.note = Some(format!("submit: {error}"));
            }
        }
    }

    fn run_command(&mut self, action: CommandAction) -> KeyOutcome {
        if let Some(reason) = self.command_unavailable(&action) {
            return KeyOutcome {
                note: Some(reason.into()),
                ..KeyOutcome::default()
            };
        }
        if action == CommandAction::CloseTab {
            let (_, index, _) = self.tab_presentation();
            // The binary applies the close and replaces the view on success.
            // A refused owner action must leave the dialog and draft intact.
            return KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index }),
                ..KeyOutcome::default()
            };
        }
        if self.regenerate_pending.is_some()
            && matches!(action, CommandAction::RenameSession { title: None })
        {
            return KeyOutcome {
                note: Some("title generation pending".into()),
                ..KeyOutcome::default()
            };
        }
        if let CommandAction::RenameSession { title: Some(title) } = &action {
            if self.rename_direct_pending.is_some() {
                return KeyOutcome {
                    note: Some("session rename pending".into()),
                    ..KeyOutcome::default()
                };
            }
            let Some(title) = oc_core::core_app::normalized_session_title(title) else {
                return KeyOutcome {
                    note: Some(format!(
                        "session title must be 1–{MAX_SESSION_TITLE_BYTES} bytes of visible text"
                    )),
                    ..KeyOutcome::default()
                };
            };
            let title = title.to_string();
            self.rename_direct_pending = Some((title.clone(), self.input_revision));
            return KeyOutcome {
                intent: Some(PanelIntent::RenameSessionDirect { title }),
                ..KeyOutcome::default()
            };
        }
        self.select.reset();
        self.mouse_down = None;
        self.tab_down = None;
        self.hovered_tab.set(None);
        self.close_hold = None;
        self.last_mouse = None;
        self.leader = None;
        let mut outcome = KeyOutcome::default();
        match action {
            CommandAction::OpenCommands => {
                self.panel = TuiPanel::Commands;
            }
            CommandAction::ToggleSidebar => {
                self.chrome.sidebar_hidden = !self.chrome.sidebar_hidden;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::ToggleThinking => {
                self.thinking_expanded = !self.thinking_expanded;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::Quit => {
                self.status = TuiStatus::Quit;
                outcome.consumed_input = true;
            }
            CommandAction::OpenModelPicker => {
                self.panel = TuiPanel::Model;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenVariants => self.open_variants(),
            CommandAction::NewSession => {
                outcome.intent = Some(PanelIntent::NewSession);
            }
            CommandAction::RenameSession { title: None } => {
                self.panel = TuiPanel::Rename;
                // Generated titles contain at most 100 Unicode scalar values (<=400
                // UTF-8 bytes). Keep the whole title, even when the owner would
                // reject it as a replacement, so Enter cannot submit a prefix.
                // An unexpectedly larger title stays only in session_title, not
                // in the editor's undo history or an unbounded modal copy.
                const MAX_RENAME_PREFILL_BYTES: usize = 100 * 4;
                self.rename_input = self
                    .session_title
                    .as_ref()
                    .filter(|title| title.len() <= MAX_RENAME_PREFILL_BYTES)
                    .cloned()
                    .unwrap_or_default();
                if self
                    .session_title
                    .as_ref()
                    .is_some_and(|title| title.len() > MAX_RENAME_PREFILL_BYTES)
                {
                    outcome.note =
                        Some("existing title too long to prefill; type a replacement".into());
                } else if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    outcome.note = Some(self.rename_limit_note());
                }
                self.rename_editor.clear();
                self.rename_editor.cursor = self.rename_input.len();
                self.rename_pending = None;
            }
            CommandAction::RenameSession { title: Some(_) } => {
                unreachable!("direct rename is returned before modal reset")
            }
            CommandAction::CloseTab => unreachable!("close is returned before modal reset"),
            CommandAction::OpenAgents => {
                self.panel = TuiPanel::Agents;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenSessions => {
                self.panel = TuiPanel::Sessions;
                // Sessions can be created by the application since the last opening.
                outcome.intent = Some(PanelIntent::LoadSessions);
                outcome.consumed_input = self.sessions_loaded;
            }
            CommandAction::OpenSkills => {
                self.panel = TuiPanel::Skills;
                open_snapshot(&mut outcome, self.skills_loaded, PanelIntent::LoadSkills);
            }
            CommandAction::OpenCards => {
                self.panel = TuiPanel::Cards;
                self.card_output = None;
                self.card_scroll = 0;
                self.card_seen.set(0);
                open_snapshot(&mut outcome, self.cards_loaded, PanelIntent::LoadCards);
            }
            CommandAction::SwitchLocation { path } => {
                if path.is_empty() {
                    outcome.note = Some("usage: /location <project-path>".to_string());
                    outcome.consumed_input = true;
                } else {
                    outcome.intent = Some(PanelIntent::SwitchLocation { path });
                }
            }
            CommandAction::Help(topic) => {
                self.panel = TuiPanel::Help(topic);
                outcome.consumed_input = true;
            }
            CommandAction::DcpCompress { focus } => {
                self.panel = TuiPanel::Dcp;
                outcome.intent = Some(PanelIntent::Compress { focus });
            }
        }
        self.sync_modal_cursor();
        outcome
    }

    /// Panel navigation: Up/Down move the panel cursor, Enter chooses,
    /// Esc closes; text and paste belong to the focused modal search.
    pub fn handle_panel_key(&mut self, action: KeyAction) -> KeyOutcome {
        if self.panel == TuiPanel::Rename {
            return self.handle_rename_key(action);
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (start, height, count) = crate::views::card_window(self);
            self.card_scroll = start;
            match action {
                KeyAction::Up => self.card_scroll = start.saturating_sub(1),
                KeyAction::Down => self.card_scroll = (start + 1).min(count.saturating_sub(height)),
                KeyAction::PageUp => self.card_scroll = start.saturating_sub(height),
                KeyAction::PageDown => {
                    self.card_scroll = (start + height).min(count.saturating_sub(height))
                }
                KeyAction::Home => self.card_scroll = 0,
                KeyAction::End => self.card_scroll = count.saturating_sub(height),
                KeyAction::Enter
                    if height > 0 && start + height >= count && self.card_seen.get() >= count =>
                {
                    return self.panel_enter();
                }
                KeyAction::Cancel => {
                    self.card_output = None;
                    self.card_scroll = 0;
                    self.select.reset();
                }
                KeyAction::Quit | KeyAction::Interrupt => self.status = TuiStatus::Quit,
                _ => {}
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Char(c) => {
                if self.select.query.len() + c.len_utf8() <= 512 {
                    self.select.query.push(c);
                }
                self.changed_modal_query();
                return KeyOutcome::default();
            }
            KeyAction::Backspace => {
                self.select.query.pop();
                self.changed_modal_query();
                return KeyOutcome::default();
            }
            KeyAction::Interrupt
                if matches!(
                    self.panel,
                    TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                ) =>
            {
                // Existing informational-panel shutdown binding (including
                // the post-compression DCP panel); Select dialogs retain their
                // own clear-filter/dismiss behavior.
                self.status = TuiStatus::Quit;
                return KeyOutcome::default();
            }
            KeyAction::Interrupt => {
                if self.select.query.is_empty() {
                    self.close_panel();
                } else {
                    self.select.reset();
                    self.changed_modal_query();
                }
                return KeyOutcome::default();
            }
            KeyAction::Commands
            | KeyAction::Up
            | KeyAction::Down
            | KeyAction::PageUp
            | KeyAction::PageDown
            | KeyAction::Home
            | KeyAction::End
                if matches!(
                    self.panel,
                    TuiPanel::Commands
                        | TuiPanel::Model
                        | TuiPanel::Variant
                        | TuiPanel::Agents
                        | TuiPanel::Sessions
                        | TuiPanel::Skills
                ) =>
            {
                let count = self.modal_options().len();
                match action {
                    KeyAction::Home => self.select.cursor = 0,
                    KeyAction::End => self.select.cursor = count.saturating_sub(1),
                    _ => self.select.move_by(
                        match action {
                            KeyAction::Commands | KeyAction::Up => -1,
                            KeyAction::PageUp => -10,
                            KeyAction::PageDown => 10,
                            _ => 1,
                        },
                        count,
                    ),
                }
                self.sync_modal_cursor();
                self.select.follow_selection();
                return KeyOutcome::default();
            }
            KeyAction::Enter if self.modal_options().is_empty() => return KeyOutcome::default(),
            _ => {}
        }
        match action {
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.panel == TuiPanel::Cards && self.card_output.is_some() {
                    self.card_output = None;
                    self.select.reset();
                } else {
                    self.close_panel();
                }
                KeyOutcome::default()
            }
            KeyAction::Left | KeyAction::Right => KeyOutcome::default(),
            KeyAction::Up => {
                self.move_panel_cursor(-1);
                let intent = (self.panel == TuiPanel::Cards
                    && self.cards_has_older
                    && self.cards_cursor == 0)
                    .then_some(PanelIntent::LoadCards);
                KeyOutcome {
                    intent,
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Down => {
                self.move_panel_cursor(1);
                KeyOutcome::default()
            }
            KeyAction::Enter => self.panel_enter(),
            _ => KeyOutcome::default(),
        }
    }

    fn handle_rename_key(&mut self, action: KeyAction) -> KeyOutcome {
        if action == KeyAction::Cancel || action == KeyAction::Interrupt {
            if self.rename_pending.is_none() {
                self.close_panel();
            }
            return KeyOutcome::default();
        }
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Enter => {
                if let Some(reason) =
                    self.command_unavailable(&CommandAction::RenameSession { title: None })
                {
                    return KeyOutcome {
                        note: Some(reason.into()),
                        ..KeyOutcome::default()
                    };
                }
                if self.rename_input.trim().is_empty() {
                    return KeyOutcome::default();
                }
                if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    if self.session_title.as_deref() == Some(self.rename_input.as_str()) {
                        self.close_panel();
                        return KeyOutcome::default();
                    }
                    return KeyOutcome {
                        note: Some(self.rename_limit_note()),
                        ..KeyOutcome::default()
                    };
                }
                let title = self.rename_input.trim().to_string();
                self.rename_pending = Some(title.clone());
                KeyOutcome {
                    intent: Some(PanelIntent::RenameSession { title }),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Char(c) if !c.is_control() => {
                let inserted = self.rename_editor.replace(
                    &mut self.rename_input,
                    &c.to_string(),
                    MAX_SESSION_TITLE_BYTES,
                );
                KeyOutcome {
                    note: (inserted == 0).then(|| self.rename_limit_note()),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Backspace | KeyAction::WordBackspace => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    true,
                    action == KeyAction::WordBackspace,
                );
                KeyOutcome::default()
            }
            KeyAction::Delete | KeyAction::DeleteOrQuit | KeyAction::WordDelete => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    false,
                    action == KeyAction::WordDelete,
                );
                KeyOutcome::default()
            }
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                self.rename_editor.horizontal(
                    &self.rename_input,
                    matches!(
                        action,
                        KeyAction::Right
                            | KeyAction::WordRight
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::WordLeft
                            | KeyAction::WordRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::SelectLeft
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                );
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::End | KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.rename_editor.move_to(
                    if matches!(action, KeyAction::End | KeyAction::SelectEnd) {
                        self.rename_input.len()
                    } else {
                        0
                    },
                    matches!(action, KeyAction::SelectHome | KeyAction::SelectEnd),
                );
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                self.rename_editor
                    .undo(&mut self.rename_input, action == KeyAction::Redo);
                KeyOutcome::default()
            }
            _ => KeyOutcome::default(),
        }
    }

    fn move_panel_cursor(&mut self, delta: isize) {
        match self.panel {
            TuiPanel::Model => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.move_cursor(delta);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = clamp_cursor(self.agents_cursor, delta, self.agents.len());
            }
            TuiPanel::Sessions => {
                self.sessions_cursor =
                    clamp_cursor(self.sessions_cursor, delta, self.sessions.len());
            }
            TuiPanel::Skills => {
                self.skills_cursor = clamp_cursor(self.skills_cursor, delta, self.skills.len());
            }
            TuiPanel::Cards => {
                self.cards_cursor = clamp_cursor(self.cards_cursor, delta, self.cards.len());
            }
            _ => {}
        }
    }

    fn panel_enter(&mut self) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        match self.panel.clone() {
            TuiPanel::Commands => {
                let options = self.modal_options();
                if let Some(option) = options.get(self.select.cursor)
                    && let Some(command) = crate::commands::REGISTRY
                        .iter()
                        .find(|c| c.id == option.value)
                {
                    return self.run_command(command.action.clone());
                }
            }
            TuiPanel::Rename => return self.handle_rename_key(KeyAction::Enter),
            TuiPanel::Model if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenModelPicker)
                    .map(str::to_string)
            }
            TuiPanel::Variant if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenVariants)
                    .map(str::to_string)
            }
            TuiPanel::Agents if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenAgents)
                    .map(str::to_string)
            }
            TuiPanel::Sessions if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenSessions)
                    .map(str::to_string)
            }
            TuiPanel::Model => match self.picker_selection() {
                Some((id, _)) => {
                    outcome.intent = Some(PanelIntent::SelectModel { id });
                }
                None => outcome.note = Some("no model selected".to_string()),
            },
            TuiPanel::Variant => {
                if let Some(option) = self.modal_options().get(self.select.cursor)
                    && let Some(selection) = self.picker.as_ref().and_then(|p| p.selection())
                {
                    outcome.intent = Some(PanelIntent::ChooseModel {
                        id: selection.id.clone(),
                        variant: (option.value != "default").then(|| option.value.clone()),
                    });
                }
            }
            TuiPanel::Agents => match self.selected_agent() {
                Some(id) => outcome.intent = Some(PanelIntent::SelectAgent { id }),
                None => outcome.note = Some("no agent selected".to_string()),
            },
            TuiPanel::Sessions => match self.sessions.get(self.sessions_cursor) {
                Some(id) if SessionId::new(id.clone()).is_some() => {
                    outcome.intent = Some(PanelIntent::SwitchSession { id: id.clone() });
                }
                Some(_) => outcome.note = Some("bad session id".to_string()),
                None => outcome.note = Some("empty session list".to_string()),
            },
            TuiPanel::Skills => {
                self.panel = TuiPanel::None;
            }
            TuiPanel::Cards => {
                if self.session.is_none() {
                    outcome.note = Some("no session yet".into());
                    return outcome;
                }
                if let Some(detail) = &self.card_output {
                    if let Some(offset) = detail.page.next_offset {
                        outcome.intent = Some(PanelIntent::LoadCardOutput {
                            op: detail.op.clone(),
                            offset: offset as usize,
                        });
                    } else {
                        self.card_output = None;
                    }
                } else if let Some(op) = self.card_ops.get(self.cards_cursor) {
                    outcome.intent = Some(PanelIntent::LoadCardOutput {
                        op: op.clone(),
                        offset: 0,
                    });
                }
            }
            TuiPanel::Dcp => {
                if self.session.is_some() {
                    outcome.intent = Some(PanelIntent::Compress {
                        focus: String::new(),
                    });
                } else {
                    outcome.note = Some("no session yet".into());
                }
            }
            TuiPanel::Help(_) | TuiPanel::None => {
                self.panel = TuiPanel::None;
            }
        }
        outcome
    }

    // ---- worker events --------------------------------------------------

    /// Apply a worker text delta to the live line (turn-scoped: deltas for
    /// a stale turn are ignored, so a late event can never corrupt the view).
    pub fn apply_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        if self.live_text.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_text.len();
            self.live_text.push_str(crate::truncate_utf8(delta, room));
        }
    }

    /// Apply a worker reasoning delta to the live reasoning block
    /// (turn-scoped, bounded, never persisted).
    pub fn apply_reasoning_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(Instant::now());
        }
        if self.live_reasoning.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_reasoning.len();
            self.live_reasoning
                .push_str(crate::truncate_utf8(delta, room));
        }
    }

    /// Adopt explicit checkpoint identities and pinned presentation only for
    /// the current generation/turn. Deltas remain transient until checkpointed.
    pub fn apply_presentation(
        &mut self,
        turn: &WorkerTurnId,
        projection: &oc_core::queries::HistoryTurn,
    ) {
        if self.active_turn.as_ref() != Some(turn) || projection.id != turn.0 {
            return;
        }
        self.live_part_states = projection.part_states.clone();
        self.live_preview_truncated |= projection.truncated;
        self.live_agent_color_index = projection.agent_color_index;
        self.live_terminal_status = Some(projection.status.clone());
        self.live_model_label =
            (!projection.model_label.is_empty()).then(|| projection.model_label.clone());
    }

    /// Apply provider-reported usage for the active turn; without it the
    /// footer omits `tok/s` instead of inventing a rate.
    pub fn apply_usage(
        &mut self,
        turn: &WorkerTurnId,
        input_tokens: u64,
        output_tokens: u64,
        streamed_ms: u64,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.turn_usage = Some(TurnUsage {
            input_tokens,
            output_tokens,
            streamed_ms,
        });
    }

    /// Apply a worker turn-finished event: commit the live answer with its
    /// reasoning block and footer metadata, then release the turn. When tool
    /// cards or frozen segments exist, every part keeps its upstream position
    /// and the footer becomes its own row after them; the committed text is
    /// the concatenation of the frozen segments (the deltas already shown).
    pub fn apply_finished(&mut self, turn: &WorkerTurnId, text: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let meta = self.finish_meta(false, duration_ms);
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: text.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
            });
            return;
        }
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
    }

    /// Apply a worker turn-interrupted event: keep the partial text (never
    /// committed to storage), mark the footer `interrupted`, and release the
    /// turn.
    pub fn apply_interrupted(&mut self, turn: &WorkerTurnId, partial: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let meta = self.finish_meta(true, duration_ms);
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Cancelled;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                role: "assistant".to_string(),
                text: partial.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
            });
            return;
        }
        // The partial text is already the frozen trailing segment.
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
    }

    /// Release a failed turn and show its error, never a successful answer.
    pub fn apply_failed(&mut self, turn: &WorkerTurnId, error: &CoreError) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let mut meta = self.finish_meta(false, 0);
        if meta.status.is_none() {
            meta.status = Some("failed".into());
        }
        let reasoning = self.take_reasoning();
        let parts = self.commit_live_parts(reasoning);
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if !parts.is_empty() {
            // Cards already shown stay visible; the error follows them.
            self.push_committed_parts(parts);
        }
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.window
            .push_synthetic("", &format!("(error: {error})"), None, None);
    }

    /// Freeze the open live segments and return the whole part list in
    /// arrival order, including reasoning after an earlier tool round.
    fn commit_live_parts(&mut self, reasoning: Option<ReasoningBlock>) -> Vec<LivePart> {
        if let Some(reasoning) = reasoning {
            self.live_parts.push(LivePart::Reasoning {
                text: reasoning.text,
                duration_ms: reasoning.duration_ms,
            });
        }
        self.freeze_reasoning();
        self.freeze_text();
        self.live_reasoning.clear();
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        std::mem::take(&mut self.live_parts)
    }

    /// Push committed part rows into the window (bounded like any row).
    fn push_committed_parts(&mut self, parts: Vec<LivePart>) {
        for part in parts {
            self.window.push_row(part.to_row(self.active_agent.clone()));
        }
    }

    /// Freeze the open text segment (a tool call follows it).
    fn freeze_text(&mut self) {
        if !self.live_text.is_empty() {
            self.live_parts
                .push(LivePart::Text(std::mem::take(&mut self.live_text)));
        }
    }

    /// Freeze the open reasoning segment with its measured window.
    fn freeze_reasoning(&mut self) {
        if self.live_reasoning.is_empty() {
            return;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            (Some(started), None) => Some(started.elapsed().as_millis() as u64),
            (None, _) => None,
        };
        self.live_parts.push(LivePart::Reasoning {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
        });
    }

    /// Apply a recorded tool-call intent: freeze the open segments, then
    /// append the running card in upstream part order.
    pub fn apply_tool_started(&mut self, turn: &WorkerTurnId, op: &str, name: &str, input: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.freeze_reasoning();
        self.freeze_text();
        let card = card_from_row(&ToolOpView {
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: "started".to_string(),
            input: Some(input.to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
        });
        self.live_parts.push(LivePart::Tool {
            card: Box::new(card),
            input: input.to_string(),
        });
        self.enforce_parts();
    }

    /// Apply a recorded tool-call outcome: rebuild the matching card from the
    /// stored input plus the outcome, then drop the transient input.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let outcome = ToolOpView {
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: None,
            output: Some(output.to_string()),
            output_bytes,
            output_truncated,
        };
        if let Some(LivePart::Tool { card, input }) = self
            .live_parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            let mut row = outcome;
            row.name = card.name.clone();
            row.input = Some(std::mem::take(input));
            **card = card_from_row(&row);
            return;
        }
        // The intent event was not observed (e.g. a late subscription): the
        // card appears with the outcome only, never with an invented input.
        self.live_parts.push(LivePart::Tool {
            card: Box::new(card_from_row(&outcome)),
            input: String::new(),
        });
        self.enforce_parts();
    }

    /// Evict oldest live parts while the count or byte cap is exceeded; the
    /// parts are transient (a reload restores committed history).
    fn enforce_parts(&mut self) {
        while self.live_parts.len() > LIVE_PARTS_MAX
            || self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
                > WINDOW_BYTES
        {
            self.live_parts.remove(0);
            self.live_preview_truncated = true;
        }
    }

    /// Footer metadata for the finished turn from real state: the effective
    /// model label, the measured turn duration, provider usage and the
    /// interrupt marker. Absent data stays `None` (the footer omits it).
    fn finish_meta(&mut self, interrupted: bool, duration_ms: u64) -> AssistantMeta {
        let model = self.live_model_label.take().or_else(|| {
            self.picker.as_ref().and_then(|picker| {
                let selection = picker.selection()?;
                Some(
                    selection
                        .entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&selection.id)
                        .to_string(),
                )
            })
        });
        let usage = self.turn_usage.take();
        AssistantMeta {
            model,
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            input_tokens: usage.map(|usage| usage.input_tokens),
            output_tokens: usage.map(|usage| usage.output_tokens),
            context_usage: None,
            streamed_ms: usage.map(|usage| usage.streamed_ms),
            interrupted,
            status: self.live_terminal_status.take(),
            agent_color_index: self.live_agent_color_index,
        }
    }

    /// Completed reasoning block for the turn, with its measured duration
    /// (`part.time.completed - part.time.created` when text followed, else the
    /// elapsed reasoning window).
    fn take_reasoning(&mut self) -> Option<ReasoningBlock> {
        if self.live_reasoning.is_empty() {
            return None;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            (Some(started), None) => Some(started.elapsed().as_millis() as u64),
            (None, _) => None,
        };
        Some(ReasoningBlock {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
            running: false,
            expanded: false,
        })
    }

    fn line_count(&self) -> usize {
        self.transcript_lines(0, u16::MAX).len()
    }

    fn max_scroll(&self) -> usize {
        self.viewport.get().map_or_else(
            || self.line_count().saturating_sub(VIEWPORT_LINES),
            |view| view.total.saturating_sub(view.height as usize),
        )
    }

    fn scroll_for_current_view(&self) -> usize {
        if self
            .viewport
            .get()
            .is_some_and(|view| view.requested_scroll == self.scroll)
        {
            self.display_scroll()
        } else {
            self.scroll
        }
    }

    /// Actual rendered geometry for input-driven row scrolling.
    pub fn observe_viewport(&self, height: u16, rendered_rows: usize) {
        self.observe_transcript_viewport(0, 0, height, rendered_rows, self.scroll);
    }

    pub(crate) fn observe_transcript_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
        total: usize,
        displayed_scroll: usize,
    ) {
        self.viewport.set(Some(TranscriptViewport {
            width,
            terminal_width,
            height,
            total,
            requested_scroll: self.scroll,
            displayed_scroll: displayed_scroll.min(total.saturating_sub(height as usize)),
        }));
    }

    /// Latest measured context, never DCP's estimate or a renderer constant.
    pub fn context_usage(&self) -> Option<(u64, Option<u64>)> {
        let rows = self.transcript_rows();
        let usage = self
            .turn_usage
            .as_ref()
            .map(|u| (u.input_tokens, u.output_tokens))
            .or_else(|| {
                rows.iter().rev().find_map(|r| {
                    let m = r.meta.as_ref()?;
                    m.context_usage
                        .or_else(|| Some((m.input_tokens?, m.output_tokens?)))
                })
            })?;
        let limit = self
            .picker
            .as_ref()
            .and_then(|p| p.selection())
            .and_then(|s| s.entry.pointer("/limit/context"))
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0);
        Some((usage.0.saturating_add(usage.1), limit))
    }
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

/// Mark a command as fully handled, or request its snapshot when the view
/// has never loaded one.
fn open_snapshot(outcome: &mut KeyOutcome, loaded: bool, intent: PanelIntent) {
    if loaded {
        outcome.consumed_input = true;
    } else {
        outcome.intent = Some(intent);
    }
}

/// One bounded render row for a tool card.
fn card_row(card: &ToolCard) -> HistoryRow {
    // `apply_patch` shows a bounded diff (touched files with +/- counts and
    // hunk counts) instead of the raw patch bytes; other tools keep the
    // parsed path list. Never a second copy of a large payload.
    let files = match &card.diff {
        Some(diff) => {
            let listed = diff
                .files
                .iter()
                .take(crate::history::CARD_FILES)
                .map(|file| {
                    let marker = match file.change {
                        "Add" => "+",
                        "Delete" => "-",
                        _ => "~",
                    };
                    let mut text = format!(
                        "{marker}{} +{} -{}",
                        file.path, file.additions, file.removals
                    );
                    if file.hunks > 0 {
                        text.push_str(&format!(" ({}h)", file.hunks));
                    }
                    if let Some(target) = &file.move_to {
                        text.push_str(&format!(" -> {target}"));
                    }
                    text
                })
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if diff.truncated || diff.files.len() > crate::history::CARD_FILES {
                ", …"
            } else {
                ""
            };
            format!(
                " [{listed}{suffix}] diff {}f +{} -{}",
                diff.files.len(),
                diff.additions,
                diff.removals
            )
        }
        None if card.files.is_empty() => String::new(),
        None => {
            let suffix = if card.files_truncated { ", …" } else { "" };
            format!(" [{}{suffix}]", card.files.join(", "))
        }
    };
    let output = if card.output_preview.is_empty() {
        String::new()
    } else if card.output_truncated {
        format!(
            " -> {}…[+{} bytes stored]",
            card.output_preview,
            card.output_bytes.max(0)
        )
    } else {
        format!(" -> {}", card.output_preview)
    };
    // Diff first: a long operation id must never push the diff off a narrow
    // panel row.
    HistoryRow {
        seq: i64::MAX,
        role: String::new(),
        text: format!(
            "{} {}{}{} ({})",
            card.name, card.state, files, output, card.op
        ),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        tool: None,
    }
}

/// Assistant footer row: no text, no reasoning, footer metadata only; the
/// upstream footer follows every part of the assistant message
/// (`routes/session/index.tsx:1934-1985`).
fn footer_row(agent: Option<String>, meta: AssistantMeta) -> HistoryRow {
    HistoryRow {
        seq: i64::MAX,
        role: "assistant".to_string(),
        text: String::new(),
        agent,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: Some(meta),
        tool: None,
    }
}

fn clamp_cursor(cursor: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = cursor as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

/// Rebuild the adapter catalog view from the application snapshot so the
/// picker keeps its exact-id/variant validation over fresh discovery data.
fn catalog_from_snapshot(snapshot: &CatalogSnapshot) -> ModelCatalog {
    let mut models = BTreeMap::new();
    for entry in &snapshot.models {
        let variants: serde_json::Map<String, serde_json::Value> = entry
            .variants
            .iter()
            .map(|variant| {
                let mut value = serde_json::Map::new();
                if variant.disabled {
                    value.insert("disabled".to_string(), serde_json::json!(true));
                }
                if let Some(effort) = &variant.reasoning_effort {
                    value.insert("reasoningEffort".to_string(), serde_json::json!(effort));
                }
                let value = serde_json::Value::Object(value);
                (variant.name.clone(), value)
            })
            .collect();
        models.insert(
            entry.id.clone(),
            serde_json::json!({
                "name": if entry.display_name.is_empty() { &entry.id } else { &entry.display_name },
                "provider_name": if entry.provider_name.is_empty() { &snapshot.provider } else { &entry.provider_name },
                "cost": entry.price.as_ref().map(|p|serde_json::json!({"input":p.input,"output":p.output})),
                "limit": { "context": entry.context_known.then_some(entry.context), "output": entry.output_known.then_some(entry.output) },
                "variants": variants,
            }),
        );
    }
    ModelCatalog {
        provider: snapshot.provider.clone(),
        models,
    }
}

/// Scripted driver used by tests: holds one broadcast subscription like the
/// real binary event loop and applies worker events in order.
pub struct ScriptDriver {
    rx: tokio::sync::broadcast::Receiver<CoreEvent>,
}

impl ScriptDriver {
    /// Attach to the same handle the `TuiState` uses.
    pub fn attach(app: &CoreApp) -> Self {
        Self {
            rx: app.subscribe(),
        }
    }

    /// Pump worker events into `state` until idle (no active turn) or
    /// timeout. Returns terminal text or interrupt marker.
    pub async fn pump_until_idle(
        &mut self,
        state: &mut TuiState,
        timeout: Duration,
    ) -> PumpOutcome {
        loop {
            if !state.is_busy() && state.status != TuiStatus::Streaming {
                return PumpOutcome::Idle;
            }
            let event = tokio::time::timeout(timeout, self.rx.recv()).await;
            state.poll_submission();
            match event {
                Err(_) => return PumpOutcome::Timeout,
                Ok(Err(_)) => return PumpOutcome::Closed,
                Ok(Ok(CoreEvent::TurnStarted { .. })) => {}
                Ok(Ok(CoreEvent::TurnPresentation {
                    turn, projection, ..
                })) => state.apply_presentation(&turn, &projection),
                Ok(Ok(CoreEvent::TurnFailed { turn, error, .. })) => {
                    state.apply_failed(&turn, &error);
                    return PumpOutcome::Closed;
                }
                Ok(Ok(CoreEvent::TextDelta { turn, delta, .. })) => {
                    state.apply_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ReasoningDelta { turn, delta, .. })) => {
                    state.apply_reasoning_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ToolCallStarted {
                    turn,
                    op,
                    name,
                    input,
                    ..
                })) => {
                    state.apply_tool_started(&turn, &op, &name, &input);
                }
                Ok(Ok(CoreEvent::ToolCallFinished {
                    turn,
                    op,
                    name,
                    state: tool_state,
                    output,
                    output_bytes,
                    output_truncated,
                    ..
                })) => {
                    state.apply_tool_finished(
                        &turn,
                        &op,
                        &name,
                        &tool_state,
                        &output,
                        output_bytes,
                        output_truncated,
                    );
                }
                Ok(Ok(CoreEvent::TurnUsage {
                    turn,
                    input_tokens,
                    output_tokens,
                    streamed_ms,
                    ..
                })) => {
                    state.apply_usage(&turn, input_tokens, output_tokens, streamed_ms);
                }
                Ok(Ok(CoreEvent::TurnFinished {
                    turn,
                    text,
                    duration_ms,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_finished(&turn, &text, duration_ms);
                        return PumpOutcome::Finished(text);
                    }
                }
                Ok(Ok(CoreEvent::TurnInterrupted {
                    turn,
                    partial,
                    duration_ms,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_interrupted(&turn, &partial, duration_ms);
                        return PumpOutcome::Interrupted(partial);
                    }
                }
            }
        }
    }
}

/// Terminal pump result.
#[derive(Debug, PartialEq, Eq)]
pub enum PumpOutcome {
    /// Turn finished with full text.
    Finished(String),
    /// Turn interrupted with partial text.
    Interrupted(String),
    /// Already idle (no active turn).
    Idle,
    /// Timed out waiting for events.
    Timeout,
    /// Channel closed.
    Closed,
}

#[cfg(test)]
mod tests {
    use super::{
        HOME_EXAMPLES, KeyOutcome, LIVE_PARTS_MAX, MAX_INPUT_BYTES, MAX_SESSION_TITLE_BYTES,
        PanelIntent, PumpOutcome, ScriptDriver, TabCloseHold, TabPresentation, TuiPanel, TuiState,
        TuiStatus, VIEWPORT_LINES,
    };
    use crate::events::KeyAction;
    use crate::history::{WINDOW_BYTES, WINDOW_ROWS};
    use oc_core::core_app::{CoreApp, CoreEvent, MockProvider, WorkerTurnId};
    use oc_core::domain::SessionId;
    use oc_core::queries::{
        AgentEntry, CatalogSnapshot, HistoryMessage, HistoryPage, ModelEntry, SkillCard,
        VariantEntry,
    };
    use oc_core::session::{CoreError, Role};
    use std::time::{Duration, Instant};

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("id")
    }

    fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            turn: None,
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>, total: usize, older: bool, newer: bool) -> HistoryPage {
        HistoryPage {
            parent_id: None,
            title: None,
            rows,
            total,
            has_older: older,
            has_newer: newer,
        }
    }

    async fn fresh_state(name: &str) -> TuiState {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid(name)).await.expect("create");
        TuiState::new(app, sid(name))
    }

    #[tokio::test]
    async fn vis25_slash_focus_navigation_completion_and_real_actions() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut state = fresh_state("slash-focus").await;
        let mut catalog = snapshot();
        catalog.commands = vec!["project-check".into()];
        state.apply_catalog(catalog);
        state.handle_paste("/side");
        assert_eq!(state.slash_options().unwrap()[0].name, "sidebar");
        assert_eq!(state.handle_key(KeyAction::Enter).await.note, None);
        assert!(
            state.chrome.sidebar_hidden,
            "selected built-in ran its real action"
        );
        assert_eq!(state.input(), "");

        state.handle_paste("/ne");
        assert_eq!(state.slash_options().unwrap()[0].name, "new");
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::NewSession)
        );
        assert_eq!(
            state.input(),
            "/new",
            "owner refusal must retain the selected command"
        );
        state.editor.clear();
        state.input.clear();

        state.handle_paste("/project");
        assert_eq!(state.slash_options().unwrap()[0].name, "project-check");
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.input(), "/project-check ");
        assert!(state.slash_options().is_none());
        assert!(state.is_workspace_command(state.input()));
        assert_eq!(state.editor.cursor, state.input().len());

        state.editor.clear();
        state.input.clear();
        state.handle_paste("/ren");
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), "/rename ");
        state.handle_paste("title");
        assert!(state.slash_options().is_none());
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::RenameSessionDirect {
                title: "title".into()
            })
        );

        state.editor.clear();
        state.input.clear();
        state.handle_paste("/");
        let options = state.slash_options().unwrap();
        assert!(options.len() > 1);
        assert_eq!(state.slash_selected, 0);
        state.handle_key(KeyAction::Down).await;
        assert_eq!(state.slash_selected, 1);
        let prev = crate::events::map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
            .unwrap();
        state.handle_key(prev).await;
        assert_eq!(state.slash_selected, 0);
        let next = crate::events::map_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
            .unwrap();
        state.handle_key(next).await;
        assert_eq!(state.slash_selected, 1);
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.slash_selected, 0);
        state.handle_key(KeyAction::Cancel).await;
        assert_eq!(state.input(), "/");
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(state.slash_options().is_none());
        state.handle_key(KeyAction::Char('x')).await;
        assert!(
            state.slash_options().is_some(),
            "editing reopens the overlay"
        );
        state.handle_key(KeyAction::Char(' ')).await;
        assert!(state.slash_options().is_none());
        state.handle_key(KeyAction::Backspace).await;
        assert!(state.slash_options().is_some());
        state.reset_workspace();
        assert!(state.commands.is_empty(), "old Location commands are gone");
    }

    #[tokio::test]
    async fn vis25_home_no_match_and_history_keep_editor_ownership() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        let mut state = TuiState::new_home(app);
        state.handle_paste("/zzzznotacommand");
        assert!(state.slash_options().unwrap().is_empty());
        assert_eq!(
            state.handle_key(KeyAction::Enter).await,
            KeyOutcome::default()
        );
        assert_eq!(
            state.input(),
            "/zzzznotacommand",
            "no-match Enter selects nothing"
        );
        assert_eq!(state.panel(), &TuiPanel::None);
        state.handle_key(KeyAction::Cancel).await;
        assert_eq!(state.input(), "/zzzznotacommand");
        state.editor.clear();
        state.input.clear();
        state.handle_paste("/sessions");
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::LoadSessions)
        );
        assert_eq!(state.panel(), &TuiPanel::Sessions);
        assert_eq!(state.input(), "");
        state.handle_key(KeyAction::Cancel).await;
        assert_eq!(state.panel(), &TuiPanel::None);
        state.handle_paste("/cards");
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.note.as_deref(),
            Some("no session yet")
        );
        assert_eq!(
            state.input(),
            "/cards",
            "unavailable action keeps the draft"
        );
        state.handle_key(KeyAction::SelectHome).await;
        state.handle_paste("/location elsewhere");
        assert!(state.slash_options().is_none());
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::SwitchLocation {
                path: "elsewhere".into()
            })
        );
        assert_eq!(
            state.input(),
            "/location elsewhere",
            "intent awaits owner ACK"
        );
    }

    #[tokio::test]
    async fn vis25_up_owns_selection_then_history_recovers_after_escape() {
        let mut state = fresh_state("slash-history").await;
        state.attach_page(&page(
            vec![msg(1, Role::User, "older prompt")],
            1,
            false,
            false,
        ));
        state.handle_paste("/");
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.input(), "/");
        assert!(state.slash_selected > 0);
        state.handle_key(KeyAction::Cancel).await;
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.input(), "older prompt");
        state.handle_key(KeyAction::Down).await;
        assert_eq!(state.input(), "/", "history returns to saved draft");
    }

    #[tokio::test]
    async fn vis25_selection_matches_highlight_after_caret_motion_and_catalog_refresh() {
        let mut state = fresh_state("slash-stale-selection").await;
        state.handle_paste("/sidebar");
        state.handle_key(KeyAction::Home).await;
        state.handle_key(KeyAction::Right).await; // The caret is just after `/`.
        state.handle_key(KeyAction::Up).await; // Last row in the unfiltered list.
        let old = state.slash_selected;
        state.handle_key(KeyAction::End).await;
        assert!(old >= state.slash_options().unwrap().len());
        assert_eq!(state.slash_options().unwrap().len(), 1);
        assert_eq!(
            state.slash_options().unwrap()[state.slash_selected(1)].name,
            "sidebar"
        );
        assert_eq!(
            state.handle_key(KeyAction::Enter).await,
            KeyOutcome {
                consumed_input: true,
                ..KeyOutcome::default()
            }
        );
        assert!(
            state.chrome.sidebar_hidden,
            "Enter activates the highlighted row"
        );
        assert_eq!(state.input(), "");

        let mut catalog = snapshot();
        catalog.commands = vec!["zzzz-project".into()];
        state.apply_catalog(catalog);
        state.handle_paste("/");
        state.handle_key(KeyAction::Up).await;
        assert_eq!(
            state.slash_options().unwrap()[state.slash_selected].name,
            "zzzz-project"
        );
        state.apply_catalog(snapshot()); // This generation no longer has that command.
        let options = state.slash_options().unwrap();
        assert!(state.slash_selected >= options.len());
        let highlighted = options[state.slash_selected(options.len())].name.clone();
        state.handle_key(KeyAction::Tab).await;
        assert_eq!(state.input(), format!("/{highlighted} "));
    }

    #[tokio::test]
    async fn sidebar_palette_title_tracks_rendered_visibility_and_runs_same_action() {
        use crate::commands::CommandAction;
        use ratatui::{Terminal, backend::TestBackend};

        let mut state = fresh_state("sidebar-palette").await;
        let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("sidebar");
        let options = state.modal_options();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "session.sidebar.toggle");
        assert_eq!(options[0].title, "Hide sidebar");
        assert_eq!(options[0].footer, "ctrl+x b");
        assert_eq!(
            state.command_unavailable(&CommandAction::ToggleSidebar),
            None
        );
        assert!(state.handle_panel_key(KeyAction::Enter).consumed_input);
        assert!(state.chrome.sidebar_hidden);
        assert_eq!(state.panel(), &TuiPanel::None);

        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("sidebar");
        let options = state.modal_options();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].title, "Show sidebar");
        assert_eq!(options[0].value, "session.sidebar.toggle");
        assert!(state.handle_panel_key(KeyAction::Enter).consumed_input);
        assert!(!state.chrome.sidebar_hidden);
    }

    #[tokio::test]
    async fn sidebar_palette_uses_effective_width_home_and_child() {
        use ratatui::{Terminal, backend::TestBackend};

        let mut state = fresh_state("sidebar-breakpoint").await;
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("sidebar");
        let title = |state: &TuiState| {
            state
                .modal_options()
                .iter()
                .find(|o| o.value == "session.sidebar.toggle")
                .expect("sidebar palette option")
                .title
                .clone()
        };
        assert_eq!(title(&state), "Show sidebar", "no width observed yet");
        for (width, rail, expected) in [
            (120, 0, "Show sidebar"),
            (121, 0, "Hide sidebar"),
            (160, 40, "Show sidebar"),
            (161, 40, "Hide sidebar"),
        ] {
            state.chrome.vertical_tabs_width = rail;
            let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
            terminal
                .draw(|frame| crate::shell::render(frame, &state))
                .unwrap();
            assert_eq!(
                title(&state),
                expected,
                "terminal width {width}, rail {rail}"
            );
        }
        state.parent_id = Some("parent".into());
        state.chrome.vertical_tabs_width = 0;
        let mut wide = Terminal::new(TestBackend::new(160, 40)).unwrap();
        wide.draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        assert_eq!(title(&state), "Show sidebar");

        let (app, _, _) = CoreApp::channel(4);
        let mut home = TuiState::new_home(app);
        wide.draw(|frame| crate::shell::render(frame, &home))
            .unwrap();
        home.handle_key(KeyAction::Commands).await;
        home.handle_paste("sidebar");
        assert_eq!(title(&home), "Show sidebar");
    }

    #[tokio::test]
    async fn rename_shortcut_edits_graphemes_and_waits_for_owner_ack() {
        use crate::commands::CommandAction;
        let mut state = fresh_state("rename-shortcut").await;
        state.session_title = Some("е\u{301}🧑‍💻界".into());
        state.set_tab_strip(
            vec![TabPresentation {
                title: state.session_title.clone(),
                home: false,
                busy: false,
            }],
            0,
            true,
        );
        type_text(&mut state, "unsent prompt").await;
        let (draft, caret) = (state.input.clone(), state.editor.cursor);
        assert_eq!(state.handle_key(KeyAction::Rename).await.intent, None);
        assert_eq!(state.panel(), &TuiPanel::Rename);
        assert_eq!(state.rename_title(), Some("е\u{301}🧑‍💻界"));
        assert_eq!(state.rename_cursor(), "е\u{301}🧑‍💻界".len());
        state.handle_key(KeyAction::Left).await;
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(state.rename_title(), Some("е\u{301}界"));
        state.handle_paste("  🦊\n\tnew  ");
        assert_eq!(state.rename_title(), Some("е\u{301}  🦊  new  界"));
        let outcome = state.handle_key(KeyAction::Enter).await;
        let title = "е\u{301}  🦊  new  界".to_string();
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::RenameSession {
                title: title.clone()
            })
        );
        assert_eq!(state.panel(), &TuiPanel::Rename);
        assert_eq!(state.session_title.as_deref(), Some("е\u{301}🧑‍💻界"));
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            None,
            "no duplicate while waiting"
        );
        state.rename_session_rejected("rename failed".into());
        assert_eq!(state.note(), Some("rename failed"));
        assert_eq!(state.rename_title(), Some(title.as_str()));
        assert_eq!(state.input(), draft);
        assert_eq!(state.editor.cursor, caret);
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::RenameSession {
                title: title.clone()
            })
        );
        state.rename_session_applied("wrong title".into());
        assert_eq!(state.panel(), &TuiPanel::Rename);
        state.rename_session_applied(title.clone());
        assert_eq!(state.panel(), &TuiPanel::None);
        assert_eq!(state.session_title.as_deref(), Some(title.as_str()));
        assert_eq!(
            state.tab_presentation().0[0].title.as_deref(),
            Some(title.as_str())
        );
        assert_eq!(state.input(), draft);
        assert_eq!(state.editor.cursor, caret);
        assert_eq!(
            state.command_unavailable(&CommandAction::RenameSession { title: None }),
            None
        );
    }

    #[tokio::test]
    async fn rename_palette_slash_blank_cancel_and_refusal() {
        use crate::commands::CommandAction;
        let (app, _, _) = CoreApp::channel(4);
        let mut home = TuiState::new_home(app);
        assert_eq!(
            home.handle_key(KeyAction::Rename).await.note.as_deref(),
            Some("no session yet")
        );
        home.handle_key(KeyAction::Commands).await;
        home.handle_paste("Rename session");
        assert!(home.modal_options().is_empty());

        let mut state = fresh_state("rename-palette").await;
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("Rename session");
        assert_eq!(state.modal_options()[0].value, "session.rename");
        assert_eq!(state.modal_options()[0].footer, "ctrl+r");
        state.handle_panel_key(KeyAction::Enter);
        assert_eq!(state.rename_title(), Some(""));
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        state.handle_paste("  \n\t");
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        assert_eq!(state.panel(), &TuiPanel::Rename);
        state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(state.rename_title(), None);
        assert_eq!(state.session_title, None);

        type_text(&mut state, "/rename").await;
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        assert_eq!(
            state.input(),
            "/rename ",
            "upstream slash.arguments completes first"
        );
        state.handle_key(KeyAction::Backspace).await;
        state.handle_key(KeyAction::Cancel).await; // Dismiss to submit the owner-backed bare action.
        let bare = state.handle_key(KeyAction::Enter).await;
        assert_eq!(bare.intent, Some(PanelIntent::RegenerateTitle));
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        assert_eq!(state.panel(), &TuiPanel::None);
        assert_eq!(state.input(), "/rename");
        state.regenerated_title(Err("provider unavailable".into()));
        assert_eq!(state.input(), "/rename");
        assert_eq!(state.note(), Some("provider unavailable"));
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::RegenerateTitle)
        );
        state.regenerated_title(Ok("New generated title".into()));
        assert_eq!(state.input(), "");
        assert_eq!(state.session_title.as_deref(), Some("New generated title"));

        state.parent_id = Some("parent".into());
        assert_eq!(
            state
                .run_command(CommandAction::RenameSession { title: None })
                .note
                .as_deref(),
            Some("child session is read-only")
        );
        state.parent_id = None;
        state.set_tab_strip(
            vec![TabPresentation {
                title: None,
                home: false,
                busy: true,
            }],
            0,
            false,
        );
        assert_eq!(
            state.handle_key(KeyAction::Rename).await.note.as_deref(),
            Some("tab busy; action unavailable")
        );
    }

    #[tokio::test]
    async fn rename_oversized_prefill_never_submits_a_prefix_and_can_be_replaced() {
        let mut state = fresh_state("rename-generated").await;
        let generated = "🙂".repeat(100);
        assert_eq!(generated.len(), 400);
        state.session_title = Some(generated.clone());
        let opened = state.handle_key(KeyAction::Rename).await;
        assert_eq!(state.rename_title(), Some(generated.as_str()));
        assert!(
            opened
                .note
                .as_deref()
                .unwrap()
                .contains("shorten it or select all")
        );
        assert_eq!(state.rename_cursor(), generated.len());
        assert!(state.rename_editor.retained_bytes() <= 400 * 32);
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        assert_eq!(state.panel(), &TuiPanel::None);
        assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));

        state.handle_key(KeyAction::Rename).await;
        assert_eq!(state.handle_key(KeyAction::Char('x')).await.intent, None);
        assert_eq!(state.rename_title(), Some(generated.as_str()));
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(state.rename_title(), Some("🙂".repeat(99).as_str()));
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        assert_eq!(state.panel(), &TuiPanel::Rename);
        assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));
        state.handle_key(KeyAction::SelectHome).await;
        state.handle_paste("е\u{301}🧑‍💻 renamed");
        let title = "е\u{301}🧑‍💻 renamed".to_string();
        assert_eq!(state.rename_title(), Some(title.as_str()));
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::RenameSession {
                title: title.clone()
            })
        );
        assert_eq!(state.session_title.as_deref(), Some(generated.as_str()));
        state.rename_session_applied(title.clone());
        assert_eq!(state.session_title.as_deref(), Some(title.as_str()));
        assert_eq!(state.panel(), &TuiPanel::None);
    }

    #[tokio::test]
    async fn rename_full_unicode_prefill_stays_on_one_row_at_narrow_width() {
        use ratatui::{
            Terminal,
            backend::{Backend, TestBackend},
            layout::Rect,
        };

        let mut state = fresh_state("rename-narrow").await;
        let title = "🙂".repeat(100);
        state.session_title = Some(title.clone());
        state.handle_key(KeyAction::Rename).await;
        let mut terminal = Terminal::new(TestBackend::new(43, 24)).unwrap();
        terminal
            .draw(|frame| crate::dialog::render(frame, &state))
            .unwrap();
        let rect = crate::dialog::rename_geometry(Rect::new(0, 0, 43, 24));
        let buffer = terminal.backend().buffer();
        assert!((rect.x + 2..rect.right() - 2).any(|x| buffer[(x, rect.y + 3)].symbol() == "🙂"));
        assert!((rect.x + 2..rect.right() - 2).all(|x| buffer[(x, rect.y + 4)].symbol() == " "));
        assert_eq!(
            terminal.backend_mut().get_cursor_position().unwrap().y,
            rect.y + 3
        );
        assert_eq!(state.rename_title(), Some(title.as_str()));
    }

    #[tokio::test]
    async fn rename_limits_prefill_typing_and_paste_to_owner_bytes_without_splitting_graphemes() {
        let mut state = fresh_state("rename-limits").await;
        state.session_title = Some(format!("{}🧑‍💻", "a".repeat(252)));
        let opened = state.handle_key(KeyAction::Rename).await;
        assert_eq!(state.rename_title(), state.session_title.as_deref());
        assert!(
            opened
                .note
                .as_deref()
                .unwrap()
                .contains("shorten it or select all")
        );
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(state.rename_title(), Some("a".repeat(252).as_str()));
        assert_eq!(state.handle_key(KeyAction::Char('界')).await.intent, None);
        assert_eq!(state.rename_title().unwrap().len(), 255);
        assert_eq!(
            state
                .handle_key(KeyAction::Char('界'))
                .await
                .note
                .as_deref(),
            Some("session title truncated at 256 bytes")
        );
        let outcome = state.handle_paste("🦊界");
        assert_eq!(
            outcome.note.as_deref(),
            Some("session title truncated at 256 bytes")
        );
        assert_eq!(state.rename_title().unwrap().len(), 255);
        state.handle_key(KeyAction::Backspace).await;
        let outcome = state.handle_paste("界🦊");
        assert_eq!(
            outcome.note.as_deref(),
            Some("session title truncated at 256 bytes")
        );
        assert_eq!(
            state.rename_title(),
            Some(format!("{}界", "a".repeat(252)).as_str())
        );
        let intent = state.handle_key(KeyAction::Enter).await.intent;
        assert_eq!(
            intent,
            Some(PanelIntent::RenameSession {
                title: format!("{}界", "a".repeat(252))
            })
        );
        assert!(state.rename_title().unwrap().len() <= MAX_SESSION_TITLE_BYTES);
    }

    #[tokio::test]
    async fn rename_combined_unicode_prefill_and_editor_history_stay_bounded() {
        let mut state = fresh_state("rename-combined").await;
        // 100 Unicode scalar values, with combined graphemes and UTF-8 >256.
        let original = format!("{}{}", "🧑‍💻".repeat(20), "е\u{301}".repeat(20));
        assert_eq!(original.chars().count(), 100);
        assert!(original.len() > MAX_SESSION_TITLE_BYTES);
        assert!(original.len() <= 400);
        state.session_title = Some(original.clone());
        state.handle_key(KeyAction::Rename).await;
        assert_eq!(state.rename_title(), Some(original.as_str()));
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(
            state.rename_title(),
            Some(format!("{}{}", "🧑‍💻".repeat(20), "е\u{301}".repeat(19)).as_str())
        );
        state.handle_key(KeyAction::Undo).await;
        assert_eq!(state.rename_title(), Some(original.as_str()));
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        assert_eq!(state.session_title.as_deref(), Some(original.as_str()));

        // Even malformed/older oversized titles do not create an unbounded
        // modal copy or retained undo snapshots.
        state.session_title = Some("🙂".repeat(10_000));
        state.handle_key(KeyAction::Rename).await;
        assert_eq!(state.rename_title(), Some(""));
        for _ in 0..40 {
            state.handle_paste(&"🧑‍💻".repeat(100));
            state.handle_key(KeyAction::Undo).await;
        }
        assert!(state.rename_title().unwrap().len() <= 400);
        assert!(state.rename_editor.retained_bytes() <= 32 * 400);
        state.close_panel();
        assert_eq!(state.rename_editor.retained_bytes(), 0);
    }

    #[tokio::test]
    async fn slash_rename_direct_waits_for_owner_and_retains_draft_on_async_failure() {
        let mut state = fresh_state("rename-direct").await;
        state.set_tab_strip(
            vec![TabPresentation {
                title: None,
                home: false,
                busy: false,
            }],
            0,
            true,
        );
        state.input = "/rename  🦊 edited  ".into();
        state.editor.cursor = state.input.len();
        let original = state.input.clone();
        let intent = state.handle_key(KeyAction::Enter).await.intent;
        assert_eq!(
            intent,
            Some(PanelIntent::RenameSessionDirect {
                title: "🦊 edited".into()
            })
        );
        assert_eq!(state.panel(), &TuiPanel::None);
        assert_eq!(state.input(), original);
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, None);
        state.rename_session_applied("🦊 edited".into());
        assert_eq!(
            state.session_title, None,
            "dialog ACK must not accept a direct request"
        );
        state.rename_session_direct_rejected("storage failed".into());
        assert_eq!(state.note(), Some("storage failed"));
        assert_eq!(state.input(), original);
        assert_eq!(state.handle_key(KeyAction::Enter).await.intent, intent);
        state.rename_session_direct_applied("wrong title".into());
        assert_eq!(state.input(), original);
        state.rename_session_direct_applied("🦊 edited".into());
        assert_eq!(state.input(), "");
        assert_eq!(state.session_title.as_deref(), Some("🦊 edited"));
        assert_eq!(
            state.tab_presentation().0[0].title.as_deref(),
            Some("🦊 edited")
        );

        state.input = format!("/rename {}🦊", "a".repeat(MAX_SESSION_TITLE_BYTES));
        state.editor.cursor = state.input.len();
        let invalid = state.handle_key(KeyAction::Enter).await;
        assert_eq!(invalid.intent, None);
        assert!(invalid.note.as_deref().unwrap().contains("256 bytes"));
        assert!(!state.input().is_empty());

        state.input = "/rename short".into();
        state.editor.cursor = state.input.len();
        assert_eq!(
            state.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::RenameSessionDirect {
                title: "short".into()
            })
        );
        state.handle_key(KeyAction::Char('!')).await;
        state.rename_session_direct_applied("short".into());
        assert_eq!(state.input(), "/rename short!");
    }

    #[tokio::test]
    async fn close_tab_chord_uses_the_presented_active_slot_without_mutating_the_deck() {
        let mut state = fresh_state("close-chord").await;
        let tabs = vec![
            TabPresentation {
                title: Some("Old".into()),
                home: false,
                busy: false,
            },
            TabPresentation {
                title: Some("Current".into()),
                home: false,
                busy: false,
            },
        ];
        state.set_tab_strip(tabs.clone(), 1, true);
        type_text(&mut state, "draft").await;
        assert_eq!(
            state.handle_key(KeyAction::Leader).await,
            KeyOutcome::default()
        );
        assert_eq!(
            state.handle_key(KeyAction::Char('w')).await,
            KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index: 1 }),
                ..KeyOutcome::default()
            }
        );
        assert_eq!(state.tab_presentation().0, tabs);
        assert_eq!(state.input(), "draft");
        assert_eq!(state.panel(), &TuiPanel::None);
    }

    #[tokio::test]
    async fn close_tab_palette_selection_preserves_modal_and_draft_until_owner_accepts() {
        let mut state = fresh_state("close-palette").await;
        state.set_tab_strip(
            vec![TabPresentation {
                title: Some("Only tab".into()),
                home: false,
                busy: false,
            }],
            0,
            true,
        );
        type_text(&mut state, "unsent draft").await;
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("Close tab");
        let options = state.modal_options();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "session.tab.close");
        assert_eq!(options[0].footer, "ctrl+x w");
        assert_eq!(
            state.handle_panel_key(KeyAction::Enter),
            KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index: 0 }),
                ..KeyOutcome::default()
            }
        );
        assert_eq!(state.panel(), &TuiPanel::Commands);
        assert_eq!(state.select.query, "Close tab");
        assert_eq!(state.input(), "unsent draft");
        assert_eq!(state.tab_presentation().0.len(), 1);
    }

    #[tokio::test]
    async fn close_tab_home_last_and_busy_availability() {
        use crate::commands::CommandAction;
        use oc_core::core_app::InboxMsg;

        let (app, _, _) = CoreApp::channel(4);
        let mut home = TuiState::new_home(app);
        assert_eq!(
            home.command_unavailable(&CommandAction::CloseTab),
            Some("no tab to close")
        );
        home.handle_key(KeyAction::Commands).await;
        home.handle_paste("Close tab");
        assert!(home.modal_options().is_empty());
        home.handle_panel_key(KeyAction::Cancel);
        home.handle_key(KeyAction::Leader).await;
        let bare = home.handle_key(KeyAction::Char('w')).await;
        assert_eq!(bare.intent, None);
        assert_eq!(bare.note.as_deref(), Some("no tab to close"));
        home.set_tab_strip(
            vec![TabPresentation {
                title: Some("Old".into()),
                home: false,
                busy: false,
            }],
            0,
            false,
        );
        assert_eq!(home.command_unavailable(&CommandAction::CloseTab), None);
        assert_eq!(home.handle_key(KeyAction::Leader).await.intent, None);
        assert_eq!(
            home.handle_key(KeyAction::Char('w')).await.intent,
            Some(PanelIntent::CloseTab { index: 1 })
        );
        assert!(home.home);
        assert_eq!(home.tab_presentation().0.len(), 1);

        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut last = TuiState::new(app, sid("close-last"));
        assert_eq!(
            last.command_unavailable(&CommandAction::CloseTab),
            Some("no tab to close")
        );
        last.set_tab_strip(
            vec![TabPresentation {
                title: Some("Last".into()),
                home: false,
                busy: false,
            }],
            0,
            false,
        );
        assert_eq!(
            last.run_command(CommandAction::CloseTab).intent,
            Some(PanelIntent::CloseTab { index: 0 })
        );
        assert_eq!(last.tab_presentation().0.len(), 1);

        last.set_tab_strip(
            vec![TabPresentation {
                title: Some("Busy tab".into()),
                home: false,
                busy: true,
            }],
            0,
            false,
        );
        assert_eq!(
            last.command_unavailable(&CommandAction::CloseTab),
            Some("tab busy; action unavailable")
        );
        assert_eq!(last.run_command(CommandAction::CloseTab).intent, None);
        last.set_tab_strip(
            vec![TabPresentation {
                title: Some("Last".into()),
                home: false,
                busy: false,
            }],
            0,
            false,
        );

        type_text(&mut last, "pending draft").await;
        last.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("submit")
        };
        last.handle_key(KeyAction::Commands).await;
        last.handle_paste("Close tab");
        assert!(last.modal_options()[0].footer.contains("turn active"));
        let refused = last.handle_panel_key(KeyAction::Enter);
        assert_eq!(refused.intent, None);
        assert_eq!(
            refused.note.as_deref(),
            Some("turn active; action unavailable")
        );
        assert_eq!(last.panel(), &TuiPanel::Commands);
        assert_eq!(last.input(), "pending draft");
        last.handle_panel_key(KeyAction::Cancel);
        assert_eq!(last.run_command(CommandAction::CloseTab).intent, None);
        assert!(inbox.try_recv().is_err());
        drop(ack);

        let (app, _, _) = CoreApp::channel(4);
        let mut bare = TuiState::new_home(app);
        type_text(&mut bare, "/close-tab").await;
        let refused = bare.handle_key(KeyAction::Enter).await;
        assert_eq!(refused.intent, None);
        assert_eq!(refused.note.as_deref(), Some("no tab to close"));
        assert_eq!(bare.input(), "/close-tab");
    }

    #[tokio::test]
    async fn retained_tab_mouse_routes_only_matching_painted_left_clicks() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;

        let mut state = fresh_state("tab-mouse").await;
        let area = Rect::new(0, 0, 31, 24);
        let event = |kind, x, y, modifiers| MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers,
        };
        let left = MouseEventKind::Down(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        let plain = KeyModifiers::NONE;
        state.set_tab_strip(
            (0..12)
                .map(|i| TabPresentation {
                    title: Some(format!("Tab {i}")),
                    home: false,
                    busy: i == 3,
                })
                .collect(),
            10,
            true,
        );
        assert_eq!(state.tab_presentation().0.len(), 12);
        let strip = crate::shell::tab_strip(&state, area).unwrap();
        let active = strip.tabs.iter().find(|tab| tab.index == 10).unwrap().rect;
        let add = strip.add.unwrap();
        let click = |state: &mut TuiState, x, y| {
            state.handle_mouse(event(left, x, y, plain), area);
            state.handle_mouse(event(up, x, y, plain), area).intent
        };
        assert_eq!(
            click(&mut state, active.x, 0),
            Some(PanelIntent::ActivateTab { index: 10 })
        );
        for rect in [strip.before_marker.unwrap(), strip.after_marker.unwrap()] {
            state.handle_mouse(event(MouseEventKind::Moved, active.x, 0, plain), area);
            state.handle_mouse(event(MouseEventKind::Moved, rect.x, 0, plain), area);
            assert_eq!(state.hovered_tab(area), None);
            assert_eq!(click(&mut state, rect.x, 0), None);
        }
        assert_eq!(click(&mut state, area.right() - 1, 1), None);
        assert_eq!(click(&mut state, add.x, 0), Some(PanelIntent::NewSession));
        assert_eq!(
            click(&mut state, add.x + 1, 0),
            Some(PanelIntent::NewSession)
        );
        assert_eq!(
            click(&mut state, add.x + 2, 0),
            Some(PanelIntent::NewSession)
        );
        state.handle_mouse(event(left, active.x, 0, plain), area);
        assert_eq!(
            state.handle_mouse(event(up, add.x, 0, plain), area).intent,
            None
        );
        state.handle_mouse(event(left, active.x, 0, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, KeyModifiers::SHIFT), area)
                .intent,
            None
        );
        state.handle_mouse(event(left, active.x, 0, KeyModifiers::CONTROL), area);
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(left, active.x, 0, plain), area);
        state.handle_mouse(
            event(
                MouseEventKind::Drag(MouseButton::Left),
                active.x + 1,
                0,
                plain,
            ),
            area,
        );
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(left, active.x, 0, plain), area);
        state.handle_mouse(
            event(MouseEventKind::Down(MouseButton::Right), active.x, 0, plain),
            area,
        );
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(left, active.x, 0, plain), area);
        state.run_command(crate::commands::CommandAction::OpenCommands);
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, plain), area)
                .intent,
            None
        );
        state.close_panel();
        assert_eq!(
            state
                .handle_mouse(event(up, active.x, 0, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(left, add.x, 0, plain), area);
        state.reset_workspace();
        assert_eq!(
            state.handle_mouse(event(up, add.x, 0, plain), area).intent,
            None
        );
        assert!(state.tab_presentation().0.is_empty());
    }

    #[tokio::test]
    async fn home_slot_is_not_an_activation_and_busy_tabs_remain_selectable() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let (app, _inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        let area = Rect::new(0, 0, 80, 24);
        let mouse = |kind, x| MouseEvent {
            kind,
            column: x,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        let down = MouseEventKind::Down(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        assert!(crate::shell::tab_strip(&state, area).is_none());
        assert_eq!(state.handle_mouse(mouse(down, 1), area).intent, None);
        assert_eq!(state.handle_mouse(mouse(up, 1), area).intent, None);
        state.set_tab_strip(
            vec![TabPresentation {
                title: Some("Busy".into()),
                home: false,
                busy: true,
            }],
            0,
            true,
        );
        let strip = crate::shell::tab_strip(&state, area).unwrap();
        assert_eq!(strip.tabs.len(), 2);
        assert!(strip.add.is_none());
        let real = strip.tabs[0].rect;
        let home = strip.tabs[1].rect;
        state.handle_mouse(mouse(down, home.x + 1), area);
        assert_eq!(state.handle_mouse(mouse(up, home.x + 1), area).intent, None);
        state.handle_mouse(mouse(down, real.x + 1), area);
        assert_eq!(
            state.handle_mouse(mouse(up, real.x + 2), area).intent,
            Some(PanelIntent::ActivateTab { index: 0 })
        );
        state.handle_mouse(mouse(down, real.x + 1), area);
        state.set_tab_strip(
            vec![TabPresentation {
                title: None,
                home: false,
                busy: false,
            }],
            0,
            false,
        );
        assert_eq!(state.handle_mouse(mouse(up, real.x + 1), area).intent, None);
    }

    #[tokio::test]
    async fn hovered_close_requires_painted_cell_matching_press_and_no_drag() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let (app, _inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        state.set_tab_strip(
            vec![TabPresentation {
                title: Some("Long real tab title".into()),
                home: false,
                busy: false,
            }],
            0,
            false,
        );
        let area = Rect::new(0, 0, 80, 24);
        let strip = crate::shell::tab_strip(&state, area).unwrap();
        let real = strip.tabs[0].rect;
        let home = strip.tabs[1].rect;
        let event = |kind, x, modifiers| MouseEvent {
            kind,
            column: x,
            row: 0,
            modifiers,
        };
        let plain = KeyModifiers::NONE;
        let down = MouseEventKind::Down(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        let real_close = real.right() - 2;
        let home_close = home.right() - 2;
        assert_eq!(
            state
                .handle_mouse(event(down, real_close, plain), area)
                .intent,
            None
        );
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            Some(PanelIntent::ActivateTab { index: 0 })
        );

        state.handle_mouse(event(MouseEventKind::Moved, real.x + 3, plain), area);
        assert_eq!(state.hovered_tab(area), Some(0));
        state.handle_mouse(event(down, real_close, plain), area);
        assert_eq!(state.hovered_tab(area), Some(0));
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            Some(PanelIntent::CloseTab { index: 0 })
        );

        state.handle_mouse(event(down, real_close, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, real_close - 1, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(down, real_close - 1, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(down, real_close, plain), area);
        state.handle_mouse(
            event(MouseEventKind::Drag(MouseButton::Left), real_close, plain),
            area,
        );
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(down, real_close, KeyModifiers::SHIFT), area);
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(down, real_close, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, KeyModifiers::SHIFT), area)
                .intent,
            None
        );
        state.handle_mouse(
            event(MouseEventKind::Down(MouseButton::Right), real_close, plain),
            area,
        );
        assert_eq!(
            state
                .handle_mouse(event(up, real_close, plain), area)
                .intent,
            None
        );

        state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
        state.handle_mouse(event(down, home_close, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, home_close, plain), area)
                .intent,
            Some(PanelIntent::CloseTab { index: 1 })
        );
        state.handle_mouse(event(MouseEventKind::Moved, home.right(), plain), area);
        assert_eq!(state.hovered_tab(area), None);
        state.handle_mouse(event(down, home_close, plain), area);
        assert_eq!(
            state
                .handle_mouse(event(up, home_close, plain), area)
                .intent,
            None
        );
        state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
        state.handle_mouse(event(down, home_close, plain), area);
        state.run_command(crate::commands::CommandAction::OpenCommands);
        assert_eq!(state.hovered_tab(area), None);
        assert_eq!(
            state
                .handle_mouse(event(up, home_close, plain), area)
                .intent,
            None
        );
        state.close_panel();
        assert_eq!(state.hovered_tab(area), None);
        state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
        state.set_tab_strip(vec![], 0, false);
        assert_eq!(state.hovered_tab(area), None);
    }

    #[tokio::test]
    async fn post_close_hold_is_released_when_a_modal_opens() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        let real = TabPresentation {
            title: Some("survivor".into()),
            home: false,
            busy: false,
        };
        state.set_tab_strip(vec![real.clone()], 0, false);
        let area = Rect::new(0, 0, 120, 40);
        let before = crate::shell::tab_strip(&state, area).unwrap();
        let x = before.tabs[1].rect.right() - 2;
        let mouse = |kind| MouseEvent {
            kind,
            column: x,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(mouse(MouseEventKind::Moved), area);
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
        assert_eq!(
            state
                .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
                .intent,
            Some(PanelIntent::CloseTab { index: 1 })
        );
        let snapshot = state.mouse_close_snapshot(1).unwrap();
        state.home = false;
        state.set_tab_strip(vec![real], 0, true);
        state.restore_mouse_close(snapshot);
        assert_eq!(
            crate::shell::tab_strip(&state, area).unwrap().tabs[0]
                .rect
                .width,
            64
        );
        state.run_command(crate::commands::CommandAction::OpenCommands);
        assert_eq!(
            crate::shell::tab_strip(&state, area).unwrap().tabs[0]
                .rect
                .width,
            32
        );
        state.close_panel();
        assert_eq!(state.mouse_position(), None);
        assert_eq!(
            state.tab_close_cell(
                area,
                0,
                crate::shell::tab_strip(&state, area).unwrap().tabs[0].rect
            ),
            None
        );
        // Wheel input below the tab strip also moves the pointer and ends
        // the temporary close hold without disabling transcript scrolling.
        state.restore_mouse_hover((x, 0, area));
        state.close_hold = Some(TabCloseHold {
            area,
            strip: crate::shell::tab_strip(&state, area).unwrap(),
            until: Instant::now() + std::time::Duration::from_secs(5),
        });
        let mut wheel = mouse(MouseEventKind::ScrollDown);
        wheel.row = 12;
        state.handle_mouse(wheel, area);
        assert!(state.close_hold.is_none());
        assert_eq!(state.hovered_tab(area), None);
    }

    #[tokio::test]
    async fn busy_and_clipped_tabs_never_emit_close() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let mut state = fresh_state("close-busy").await;
        let wide = Rect::new(0, 0, 80, 24);
        let mouse = |kind, x| MouseEvent {
            kind,
            column: x,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        let down = MouseEventKind::Down(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        let legacy = crate::shell::tab_strip(&state, wide).unwrap().tabs[0].rect;
        state.handle_mouse(mouse(MouseEventKind::Moved, legacy.right() - 2), wide);
        assert_eq!(state.tab_close_cell(wide, 0, legacy), None);
        state.handle_mouse(mouse(down, legacy.right() - 2), wide);
        assert_eq!(
            state
                .handle_mouse(mouse(up, legacy.right() - 2), wide)
                .intent,
            None
        );
        state.set_tab_strip(
            vec![TabPresentation {
                title: Some("Busy".into()),
                home: false,
                busy: true,
            }],
            0,
            false,
        );
        let narrow = Rect::new(0, 0, 4, 24);
        let x = crate::shell::tab_strip(&state, wide).unwrap().tabs[0]
            .rect
            .right()
            - 2;
        state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
        state.handle_mouse(mouse(down, x), wide);
        assert_eq!(
            state.handle_mouse(mouse(up, x), wide).intent,
            Some(PanelIntent::ActivateTab { index: 0 })
        );
        state.set_tab_strip(
            vec![TabPresentation {
                title: None,
                home: false,
                busy: false,
            }],
            0,
            false,
        );
        state.active_turn = Some(WorkerTurnId("busy-close".into()));
        state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
        assert_eq!(
            state.tab_close_cell(
                wide,
                0,
                crate::shell::tab_strip(&state, wide).unwrap().tabs[0].rect
            ),
            None
        );
        state.handle_mouse(mouse(down, x), wide);
        assert_eq!(
            state.handle_mouse(mouse(up, x), wide).intent,
            Some(PanelIntent::ActivateTab { index: 0 })
        );
        state.active_turn = None;
        let small = crate::shell::tab_strip(&state, narrow).unwrap().tabs[0].rect;
        assert_eq!(small.width, 4);
        state.handle_mouse(mouse(MouseEventKind::Moved, small.right() - 2), narrow);
        assert_eq!(state.tab_close_cell(narrow, 0, small), None);
        state.handle_mouse(mouse(down, small.right() - 2), narrow);
        assert_eq!(
            state
                .handle_mouse(mouse(up, small.right() - 2), narrow)
                .intent,
            Some(PanelIntent::ActivateTab { index: 0 })
        );
        state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
        assert_eq!(state.hovered_tab(wide), Some(0));
        assert_eq!(
            state.hovered_tab(narrow),
            None,
            "resize invalidates painted hover"
        );
        assert_eq!(
            state.hovered_tab(wide),
            None,
            "old geometry must not resurrect on grow"
        );
    }

    async fn submit_echo(
        state: &mut TuiState,
        events: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
        text: &str,
    ) {
        state.input = text.to_string();
        state.handle_key(KeyAction::Enter).await;
        let start = Instant::now();
        let turn = loop {
            state.poll_submission();
            if let Some(turn) = &state.active_turn {
                break turn.clone();
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "submit was not accepted"
            );
            tokio::task::yield_now().await;
        };
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
                .await
                .expect("echo turn timed out")
                .expect("event channel closed");
            match event {
                CoreEvent::TurnPresentation {
                    turn: event_turn,
                    projection,
                    ..
                } if event_turn == turn => state.apply_presentation(&turn, &projection),
                CoreEvent::TurnFinished {
                    turn: event_turn,
                    text,
                    duration_ms,
                    ..
                } if event_turn == turn => {
                    state.apply_finished(&turn, &text, duration_ms);
                    break;
                }
                CoreEvent::TurnFailed {
                    turn: event_turn,
                    error,
                    ..
                } if event_turn == turn => panic!("echo turn failed: {error}"),
                _ => {}
            }
        }
    }

    fn user_border_colors(state: &TuiState) -> Vec<ratatui::style::Color> {
        state
            .transcript_lines(100, 120)
            .iter()
            .filter_map(|line| {
                line.spans()
                    .iter()
                    .find(|span| span.content() == "┃")
                    .and_then(|span| span.style().fg)
            })
            .collect()
    }

    #[tokio::test]
    async fn sent_user_messages_keep_the_profile_color_across_profile_switches() {
        let mut state = fresh_state("user-message-agent-color").await;
        let mut events = state.app.subscribe();
        let colors = crate::theme::Theme::dark().categorical_agents();

        let mut selected = snapshot();
        selected.agent_id = Some("y".to_string());
        state.apply_catalog(selected);
        submit_echo(&mut state, &mut events, "message from second profile").await;

        assert_eq!(
            state.history().rows()[0].agent.as_deref(),
            Some("y"),
            "the accepted user echo must retain its submitted profile"
        );
        assert_eq!(state.history().rows()[0].agent_color_index, Some(1));
        assert!(
            user_border_colors(&state)
                .iter()
                .all(|color| *color == colors[1]),
            "first profile's user stripe must use its categorical color"
        );

        state.apply_catalog(snapshot());
        submit_echo(&mut state, &mut events, "message from first profile").await;

        let users: Vec<_> = state
            .history()
            .rows()
            .iter()
            .filter(|row| row.role == "user")
            .collect();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].agent.as_deref(), Some("y"));
        assert_eq!(users[0].agent_color_index, Some(1));
        assert_eq!(users[1].agent.as_deref(), Some("x"));
        assert_eq!(users[1].agent_color_index, Some(0));
        let mut stripe_runs = Vec::new();
        for color in user_border_colors(&state) {
            if stripe_runs.last() != Some(&color) {
                stripe_runs.push(color);
            }
        }
        assert_eq!(stripe_runs, [colors[1], colors[0]]);
    }

    #[tokio::test]
    async fn replayed_user_messages_use_their_persisted_turn_color() {
        use oc_core::queries::HistoryTurn;

        let mut state = fresh_state("replay-user-agent-color").await;
        state.apply_catalog(snapshot()); // current profile is `x`
        let mut old = msg(1, Role::User, "previously sent as y");
        old.turn = Some(HistoryTurn {
            agent: Some("y".into()),
            agent_color_index: Some(1),
            ..Default::default()
        });
        let mut new = msg(2, Role::User, "previously sent as x");
        new.turn = Some(HistoryTurn {
            agent: Some("x".into()),
            agent_color_index: Some(0),
            ..Default::default()
        });
        state.attach_page(&page(vec![old, new], 2, false, false));

        let colors = crate::theme::Theme::dark().categorical_agents();
        let mut stripe_runs = Vec::new();
        for color in user_border_colors(&state) {
            if stripe_runs.last() != Some(&color) {
                stripe_runs.push(color);
            }
        }
        assert_eq!(stripe_runs, [colors[1], colors[0]]);
    }

    #[tokio::test]
    async fn live_footer_uses_the_turns_pinned_model_not_the_current_picker() {
        let mut state = fresh_state("pinned-model-footer").await;
        let turn = WorkerTurnId("turn-pinned-model".into());
        state.active_turn = Some(turn.clone());
        state.apply_presentation(
            &turn,
            &oc_core::queries::HistoryTurn {
                id: turn.0.clone(),
                model_label: "Muse Spark".into(),
                status: "completed".into(),
                ..Default::default()
            },
        );
        state.apply_finished(&turn, "answer", 100);
        assert_eq!(
            state
                .history()
                .rows()
                .last()
                .unwrap()
                .meta
                .as_ref()
                .unwrap()
                .model
                .as_deref(),
            Some("Muse Spark")
        );
    }

    #[tokio::test]
    async fn v06b_reasoning_toggle_projects_public_history_without_modifying_owner() {
        use oc_core::queries::{HistoryTurn, TranscriptPart};
        let mut state = fresh_state("s-v06b-reasoning").await;
        let mut row = msg(2, Role::Assistant, "answer");
        row.turn = Some(HistoryTurn {
            id: "turn-1".into(),
            status: "failed".into(),
            agent: Some("build".into()),
            model_label: "fixture".into(),
            parts: vec![
                TranscriptPart::Reasoning {
                    text: "**Plan**\n\nPublic detail [REDACTED]".into(),
                    duration_ms: Some(1500),
                },
                TranscriptPart::Text("answer".into()),
            ],
            ..Default::default()
        });
        let owner_page = page(vec![row], 1, false, false);
        state.attach_page(&owner_page);
        let collapsed = state
            .transcript_lines(100, 120)
            .iter()
            .map(|l| l.spans().iter().map(|s| s.content()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            collapsed.contains("+ Thought: Plan · 1.5s") && !collapsed.contains("Public detail")
        );
        state.run_command(crate::commands::CommandAction::ToggleThinking);
        let expanded = state
            .transcript_lines(100, 120)
            .iter()
            .map(|l| l.spans().iter().map(|s| s.content()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            expanded.contains("- Thought: 1.5s")
                && expanded.contains("Public detail")
                && !expanded.contains("[REDACTED]"),
            "{expanded}"
        );
        state.attach_page(&owner_page);
        assert_eq!(owner_page.rows[0].turn.as_ref().unwrap().parts.len(), 2);
        assert!(state.transcript_lines(100, 120).iter().any(|line| {
            line.spans()
                .iter()
                .any(|s| s.content().contains("Public detail"))
        }));
        state.run_command(crate::commands::CommandAction::ToggleThinking);
        assert!(
            !state.transcript_rows()[0]
                .reasoning
                .as_ref()
                .unwrap()
                .expanded
        );
    }

    #[tokio::test]
    async fn v06b_session_switch_discards_tool_cards_before_next_owner_load() {
        let mut state = fresh_state("owner-a").await;
        state.app.create_session(sid("owner-b")).await.unwrap();
        let card = crate::history::ToolCard {
            op: "a-only".into(),
            name: "read".into(),
            state: "completed".into(),
            input_preview: String::new(),
            output_preview: "A-only preview".into(),
            output_bytes: 14,
            output_truncated: false,
            files: Vec::new(),
            files_truncated: false,
            diff: None,
            render: crate::tools::ToolRender::Inline(crate::tools::InlineRender::Read {
                path: "a-only".into(),
            }),
        };
        state.apply_cards(vec![card], true);
        state.set_session(sid("owner-b"));
        let action = state.run_command(crate::commands::CommandAction::OpenCards);
        assert_eq!(action.intent, Some(PanelIntent::LoadCards));
        assert!(state.cards.is_empty());
        assert!(state.card_ops.is_empty());
        assert!(state.card_output.is_none());
        assert!(!state.cards_has_older);
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
    }

    #[tokio::test]
    async fn v06b_detail_scrolls_every_unicode_row_before_next_owner_page() {
        use oc_core::queries::ToolOutputPage;
        for (width, height) in [(22, 10), (14, 9)] {
            let mut state = fresh_state(&format!("v06b-detail-{width}")).await;
            state.panel = TuiPanel::Cards;
            state.card_ops.push("real-op".into());
            let text = format!("{}END", "界🙂 ↵\n\\n\u{1b}[31m ".repeat(9));
            let next = text.len() as i64;
            state.apply_card_output(
                "real-op".into(),
                0,
                ToolOutputPage {
                    text,
                    total_bytes: next + 12,
                    next_offset: Some(next),
                },
            );
            let first = crate::views::render_test(&state, width, height).join("\n");
            assert!(first.contains("\\n") && first.contains('↵'), "{first}");
            assert!(!first.contains('\u{1b}'), "no raw terminal escapes");
            assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
            state.handle_panel_key(KeyAction::End);
            assert_eq!(
                state.handle_panel_key(KeyAction::Enter).intent,
                None,
                "jumping to the end without viewing preceding rows cannot skip the page"
            );
            state.handle_panel_key(KeyAction::Home);
            for _ in 0..300 {
                let frame = crate::views::render_test(&state, width, height).join("\n");
                let lines = crate::views::panel_lines(&state);
                let last = &lines[lines.len() - 2];
                if last.is_ascii() {
                    assert!(frame.contains(last), "last visible detail row: {last:?}");
                }
                let (_, _, count) = crate::views::card_window(&state);
                if state.card_seen() == count {
                    break;
                }
                assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
                state.handle_panel_key(KeyAction::Down);
            }
            assert_eq!(state.card_seen(), crate::views::card_window(&state).2);
            let (_, body_width, _) = crate::dialog::card_geometry(state.detail_area());
            assert!(
                crate::views::card_body_rows(
                    &state.card_output.as_ref().unwrap().page.text,
                    body_width
                )
                .join("")
                .contains("END")
            );
            assert_eq!(
                state.handle_panel_key(KeyAction::Enter).intent,
                Some(PanelIntent::LoadCardOutput {
                    op: "real-op".into(),
                    offset: next as usize
                })
            );
        }
    }

    #[tokio::test]
    async fn aud33_paste_is_never_silently_cut_below_the_input_budget() {
        let mut state = fresh_state("s-aud33-paste").await;
        // 100 KiB of multibyte text: above the old 64 KiB transport cap and
        // well inside the configured input budget, so nothing may be dropped.
        let big = "п".repeat(50 * 1024);
        let outcome = state.handle_paste(&big);
        assert_eq!(state.input().len(), big.len(), "paste must be kept whole");
        assert!(outcome.note.is_none(), "no diagnostic without a drop");

        // Above the input budget the cut is bounded and reported.
        let huge = "x".repeat(MAX_INPUT_BYTES + 4096);
        let outcome = state.handle_paste(&huge);
        assert_eq!(state.input().len(), MAX_INPUT_BYTES);
        let note = outcome.note.expect("a dropped paste must be reported");
        assert!(
            note.contains("paste") && note.contains(&MAX_INPUT_BYTES.to_string()),
            "note must name the paste and the budget: {note}"
        );
    }

    async fn type_text(state: &mut TuiState, text: &str) {
        for c in text.chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
    }

    async fn await_submission(state: &mut TuiState) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while state.pending.is_some() {
                tokio::task::yield_now().await;
                state.poll_submission();
            }
        })
        .await
        .expect("submission completed");
    }

    #[tokio::test]
    async fn sessionless_home_keeps_bounded_editor_and_refuses_session_actions() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        assert!(state.home);
        assert!(HOME_EXAMPLES.contains(&state.home_example));
        assert_eq!(state.attached_session(), None);
        assert!(state.history().rows().is_empty());
        assert!(
            crate::views::render_test(&state, 80, 24)
                .join("\n")
                .contains("Ask anything")
        );
        state.handle_key(KeyAction::Enter).await;
        state.handle_paste("  \n ");
        state.handle_key(KeyAction::Enter).await;
        assert!(
            inbox.try_recv().is_err(),
            "empty Home has no candidate session"
        );
        assert!(state.attached_session().is_none());
        assert_eq!(
            state.request_compress(String::new()),
            Err(CoreError::SessionNotFound)
        );
        for action in [
            crate::commands::CommandAction::OpenCards,
            crate::commands::CommandAction::DcpCompress {
                focus: String::new(),
            },
        ] {
            let result = state.run_command(action);
            assert_eq!(result.note.as_deref(), Some("no session yet"));
            assert_eq!(result.intent, None);
            assert_eq!(state.panel(), &TuiPanel::None);
        }
        state.panel = TuiPanel::Dcp;
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        state.panel = TuiPanel::Cards;
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        state.close_panel();
        state.handle_paste(&"x".repeat(MAX_INPUT_BYTES + 1));
        assert_eq!(state.input().len(), MAX_INPUT_BYTES);
        assert!(state.attached_session().is_none());
    }

    #[tokio::test]
    async fn fresh_rejection_preserves_home_draft_and_retry_uses_new_candidate() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        type_text(&mut state, "retry me").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::SubmitFresh {
            session: first,
            text,
            selection,
            ack,
        }) = inbox.recv().await
        else {
            panic!("fresh submit")
        };
        assert_eq!(text, "retry me");
        assert!(
            selection.is_none(),
            "the application owns the Home selection"
        );
        assert!(state.attached_session().is_none());
        assert!(state.history().rows().is_empty());
        state.handle_key(KeyAction::Enter).await;
        assert!(inbox.try_recv().is_err(), "no duplicate while pending");
        ack.send(Err(CoreError::Application("refused".into())))
            .unwrap();
        state.poll_submission();
        assert!(state.home);
        assert!(state.attached_session().is_none());
        assert_eq!(state.input(), "retry me");
        assert!(state.history().rows().is_empty());
        assert_eq!(state.status(), &TuiStatus::Idle);
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::SubmitFresh {
            session: retry,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("fresh retry")
        };
        assert_ne!(first, retry);
        state.set_session(sid("switched"));
        assert!(ack.send(Ok(WorkerTurnId("stale".into()))).is_err());
        state.poll_submission();
        assert!(state.history().rows().is_empty());
    }

    #[tokio::test]
    async fn mock_busy_rejection_creates_no_fresh_root() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["slow".into(); 20], 100));
        let existing = sid("busy-existing");
        app.create_session(existing.clone()).await.unwrap();
        app.try_submit(existing.clone(), "occupy worker".into())
            .await
            .unwrap();
        let mut state = TuiState::new_home(app.clone());
        type_text(&mut state, "retry later").await;
        state.handle_key(KeyAction::Enter).await;
        await_submission(&mut state).await;
        assert!(state.home);
        assert_eq!(state.attached_session(), None);
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert_eq!(state.input(), "retry later");
        assert!(state.history().rows().is_empty());
        assert_eq!(app.list_sessions().await.unwrap(), vec![existing.clone()]);
        app.cancel(existing).await.unwrap();
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn mock_fresh_acceptance_binds_only_on_receipt_and_preserves_pending_edit() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut driver = ScriptDriver::attach(&app);
        let mut state = TuiState::new_home(app.clone());
        type_text(&mut state, "first prompt").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.status(), &TuiStatus::PendingSubmission);
        assert!(state.home);
        assert!(state.attached_session().is_none());
        // Paste does not poll the receipt, so this edit deterministically
        // precedes reconciliation even if the mock owner already accepted.
        state.handle_paste(" edited");
        await_submission(&mut state).await;
        let accepted = state.attached_session().expect("accepted ID").clone();
        assert!(!state.home);
        assert_eq!(state.input(), "first prompt edited");
        assert_eq!(state.history().rows()[0].text, "first prompt");
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert_eq!(app.list_sessions().await.unwrap(), vec![accepted.clone()]);
        assert_eq!(
            driver
                .pump_until_idle(&mut state, Duration::from_secs(5))
                .await,
            PumpOutcome::Finished("echo: first prompt".into())
        );
        assert_eq!(state.attached_session(), Some(&accepted));
        state.handle_key(KeyAction::Enter).await;
        await_submission(&mut state).await;
        assert_eq!(state.attached_session(), Some(&accepted));
        assert_eq!(
            state
                .history()
                .rows()
                .iter()
                .filter(|row| row.role == "user")
                .count(),
            2
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn fresh_cancel_pending_uses_candidate_and_keeps_draft_on_late_accept() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        type_text(&mut state, "cancel me").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::SubmitFresh {
            session: candidate,
            ack: submit_ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("fresh request")
        };
        assert!(state.attached_session().is_none());
        let cancel = tokio::spawn(async move {
            let Some(InboxMsg::Cancel { session, ack }) = inbox.recv().await else {
                panic!("candidate cancel")
            };
            assert_eq!(session, candidate);
            ack.send(Ok(())).unwrap();
            submit_ack
                .send(Ok(WorkerTurnId("accepted-after-cancel".into())))
                .unwrap();
            session
        });
        assert_eq!(state.handle_key(KeyAction::Cancel).await.note, None);
        let candidate = cancel.await.unwrap();
        await_submission(&mut state).await;
        assert_eq!(state.attached_session(), Some(&candidate));
        assert_eq!(
            state.input(),
            "cancel me",
            "cancel keeps the editable draft"
        );
        assert!(!state.home);
        assert_eq!(state.history().rows()[0].text, "cancel me");
    }

    #[tokio::test]
    async fn pending_receipt_preserves_edits_and_ignores_old_generation() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("pending"));
        type_text(&mut state, "original").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        assert_eq!(text, "original");
        state.handle_key(KeyAction::Enter).await;
        assert!(inbox.try_recv().is_err(), "duplicate not enqueued");
        type_text(&mut state, " edited").await;
        ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
        state.poll_submission();
        assert_eq!(
            state.input(),
            "original edited",
            "acceptance cannot clear later edits"
        );
        assert_eq!(state.history().rows()[0].text, "original");
        state.apply_finished(&WorkerTurnId("accepted".into()), "done", 0);
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        // Exercise stale receipt defense even if a caller violates the normal
        // switch-refused-while-busy gate; A→B→A also changes the generation.
        state.set_session(sid("other"));
        state.reset_workspace();
        state.set_session(sid("pending"));
        type_text(&mut state, "new generation draft").await;
        assert!(
            ack.send(Ok(WorkerTurnId("old".into()))).is_err(),
            "old receipt invalidated"
        );
        state.poll_submission();
        assert!(!state.is_busy());
        assert_eq!(state.status(), &TuiStatus::Idle);
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("new generation submission")
        };
        type_text(&mut state, " edited after enqueue").await;
        ack.send(Ok(WorkerTurnId("new".into()))).unwrap();
        state.poll_submission();
        state.apply_delta(&WorkerTurnId("old".into()), "stale text");
        state.apply_tool_started(&WorkerTurnId("old".into()), "old-op", "read", "{}");
        state.apply_finished(&WorkerTurnId("old".into()), "stale answer", 0);
        state.apply_interrupted(&WorkerTurnId("old".into()), "stale partial", 0);
        state.apply_failed(
            &WorkerTurnId("old".into()),
            &CoreError::Application("stale failure".into()),
        );
        assert_eq!(state.input(), "new generation draft edited after enqueue");
        assert_eq!(state.history().rows().len(), 1);
        assert_eq!(state.active_turn(), Some(&WorkerTurnId("new".into())));
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert!(state.live_parts.is_empty());
    }

    #[tokio::test]
    async fn v05_review_overlimit_selection_during_pending_cannot_lose_draft() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("overlimit-pending"));
        let mut full = "a".repeat(MAX_INPUT_BYTES - 1);
        assert_eq!(state.handle_paste(&full).note, None);
        assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
        state.handle_key(KeyAction::Char('z')).await;
        full.push('z');
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        assert_eq!(text.len(), MAX_INPUT_BYTES);
        state.handle_key(KeyAction::SelectLeft).await;
        let outcome = state.handle_key(KeyAction::Char('🦊')).await;
        assert!(outcome.note.is_some(), "replacement cannot fit");
        assert_eq!(
            state.input(),
            full,
            "rejected replacement must not delete selection"
        );
        state.handle_key(KeyAction::Backspace).await;
        assert_eq!(state.input().len(), MAX_INPUT_BYTES - 1);
        assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
        ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
        state.poll_submission();
        assert_eq!(
            state.input().len(),
            MAX_INPUT_BYTES - 1,
            "acceptance must not clear revised draft"
        );
        assert_eq!(state.active_turn(), Some(&WorkerTurnId("accepted".into())));
        assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~1 lines] ");
    }

    #[tokio::test]
    async fn v05_review_chip_trim_keeps_visible_and_submitted_draft_in_sync() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("chip-trim"));
        let outcome = state.handle_paste("a\nb\nc\n");
        assert!(
            outcome
                .note
                .as_deref()
                .is_some_and(|n| n.contains("trimmed"))
        );
        assert_eq!(state.input(), "a\nb\nc");
        assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~3 lines] ");
        state.handle_key(KeyAction::Char('Z')).await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        assert_eq!(text, "a\nb\ncZ");
        ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
        state.poll_submission();
        assert_eq!(state.history().rows()[0].text, text);

        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("plain-paste"));
        state.handle_paste("a\nb\n");
        assert_eq!(state.input(), "a\nb\n", "non-chip paste retains whitespace");
        assert_eq!(state.prompt_layout(80).0.len(), 3);
    }

    #[tokio::test]
    async fn v05_review_trimmed_chip_edit_during_pending_keeps_receipt_immutable() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("trimmed-pending"));
        assert!(state.handle_paste("a\nb\nc\n").note.is_some());
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        assert_eq!(text, "a\nb\nc");
        state.handle_key(KeyAction::Char('Z')).await;
        state.handle_key(KeyAction::Enter).await; // pending cannot duplicate
        assert!(inbox.try_recv().is_err());
        ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
        state.poll_submission();
        assert_eq!(state.input(), "a\nb\ncZ");
        assert_eq!(state.prompt_layout(80).0[0].text, "[Pasted ~3 lines] Z");
        assert_eq!(state.history().rows()[0].text, text);
    }

    #[tokio::test]
    async fn v05_review_leader_cannot_swallow_exit() {
        let mut state = fresh_state("leader-exit").await;
        state.handle_key(KeyAction::Leader).await;
        state.handle_key(KeyAction::Interrupt).await;
        assert_eq!(state.status(), &TuiStatus::Quit);
    }

    #[tokio::test]
    async fn v05_review_shift_edges_select_entire_multiline_buffer() {
        let mut state = fresh_state("buffer-edges").await;
        state.handle_paste("один\nдва");
        state.handle_key(KeyAction::SelectHome).await;
        state.handle_key(KeyAction::Char('X')).await;
        assert_eq!(state.input(), "X");
        state.handle_key(KeyAction::Home).await;
        state.handle_key(KeyAction::SelectEnd).await;
        state.handle_key(KeyAction::Char('Y')).await;
        assert_eq!(state.input(), "Y");
    }

    #[tokio::test]
    async fn v05_review_empty_draft_up_recalls_durable_history() {
        let mut state = fresh_state("empty-recall").await;
        state.attach_page(&page(
            vec![msg(1, Role::User, "stored prompt")],
            1,
            false,
            false,
        ));
        assert_eq!(state.input(), "");
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.input(), "stored prompt");
        state.handle_key(KeyAction::Down).await;
        assert_eq!(state.input(), "");
    }

    #[tokio::test]
    async fn workspace_reset_invalidates_pending_state_without_waiting_for_old_receipt() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("pending"));
        type_text(&mut state, "retained draft").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        state.reset_workspace();
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(!state.is_busy());
        assert_eq!(state.input(), "retained draft");
        assert!(ack.send(Ok(WorkerTurnId("old".into()))).is_err());
        state.poll_submission();
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert_eq!(state.input(), "retained draft");
    }

    #[tokio::test]
    async fn pending_failure_keeps_draft_and_quit_is_not_overwritten() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("pending"));
        type_text(&mut state, "retry me").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("submission")
        };
        ack.send(Err(CoreError::Application("safe failure".into())))
            .unwrap();
        state.poll_submission();
        assert_eq!(state.input(), "retry me");
        assert_eq!(state.status(), &TuiStatus::Idle);
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("retry")
        };
        state.handle_key(KeyAction::Quit).await;
        ack.send(Ok(WorkerTurnId("accepted-at-quit".into())))
            .unwrap();
        state.poll_submission();
        assert_eq!(state.status(), &TuiStatus::Quit);
        assert_eq!(state.input(), "retry me");
    }

    fn snapshot() -> CatalogSnapshot {
        CatalogSnapshot {
            chrome: Default::default(),
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            provider: "ludka2".to_string(),
            models: vec![
                ModelEntry {
                    display_name: String::new(),
                    provider_name: String::new(),
                    price: None,
                    id: "a".to_string(),
                    variants: Vec::new(),
                    context: 1000,
                    context_known: true,
                    output_known: true,
                    output: 100,
                },
                ModelEntry {
                    display_name: String::new(),
                    provider_name: String::new(),
                    price: None,
                    id: "b".to_string(),
                    variants: vec![VariantEntry {
                        name: "low".to_string(),
                        disabled: false,
                        reasoning_effort: Some("low".to_string()),
                    }],
                    context: 1000,
                    context_known: true,
                    output_known: true,
                    output: 100,
                },
            ],
            model_id: "a".to_string(),
            variant: None,
            agents: vec![
                AgentEntry {
                    id: "x".to_string(),
                    color_index: 0,
                    description: "first profile".to_string(),
                    model: None,
                    variant: None,
                },
                AgentEntry {
                    id: "y".to_string(),
                    color_index: 1,
                    description: "second profile".to_string(),
                    model: Some("b".to_string()),
                    variant: None,
                },
            ],
            agent_id: Some("x".to_string()),
            commands: Vec::new(),
        }
    }

    #[tokio::test]
    async fn slash_overlay_commands_consume_alias_but_compress_keeps_input() {
        let mut state = fresh_state("s-slash").await;
        for (command, intent) in [
            ("/model", PanelIntent::LoadCatalog),
            ("/agents", PanelIntent::LoadCatalog),
            ("/sessions", PanelIntent::LoadSessions),
            ("/skills", PanelIntent::LoadSkills),
        ] {
            type_text(&mut state, command).await;
            let outcome = state.handle_key(KeyAction::Enter).await;
            assert_eq!(outcome.intent, Some(intent), "{command}");
            assert!(!outcome.consumed_input, "snapshot still pending");
            assert_eq!(
                state.input(),
                "",
                "opening an overlay consumes the alias, not its search query"
            );
            assert_eq!(outcome.note, None);
            state.accept_intent();
            state.close_panel();
        }

        type_text(&mut state, "/dcp-compress draft span").await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::Compress {
                focus: "draft span".to_string()
            })
        );
        assert!(!outcome.consumed_input);
        assert_eq!(state.input(), "/dcp-compress draft span");
        assert_eq!(state.panel(), &TuiPanel::Dcp);

        // Once the snapshots have arrived, opening a panel consumes input.
        let mut state = fresh_state("s-slash2").await;
        state.apply_catalog(snapshot());
        state.apply_sessions(vec!["s-slash2".to_string()]);
        state.apply_skills(vec![SkillCard {
            id: "sk".to_string(),
            name: "Skill".to_string(),
            description: "does things".to_string(),
        }]);
        for command in ["/model", "/agents", "/sessions", "/skills"] {
            type_text(&mut state, command).await;
            let outcome = state.handle_key(KeyAction::Enter).await;
            assert_eq!(
                outcome.intent,
                if command == "/sessions" {
                    Some(PanelIntent::LoadSessions)
                } else {
                    None
                },
                "{command}"
            );
            assert!(outcome.consumed_input, "{command}");
            assert!(state.input().is_empty(), "{command}");
            state.close_panel();
        }
        type_text(&mut state, "/quit").await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert!(outcome.consumed_input);
        assert_eq!(state.status(), &TuiStatus::Quit);
    }

    #[tokio::test]
    async fn accepted_submit_clears_input_and_streams() {
        let mut state = fresh_state("s-sub").await;
        let mut driver = ScriptDriver::attach(&state.app);

        type_text(&mut state, "hi").await;
        assert_eq!(state.input(), "hi");
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert!(!outcome.consumed_input);
        assert_eq!(outcome.note, None);
        assert_eq!(outcome.intent, None);
        assert_eq!(state.status(), &TuiStatus::PendingSubmission);
        assert_eq!(state.input(), "hi");
        await_submission(&mut state).await;
        assert!(state.input().is_empty());
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert!(state.is_busy());
        // Upstream user block: `┃` border plus 2-cell inner padding.
        assert!(state.viewport().iter().any(|line| line == "┃  hi"));

        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: hi".to_string()));
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(!state.is_busy());
        let view = state.viewport();
        assert!(view.iter().any(|line| line == "┃  hi"), "{view:?}");
        // Assistant markdown sits at paddingLeft=3.
        assert!(view.iter().any(|line| line == "   echo: hi"), "{view:?}");
    }

    #[tokio::test]
    async fn v04_pending_command_availability_preserves_receipt_focus_and_draft() {
        use crate::commands::CommandAction;
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app, sid("v04-pending"));
        state.apply_catalog(snapshot());
        type_text(&mut state, "pending prompt").await;
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
            panic!("submit")
        };
        state.handle_key(KeyAction::Commands).await;
        state.handle_paste("New session");
        assert!(state.modal_options()[0].footer.contains("turn active"));
        let outcome = state.handle_panel_key(KeyAction::Enter);
        assert_eq!(outcome.intent, None);
        assert_eq!(
            outcome.note.as_deref(),
            Some("turn active; action unavailable")
        );
        assert_eq!(state.panel(), &TuiPanel::Commands);
        state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(state.input(), "pending prompt");
        assert_eq!(state.status(), &TuiStatus::PendingSubmission);
        for action in [
            CommandAction::NewSession,
            CommandAction::OpenSessions,
            CommandAction::OpenModelPicker,
            CommandAction::OpenVariants,
            CommandAction::OpenAgents,
        ] {
            let outcome = state.run_command(action);
            assert_eq!(outcome.intent, None);
            assert!(outcome.note.unwrap().contains("turn active"));
            assert_eq!(state.panel(), &TuiPanel::None);
        }
        assert!(inbox.try_recv().is_err());
        ack.send(Ok(WorkerTurnId("receipt-intact".into()))).unwrap();
        state.poll_submission();
        assert_eq!(
            state.active_turn(),
            Some(&WorkerTurnId("receipt-intact".into()))
        );
        assert_eq!(state.history().rows()[0].text, "pending prompt");
        assert!(state.input().is_empty());
    }

    #[tokio::test]
    async fn v04_variant_current_focus_restores_after_clearing_search() {
        use crate::commands::CommandAction;
        let mut state = fresh_state("v04-variant").await;
        let mut catalog = snapshot();
        catalog.variant = Some("none".into());
        catalog.models[0].variants.push(VariantEntry {
            name: "none".into(),
            disabled: false,
            reasoning_effort: Some("low".into()),
        });
        state.apply_catalog(catalog.clone());
        state.run_command(CommandAction::OpenVariants);
        assert!(state.modal_options()[state.select.cursor].current);
        state.handle_paste("Default");
        assert!(!state.modal_options()[state.select.cursor].current);
        state.handle_panel_key(KeyAction::Interrupt);
        assert!(state.modal_options()[state.select.cursor].current);
        let outcome = state.handle_panel_key(KeyAction::Enter);
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::ChooseModel {
                id: "a".into(),
                variant: Some("none".into())
            })
        );
        state.model_choice_applied(catalog);
        assert_eq!(state.panel(), &TuiPanel::None);
    }

    #[tokio::test]
    async fn v04_modal_search_scroll_focus_and_draft() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::{Terminal, backend::TestBackend};
        let mut state = fresh_state("v04-draft").await;
        let mut catalog = snapshot();
        let model = catalog.models[0].clone();
        catalog.models = (0..30)
            .map(|i| {
                let mut m = model.clone();
                m.id = format!("m{i:02}");
                m.display_name = format!("Display {i:02}");
                m.provider_name = "Real provider".into();
                m
            })
            .collect();
        catalog.model_id = "m04".into();
        state.apply_catalog(catalog);
        type_text(&mut state, "kept draft 🌍").await;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|f| crate::views::render_frame(f, &state))
            .unwrap();
        let base = terminal.backend().buffer().clone();
        let key = crate::events::map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
            .unwrap();
        state.handle_key(key).await;
        assert_eq!(state.panel(), &TuiPanel::Commands);
        state.handle_paste("model");
        assert_eq!(
            state
                .modal_options()
                .iter()
                .map(|o| o.title.as_str())
                .collect::<Vec<_>>(),
            ["Switch model"]
        );
        state.handle_panel_key(KeyAction::Enter);
        assert_eq!(state.panel(), &TuiPanel::Model);
        terminal
            .draw(|f| crate::views::render_frame(f, &state))
            .unwrap();
        let modal = terminal.backend().buffer();
        for y in [0, 1, 35, 39] {
            for x in 0..120 {
                assert_eq!(
                    base[(x, y)].symbol(),
                    modal[(x, y)].symbol(),
                    "underlay reflow at {x},{y}"
                );
                assert_eq!(
                    modal[(x, y)].fg,
                    if base[(x, y)].symbol() == " " {
                        ratatui::style::Color::Rgb(255, 255, 255)
                    } else {
                        crate::dialog::backdrop(base[(x, y)].fg, crate::theme::Theme::dark().text())
                    }
                );
                assert_eq!(
                    modal[(x, y)].modifier,
                    if base[(x, y)].symbol() == " " {
                        ratatui::style::Modifier::empty()
                    } else {
                        base[(x, y)].modifier
                    }
                );
                assert_eq!(
                    modal[(x, y)].bg,
                    crate::dialog::backdrop(
                        base[(x, y)].bg,
                        crate::theme::Theme::dark().background()
                    )
                );
            }
        }
        for _ in 0..25 {
            state.handle_panel_key(KeyAction::Down);
        }
        assert_eq!(state.picker_selection().unwrap().0, "m25");
        let text = crate::views::render_test(&state, 120, 40).join("\n");
        assert!(text.contains("Display 25") && !text.contains("Display 00"));
        state.handle_paste("missing-no-results");
        assert!(state.modal_options().is_empty());
        assert_eq!(state.handle_panel_key(KeyAction::Enter).intent, None);
        assert!(
            crate::views::render_test(&state, 120, 40)
                .join("\n")
                .contains("No results found")
        );
        state.handle_panel_key(KeyAction::Interrupt); // clears only the filter
        state.handle_paste("Display 29");
        assert_eq!(state.modal_options().len(), 1);
        assert_eq!(state.picker_selection().unwrap().0, "m29");
        state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(state.input(), "kept draft 🌍");
        assert_eq!(state.active_model_label().unwrap().0, "Display 04");
        assert_eq!(state.status(), &TuiStatus::Idle);
        state.handle_key(KeyAction::Leader).await;
        state.handle_key(KeyAction::Char('m')).await;
        assert_eq!(state.panel(), &TuiPanel::Model);
        state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(state.input(), "kept draft 🌍");
    }

    #[tokio::test]
    async fn v04_mouse_scroll_hover_and_drag_release_keep_modal_owner() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let mut state = fresh_state("v04-pointer").await;
        let mut catalog = snapshot();
        let model = catalog.models[0].clone();
        catalog.models = (0..20)
            .map(|i| {
                let mut entry = model.clone();
                entry.id = format!("m{i:02}");
                entry.display_name = format!("Mouse {i:02}");
                entry
            })
            .collect();
        catalog.model_id = "m00".into();
        state.apply_catalog(catalog);
        type_text(&mut state, "draft stays").await;
        state.handle_key(KeyAction::Leader).await;
        state.handle_key(KeyAction::Char('m')).await;
        assert_eq!(state.panel(), &TuiPanel::Model);
        let area = Rect::new(0, 0, 80, 24);
        let event = |kind, column, row| MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        let pointer =
            |state: &mut TuiState, kind, x, y| state.handle_mouse(event(kind, x, y), area);
        // Second ungrouped option is at y=12 (the list starts at y=11).
        pointer(&mut state, MouseEventKind::Moved, 20, 12);
        assert_eq!(state.picker_selection().unwrap().0, "m01");
        pointer(&mut state, MouseEventKind::ScrollDown, 20, 12);
        assert_eq!(
            state.picker_selection().unwrap().0,
            "m01",
            "wheel scrolls without selecting"
        );
        let options = state.modal_options();
        assert_eq!(
            state
                .select
                .hit(area, crate::dialog::DialogSize::Medium, &options, 20, 12),
            crate::dialog::DialogHit::Option(4)
        );
        pointer(&mut state, MouseEventKind::Moved, 20, 12);
        assert_eq!(state.picker_selection().unwrap().0, "m04");
        // Releasing over the backdrop after starting a text selection inside
        // the dialog must not dismiss it or submit an option.
        pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
        pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 0, 0);
        assert_eq!(state.panel(), &TuiPanel::Model);
        assert_eq!(state.input(), "draft stays");
        assert_eq!(
            pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
            None
        );
        pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
        assert_eq!(
            pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
            Some(PanelIntent::SelectModel { id: "m04".into() })
        );
        assert_eq!(
            pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
            None,
            "release cannot submit the same option twice"
        );
        // A held press cannot cross a keyboard-driven panel replacement.
        pointer(&mut state, MouseEventKind::Down(MouseButton::Left), 20, 12);
        state.open_variants();
        assert_eq!(
            pointer(&mut state, MouseEventKind::Up(MouseButton::Left), 20, 12).intent,
            None,
            "release in replacement dialog cannot select a variant"
        );
        state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(state.input(), "draft stays");
        state.handle_key(KeyAction::Char('!')).await;
        assert_eq!(state.input(), "draft stays!");
    }

    #[tokio::test]
    async fn v04_mouse_status_rows_do_not_start_compression() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;
        let mut state = fresh_state("v04-dcp-mouse").await;
        state.apply_catalog(snapshot());
        state.panel = TuiPanel::Dcp;
        let area = Rect::new(0, 0, 80, 24);
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            let outcome = state.handle_mouse(
                MouseEvent {
                    kind,
                    column: 20,
                    row: 11,
                    modifiers: KeyModifiers::NONE,
                },
                area,
            );
            assert_eq!(outcome.intent, None);
        }
        assert_eq!(state.panel(), &TuiPanel::Dcp);
    }

    #[tokio::test]
    async fn exploration_mouse_hits_only_visible_header_text_without_drag_or_modal_leak() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let mut state = fresh_state("exploration-mouse").await;
        for (i, name) in ["read", "glob", "grep"].iter().enumerate() {
            let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
                rowid: i as i64 + 1,
                op: format!("op-{i}"),
                name: (*name).into(),
                state: "completed".into(),
                input: Some(
                    if *name == "read" {
                        r#"{"path":"fixture-note.txt"}"#
                    } else {
                        r#"{"pattern":"*.rs"}"#
                    }
                    .into(),
                ),
                output: Some("fixture result".into()),
                output_bytes: 14,
                output_truncated: false,
            });
            state.window.push_row(crate::history::HistoryRow {
                seq: i as i64 + 1,
                role: "tool".into(),
                text: String::new(),
                agent: None,
                agent_color_index: None,
                chips: vec![],
                reasoning: None,
                meta: None,
                tool: Some(card),
            });
        }
        let event = |kind, x, y| MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        let click = |state: &mut TuiState, area: ratatui::layout::Rect, x, y| {
            state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
            state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
        };
        for area in [
            ratatui::layout::Rect::new(0, 0, 80, 24),
            ratatui::layout::Rect::new(0, 0, 121, 40),
            ratatui::layout::Rect::new(0, 0, 25, 24),
        ] {
            state.exploration_expanded.clear();
            let transcript = crate::shell::transcript_area(&state, area);
            let (lines, total) =
                state.visible_transcript(transcript.width, area.width, transcript.height);
            state.observe_viewport(transcript.height, total);
            let row = lines
                .iter()
                .position(|line| line.plain_text().contains("Explored"))
                .expect("header visible");
            let x = transcript.x + 4;
            let y = transcript.y + row as u16;
            assert!(
                !state
                    .transcript_lines(80, 80)
                    .iter()
                    .any(|line| line.plain_text().contains("Read fixture-note.txt"))
            );
            click(&mut state, area, x, y - 1); // vertical gap
            click(&mut state, area, transcript.x, y); // left padding
            click(&mut state, area, transcript.right() - 1, y); // blank row tail
            click(&mut state, area, x, transcript.bottom()); // status/prompt
            assert!(state.exploration_expanded.is_empty());
            state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
            state.handle_mouse(
                event(MouseEventKind::Drag(MouseButton::Left), x + 1, y),
                area,
            );
            state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x, y), area);
            assert!(state.exploration_expanded.is_empty());
            state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
            state.handle_mouse(
                event(
                    MouseEventKind::Up(MouseButton::Left),
                    x,
                    transcript.bottom(),
                ),
                area,
            );
            assert!(state.exploration_expanded.is_empty());
            state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x, y), area);
            state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left), x + 1, y), area);
            assert!(
                state.exploration_expanded.is_empty(),
                "text selection cannot toggle"
            );
            state.panel = TuiPanel::Help(None);
            click(&mut state, area, x, y);
            assert!(state.exploration_expanded.is_empty());
            state.close_panel();
            click(&mut state, area, x, y);
            assert!(state.exploration_expanded.contains("op-0"));
            assert!(
                state
                    .transcript_lines(80, 80)
                    .iter()
                    .any(|line| line.plain_text().contains("Read fixture-note.txt"))
            );
            click(&mut state, area, x, y);
            assert!(state.exploration_expanded.is_empty());
            if area.width == 25 {
                let continuation = lines[row + 1].plain_text();
                assert!(
                    continuation.contains("search"),
                    "header must wrap at the actual content width: {continuation}"
                );
                click(&mut state, area, transcript.x + 1, y + 1);
                assert!(state.exploration_expanded.contains("op-0"));
                click(&mut state, area, transcript.x + 1, y + 1);
                assert!(state.exploration_expanded.is_empty());
            }
        }
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        for i in 0..20 {
            state
                .window
                .push_synthetic("assistant", &format!("later message {i}"), None, None);
        }
        let rect = crate::shell::transcript_area(&state, area);
        let (_, total) = state.visible_transcript(rect.width, area.width, rect.height);
        assert!(total > rect.height as usize);
        assert!(
            state
                .exploration_hit(area, rect.x + 4, rect.y + 1)
                .is_none(),
            "offscreen group is not clickable"
        );
        state.scroll = total - rect.height as usize;
        let (lines, _) = state.visible_transcript(rect.width, area.width, rect.height);
        let row = lines
            .iter()
            .position(|line| line.plain_text().contains("Explored"))
            .unwrap();
        click(&mut state, area, rect.x + 4, rect.y + row as u16);
        assert!(state.exploration_expanded.contains("op-0"));
        let (expanded, _) = state.visible_transcript(rect.width, area.width, rect.height);
        assert!(expanded[row].plain_text().contains("Explored"));
        click(&mut state, area, rect.x + 4, rect.y + row as u16);
        assert!(state.exploration_expanded.is_empty());
        assert_eq!(state.scroll, total - rect.height as usize);
        click(&mut state, area, rect.x + 4, rect.y + row as u16);
        state.set_session(sid("other-session"));
        assert!(state.exploration_expanded.is_empty());
    }

    #[tokio::test]
    async fn exploration_toggle_anchors_long_result_and_attach_page_discards_expansion() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let mut state = fresh_state("exploration-anchor").await;
        for i in 0..16 {
            let result = format!("loaded result {i}: {}", "contents ".repeat(30));
            let card = crate::history::card_from_row(&oc_core::queries::ToolOpView {
                rowid: i + 1,
                op: format!("read-{i}"),
                name: "read".into(),
                state: "completed".into(),
                input: Some(format!(r#"{{"path":"file-{i}.txt"}}"#)),
                output_bytes: result.len() as i64,
                output: Some(result),
                output_truncated: false,
            });
            state.window.push_row(crate::history::HistoryRow {
                seq: i + 1,
                role: "tool".into(),
                text: String::new(),
                agent: None,
                agent_color_index: None,
                chips: vec![],
                reasoning: None,
                meta: None,
                tool: Some(card),
            });
        }
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let rect = crate::shell::transcript_area(&state, area);
        let (collapsed, before) = state.visible_transcript(rect.width, area.width, rect.height);
        assert!(before < rect.height as usize);
        let header_y = collapsed
            .iter()
            .position(|line| line.plain_text().contains("Explored"))
            .unwrap() as u16;
        let x = rect.x + 4;
        let y = rect.y + header_y;
        let click = |state: &mut TuiState| {
            for kind in [
                MouseEventKind::Down(MouseButton::Left),
                MouseEventKind::Up(MouseButton::Left),
            ] {
                state.handle_mouse(
                    MouseEvent {
                        kind,
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    },
                    area,
                );
            }
        };
        click(&mut state);
        assert!(state.exploration_expanded.contains("read-0"));
        let (expanded, after) = state.visible_transcript(rect.width, area.width, rect.height);
        assert!(after > rect.height as usize);
        assert!(
            state.scroll() > 0,
            "expansion must leave the bottom to keep the header visible"
        );
        assert!(
            expanded[header_y as usize]
                .plain_text()
                .contains("Explored")
        );
        assert_eq!(state.exploration_hit(area, x, y).as_deref(), Some("read-0"));
        click(&mut state);
        assert!(state.exploration_expanded.is_empty());
        assert_eq!(state.scroll(), 0);
        let (restored, total) = state.visible_transcript(rect.width, area.width, rect.height);
        assert_eq!((restored, total), (collapsed, before));

        click(&mut state);
        assert!(state.exploration_expanded.contains("read-0"));
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        state.attach_page(&page(vec![], 0, false, false));
        assert!(state.exploration_expanded.is_empty());
        assert!(state.exploration_down.is_none());
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
        assert!(state.exploration_expanded.is_empty());
    }

    #[tokio::test]
    async fn panel_enter_returns_selection_intents() {
        let mut state = fresh_state("s-panels").await;
        state.apply_catalog(snapshot());

        type_text(&mut state, "/model").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.panel(), &TuiPanel::Model);
        state.handle_panel_key(KeyAction::Down);
        let outcome = state.handle_panel_key(KeyAction::Enter);
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::SelectModel {
                id: "b".to_string(),
            })
        );
        assert_eq!(state.picker_selection(), Some(("b".to_string(), None)));
        state.accept_intent();
        state.close_panel();

        type_text(&mut state, "/agents").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.panel(), &TuiPanel::Agents);
        state.handle_panel_key(KeyAction::Down);
        let outcome = state.handle_panel_key(KeyAction::Enter);
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::SelectAgent {
                id: "y".to_string()
            })
        );
        state.accept_intent();

        state.apply_sessions(vec!["s1".to_string(), "s2".to_string()]);
        state.close_panel();
        type_text(&mut state, "/sessions").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.panel(), &TuiPanel::Sessions);
        assert_eq!(state.sessions_cursor(), 0);
        state.handle_panel_key(KeyAction::Down);
        let outcome = state.handle_panel_key(KeyAction::Enter);
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::SwitchSession {
                id: "s2".to_string()
            })
        );

        // Esc closes without an intent; DCP Enter asks for a compress.
        let outcome = state.handle_panel_key(KeyAction::Cancel);
        assert_eq!(outcome, KeyOutcome::default());
        assert_eq!(state.panel(), &TuiPanel::None);
    }

    #[tokio::test]
    async fn scroll_edges_request_pages() {
        let mut state = fresh_state("s-scroll").await;

        // Fewer rows than the viewport: the top edge is already reached.
        state.attach_page(&page(
            vec![msg(1, Role::User, "m1"), msg(2, Role::Assistant, "m2")],
            9,
            true,
            false,
        ));
        assert!(state.needs_older());
        let outcome = state.scroll_transcript(true);
        assert_eq!(outcome.intent, Some(PanelIntent::LoadOlder));

        // A window that evicted its newest rows asks for a newer page at the
        // bottom edge, never while scrolling inside the window.
        let bulk: Vec<HistoryMessage> = (0..WINDOW_ROWS + 2)
            .map(|i| msg(100 + i as i64, Role::User, "older"))
            .collect();
        state.prepend_page(&page(bulk, 500, true, true));
        assert!(state.needs_newer());
        let outcome = state.scroll_transcript(false);
        assert_eq!(outcome.intent, Some(PanelIntent::LoadNewer));

        state.append_page(&page(
            vec![msg(999, Role::User, "newest")],
            500,
            true,
            false,
        ));
        assert!(!state.needs_newer());
        let outcome = state.scroll_transcript(true);
        assert_eq!(outcome.intent, None, "only the top edge loads older");

        let mut requested = None;
        // Blocks render as multiple lines (blank/border rows), so walk the
        // whole rendered transcript to reach the top edge.
        let max_scroll = state.max_scroll();
        for _ in 0..=max_scroll {
            let outcome = state.scroll_transcript(true);
            if outcome.intent.is_some() {
                requested = outcome.intent;
                break;
            }
        }
        assert_eq!(requested, Some(PanelIntent::LoadOlder));

        // Nothing older exists: the clamp keeps scrolling usable.
        let mut state = fresh_state("s-scroll2").await;
        state.attach_page(&page(
            (0..WINDOW_ROWS)
                .map(|i| msg(i as i64, Role::Assistant, "filler"))
                .collect(),
            100,
            false,
            false,
        ));
        let max_scroll = state.max_scroll();
        for _ in 0..=max_scroll {
            assert_eq!(state.scroll_transcript(true).intent, None);
        }
        assert_eq!(state.scroll, state.max_scroll());
        assert!(!state.needs_older());
    }

    #[tokio::test]
    async fn retained_bytes_are_bounded_after_many_pages() {
        let mut state = fresh_state("s-retained").await;
        assert!(state.retained_bytes() < WINDOW_BYTES);
        let blob = "z".repeat(2048);
        for round in 0..60 {
            let older: Vec<HistoryMessage> = (0..8)
                .map(|i| msg(round as i64 * 16 + i, Role::User, &blob))
                .collect();
            let newer: Vec<HistoryMessage> = (0..8)
                .map(|i| msg(10_000 + round as i64 * 16 + i, Role::Assistant, &blob))
                .collect();
            state.prepend_page(&page(older, 100_000, true, true));
            state.append_page(&page(newer, 100_000, true, true));
            assert!(
                state.retained_bytes() <= WINDOW_BYTES + MAX_INPUT_BYTES,
                "round {round}: {} bytes",
                state.retained_bytes()
            );
            assert!(state.viewport().len() <= VIEWPORT_LINES);
        }
    }

    #[tokio::test]
    async fn viewport_is_bounded_with_unicode() {
        let mut state = fresh_state("s-uni").await;
        let rows: Vec<HistoryMessage> = (0..80)
            .map(|i| {
                let role = if i % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                };
                msg(i, role, "привет 🌍 мир")
            })
            .collect();
        state.attach_page(&page(rows, 80, true, false));
        type_text(&mut state, "next…").await;

        let view = state.viewport();
        assert!(view.len() <= VIEWPORT_LINES);
        assert!(
            view.iter().any(|line| line.contains("привет 🌍")),
            "{view:?}"
        );
        assert!(view.iter().any(|line| line == "┃  привет 🌍 мир"));
        assert_eq!(state.input(), "next…");
    }

    #[tokio::test]
    async fn v02_multiround_reasoning_keeps_arrival_order() {
        let mut state = fresh_state("ordered").await;
        let turn = WorkerTurnId("turn".into());
        state.active_turn = Some(turn.clone());
        state.apply_reasoning_delta(&turn, "first thought");
        state.apply_tool_started(&turn, "op", "read", "{}");
        state.apply_reasoning_delta(&turn, "second thought");
        state.apply_delta(&turn, "answer");
        state.apply_finished(&turn, "answer", 1);
        let kinds: Vec<_> = state
            .history()
            .rows()
            .iter()
            .map(|r| {
                if let Some(reasoning) = &r.reasoning {
                    reasoning.text.as_str()
                } else if r.tool.is_some() {
                    "tool"
                } else if r.meta.is_some() {
                    "footer"
                } else {
                    r.text.as_str()
                }
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "first thought",
                "tool",
                "second thought",
                "answer",
                "footer"
            ]
        );
    }

    #[tokio::test]
    async fn stale_turn_events_are_ignored() {
        let mut state = fresh_state("s-stale").await;
        let mut driver = ScriptDriver::attach(&state.app);
        type_text(&mut state, "go").await;
        state.handle_key(KeyAction::Enter).await;
        await_submission(&mut state).await;
        assert_eq!(state.status(), &TuiStatus::Streaming);

        let stale = WorkerTurnId("t-stale".to_string());
        state.apply_delta(&stale, "junk");
        state.apply_finished(&stale, "junk", 0);
        state.apply_interrupted(&stale, "junk", 0);
        state.apply_failed(&stale, &CoreError::TurnBusy);
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert!(state.is_busy());
        assert!(!state.viewport().iter().any(|line| line.contains("junk")));

        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: go".to_string()));
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(state.viewport().iter().any(|line| line == "   echo: go"));

        // A fresh submit is accepted right after the finish.
        type_text(&mut state, "go2").await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert_eq!(outcome.note, None);
        await_submission(&mut state).await;
        assert_eq!(state.status(), &TuiStatus::Streaming);
        let _ = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(state.status(), &TuiStatus::Idle);
    }

    /// Tool-call events build transcript cards from real event payloads, in
    /// upstream part order (text, tool, text) and survive the turn finish as
    /// committed rows with the footer after them.
    #[tokio::test]
    async fn tool_events_build_transcript_cards_in_part_order() {
        let mut state = fresh_state("s-tools-ui").await;
        state.active_agent = Some("build".to_string());
        state.active_turn = Some(WorkerTurnId("t-ui-tools".to_string()));
        state.status = TuiStatus::Streaming;

        state.apply_delta(&WorkerTurnId("t-ui-tools".to_string()), "working");
        let patch = "*** Begin Patch\n*** Update File: a.txt\n@@\n-old\n+new\n*** End Patch";
        let input = serde_json::json!({"patchText": patch}).to_string();
        state.apply_tool_started(
            &WorkerTurnId("t-ui-tools".to_string()),
            "op-1",
            "apply_patch",
            &input,
        );
        // The running card is already a transcript row with the parsed diff.
        let rows = state.transcript_rows();
        assert_eq!(rows.len(), 2, "text part then tool card: {rows:?}");
        assert_eq!(rows[0].role, "assistant");
        assert_eq!(rows[0].text, "working");
        assert_eq!(rows[1].role, "tool");
        assert_eq!(rows[1].tool.as_ref().expect("card").state, "started");
        let lines = state.transcript_lines(0, 80);
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().contains("← Patched a.txt")),
            "the diff card is visible while running"
        );

        state.apply_tool_finished(
            &WorkerTurnId("t-ui-tools".to_string()),
            "op-1",
            "apply_patch",
            "completed",
            "Update a.txt (hash_before=x, hash_after=y)",
            42,
            false,
        );
        state.apply_delta(&WorkerTurnId("t-ui-tools".to_string()), "after tool");
        let rows = state.transcript_rows();
        assert_eq!(rows.len(), 3, "text, card, open text: {rows:?}");
        assert_eq!(rows[1].tool.as_ref().expect("card").state, "completed");
        assert_eq!(rows[2].text, "after tool");

        state.apply_finished(
            &WorkerTurnId("t-ui-tools".to_string()),
            "workingafter tool",
            2500,
        );
        assert!(!state.is_busy());
        let window = state.history().rows();
        assert_eq!(window.len(), 4, "three parts plus the footer: {window:?}");
        assert_eq!(window[0].role, "assistant");
        assert_eq!(window[0].text, "working");
        assert_eq!(window[1].role, "tool");
        assert_eq!(window[1].tool.as_ref().expect("card").state, "completed");
        assert_eq!(window[2].text, "after tool");
        assert_eq!(window[3].role, "assistant");
        assert!(window[3].text.is_empty(), "footer row carries no text");
        assert_eq!(
            window[3].meta.as_ref().expect("meta").duration_ms,
            Some(2500)
        );
        // Footer renders after every part (`routes/session/index.tsx:1934-1985`).
        let lines = state.transcript_lines(0, 80);
        let texts: Vec<String> = lines.iter().map(|line| line.plain_text()).collect();
        let footer = texts
            .iter()
            .position(|text| text.contains("Build"))
            .expect("footer");
        let diff = texts
            .iter()
            .position(|text| text.contains("← Patched"))
            .expect("diff");
        assert!(footer > diff, "footer follows the parts: {texts:?}");
    }

    /// Live parts stay bounded when a hostile stream floods tool events.
    #[tokio::test]
    async fn live_parts_stay_bounded_under_tool_flood() {
        let mut state = fresh_state("s-tools-flood").await;
        state.active_turn = Some(WorkerTurnId("t-flood".to_string()));
        state.status = TuiStatus::Streaming;
        for index in 0..(LIVE_PARTS_MAX + 20) {
            state.apply_tool_started(
                &WorkerTurnId("t-flood".to_string()),
                &format!("op-{index}"),
                "read",
                &serde_json::json!({"path": format!("f{index}")}).to_string(),
            );
            state.apply_tool_finished(
                &WorkerTurnId("t-flood".to_string()),
                &format!("op-{index}"),
                "read",
                "completed",
                "ok",
                2,
                false,
            );
        }
        assert!(state.live_parts.len() <= LIVE_PARTS_MAX);
        assert!(
            state
                .viewport()
                .iter()
                .any(|line| line.contains("Live preview truncated"))
        );
        assert!(state.retained_bytes() <= 2 * WINDOW_BYTES + MAX_INPUT_BYTES);
        assert!(state.viewport().len() <= VIEWPORT_LINES);
    }

    /// A stale turn can never grow cards into the transcript.
    #[tokio::test]
    async fn stale_tool_events_are_ignored() {
        let mut state = fresh_state("s-tools-stale").await;
        state.active_turn = Some(WorkerTurnId("t-live".to_string()));
        state.status = TuiStatus::Streaming;
        let stale = WorkerTurnId("t-stale".to_string());
        state.apply_tool_started(
            &stale,
            "op",
            "bash",
            &serde_json::json!({"argv": ["ls"]}).to_string(),
        );
        state.apply_tool_finished(&stale, "op", "bash", "completed", "ok", 2, false);
        assert!(state.live_parts.is_empty());
        assert!(state.transcript_rows().is_empty());
    }

    #[tokio::test]
    async fn cancel_releases_turn() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            (0..20).map(|i| format!("t{i} ")).collect(),
            10,
        ));
        std::mem::forget(guard);
        app.create_session(sid("s-cancel")).await.expect("create");
        let mut state = TuiState::new(app.clone(), sid("s-cancel"));
        let mut driver = ScriptDriver::attach(&app);

        type_text(&mut state, "long").await;
        state.handle_key(KeyAction::Enter).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        let outcome = state.handle_key(KeyAction::Cancel).await;
        assert_eq!(outcome.note, None);

        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert!(matches!(outcome, PumpOutcome::Interrupted(_)));
        assert_eq!(state.status(), &TuiStatus::Cancelled);
        assert!(!state.is_busy());
        // Interrupted turn: partial text plus the footer's `interrupted` marker
        // (upstream `Step interrupted`, `routes/session/index.tsx:1955,1977-1980`).
        assert!(
            state
                .viewport()
                .iter()
                .any(|line| line.contains("interrupted")),
            "{:?}",
            state.viewport()
        );

        // The same handle accepts the next prompt.
        type_text(&mut state, "after").await;
        state.handle_key(KeyAction::Enter).await;
        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        let expected: String = (0..20).map(|i| format!("t{i} ")).collect();
        assert_eq!(outcome, PumpOutcome::Finished(expected));
        assert_eq!(state.status(), &TuiStatus::Idle);
    }
}
