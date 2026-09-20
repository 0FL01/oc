//! Minimal chat state over the shared `CoreApp` handle.
//!
//! Same worker as headless (`CoreApp` + `MockProvider` in tests, real
//! provider later): prompt → stream → history, cancel, exit. Viewport is
//! bounded (`VIEWPORT_LINES`); history itself lives in the worker, the view
//! keeps only a cursor. No storage import here — persistence is wired in
//! the `oc` binary to preserve `oc-tui → oc-core` direction.

use oc_core::core_app::{CoreApp, CoreEvent, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_core::session::CoreError;

use crate::commands::{CommandAction, dispatch};
use crate::dcp_panel::{DcpOutcome, DcpPanelState};
use crate::events::KeyAction;
use crate::history::HistoryPager;
use crate::picker::ModelPicker;
use crate::workspace::WorkspaceRegistry;

/// Visible lines kept in the viewport (scroll window).
pub const VIEWPORT_LINES: usize = 20;
/// Bounded input buffer (bytes).
pub const MAX_INPUT_BYTES: usize = 4096;

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
    /// Session list with resume (UI03).
    Sessions,
    /// Skill catalog (UI06).
    Skills,
    /// Help, optionally for one topic.
    Help(Option<String>),
    /// DCP context panel (UI04).
    Dcp,
}

/// Minimal chat state bound to one session on the shared handle.
pub struct TuiState {
    app: CoreApp,
    session: SessionId,
    /// Current input buffer.
    pub input: String,
    /// Rendered lines (role-prefixed history + live deltas).
    pub lines: Vec<String>,
    /// Scroll offset from the bottom (0 = pinned to latest).
    pub scroll: usize,
    /// Current status.
    pub status: TuiStatus,
    active_turn: Option<WorkerTurnId>,
    live_text: String,
    /// Open panel, if any.
    pub panel: TuiPanel,
    /// Model picker (present while the Model panel lives).
    pub picker: Option<ModelPicker>,
    /// Session pager for the attached session (UI03, newest pages on demand).
    pub pager: Option<HistoryPager>,
    /// Session ids for the Sessions panel (bounded snapshot from the binary).
    pub sessions: Vec<String>,
    /// Sessions cursor.
    pub sessions_cursor: usize,
    /// Single-generation workspace registry wired by the binary (UI06).
    pub workspace: Option<WorkspaceRegistry>,
    /// DCP panel state: snapshot in, request out, transient outcome (UI04).
    pub dcp: DcpPanelState,
}

impl TuiState {
    /// Bind to a session; the session must already exist on the handle.
    pub fn new(app: CoreApp, session: SessionId) -> Self {
        Self {
            app,
            session,
            input: String::new(),
            lines: Vec::new(),
            scroll: 0,
            status: TuiStatus::Idle,
            active_turn: None,
            live_text: String::new(),
            panel: TuiPanel::None,
            picker: None,
            pager: None,
            sessions: Vec::new(),
            sessions_cursor: 0,
            workspace: None,
            dcp: DcpPanelState::default(),
        }
    }

    /// Attached session id.
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// Attach the binary-wired workspace registry (UI06, one generation).
    pub fn set_workspace(&mut self, workspace: WorkspaceRegistry) {
        self.workspace = Some(workspace);
    }

    /// Switch to another session: re-point, clear view state, drop the
    /// pager (the binary re-opens it). The target must exist on the
    /// handle; storage resume is loaded explicitly via `resume_session`.
    pub fn switch_session(&mut self, session: SessionId) {
        self.session = session;
        self.input.clear();
        self.lines.clear();
        self.scroll = 0;
        self.live_text.clear();
        self.active_turn = None;
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
        self.pager = None;
        self.panel = TuiPanel::None;
    }

    /// Resume committed history into the view (first page, oldest-first).
    pub fn resume_session(
        &mut self,
        db: &oc_adapters::storage::Db,
        limit: usize,
    ) -> Result<usize, String> {
        let mut pager = HistoryPager::open(db, &self.session.0).map_err(|e| e.to_string())?;
        let added = pager.load_older(db, limit).map_err(|e| e.to_string())?;
        self.lines = pager
            .rows()
            .iter()
            .map(|row| format!("{}: {}", row.role, row.text))
            .collect();
        self.scroll = 0;
        self.pager = Some(pager);
        Ok(added)
    }

