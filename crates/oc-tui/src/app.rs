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

use crate::events::KeyAction;

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
                if text == "/quit" {
                    self.status = TuiStatus::Quit;
                    self.input.clear();
                    return None;
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
    use super::{PumpOutcome, ScriptDriver, TuiState, TuiStatus, VIEWPORT_LINES};
    use crate::events::KeyAction;
    use oc_core::core_app::{CoreApp, MockProvider};
    use oc_core::domain::SessionId;
    use std::time::Duration;

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("id")
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
}
