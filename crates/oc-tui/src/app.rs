//! Bounded chat state over the shared `CoreApp` handle.
//!
//! The view never touches storage: history pages, catalogs, skills and DCP
//! snapshots arrive as bounded application DTOs, and user choices leave as
//! [`PanelIntent`] values that the binary applies through the application
//! API (then reports acceptance or failure). Worker event draining stays in
//! the binary; the state only applies turn-scoped events, so a late event
//! for a stale turn can never corrupt the view.

use std::collections::BTreeMap;
use std::time::Duration;

use oc_adapters::models::ModelCatalog;
use oc_core::core_app::{CoreApp, CoreEvent, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_core::queries::{AgentEntry, CatalogSnapshot, DcpSnapshot, HistoryPage, SkillCard};
use oc_core::session::CoreError;

use crate::commands::{CommandAction, dispatch};
use crate::dcp_panel::{DcpOutcome, DcpPanelState};
use crate::events::KeyAction;
use crate::history::{HistoryRow, HistoryWindow, ToolCard, WINDOW_BYTES};
use crate::picker::ModelPicker;

/// Visible lines kept in the viewport (scroll window).
pub const VIEWPORT_LINES: usize = 20;
/// Bounded input buffer (bytes): the core input budget, so the view never
/// drops bytes the runtime would have accepted.
pub const MAX_INPUT_BYTES: usize = oc_core::session::MAX_INPUT_BYTES;
/// Max card rows retained by the Cards panel.
pub const CARDS_MAX: usize = 160;

/// TUI status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiStatus {
    /// Ready for input.
    Idle,
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
    /// No panel (chat view).
    None,
    /// Model picker (UI02).
    Model,
    /// Primary agent selector.
    Agents,
    /// Session list with resume (UI03).
    Sessions,
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

/// Bounded chat state bound to one session on the shared handle.
pub struct TuiState {
    app: CoreApp,
    session: SessionId,
    status: TuiStatus,
    panel: TuiPanel,
    input: String,
    window: HistoryWindow,
    live_text: String,
    scroll: usize,
    note: Option<String>,
    active_turn: Option<WorkerTurnId>,
    /// Model picker (present while the Model panel lives).
    pub(crate) picker: Option<ModelPicker>,
    catalog_loaded: bool,
    /// Agent profiles from the catalog snapshot.
    pub(crate) agents: Vec<AgentEntry>,
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
    /// Cards cursor.
    pub(crate) cards_cursor: usize,
    cards_loaded: bool,
    cards_has_older: bool,
}

impl TuiState {
    /// Bind to a session; the session must already exist on the handle.
    pub fn new(app: CoreApp, session: SessionId) -> Self {
        Self {
            app,
            session,
            status: TuiStatus::Idle,
            panel: TuiPanel::None,
            input: String::new(),
            window: HistoryWindow::new(),
            live_text: String::new(),
            scroll: 0,
            note: None,
            active_turn: None,
            picker: None,
            catalog_loaded: false,
            agents: Vec::new(),
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
            cards_cursor: 0,
            cards_loaded: false,
            cards_has_older: false,
        }
    }

    /// Attached session id.
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// Switch to another session after an accepted switch: clears view state
    /// and the history window; status returns to `Idle` unless quitting.
    pub fn set_session(&mut self, session: SessionId) {
        self.session = session;
        self.input.clear();
        self.window = HistoryWindow::new();
        self.live_text.clear();
        self.scroll = 0;
        self.active_turn = None;
        self.panel = TuiPanel::None;
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

    /// Current input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Active turn, if any.
    pub fn active_turn(&self) -> Option<&WorkerTurnId> {
        self.active_turn.as_ref()
    }

    /// True while a turn streams.
    pub fn is_busy(&self) -> bool {
        self.active_turn.is_some()
    }

    /// Status note, if any (intent errors and hints; never chat history).
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Newest page becomes the whole window; scroll pins to the newest row.
    pub fn attach_page(&mut self, page: &HistoryPage) {
        self.window.reset(page);
        self.scroll = 0;
    }

    /// Add an older page at the front of the window.
    pub fn prepend_page(&mut self, page: &HistoryPage) {
        self.window.prepend_older(page);
    }

    /// Add a newer page at the back of the window.
    pub fn append_page(&mut self, page: &HistoryPage) {
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
    }

    /// Window bytes plus live text plus input; bounded by the window caps.
    pub fn retained_bytes(&self) -> usize {
        self.window.retained_bytes() + self.live_text.len() + self.input.len()
    }

    /// Visible viewport lines (bounded, scroll-aware, live answer last).
    pub fn viewport(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .window
            .rows()
            .iter()
            .map(|row| {
                if row.role.is_empty() {
                    row.text.clone()
                } else {
                    format!("{}: {}", row.role, row.text)
                }
            })
            .collect();
        if !self.live_text.is_empty() {
            lines.push(format!("ai: {}", self.live_text));
        }
        let total = lines.len();
        let max_scroll = total.saturating_sub(VIEWPORT_LINES);
        let scroll = self.scroll.min(max_scroll);
        let end = total - scroll;
        let start = end.saturating_sub(VIEWPORT_LINES);
        lines[start..end].to_vec()
    }

    // ---- snapshots from the binary -------------------------------------

    /// Apply a catalog snapshot: picker, agents and the effective selection.
    pub fn apply_catalog(&mut self, snapshot: CatalogSnapshot) {
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
        self.picker = Some(picker);
        self.commands = snapshot.commands;
        self.agents = snapshot.agents;
        self.agents_cursor = snapshot
            .agent_id
            .as_ref()
            .and_then(|id| self.agents.iter().position(|agent| &agent.id == id))
            .unwrap_or(0);
        self.catalog_loaded = true;
    }

    /// Apply the session list snapshot.
    pub fn apply_sessions(&mut self, sessions: Vec<String>) {
        self.sessions = sessions;
        self.sessions_cursor = 0;
        self.sessions_loaded = true;
    }

    /// Apply the skill card snapshot (bodies never reach the view).
    pub fn apply_skills(&mut self, cards: Vec<SkillCard>) {
        self.skills = cards;
        self.skills_cursor = 0;
        self.skills_loaded = true;
    }

    /// Apply a DCP context/stats snapshot.
    pub fn apply_dcp_snapshot(&mut self, snapshot: DcpSnapshot) {
        self.dcp.set_snapshot(snapshot);
    }

    /// Apply a newest-first tool-card page; rendered as bounded rows.
    pub fn apply_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        self.cards = cards.iter().map(card_row).collect();
        self.cards_cursor = 0;
        self.cards_loaded = true;
        self.cards_has_older = has_older;
    }

    /// Prepend an older tool-card page (paging up in the Cards panel).
    pub fn prepend_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
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

    /// Handle a bracketed paste as one bounded event (never per-char).
    pub fn handle_paste(&mut self, text: &str) -> KeyOutcome {
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        let room = MAX_INPUT_BYTES.saturating_sub(self.input.len());
        let kept = crate::truncate_utf8(text, room);
        let dropped = text.len().saturating_sub(kept.len());
        self.input.push_str(kept);
        if dropped == 0 {
            return KeyOutcome::default();
        }
        KeyOutcome {
            note: Some(format!(
                "paste truncated: {dropped} bytes dropped at the {MAX_INPUT_BYTES} byte input limit"
            )),
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

    /// Report that an intent was accepted and applied; clears the input.
    pub fn accept_intent(&mut self) {
        self.input.clear();
    }

    /// The accepted compress turn starts streaming: status, turn, DCP panel.
    pub fn begin_compress_turn(&mut self, turn: WorkerTurnId) {
        self.active_turn = Some(turn);
        self.status = TuiStatus::Streaming;
        self.panel = TuiPanel::Dcp;
        self.input.clear();
        self.live_text.clear();
        self.scroll = 0;
        self.push_note("dcp: compressing…");
    }

    /// Set the transient status note.
    pub fn push_note(&mut self, note: &str) {
        self.note = Some(note.to_string());
    }

    /// Model under the picker cursor; the active variant is preserved when
    /// the cursor still points at the selected model.
    pub fn picker_selection(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        let id = picker.cursor_id()?;
        if let Some(pending) = picker.pending_variant() {
            return Some((id, Some(pending)));
        }
        let variant = picker
            .selection()
            .filter(|selection| selection.id == id)
            .and_then(|selection| selection.variant.as_ref())
            .map(|variant| variant.name.clone());
        Some((id, variant))
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
        match action {
            KeyAction::Left | KeyAction::Right => KeyOutcome::default(),
            KeyAction::Char(c) => {
                if self.input.len() + c.len_utf8() <= MAX_INPUT_BYTES {
                    self.input.push(c);
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
                self.input.pop();
                KeyOutcome::default()
            }
            KeyAction::Up => {
                let max_scroll = self.max_scroll();
                if self.scroll < max_scroll {
                    self.scroll += 1;
                }
                let intent = (self.window.has_older() && self.scroll >= self.max_scroll())
                    .then_some(PanelIntent::LoadOlder);
                KeyOutcome {
                    intent,
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Down => {
                if self.scroll > 0 {
                    self.scroll -= 1;
                    KeyOutcome::default()
                } else {
                    KeyOutcome {
                        intent: self.window.has_newer().then_some(PanelIntent::LoadNewer),
                        ..KeyOutcome::default()
                    }
                }
            }
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.active_turn.is_some() {
                    match self.app.cancel(self.session.clone()).await {
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
        }
    }

    async fn handle_enter(&mut self) -> KeyOutcome {
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return KeyOutcome::default();
        }
        if let Some(action) = dispatch(&text) {
            // Workspace commands reach the application, which owns their
            // templates; the built-in table only routes known commands.
            if !matches!(action, CommandAction::Help(None)) || !self.is_workspace_command(&text) {
                return self.run_command(action);
            }
        }
        match self.app.submit(self.session.clone(), text.clone()).await {
            Ok(turn) => {
                self.window.push_synthetic("you", &text);
                self.live_text.clear();
                self.active_turn = Some(turn);
                self.status = TuiStatus::Streaming;
                self.scroll = 0;
                self.input.clear();
                self.dcp.clear_notice();
                self.note = None;
                KeyOutcome {
                    consumed_input: true,
                    ..KeyOutcome::default()
                }
            }
            Err(CoreError::TurnBusy) => KeyOutcome {
                note: Some("turn busy".to_string()),
                ..KeyOutcome::default()
            },
            Err(error) => KeyOutcome {
                note: Some(format!("submit: {error}")),
                ..KeyOutcome::default()
            },
        }
    }

    fn run_command(&mut self, action: CommandAction) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        match action {
            CommandAction::Quit => {
                self.status = TuiStatus::Quit;
                outcome.consumed_input = true;
            }
            CommandAction::OpenModelPicker => {
                self.panel = TuiPanel::Model;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenAgents => {
                self.panel = TuiPanel::Agents;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenSessions => {
                self.panel = TuiPanel::Sessions;
                open_snapshot(
                    &mut outcome,
                    self.sessions_loaded,
                    PanelIntent::LoadSessions,
                );
            }
            CommandAction::OpenSkills => {
                self.panel = TuiPanel::Skills;
                open_snapshot(&mut outcome, self.skills_loaded, PanelIntent::LoadSkills);
            }
            CommandAction::OpenCards => {
                self.panel = TuiPanel::Cards;
                open_snapshot(&mut outcome, self.cards_loaded, PanelIntent::LoadCards);
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
        if outcome.consumed_input {
            self.input.clear();
        }
        outcome
    }

    /// Panel navigation: Up/Down move the panel cursor, Enter chooses,
    /// Esc closes. Char input inside a panel is a no-op.
    pub fn handle_panel_key(&mut self, action: KeyAction) -> KeyOutcome {
        match action {
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                self.panel = TuiPanel::None;
                KeyOutcome::default()
            }
            KeyAction::Left => {
                self.cycle_panel_variant(-1);
                KeyOutcome::default()
            }
            KeyAction::Right => {
                self.cycle_panel_variant(1);
                KeyOutcome::default()
            }
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

    /// Cycle the pending variant of the model under the cursor (Model panel).
    fn cycle_panel_variant(&mut self, delta: isize) {
        if self.panel == TuiPanel::Model
            && let Some(picker) = self.picker.as_mut()
        {
            picker.cycle_variant(delta);
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
            TuiPanel::Model => match self.picker_selection() {
                Some((id, variant)) => {
                    outcome.intent = Some(PanelIntent::ChooseModel { id, variant });
                }
                None => outcome.note = Some("no model selected".to_string()),
            },
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
                self.panel = TuiPanel::None;
            }
            TuiPanel::Dcp => {
                outcome.intent = Some(PanelIntent::Compress {
                    focus: String::new(),
                });
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
        if self.live_text.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_text.len();
            self.live_text.push_str(crate::truncate_utf8(delta, room));
        }
    }

    /// Apply a worker turn-finished event: replace the live line with the
    /// final text and release the turn (the loop accepts submits again).
    pub fn apply_finished(&mut self, turn: &WorkerTurnId, text: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        self.live_text.clear();
        self.window.push_synthetic("ai", text);
    }

    /// Apply a worker turn-interrupted event: drop the live line, mark the
    /// turn cancelled, and release the turn.
    pub fn apply_interrupted(&mut self, turn: &WorkerTurnId) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.active_turn = None;
        self.status = TuiStatus::Cancelled;
        self.live_text.clear();
        self.window.push_synthetic("", "(cancelled)");
    }

    /// Release a failed turn and show its error, never a successful answer.
    pub fn apply_failed(&mut self, turn: &WorkerTurnId, error: &CoreError) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.active_turn = None;
        self.live_text.clear();
        self.status = TuiStatus::Idle;
        self.window.push_synthetic("", &format!("(error: {error})"));
    }

    fn line_count(&self) -> usize {
        self.window.len() + usize::from(!self.live_text.is_empty())
    }

    fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(VIEWPORT_LINES)
    }
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
    let files = if card.files.is_empty() {
        String::new()
    } else {
        let suffix = if card.files_truncated { ", …" } else { "" };
        format!(" [{}{suffix}]", card.files.join(", "))
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
    HistoryRow {
        seq: i64::MAX,
        role: String::new(),
        text: format!(
            "{} {} ({}){}{}",
            card.name, card.state, card.op, files, output
        ),
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
                "limit": { "context": entry.context, "output": entry.output },
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
            match tokio::time::timeout(timeout, self.rx.recv()).await {
                Err(_) => return PumpOutcome::Timeout,
                Ok(Err(_)) => return PumpOutcome::Closed,
                Ok(Ok(CoreEvent::TurnStarted { .. })) => {}
                Ok(Ok(CoreEvent::TurnFailed { turn, error, .. })) => {
                    state.apply_failed(&turn, &error);
                    return PumpOutcome::Closed;
                }
                Ok(Ok(CoreEvent::TextDelta { turn, delta, .. })) => {
                    state.apply_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::TurnFinished { turn, text, .. })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_finished(&turn, &text);
                        return PumpOutcome::Finished(text);
                    }
                }
                Ok(Ok(CoreEvent::TurnInterrupted { turn, partial, .. })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_interrupted(&turn);
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
        KeyOutcome, MAX_INPUT_BYTES, PanelIntent, PumpOutcome, ScriptDriver, TuiPanel, TuiState,
        TuiStatus, VIEWPORT_LINES,
    };
    use crate::events::KeyAction;
    use crate::history::{WINDOW_BYTES, WINDOW_ROWS};
    use oc_core::core_app::{CoreApp, MockProvider, WorkerTurnId};
    use oc_core::domain::SessionId;
    use oc_core::queries::{
        AgentEntry, CatalogSnapshot, HistoryMessage, HistoryPage, ModelEntry, SkillCard,
        VariantEntry,
    };
    use oc_core::session::{CoreError, Role};
    use std::time::Duration;

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("id")
    }

    fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>, total: usize, older: bool, newer: bool) -> HistoryPage {
        HistoryPage {
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

    fn snapshot() -> CatalogSnapshot {
        CatalogSnapshot {
            provider: "ludka2".to_string(),
            models: vec![
                ModelEntry {
                    id: "a".to_string(),
                    variants: Vec::new(),
                    context: 1000,
                    output: 100,
                },
                ModelEntry {
                    id: "b".to_string(),
                    variants: vec![VariantEntry {
                        name: "low".to_string(),
                        disabled: false,
                        reasoning_effort: Some("low".to_string()),
                    }],
                    context: 1000,
                    output: 100,
                },
            ],
            model_id: "a".to_string(),
            variant: None,
            agents: vec![
                AgentEntry {
                    id: "x".to_string(),
                    description: "first profile".to_string(),
                    model: None,
                    variant: None,
                },
                AgentEntry {
                    id: "y".to_string(),
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
    async fn slash_commands_return_intents_and_keep_input() {
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
            assert!(!outcome.consumed_input, "{command} keeps the input");
            assert_eq!(state.input(), command);
            assert_eq!(outcome.note, None);
            state.accept_intent();
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
            assert_eq!(outcome.intent, None, "{command}");
            assert!(outcome.consumed_input, "{command}");
            assert!(state.input().is_empty(), "{command}");
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
        assert!(outcome.consumed_input);
        assert_eq!(outcome.note, None);
        assert_eq!(outcome.intent, None);
        assert!(state.input().is_empty());
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert!(state.is_busy());
        assert!(state.viewport().iter().any(|line| line == "you: hi"));

        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: hi".to_string()));
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(!state.is_busy());
        let view = state.viewport();
        assert!(view.iter().any(|line| line == "you: hi"), "{view:?}");
        assert!(view.iter().any(|line| line == "ai: echo: hi"), "{view:?}");
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
            Some(PanelIntent::ChooseModel {
                id: "b".to_string(),
                variant: None
            })
        );
        assert_eq!(state.picker_selection(), Some(("b".to_string(), None)));
        state.accept_intent();

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
        let outcome = state.handle_key(KeyAction::Up).await;
        assert_eq!(outcome.intent, Some(PanelIntent::LoadOlder));

        // A window that evicted its newest rows asks for a newer page at the
        // bottom edge, never while scrolling inside the window.
        let bulk: Vec<HistoryMessage> = (0..WINDOW_ROWS + 2)
            .map(|i| msg(100 + i as i64, Role::User, "older"))
            .collect();
        state.prepend_page(&page(bulk, 500, true, true));
        assert!(state.needs_newer());
        let outcome = state.handle_key(KeyAction::Down).await;
        assert_eq!(outcome.intent, Some(PanelIntent::LoadNewer));

        state.append_page(&page(
            vec![msg(999, Role::User, "newest")],
            500,
            true,
            false,
        ));
        assert!(!state.needs_newer());
        let outcome = state.handle_key(KeyAction::Up).await;
        assert_eq!(outcome.intent, None, "only the top edge loads older");

        let mut requested = None;
        for _ in 0..WINDOW_ROWS * 2 {
            let outcome = state.handle_key(KeyAction::Up).await;
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
        for _ in 0..WINDOW_ROWS * 2 {
            assert_eq!(state.handle_key(KeyAction::Up).await.intent, None);
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
        assert!(view.iter().any(|line| line == "user: привет 🌍 мир"));
        assert_eq!(state.input(), "next…");
    }

    #[tokio::test]
    async fn stale_turn_events_are_ignored() {
        let mut state = fresh_state("s-stale").await;
        let mut driver = ScriptDriver::attach(&state.app);
        type_text(&mut state, "go").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.status(), &TuiStatus::Streaming);

        let stale = WorkerTurnId("t-stale".to_string());
        state.apply_delta(&stale, "junk");
        state.apply_finished(&stale, "junk");
        state.apply_interrupted(&stale);
        state.apply_failed(&stale, &CoreError::TurnBusy);
        assert_eq!(state.status(), &TuiStatus::Streaming);
        assert!(state.is_busy());
        assert!(!state.viewport().iter().any(|line| line.contains("junk")));

        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: go".to_string()));
        assert_eq!(state.status(), &TuiStatus::Idle);
        assert!(state.viewport().iter().any(|line| line == "ai: echo: go"));

        // A fresh submit is accepted right after the finish.
        type_text(&mut state, "go2").await;
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert_eq!(outcome.note, None);
        assert_eq!(state.status(), &TuiStatus::Streaming);
        let _ = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(state.status(), &TuiStatus::Idle);
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
        assert!(state.viewport().iter().any(|line| line == "(cancelled)"));

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