    /// Load the next older history page into the top of the view.
    pub fn history_older(
        &mut self,
        db: &oc_adapters::storage::Db,
        limit: usize,
    ) -> Result<usize, String> {
        let pager = self.pager.as_mut().ok_or_else(|| "no pager".to_string())?;
        let added = pager.load_older(db, limit).map_err(|e| e.to_string())?;
        self.lines = pager
            .rows()
            .iter()
            .map(|row| format!("{}: {}", row.role, row.text))
            .collect();
        Ok(added)
    }

    /// Run a dispatched slash command (panel routing only; snapshots load next).
    fn run_command(&mut self, action: CommandAction) -> Option<String> {
        match action {
            CommandAction::Quit => {
                self.status = TuiStatus::Quit;
                None
            }
            CommandAction::OpenModelPicker => {
                self.panel = TuiPanel::Model;
                None
            }
            CommandAction::OpenSessions => {
                self.open_sessions(Vec::new());
                None
            }
            CommandAction::OpenSkills => {
                if self.workspace.is_none() {
                    return Some("no workspace registry".to_string());
                }
                self.open_skills();
                None
            }
            CommandAction::Help(topic) => {
                self.panel = TuiPanel::Help(topic);
                None
            }
            CommandAction::DcpCompress { focus } => match self.dcp.request_compress(&focus) {
                Ok(()) => {
                    self.panel = TuiPanel::Dcp;
                    None
                }
                Err(e) => Some(e),
            },
        }
    }

    /// Report a runtime DCP outcome: transient notice, never chat history.
    pub fn notify_dcp(&mut self, outcome: DcpOutcome) {
        self.dcp.set_outcome(outcome);
    }
    /// Open the model picker over a fresh catalog (UI02).
    pub fn open_picker(
        &mut self,
        catalog: oc_adapters::models::ModelCatalog,
        db: &oc_adapters::storage::Db,
    ) -> Result<(), String> {
        let mut picker = ModelPicker::new(catalog);
        picker.load_persisted(db).map_err(|e| e.to_string())?;
        self.picker = Some(picker);
        self.panel = TuiPanel::Model;
        Ok(())
    }

    /// Open the session list snapshot (UI03).
    pub fn open_sessions(&mut self, sessions: Vec<String>) {
        self.sessions_cursor = 0;
        self.sessions = sessions;
        self.panel = TuiPanel::Sessions;
    }

    /// Open the skill catalog panel (UI06).
    pub fn open_skills(&mut self) {
        self.panel = TuiPanel::Skills;
    }

    /// Close any open panel.
    pub fn close_panel(&mut self) {
        self.panel = TuiPanel::None;
    }

    /// Panel navigation: Up/Down move the panel cursor, Enter chooses,
    /// Esc closes. Returns an optional status message.
    pub fn handle_panel_key(
        &mut self,
        action: KeyAction,
        db: &oc_adapters::storage::Db,
    ) -> Option<String> {
        match action {
            KeyAction::Cancel => {
                self.close_panel();
                None
            }
            KeyAction::Up => {
                match self.panel {
                    TuiPanel::Model => {
                        if let Some(picker) = self.picker.as_mut() {
                            picker.move_cursor(-1);
                        }
                    }
                    TuiPanel::Sessions => {
                        self.sessions_cursor = self.sessions_cursor.saturating_sub(1);
                    }
                    _ => {}
                }
                None
            }
            KeyAction::Down => {
                match self.panel {
                    TuiPanel::Model => {
                        if let Some(picker) = self.picker.as_mut() {
                            picker.move_cursor(1);
                        }
                    }
                    TuiPanel::Sessions => {
                        if !self.sessions.is_empty() {
                            self.sessions_cursor =
                                (self.sessions_cursor + 1).min(self.sessions.len() - 1);
                        }
                    }
                    _ => {}
                }
                None
            }
            KeyAction::Enter => match self.panel {
                TuiPanel::Model => {
                    let Some(picker) = self.picker.as_mut() else {
                        return Some("no picker".to_string());
                    };
                    match picker.choose_cursor(db) {
                        Ok(()) => {
                            self.close_panel();
                            None
                        }
                        Err(e) => Some(e),
                    }
                }
                TuiPanel::Sessions => {
                    if let Some(id) = self.sessions.get(self.sessions_cursor).cloned() {
                        match SessionId::new(id) {
                            Some(session) => {
                                self.switch_session(session);
                                None
                            }
                            None => Some("bad session id".to_string()),
                        }
                    } else {
                        Some("empty session list".to_string())
                    }
                }
                _ => {
                    self.close_panel();
                    None
                }
            },
            _ => None,
        }
    }

    /// Handle one key action. Returns an optional error message for status.
    pub async fn handle_key(&mut self, action: KeyAction) -> Option<String> {
        match action {
            KeyAction::Char(c) => {
                if self.input.len() < MAX_INPUT_BYTES {
                    self.input.push(c);
                }
                None
            }
            KeyAction::Backspace => {
                self.input.pop();
                None
            }
            KeyAction::Up => {
                self.scroll = self.scroll.saturating_add(1);
                None
            }
            KeyAction::Down => {
                self.scroll = self.scroll.saturating_sub(1);
                None
            }
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                None
            }
            KeyAction::Cancel => {
                if self.active_turn.is_some() {
                    match self.app.cancel(self.session.clone()).await {
                        Ok(()) => None,
                        Err(e) => Some(format!("cancel: {e}")),
                    }
                } else {
                    self.status = TuiStatus::Quit;
                    None
                }
            }
            KeyAction::Enter => {
                if self.status == TuiStatus::Quit {
                    return None;
                }
                let text = self.input.trim().to_string();
                if text.is_empty() {
                    return None;
                }
                // Slash commands drive panels (UI06); the binary fills
                // catalog/session snapshots after dispatch.
                if let Some(action) = dispatch(&text) {
                    self.input.clear();
                    return self.run_command(action);
                }
                if self.active_turn.is_some() {
                    return Some("turn busy".to_string());
                }
                match self.app.submit(self.session.clone(), text.clone()).await {
                    Ok(turn) => {
                        self.active_turn = Some(turn);
                        self.live_text.clear();
                        self.status = TuiStatus::Streaming;
                        self.lines.push(format!("you: {text}"));
                        self.input.clear();
                        self.scroll = 0;
                        self.dcp.clear_notice();
                        None
                    }
                    Err(CoreError::TurnBusy) => Some("turn busy".to_string()),
                    Err(e) => Some(format!("submit: {e}")),
                }
            }
        }
    }

    /// Drain one worker event into view state. Returns true when a turn
    /// reached a terminal event.
    pub async fn poll_event(&mut self) -> bool {
        // Non-blocking drain via a short timeout so the key loop stays alive.
        let mut rx = self.app.subscribe();
        // NOTE: subscribing fresh each poll misses in-flight deltas; the real
        // event loop in the binary holds one subscription. Tests drive the
        // binary-style loop via `drive_script` below instead.
        let _ = &mut rx;
        false
    }

    /// Visible viewport lines (bounded, scroll-aware).
    pub fn viewport(&self) -> Vec<String> {
        let total = self.lines.len();
        let end = total.saturating_sub(self.scroll);
        let start = end.saturating_sub(VIEWPORT_LINES);
        self.lines[start..end].to_vec()
    }
}

/// Scripted driver used by tests: holds one broadcast subscription like the
/// real binary event loop and applies key actions + worker events in order.
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

    /// Pump worker events into `state` until `idle` (no active turn) or
    /// timeout. Returns terminal text or interrupt marker.
    pub async fn pump_until_idle(
        &mut self,
        state: &mut TuiState,
        timeout: std::time::Duration,
    ) -> PumpOutcome {
        loop {
            if state.active_turn.is_none() && state.status != TuiStatus::Streaming {
                return PumpOutcome::Idle;
            }
            match tokio::time::timeout(timeout, self.rx.recv()).await {
                Err(_) => return PumpOutcome::Timeout,
                Ok(Err(_)) => return PumpOutcome::Closed,
                Ok(Ok(CoreEvent::TurnStarted { .. })) => {}
                Ok(Ok(CoreEvent::TextDelta { turn, delta, .. })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.live_text.push_str(&delta);
                        Self::push_or_extend_assistant(state, &delta);
                    }
                }
                Ok(Ok(CoreEvent::TurnFinished { turn, text, .. })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.active_turn = None;
                        state.status = TuiStatus::Idle;
                        Self::replace_live_with_final(state, &text);
                        return PumpOutcome::Finished(text);
                    }
                }
                Ok(Ok(CoreEvent::TurnInterrupted { turn, partial, .. })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.active_turn = None;
                        state.status = TuiStatus::Cancelled;
                        Self::drop_live(state, &partial);
                        return PumpOutcome::Interrupted(partial);
                    }
                }
            }
        }
    }

    fn push_or_extend_assistant(state: &mut TuiState, delta: &str) {
        if let Some(last) = state.lines.last_mut()
            && last.starts_with("ai: ")
        {
            last.push_str(delta);
            return;
        }
        state.lines.push(format!("ai: {delta}"));
    }

    fn replace_live_with_final(state: &mut TuiState, text: &str) {
        if let Some(last) = state.lines.last_mut()
            && last.starts_with("ai: ")
        {
            *last = format!("ai: {text}");
            return;
        }
        state.lines.push(format!("ai: {text}"));
    }

    fn drop_live(state: &mut TuiState, _partial: &str) {
        if let Some(last) = state.lines.last()
            && last.starts_with("ai: ")
        {
            state.lines.pop();
        }
        state.lines.push("(cancelled)".to_string());
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
    use super::{PumpOutcome, ScriptDriver, TuiPanel, TuiState, TuiStatus, VIEWPORT_LINES};
    use crate::events::KeyAction;
    use oc_adapters::models::ModelCatalog;
    use oc_adapters::storage::Db;
    use oc_core::core_app::{CoreApp, MockProvider};
    use oc_core::domain::SessionId;
    use std::time::Duration;

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("id")
    }

    fn test_db(name: &str) -> Db {
        let root = std::env::temp_dir().join(format!("oc-tui-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Db::open(&root).expect("db")
    }

    async fn type_text(state: &mut TuiState, text: &str) -> Option<String> {
        let mut out = None;
        for c in text.chars() {
            out = state.handle_key(KeyAction::Char(c)).await;
        }
        out
    }

    #[tokio::test]
    async fn slash_sessions_switch_and_resume_pages() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid("s1")).await.expect("s1");
        app.create_session(sid("s2")).await.expect("s2");
        let db = test_db("switch");
        db.create_session("s2").expect("db session");
        for (i, role) in ["user", "assistant", "user"].iter().enumerate() {
            db.append_message("s2", role, &format!("m{i}"))
                .expect("msg");
        }
        let mut state = TuiState::new(app, sid("s1"));
        type_text(&mut state, "/sessions").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.panel, TuiPanel::Sessions);
        state.open_sessions(db.list_sessions().expect("list"));
        state.handle_panel_key(KeyAction::Down, &db);
        state.handle_panel_key(KeyAction::Enter, &db);
        assert_eq!(state.session().0, "s2");
        assert!(state.lines.is_empty(), "view cleared on switch");

        let added = state.resume_session(&db, 2).expect("resume");
        assert_eq!(added, 2);
        assert_eq!(state.lines, ["assistant: m1", "user: m2"].map(String::from));
        let added = state.history_older(&db, 10).expect("older");
        assert_eq!(added, 1);
        assert_eq!(state.lines[0], "user: m0");
    }

    async fn setup() -> (TuiState, ScriptDriver) {
        let (app, _guard) = CoreApp::spawn(MockProvider::echo());
        // Leak the guard for the test lifetime: the worker must outlive the
        // state/driver pair within a single #[tokio::test].
        std::mem::forget(_guard);
        app.create_session(sid("s-tui")).await.expect("create");
        let mut driver = ScriptDriver::attach(&app);
        let state = TuiState::new(app, sid("s-tui"));
        let _ = &mut driver;
        (state, driver)
    }

    #[tokio::test]
    async fn prompt_stream_history_exit() {
        let (mut state, mut driver) = setup().await;
        for c in "hi".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        assert_eq!(state.input, "hi");
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.status, TuiStatus::Streaming);
        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: hi".to_string()));
        assert!(state.lines.contains(&"you: hi".to_string()));
        assert!(state.lines.contains(&"ai: echo: hi".to_string()));
        // History came through the same CoreApp worker headless uses.
        state.handle_key(KeyAction::Char('x')).await;
        state.input.clear();
        for c in "/quit".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.status, TuiStatus::Quit);
    }

    #[tokio::test]
    async fn cancel_drops_live_and_stays_usable() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            (0..20).map(|i| format!("t{i} ")).collect(),
            10,
        ));
        std::mem::forget(guard);
        app.create_session(sid("s-c")).await.expect("create");
        let mut state = TuiState::new(app.clone(), sid("s-c"));
        let mut driver = ScriptDriver::attach(&app);
        for c in "long".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        state.handle_key(KeyAction::Enter).await;
        // Let one delta land, then Esc.
        tokio::time::sleep(Duration::from_millis(30)).await;
        state.handle_key(KeyAction::Cancel).await;
        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert!(matches!(outcome, PumpOutcome::Interrupted(_)));
        assert!(state.lines.contains(&"(cancelled)".to_string()));
        assert_eq!(state.status, TuiStatus::Cancelled);
        // Next prompt works on the same handle.
        for c in "ok".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        state.handle_key(KeyAction::Enter).await;
        let _ = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(state.status, TuiStatus::Idle);
    }

    #[tokio::test]
    async fn viewport_is_bounded_and_scrolls() {
        let (app, _guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(_guard);
        let mut state = TuiState::new(app, sid("s-v"));
        for i in 0..50 {
            state.lines.push(format!("line {i}"));
        }
        let view = state.viewport();
        assert_eq!(view.len(), VIEWPORT_LINES);
        assert_eq!(view[0], format!("line {}", 50 - VIEWPORT_LINES));
        state.scroll = 5;
        let view = state.viewport();
        assert_eq!(view.len(), VIEWPORT_LINES);
        assert_eq!(view[0], format!("line {}", 50 - VIEWPORT_LINES - 5));
    }

    fn catalog() -> ModelCatalog {
        ModelCatalog {
            provider: "ludka2".to_string(),
            models: [
                ("a".to_string(), serde_json::json!({})),
                ("b".to_string(), serde_json::json!({})),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[tokio::test]
    async fn picker_choose_flow_through_panel() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid("s-p")).await.expect("s");
        let db = test_db("picker");
        let mut state = TuiState::new(app, sid("s-p"));
        state.open_picker(catalog(), &db).expect("open");
        assert_eq!(state.panel, TuiPanel::Model);
        state.handle_panel_key(KeyAction::Down, &db);
        state.handle_panel_key(KeyAction::Enter, &db);
        assert_eq!(state.panel, TuiPanel::None);
        let selection = state
            .picker
            .as_ref()
            .expect("picker")
            .selection()
            .expect("choice");
        assert_eq!(selection.id, "b");
        // Persisted: a fresh picker resolves without interaction.
        let mut reopened = TuiState::new(state.app.clone(), sid("s-p"));
        reopened.open_picker(catalog(), &db).expect("reopen");
        let selection = reopened
            .picker
            .as_ref()
            .expect("picker")
            .selection()
            .expect("choice");
        assert_eq!(selection.id, "b");
    }

    #[tokio::test]
    async fn slash_skills_needs_workspace() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid("s-k")).await.expect("s");
        let mut state = TuiState::new(app, sid("s-k"));
        type_text(&mut state, "/skills").await;
        let message = state.handle_key(KeyAction::Enter).await;
        assert_eq!(message.as_deref(), Some("no workspace registry"));
        assert_eq!(state.panel, TuiPanel::None);

        let workspace = crate::workspace::WorkspaceRegistry::bind(
            1,
            "work",
            &oc_adapters::config::Generation::default(),
            Vec::new(),
            vec![oc_adapters::config::SkillMeta {
                id: "s1".to_string(),
                name: "S1".to_string(),
                description: "does things".to_string(),
            }],
        );
        state.set_workspace(workspace);
        type_text(&mut state, "/skills").await;
        assert!(state.handle_key(KeyAction::Enter).await.is_none());
        assert_eq!(state.panel, TuiPanel::Skills);
        state.handle_panel_key(KeyAction::Cancel, &test_db("unused"));
        assert_eq!(state.panel, TuiPanel::None);
    }

    #[tokio::test]
    async fn slash_quit_still_quits() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid("s-q")).await.expect("s");
        let mut state = TuiState::new(app, sid("s-q"));
        type_text(&mut state, "/quit").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.status, TuiStatus::Quit);
    }

    #[tokio::test]
    async fn dcp_compress_request_and_transient_notice() {
        use crate::dcp_panel::DcpOutcome;

        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        app.create_session(sid("s-d")).await.expect("s");
        let mut state = TuiState::new(app, sid("s-d"));
        let mut driver = ScriptDriver::attach(&state.app);
        type_text(&mut state, "/dcp-compress draft span").await;
        state.handle_key(KeyAction::Enter).await;
        assert_eq!(state.panel, TuiPanel::Dcp);
        let pending = state.dcp.pending().expect("pending");
        assert_eq!(pending.focus, "draft span");

        let history_len = state.lines.len();
        state.notify_dcp(DcpOutcome::Done { saved_tokens: 128 });
        assert_eq!(state.lines.len(), history_len, "notice is not history");
        assert!(
            state.dcp.notice().expect("notice").contains("128"),
            "outcome visible"
        );

        // Next submit clears the transient notice and chats normally.
        type_text(&mut state, "hi").await;
        state.handle_key(KeyAction::Enter).await;
        let outcome = driver
            .pump_until_idle(&mut state, Duration::from_secs(5))
            .await;
        assert_eq!(outcome, PumpOutcome::Finished("echo: hi".to_string()));
        assert!(state.dcp.notice().is_none());
    }
}
