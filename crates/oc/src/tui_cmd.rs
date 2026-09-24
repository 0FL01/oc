//! Real-terminal consumer of the shared native application.
//!
//! The view-model (`oc-tui`) is storage-free: this loop answers its
//! [`PanelIntent`] values through the application API, drains worker events
//! into turn-scoped view state, and owns terminal setup/restore. UI never
//! persists input or outcomes independently of application acceptance.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use oc_adapters::application::{HISTORY_PAGE_LIMIT, TOOL_OPS_PAGE_LIMIT};
use oc_core::core_app::{CoreApp, CoreEvent};
use oc_core::domain::SessionId;
use oc_core::queries::StartupNotice;
use oc_core::queries::{CatalogSnapshot, SessionSelectionAction as SelectionAction};
use oc_core::session::{CoreError, LocationSwitchFailure};
use oc_tui::app::{KeyOutcome, PanelIntent, TabPresentation, TuiPanel, TuiState, TuiStatus};
use oc_tui::commands::{CommandAction, dispatch};
use oc_tui::dcp_panel::DcpOutcome;
use oc_tui::events::{KeyAction, UiEvent, map_event};
use oc_tui::shell::{StartupFailure, render_startup_failure};
use oc_tui::terminal::{enter, install_panic_hook};
use oc_tui::views::render_frame;

/// Qualification probe (T26): when set, panic right after entering the
/// terminal so PTY tests can verify panic-path restoration. Never set in
/// normal use.
const PANIC_PROBE_ENV: &str = "OC_TUI_TEST_PANIC";

/// Qualification probe (T39): when set, write one bounded view-metrics JSON
/// document on exit so PTY tests can assert retained-state bounds. Never set
/// in normal use.
const METRICS_ENV: &str = "OC_TUI_TEST_METRICS";

#[derive(Default)]
struct FrameMetrics {
    count: u64,
    sum_ns: u128,
    max_ns: u128,
}

/// Max key events drained per frame (paste bursts stay fast; a flooding
/// input still yields to the worker drain below each frame).
const MAX_KEYS_PER_FRAME: usize = 256;
const MAX_TABS: usize = 16;

/// Launch the interactive TUI; returns process exit code.
pub async fn run_tui(data_dir: &Path, session_opt: Option<String>) -> ExitCode {
    install_panic_hook();
    match run_inner(data_dir, session_opt).await {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

async fn run_inner(data_dir: &Path, session_opt: Option<String>) -> Result<ExitCode, String> {
    let outcome = run_stages(data_dir, session_opt).await;
    let code = match &outcome {
        Ok(code) => *code,
        Err(_) => 1,
    };
    oc_adapters::trace::log("tui.exit", &format!("code={code}"));
    outcome.map(ExitCode::from)
}

async fn run_stages(data_dir: &Path, session_opt: Option<String>) -> Result<u8, String> {
    if !at_tty() {
        return Err(
            "no TTY for interactive TUI; use `oc run \"<prompt>\"` for headless use".to_string(),
        );
    }
    let project = std::env::current_dir().map_err(|e| e.to_string())?;
    oc_adapters::trace::log("tui.begin", &format!("project={}", project.display()));
    let session = session_opt
        .map(|raw| SessionId::new(raw).ok_or_else(|| "invalid session id".to_string()))
        .transpose()?;
    let (app, guard, notices) = match oc_adapters::application::spawn_diagnostic(&project, data_dir)
        .await
    {
        Ok(runtime) => {
            oc_adapters::trace::log("spawn.ok", "");
            runtime
        }
        Err(category) => {
            oc_adapters::trace::log("spawn.fail", &format!("category={category:?}"));
            let _term = enter()?;
            let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))
                .map_err(|e| format!("terminal: {e}"))?;
            return startup_failure(&mut terminal, StartupFailure::Preflight(category)).map(|_| 1);
        }
    };
    for notice in notices {
        eprintln!("warning: {}", startup_notice(notice));
    }
    let result = drive_ui(&app, session).await;
    let _ = app.shutdown().await;
    guard
        .join()
        .await
        .map_err(|e| format!("application worker: {e}"))?;
    result
}

/// Loop-local application state that is not part of the view-model.
#[derive(Default)]
struct LoopState {
    /// Turn started by a manual `/dcp-compress` request.
    /// Cursor for paging older tool cards.
    cards_before: Option<i64>,
    /// DCP snapshot was fetched for the currently open panel.
    dcp_seen: bool,
    /// Ordered real tabs. The selected tab lives in `drive_ui`'s `state`
    /// instead of being duplicated here. A sessionless Home is a synthetic
    /// slot outside this vector until its first turn is accepted.
    tabs: Vec<Option<TuiState>>,
    /// Card cursors follow the same slots as `tabs` (including the active slot).
    tab_cards_before: Vec<Option<i64>>,
    active_tab: Option<usize>,
    home: Option<TuiState>,
}

impl LoopState {
    fn can_open_session(&self) -> bool {
        self.tabs.len() < MAX_TABS - usize::from(self.home.is_some() || self.active_tab.is_none())
    }

    fn sync_tabs(&mut self, state: &mut TuiState) {
        // Bare Home has no tab. Acceptance attaches it to the first real
        // session; subsequent Home submissions append to the existing deck.
        if self.active_tab.is_none() && state.attached_session().is_some() {
            debug_assert!(self.tabs.len() < MAX_TABS);
            self.active_tab = Some(self.tabs.len());
            self.tabs.push(None);
            self.tab_cards_before.push(self.cards_before);
        }
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, parked)| {
                let view = if self.active_tab == Some(index) {
                    &*state
                } else {
                    parked.as_ref().expect("parked tab")
                };
                TabPresentation {
                    title: view.session_title.clone(),
                    home: false,
                    busy: view.is_busy(),
                }
            })
            .collect();
        let active = self.active_tab.unwrap_or(self.tabs.len());
        let can_add = !state.is_busy() && self.tabs.len() < MAX_TABS;
        let (shown, selected, allowed) = state.tab_presentation();
        // `set_tab_strip` cancels a pending mouse Down: never call it while
        // nothing painted has changed, even across the 50ms redraw loop.
        if shown != tabs || selected != active || allowed != (can_add && !tabs.is_empty()) {
            state.set_tab_strip(tabs, active, can_add);
        }
    }

    fn activate(&mut self, state: &mut TuiState, index: usize) -> Result<(), String> {
        if state.is_busy() {
            return Err("turn active; session switch refused".into());
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        if self.active_tab == Some(index) {
            state.close_panel();
            return Ok(());
        }
        state.close_panel();
        let next = self.tabs[index].take().expect("parked tab");
        let previous = std::mem::replace(state, next);
        if let Some(old) = self.active_tab {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        } else {
            self.home = Some(previous);
        }
        self.active_tab = Some(index);
        self.cards_before = self.tab_cards_before[index];
        self.dcp_seen = false;
        self.sync_tabs(state);
        Ok(())
    }

    fn open_home(&mut self, state: &mut TuiState, next: TuiState) {
        state.close_panel();
        let previous = std::mem::replace(state, next);
        if let Some(old) = self.active_tab.take() {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        }
        // An existing synthetic Home is replaced by a fresh `/new` Home.
        self.home = None;
        self.cards_before = None;
        self.dcp_seen = false;
        self.sync_tabs(state);
    }

    fn restore_home(&mut self, state: &mut TuiState) -> bool {
        let Some(home) = self.home.take() else {
            return false;
        };
        state.close_panel();
        let old = self.active_tab.take().expect("Home parked from a tab");
        self.tabs[old] = Some(std::mem::replace(state, home));
        self.tab_cards_before[old] = self.cards_before;
        self.cards_before = None;
        self.dcp_seen = false;
        self.sync_tabs(state);
        true
    }

    async fn close_tab(
        &mut self,
        app: &CoreApp,
        state: &mut TuiState,
        index: usize,
    ) -> Result<(), String> {
        if state.is_busy() {
            return Err("turn active; tab close refused".into());
        }
        if index == self.tabs.len() {
            // Home is a synthetic slot, and cannot be closed when it is the
            // only route. When selected, activate the last real view before
            // discarding the parked Home (and its draft).
            if self.active_tab.is_none() && !self.tabs.is_empty() {
                self.activate(state, self.tabs.len() - 1)?;
            } else if self.home.is_none() {
                return Err("tab unavailable".into());
            }
            self.home = None;
            self.sync_tabs(state);
            return Ok(());
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        if self.active_tab == Some(index) {
            if self.tabs.len() == 1 {
                if self.home.is_some() {
                    self.restore_home(state);
                    self.tabs.clear();
                    self.tab_cards_before.clear();
                    self.sync_tabs(state);
                    return Ok(());
                }
                // A failed owner query must leave the active view and the
                // entire deck intact; closing never creates a durable root.
                let snapshot = app
                    .home_selection(SelectionAction::Current)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut home = TuiState::new_home(app.clone());
                home.apply_catalog(snapshot);
                *state = home;
                self.reset_deck();
                self.sync_tabs(state);
                return Ok(());
            }
            // Immediately previous if available, otherwise the next tab.
            let survivor = if index > 0 { index - 1 } else { 1 };
            self.activate(state, survivor)?;
        }
        // The former active view is now parked. Removing the parallel cursor
        // at the same index keeps paging state aligned with the real tabs.
        self.tabs.remove(index);
        self.tab_cards_before.remove(index);
        if let Some(active) = self.active_tab.as_mut()
            && *active > index
        {
            *active -= 1;
        }
        self.sync_tabs(state);
        Ok(())
    }

    fn reset_deck(&mut self) {
        self.tabs.clear();
        self.tab_cards_before.clear();
        self.active_tab = None;
        self.home = None;
        self.cards_before = None;
        self.dcp_seen = false;
    }
}

async fn drive_ui(app: &CoreApp, session: Option<SessionId>) -> Result<u8, String> {
    let _term = enter()?;
    if std::env::var_os(PANIC_PROBE_ENV).is_some() {
        panic!("{PANIC_PROBE_ENV} probe");
    }
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;
    let mut state = match initial_state(app, session).await {
        Ok(state) => state,
        Err(failure) => return startup_failure(&mut terminal, failure).map(|_| 1),
    };
    let mut rx = app.subscribe();
    let mut loop_state = LoopState::default();
    loop_state.sync_tabs(&mut state);
    let mut frame_metrics = std::env::var_os(METRICS_ENV).map(|_| FrameMetrics::default());

    loop {
        state.poll_submission();
        loop_state.sync_tabs(&mut state);
        let draw_start = frame_metrics.as_ref().map(|_| Instant::now());
        terminal
            .draw(|frame| render_frame(frame, &state))
            .map_err(|e| format!("draw: {e}"))?;
        if let (Some(metrics), Some(start)) = (&mut frame_metrics, draw_start) {
            let elapsed = start.elapsed().as_nanos();
            metrics.count += 1;
            metrics.sum_ns += elapsed;
            metrics.max_ns = metrics.max_ns.max(elapsed);
        }
        if *state.status() == TuiStatus::Quit {
            break;
        }
        // Keys: one 50 ms poll keeps streaming responsive without a busy
        // loop, then drain everything already pending — pastes arrive as
        // char bursts and one-event-per-frame would take a minute for a
        // large paste. The drain is bounded so a flooding input cannot
        // starve the worker drain below.
        if event::poll(Duration::from_millis(50)).map_err(|e| format!("input: {e}"))? {
            for _ in 0..MAX_KEYS_PER_FRAME {
                if !event::poll(Duration::ZERO).map_err(|e| format!("input: {e}"))? {
                    break;
                }
                let cev = event::read().map_err(|e| format!("input: {e}"))?;
                handle_event(app, &mut state, &mut loop_state, cev).await?;
                if *state.status() == TuiStatus::Quit {
                    break;
                }
            }
        }
        // Quit wins over an acceptance/terminal event already queued this
        // frame. run_inner owns application shutdown and joins its worker.
        if *state.status() == TuiStatus::Quit {
            break;
        }
        // Worker events, non-blocking drain.
        while let Ok(event) = rx.try_recv() {
            state.poll_submission();
            if let Some(current) = state.attached_session().cloned() {
                handle_worker_event(app, &mut state, &mut loop_state, &current, event).await?;
            }
            loop_state.sync_tabs(&mut state);
        }
        // The DCP panel shows runtime counters: refresh when it opens.
        if *state.panel() == TuiPanel::Dcp && !loop_state.dcp_seen {
            if let Some(current) = state.attached_session().cloned() {
                refresh_dcp(app, &mut state, &current).await;
            }
            loop_state.dcp_seen = true;
        } else if *state.panel() != TuiPanel::Dcp {
            loop_state.dcp_seen = false;
        }
    }
    write_metrics(&state, frame_metrics.as_ref());
    drop(_term);
    Ok(0)
}

fn at_tty() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

/// Static source/operation guidance: no raw configuration or persisted values.
fn startup_notice(source: StartupNotice) -> &'static str {
    match source {
        StartupNotice::Definitions => "agent/skill/command definitions need review",
        StartupNotice::Plugin => "configured plugin marker was ignored; review plugin settings",
        StartupNotice::Dcp => "DCP settings have unsupported entries; review native dcp settings",
        StartupNotice::Instructions => "instruction sources need review",
        StartupNotice::SavedSelection => "saved model/agent selection needs review",
    }
}

async fn initial_state(
    app: &CoreApp,
    session: Option<SessionId>,
) -> Result<TuiState, StartupFailure> {
    let Some(session) = session else {
        let snapshot = app
            .home_selection(SelectionAction::Current)
            .await
            .map_err(|_| StartupFailure::Query)?;
        let mut state = TuiState::new_home(app.clone());
        state.apply_catalog(snapshot);
        return Ok(state);
    };
    app.create_session(session.clone())
        .await
        .map_err(|_| StartupFailure::Query)?;
    let page = app
        .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await
        .map_err(|_| StartupFailure::Query)?;
    let mut state = TuiState::new(app.clone(), session);
    state.attach_page(&page);
    // A failed catalog is an initialization error, never a usable empty snapshot.
    state.apply_catalog(
        app.session_selection(state.session().clone(), false, SelectionAction::Current)
            .await
            .map_err(|_| StartupFailure::Query)?,
    );
    Ok(state)
}

fn startup_failure(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    failure: StartupFailure,
) -> Result<ExitCode, String> {
    use crossterm::event::{KeyCode, KeyModifiers};
    loop {
        terminal
            .draw(|frame| render_startup_failure(frame, failure))
            .map_err(|e| format!("draw: {e}"))?;
        if event::poll(Duration::from_millis(100)).map_err(|e| format!("input: {e}"))?
            && let CEvent::Key(key) = event::read().map_err(|e| format!("input: {e}"))?
            && (matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return Ok(ExitCode::from(1));
        }
    }
}

/// True when bare `oc` may launch the interactive TUI: both ends must be a
/// real terminal. `oc tui` keeps the stdin-only gate so an unusable stdout
/// still fails visibly at draw time instead of silently doing nothing.
pub fn interactive_ready() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Handle one Crossterm event: keys drive the open panel or the prompt,
/// bracketed paste is one bounded input event, resize is picked up by the
/// next draw (which re-queries the size).
async fn handle_event(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    cev: CEvent,
) -> Result<(), String> {
    match map_event(cev) {
        Some(UiEvent::Key(action)) => {
            let typed_new = action == KeyAction::Enter
                && *state.panel() == TuiPanel::None
                && dispatch(state.input().trim()) == Some(CommandAction::NewSession);
            let outcome = if *state.panel() == TuiPanel::None {
                state.handle_key(action).await
            } else {
                state.handle_panel_key(action)
            };
            apply_outcome(app, state, loop_state, outcome, typed_new).await;
        }
        Some(UiEvent::Paste(text)) => {
            let outcome = state.handle_paste(&text);
            if let Some(note) = outcome.note {
                state.push_note(&note);
            }
        }
        Some(UiEvent::Mouse(mouse)) => {
            let (cols, rows) =
                crossterm::terminal::size().map_err(|e| format!("mouse terminal size: {e}"))?;
            let area = ratatui::layout::Rect::new(0, 0, cols, rows);
            if *state.panel() == TuiPanel::None
                && matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                )
            {
                // A wheel event also moves the pointer. Keep transcript
                // scrolling, but invalidate a held tab when it leaves the strip.
                state.handle_mouse(mouse, area);
                let outcome = state.scroll_transcript(mouse.kind == MouseEventKind::ScrollUp);
                apply_outcome(app, state, loop_state, outcome, false).await;
            } else {
                let outcome = state.handle_mouse(mouse, area);
                apply_mouse_outcome(
                    app,
                    state,
                    loop_state,
                    outcome,
                    area,
                    mouse.column,
                    mouse.row,
                )
                .await;
            }
        }
        Some(UiEvent::Resize) => state.clear_mouse_position(),
        None => {}
    }
    Ok(())
}

/// A tab view replacement loses the old view's hover. Re-hit-test only the
/// last real mouse release, and only after the application accepts the action.
async fn apply_mouse_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    outcome: KeyOutcome,
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) {
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else { return };
    let pointer = (state.mouse_position() == Some((x, y, area))).then_some((x, y, area));
    let close = match intent {
        PanelIntent::CloseTab { index } if pointer.is_some() => state.mouse_close_snapshot(index),
        _ => None,
    };
    let activate = matches!(intent, PanelIntent::ActivateTab { .. });
    match apply_intent_with_origin(app, state, loop_state, intent, false).await {
        Ok(()) => {
            loop_state.sync_tabs(state);
            if let Some(snapshot) = close {
                state.restore_mouse_close(snapshot);
            } else if activate && let Some(pointer) = pointer {
                state.restore_mouse_hover(pointer);
            }
        }
        Err(message) => {
            state.apply_intent_error(message);
            loop_state.sync_tabs(state);
        }
    }
}

/// Report a note, then apply the intent (or its typed failure).
async fn apply_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    outcome: KeyOutcome,
    typed_new: bool,
) {
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else {
        return;
    };
    // Scrolling intents never consume typed input; commands do.
    let consumes = matches!(intent, PanelIntent::SwitchLocation { .. });
    match apply_intent_with_origin(app, state, loop_state, intent, typed_new).await {
        Ok(()) => {
            if consumes {
                state.accept_intent();
            }
        }
        Err(message) => state.apply_intent_error(message),
    }
    loop_state.sync_tabs(state);
}

#[cfg(test)]
async fn apply_intent(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    intent: PanelIntent,
) -> Result<(), String> {
    apply_intent_with_origin(app, state, loop_state, intent, false).await
}

async fn apply_intent_with_origin(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    intent: PanelIntent,
    typed_new: bool,
) -> Result<(), String> {
    match intent {
        PanelIntent::LoadCatalog => {
            let snapshot = selection(app, state, SelectionAction::Current).await?;
            state.apply_catalog(snapshot);
        }
        PanelIntent::LoadSessions => {
            let ids = app.list_sessions().await.map_err(|e| e.to_string())?;
            state.apply_sessions(ids.into_iter().map(|id| id.0).collect());
        }
        PanelIntent::LoadSkills => {
            let cards = app.skills().await.map_err(|e| e.to_string())?;
            state.apply_skills(cards);
        }
        PanelIntent::LoadCards => {
            let session = require_session(state)?;
            let page = app
                .tool_ops_page(session, loop_state.cards_before, TOOL_OPS_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            let cards = oc_tui::history::cards_from_rows(&page.rows);
            if loop_state.cards_before.is_none() {
                state.apply_cards(cards, page.has_older);
            } else {
                state.prepend_cards(cards, page.has_older);
            }
            loop_state.cards_before = page.rows.last().map(|row| row.rowid);
        }
        PanelIntent::LoadCardOutput { op, offset } => {
            let session = require_session(state)?;
            let page = app
                .tool_output_page(session, op.clone(), offset, 240)
                .await
                .map_err(|e| e.to_string())?;
            state.apply_card_output(op, offset, page);
        }
        PanelIntent::SelectModel { id } => {
            let snapshot = selection(app, state, SelectionAction::Model(id)).await?;
            let note = format!("model: {}", snapshot.model_id);
            state.model_choice_applied(snapshot);
            state.push_note(&note);
        }
        PanelIntent::ChooseModel { variant, .. } => {
            let snapshot = selection(app, state, SelectionAction::Variant(variant)).await?;
            state.model_choice_applied(snapshot);
        }
        PanelIntent::NewSession => {
            if state.is_busy() {
                return Err("turn active; action unavailable".into());
            }
            if loop_state.home.is_some() {
                if typed_new {
                    state.accept_intent();
                }
                loop_state.restore_home(state);
                return Ok(());
            }
            if loop_state.tabs.len() >= MAX_TABS && state.attached_session().is_some() {
                return Err("tab limit reached".into());
            }
            let snapshot = app
                .home_selection(SelectionAction::New(
                    state.active_agent().map(str::to_string),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let mut home = TuiState::new_home(app.clone());
            home.apply_catalog(snapshot);
            if typed_new {
                state.accept_intent();
            }
            loop_state.open_home(state, home);
        }
        PanelIntent::ActivateTab { index } => loop_state.activate(state, index)?,
        PanelIntent::CloseTab { index } => loop_state.close_tab(app, state, index).await?,
        PanelIntent::SelectAgent { id } => {
            let snapshot = selection(app, state, SelectionAction::Agent(id)).await?;
            let note = match &snapshot.agent_id {
                Some(agent) => format!("agent: {agent}"),
                None => "agent: none".to_string(),
            };
            state.apply_catalog(snapshot);
            state.close_panel();
            state.push_note(&note);
        }
        PanelIntent::SwitchSession { id } => {
            // The worker is single-turn: refuse the switch while a turn runs
            // instead of silently losing the active task.
            if state.is_busy() {
                return Err("turn active; session switch refused".to_string());
            }
            let target = SessionId::new(id).ok_or_else(|| "bad session id".to_string())?;
            if let Some(index) = loop_state
                .tabs
                .iter()
                .enumerate()
                .find_map(|(index, parked)| {
                    let view = if loop_state.active_tab == Some(index) {
                        &*state
                    } else {
                        parked.as_ref().expect("parked tab")
                    };
                    (view.attached_session() == Some(&target)).then_some(index)
                })
            {
                loop_state.activate(state, index)?;
                return Ok(());
            }
            if !loop_state.can_open_session() {
                return Err("tab limit reached".into());
            }
            let snapshot = app
                .session_selection(target.clone(), false, SelectionAction::Current)
                .await
                .map_err(|e| e.to_string())?;
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            let mut next = TuiState::new(app.clone(), target);
            next.attach_page(&page);
            next.apply_catalog(snapshot);
            state.close_panel();
            if let Some(old) = loop_state.active_tab.take() {
                loop_state.tabs[old] = Some(std::mem::replace(state, next));
                loop_state.tab_cards_before[old] = loop_state.cards_before;
            } else {
                loop_state.home = Some(std::mem::replace(state, next));
            }
            loop_state.active_tab = Some(loop_state.tabs.len());
            loop_state.tabs.push(None);
            loop_state.tab_cards_before.push(None);
            loop_state.cards_before = None;
            loop_state.dcp_seen = false;
            loop_state.sync_tabs(state);
        }
        PanelIntent::SwitchLocation { path } => {
            // The application refuses a switch during a turn; the view-model
            // keeps the current Location and session until it is published.
            if state.is_busy() {
                return Err("turn active; location switch refused".to_string());
            }
            if state.attached_session().is_none() {
                // Do not touch the view (including its editable draft) until
                // the owner has validated and published the target generation.
                let snapshot = app.switch_location_home(path).await.map_err(switch_error)?;
                loop_state.reset_deck();
                state.reset_workspace();
                state.apply_catalog(snapshot.catalog);
                loop_state.cards_before = None;
                loop_state.dcp_seen = false;
                state.push_note(&format!("location: {}", snapshot.location));
                for notice in snapshot.notices {
                    state.push_note(&format!("warning: {}", startup_notice(notice)));
                }
                return Ok(());
            }
            let snapshot = app.switch_location(path).await.map_err(switch_error)?;
            let target = SessionId::new(snapshot.session.clone())
                .ok_or_else(|| "bad session id".to_string())?;
            // Publication changed the application generation. No parked view
            // from the old Location may be reactivated, even if a later
            // history/catalog read in the new generation fails.
            loop_state.reset_deck();
            state.reset_workspace();
            state.set_session(target.clone());
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|_| {
                    "Location history unavailable; check data-directory access".to_string()
                })?;
            let catalog = app
                .session_selection(target.clone(), false, SelectionAction::Current)
                .await
                .map_err(|_| "Location selection unavailable; check saved selection".to_string())?;
            state.attach_page(&page);
            state.apply_catalog(catalog);
            state.close_panel();
            loop_state.cards_before = None;
            state.push_note(&format!("location: {}", snapshot.location));
            for notice in snapshot.notices {
                state.push_note(&format!("warning: {}", startup_notice(notice)));
            }
            loop_state.sync_tabs(state);
        }
        PanelIntent::LoadOlder => {
            let session = require_session(state)?;
            let before = state
                .history()
                .rows()
                .iter()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(before) = before {
                let page = app
                    .history_page(session, Some(before), None, HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|e| e.to_string())?;
                state.prepend_page(&page);
            }
        }
        PanelIntent::LoadNewer => {
            let session = require_session(state)?;
            let after = state
                .history()
                .rows()
                .iter()
                .rev()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(after) = after {
                let page = app
                    .history_page(session, None, Some(after), HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|e| e.to_string())?;
                state.append_page(&page);
            }
        }
        PanelIntent::Compress { focus } => {
            require_session(state)?;
            state.request_compress(focus).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn switch_error(error: CoreError) -> String {
    match error {
        CoreError::LocationSwitch { category, .. } => match category {
            LocationSwitchFailure::Configuration =>
                "Location configuration failed; check the target directory, opencode.json/jsonc and selected model".to_string(),
            LocationSwitchFailure::Storage =>
                "Location storage failed; check the data directory and saved selection".to_string(),
            LocationSwitchFailure::Runtime =>
                "Location runtime failed; check the target's native settings".to_string(),
        },
        CoreError::TurnBusy => "turn active; location switch refused".to_string(),
        _ => "Location switch unavailable; retry after checking the data directory".to_string(),
    }
}

fn require_session(state: &TuiState) -> Result<SessionId, String> {
    state
        .attached_session()
        .cloned()
        .ok_or_else(|| "no active session; submit a prompt first".to_string())
}

async fn selection(
    app: &CoreApp,
    state: &TuiState,
    action: SelectionAction,
) -> Result<CatalogSnapshot, String> {
    match state.attached_session() {
        Some(session) => app
            .session_selection(session.clone(), state.home, action)
            .await
            .map_err(|e| e.to_string()),
        None => app.home_selection(action).await.map_err(|e| e.to_string()),
    }
}

async fn handle_worker_event(
    app: &CoreApp,
    state: &mut TuiState,
    _loop_state: &mut LoopState,
    session: &SessionId,
    event: CoreEvent,
) -> Result<(), String> {
    let owner = match &event {
        CoreEvent::TurnStarted { session, .. }
        | CoreEvent::TurnPresentation { session, .. }
        | CoreEvent::TextDelta { session, .. }
        | CoreEvent::ReasoningDelta { session, .. }
        | CoreEvent::ToolCallStarted { session, .. }
        | CoreEvent::ToolCallFinished { session, .. }
        | CoreEvent::TurnUsage { session, .. }
        | CoreEvent::TurnFinished { session, .. }
        | CoreEvent::TurnInterrupted { session, .. }
        | CoreEvent::TurnFailed { session, .. } => session,
    };
    if state.attached_session() != Some(owner) {
        return Ok(());
    }
    match event {
        CoreEvent::TurnStarted { .. } => {}
        CoreEvent::TurnPresentation {
            turn, projection, ..
        } => state.apply_presentation(&turn, &projection),
        CoreEvent::TextDelta { turn, delta, .. } => state.apply_delta(&turn, &delta),
        CoreEvent::ReasoningDelta { turn, delta, .. } => {
            state.apply_reasoning_delta(&turn, &delta);
        }
        CoreEvent::ToolCallStarted {
            turn,
            op,
            name,
            input,
            ..
        } => state.apply_tool_started(&turn, &op, &name, &input),
        CoreEvent::ToolCallFinished {
            turn,
            op,
            name,
            state: tool_state,
            output,
            output_bytes,
            output_truncated,
            ..
        } => state.apply_tool_finished(
            &turn,
            &op,
            &name,
            &tool_state,
            &output,
            output_bytes,
            output_truncated,
        ),
        CoreEvent::TurnUsage {
            turn,
            input_tokens,
            output_tokens,
            streamed_ms,
            ..
        } => state.apply_usage(&turn, input_tokens, output_tokens, streamed_ms),
        CoreEvent::TurnFinished {
            turn,
            text,
            duration_ms,
            warnings,
            ..
        } => {
            let current = state.active_turn() == Some(&turn);
            let compress = state.is_compress_turn(&turn);
            state.apply_finished(&turn, &text, duration_ms);
            if current {
                let page = app
                    .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|e| e.to_string())?;
                state.attach_page(&page);
            }
            if compress {
                report_compress_outcome(app, state, session).await?;
            }
            // After the durable page: degradation rows are transient notices,
            // so they must follow the page attach that rebuilds the transcript.
            for warning in warnings {
                state.push_warning(&warning);
            }
        }
        CoreEvent::TurnInterrupted {
            turn,
            partial,
            duration_ms,
            ..
        } => {
            let compress = state.is_compress_turn(&turn);
            state.apply_interrupted(&turn, &partial, duration_ms);
            if compress {
                state.notify_dcp(DcpOutcome::Failed {
                    reason: "compress turn cancelled".to_string(),
                });
            }
        }
        CoreEvent::TurnFailed {
            turn,
            error,
            warnings,
            ..
        } => {
            let compress = state.is_compress_turn(&turn);
            state.apply_failed(&turn, &error);
            for warning in warnings {
                state.push_warning(&warning);
            }
            if compress {
                state.notify_dcp(DcpOutcome::Failed {
                    reason: error.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Refresh the DCP snapshot for the attached session.
async fn refresh_dcp(app: &CoreApp, state: &mut TuiState, session: &SessionId) {
    if let Ok(snapshot) = app.dcp_snapshot(session.clone()).await {
        state.apply_dcp_snapshot(snapshot);
    }
}

/// Report the real outcome of a manual compress turn from recorded tool
/// operations (never invented numbers).
async fn report_compress_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    session: &SessionId,
) -> Result<(), String> {
    refresh_dcp(app, state, session).await;
    let page = app
        .tool_ops_page(session.clone(), None, TOOL_OPS_PAGE_LIMIT)
        .await
        .map_err(|e| e.to_string())?;
    let saved = page
        .rows
        .iter()
        .filter(|row| row.name == "compress")
        .find_map(|row| {
            let output = row.output.as_deref()?;
            let value: serde_json::Value = serde_json::from_str(output).ok()?;
            value.get("savedTokens").and_then(serde_json::Value::as_u64)
        });
    match saved {
        Some(saved_tokens) => state.notify_dcp(DcpOutcome::Done { saved_tokens }),
        None => state.notify_dcp(DcpOutcome::Failed {
            reason: "no compression recorded in this turn".to_string(),
        }),
    }
    Ok(())
}

/// Bounded view metrics for PTY qualification (opt-in, never in normal use).
fn write_metrics(state: &TuiState, frames: Option<&FrameMetrics>) {
    let Some(path) = std::env::var_os(METRICS_ENV) else {
        return;
    };
    let metrics = serde_json::json!({
        "session": state.attached_session().map(|session| &session.0),
        "tab_count": state.tab_presentation().0.len(),
        "retained_bytes": state.retained_bytes(),
        "window_rows": state.history().len(),
        "window_total": state.history().total(),
        "panel": format!("{:?}", state.panel()),
        "status": format!("{:?}", state.status()),
        "frame_count": frames.map_or(0, |frames| frames.count),
        "frame_sum_ns": frames.map_or(0, |frames| frames.sum_ns),
        "frame_max_ns": frames.map_or(0, |frames| frames.max_ns),
    });
    let _ = std::fs::write(path, metrics.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::core_app::InboxMsg;
    use oc_core::core_app::WorkerTurnId;
    use oc_core::queries::{AutoAcceptState, HistoryMessage, HistoryPage, ToolOpPage, ToolOpView};
    use oc_core::session::Role;

    fn catalog() -> CatalogSnapshot {
        CatalogSnapshot {
            chrome: Default::default(),
            auto_accept: AutoAcceptState::Unsupported,
            provider: "fixture".into(),
            models: Vec::new(),
            model_id: "fixture/model".into(),
            variant: None,
            agents: Vec::new(),
            agent_id: None,
            commands: Vec::new(),
        }
    }

    fn append_tab(app: &CoreApp, deck: &mut LoopState, state: &mut TuiState, id: &str) {
        let old = deck.active_tab.expect("active real tab");
        deck.tabs[old] = Some(std::mem::replace(
            state,
            TuiState::new(app.clone(), SessionId::new(id).unwrap()),
        ));
        deck.tab_cards_before[old] = deck.cards_before;
        deck.active_tab = Some(deck.tabs.len());
        deck.tabs.push(None);
        deck.tab_cards_before.push(None);
        deck.cards_before = None;
        deck.sync_tabs(state);
    }

    #[tokio::test]
    async fn mouse_close_home_holds_survivor_at_the_pointer_but_keyboard_switch_does_not() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};
        use ratatui::layout::Rect;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
        state.session_title = Some("Какие инструменты доступны ассистенту".into());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        let area = Rect::new(0, 0, 120, 40);
        let before = oc_tui::shell::tab_strip(&state, area).unwrap();
        let close = before.tabs[1].rect.right() - 2;
        let mouse = |kind| MouseEvent {
            kind,
            column: close,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(mouse(MouseEventKind::Moved), area);
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
        let outcome = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area);
        assert_eq!(outcome.intent, Some(PanelIntent::CloseTab { index: 1 }));
        apply_mouse_outcome(&app, &mut state, &mut deck, outcome, area, close, 0).await;
        let after = oc_tui::shell::tab_strip(&state, area).unwrap();
        assert!(deck.home.is_none());
        assert_eq!(state.session().0, "kept");
        assert_eq!(after.tabs[0].rect.right() - 2, close);
        assert_eq!(
            state.tab_close_cell(area, 0, after.tabs[0].rect),
            Some(close)
        );
        assert_eq!(after.add.unwrap().x, close + 2);
        assert!(inbox.try_recv().is_err());

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render_frame(frame, &state)).unwrap();
        let cells = terminal.backend().buffer();
        let row: String = (0..120).map(|x| cells[(x, 0)].symbol()).collect();
        assert!(
            row.starts_with("   Какие инструменты доступны ассистенту"),
            "{row}"
        );
        assert_eq!(cells[(close, 0)].symbol(), "✕");
        assert_eq!(
            cells[(close, 0)].fg,
            ratatui::style::Color::Rgb(238, 238, 238)
        );
        assert_eq!(cells[(close + 3, 0)].symbol(), "+");

        state.clear_mouse_position(); // PTY resize invalidates the held frame.
        let resized = oc_tui::shell::tab_strip(&state, area).unwrap();
        assert_eq!(resized.tabs[0].rect.width, 32);
        assert_eq!(state.tab_close_cell(area, 0, resized.tabs[0].rect), None);

        // Clicking a real tab replaces Home's view too: re-hit-test the
        // release, rather than relying on the parked view's stale hover.
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        let point = |kind| MouseEvent {
            kind,
            column: 3,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(point(MouseEventKind::Moved), area);
        state.handle_mouse(point(MouseEventKind::Down(MouseButton::Left)), area);
        let activate = state.handle_mouse(point(MouseEventKind::Up(MouseButton::Left)), area);
        assert_eq!(activate.intent, Some(PanelIntent::ActivateTab { index: 0 }));
        apply_mouse_outcome(&app, &mut state, &mut deck, activate, area, 3, 0).await;
        let clicked = oc_tui::shell::tab_strip(&state, area).unwrap().tabs[0].rect;
        assert_eq!(
            state.tab_close_cell(area, 0, clicked),
            Some(clicked.right() - 2)
        );

        // A keyboard switch has no pointer event and must not manufacture hover.
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        deck.activate(&mut state, 0).unwrap();
        let normal = oc_tui::shell::tab_strip(&state, area).unwrap();
        assert_eq!(normal.tabs[0].rect.width, 32);
        assert_eq!(state.tab_close_cell(area, 0, normal.tabs[0].rect), None);
    }

    #[tokio::test]
    async fn close_inactive_tab_reindexes_active_view_and_cursor_without_owner_query() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("first draft");
        state.attach_page(&HistoryPage {
            rows: vec![HistoryMessage {
                seq: 1,
                role: Role::User,
                text: "first viewport marker".into(),
                turn: None,
            }],
            total: 1,
            ..Default::default()
        });
        deck.cards_before = Some(11);
        append_tab(&app, &mut deck, &mut state, "b");
        state.handle_paste("middle draft");
        deck.cards_before = Some(22);
        append_tab(&app, &mut deck, &mut state, "c");
        state.handle_paste("active draft");
        deck.cards_before = Some(33);

        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 1 },
        )
        .await
        .unwrap();
        assert_eq!(deck.active_tab, Some(1));
        assert_eq!(deck.tab_cards_before, [Some(11), None]);
        assert_eq!(deck.cards_before, Some(33));
        assert_eq!(state.input(), "active draft");
        assert_eq!(state.tab_presentation().0.len(), 2);
        assert_eq!(state.tab_presentation().1, 1);
        assert!(inbox.try_recv().is_err(), "close cannot delete a session");
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(state.input(), "first draft");
        assert!(
            state
                .viewport()
                .join("\n")
                .contains("first viewport marker")
        );
        assert_eq!(deck.cards_before, Some(11));
    }

    #[tokio::test]
    async fn close_selected_real_tab_prefers_previous_and_preserves_survivors() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("first draft");
        append_tab(&app, &mut deck, &mut state, "b");
        state.handle_paste("middle draft");
        append_tab(&app, &mut deck, &mut state, "c");
        state.handle_paste("discarded draft");
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 2 },
        )
        .await
        .unwrap();
        assert_eq!(state.session().0, "b");
        assert_eq!(state.input(), "middle draft");
        assert_eq!(deck.active_tab, Some(1));
        assert_eq!(deck.tabs.len(), 2);
        assert_eq!(state.tab_presentation().0.len(), 2);
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await
        .unwrap();
        assert_eq!(state.session().0, "b");
        assert_eq!(deck.active_tab, Some(0));
        assert_eq!(state.tab_presentation().0.len(), 1);
        assert!(inbox.try_recv().is_err());
        assert_eq!(
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::CloseTab { index: 2 }
            )
            .await
            .unwrap_err(),
            "tab unavailable"
        );
        assert_eq!(state.input(), "middle draft");
    }

    #[tokio::test]
    async fn close_last_real_tab_queries_current_home_before_discarding_and_reopens() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("durable").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("saved draft");
        let worker = tokio::spawn(async move {
            for result in [Err(CoreError::Shutdown), Ok(catalog())] {
                let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
                    panic!("close must query Home only")
                };
                assert_eq!(action, SelectionAction::Current);
                ack.send(result).unwrap();
            }
            let Some(InboxMsg::SessionSelection {
                session,
                action,
                ack,
                ..
            }) = inbox.recv().await
            else {
                panic!("Sessions reopen must query existing selection")
            };
            assert_eq!(session.0, "durable");
            assert_eq!(action, SelectionAction::Current);
            ack.send(Ok(catalog())).unwrap();
            let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
                panic!("Sessions reopen must read durable history")
            };
            assert_eq!(session.0, "durable");
            ack.send(Ok(Default::default())).unwrap();
        });
        let intent = PanelIntent::CloseTab { index: 0 };
        assert!(
            apply_intent(&app, &mut state, &mut deck, intent.clone())
                .await
                .is_err()
        );
        assert_eq!(state.input(), "saved draft");
        assert_eq!(state.session().0, "durable");
        assert_eq!(deck.tabs.len(), 1);
        apply_intent(&app, &mut state, &mut deck, intent)
            .await
            .unwrap();
        assert!(state.attached_session().is_none());
        assert_eq!(deck.active_tab, None);
        assert!(deck.tabs.is_empty() && deck.tab_cards_before.is_empty());
        assert!(state.tab_presentation().0.is_empty());
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchSession {
                id: "durable".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(state.session().0, "durable");
        assert_eq!(deck.tabs.len(), 1);
        assert_eq!(deck.active_tab, Some(0));
        assert!(deck.home.is_some(), "reopening parks synthetic Home");
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await
        .unwrap();
        assert!(state.attached_session().is_none());
        assert!(deck.home.is_none() && deck.tabs.is_empty());
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn close_selected_or_parked_synthetic_home_never_creates_a_root() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("kept draft");
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        state.handle_paste("home draft");
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 1 },
        )
        .await
        .unwrap();
        assert_eq!(state.session().0, "kept");
        assert_eq!(state.input(), "kept draft");
        assert!(deck.home.is_none());
        assert_eq!(state.tab_presentation().0.len(), 1);
        assert_eq!(state.tab_presentation().1, 0);
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        deck.activate(&mut state, 0).unwrap();
        assert!(deck.home.is_some());
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 1 },
        )
        .await
        .unwrap();
        assert!(deck.home.is_none());
        assert_eq!(state.input(), "kept draft");
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await
        .unwrap();
        assert!(state.attached_session().is_none());
        assert!(deck.tabs.is_empty() && deck.tab_cards_before.is_empty());
        assert!(state.tab_presentation().0.is_empty());
        assert_eq!(
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::CloseTab { index: 0 }
            )
            .await
            .unwrap_err(),
            "tab unavailable"
        );
        assert!(inbox.try_recv().is_err());
    }

    #[tokio::test]
    async fn closing_last_real_tab_restores_parked_home_draft_without_query() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        state.handle_paste("parked Home draft");
        deck.activate(&mut state, 0).unwrap();
        assert!(deck.home.is_some());
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await
        .unwrap();
        assert!(state.attached_session().is_none());
        assert_eq!(state.input(), "parked Home draft");
        assert!(deck.tabs.is_empty() && deck.home.is_none());
        assert!(inbox.try_recv().is_err());
    }

    #[tokio::test]
    async fn close_refuses_pending_turn_and_preserves_deck() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("busy").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("pending draft");
        state.handle_key(KeyAction::Enter).await;
        let _request = inbox.recv().await.unwrap();
        let error = apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::CloseTab { index: 0 },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "turn active; tab close refused");
        assert_eq!(state.input(), "pending draft");
        assert_eq!(state.session().0, "busy");
        assert_eq!(deck.tabs.len(), 1);
        assert!(inbox.try_recv().is_err());
    }

    #[tokio::test]
    async fn home_reserves_sixteenth_slot_against_new_session_selection() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("tab-0").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        for index in 1..MAX_TABS - 1 {
            let next = TuiState::new(app.clone(), SessionId::new(format!("tab-{index}")).unwrap());
            deck.tabs[deck.active_tab.unwrap()] = Some(std::mem::replace(&mut state, next));
            deck.active_tab = Some(deck.tabs.len());
            deck.tabs.push(None);
            deck.tab_cards_before.push(None);
        }
        assert_eq!(deck.tabs.len(), MAX_TABS - 1);
        deck.open_home(&mut state, TuiState::new_home(app.clone()));
        state.handle_paste("Home draft");
        deck.activate(&mut state, 0).unwrap();
        state.handle_paste("parked draft");
        let error = apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchSession {
                id: "tab-15".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "tab limit reached");
        assert!(
            inbox.try_recv().is_err(),
            "rejection must precede app selection"
        );
        assert_eq!(state.input(), "parked draft");
        assert_eq!(deck.tabs.len(), MAX_TABS - 1);
        apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
            .await
            .unwrap();
        assert_eq!(state.input(), "Home draft");
        assert!(state.attached_session().is_none());
        // The actual first-turn receipt binds Home to its reserved slot.
        state.handle_key(KeyAction::Enter).await;
        let Some(InboxMsg::SubmitFresh { text, ack, .. }) = inbox.recv().await else {
            panic!("expected fresh Home submission")
        };
        assert_eq!(text, "Home draft");
        ack.send(Ok(WorkerTurnId("accepted-home-turn".into())))
            .unwrap();
        state.poll_submission();
        deck.sync_tabs(&mut state);
        assert_eq!(deck.tabs.len(), MAX_TABS);
        assert!(state.attached_session().is_some());
        assert_eq!(deck.tabs[0].as_ref().unwrap().input(), "parked draft");
    }

    #[tokio::test]
    async fn typed_new_consumes_only_successful_command_before_parking() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("old").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("ordinary draft");
        // Mouse + leaves the parked editor untouched.
        let worker = tokio::spawn(async move {
            for _ in 0..2 {
                let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
                    panic!("expected Home selection")
                };
                ack.send(Ok(catalog())).unwrap();
            }
        });
        apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
            .await
            .unwrap();
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(state.input(), "ordinary draft");
        state.accept_intent();
        state.handle_paste("/new");
        // The parked Home is restored without querying a new selection.
        let outcome = state.handle_key(KeyAction::Enter).await;
        apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(state.input(), "");
        // An initial attached view follows the selection-success path too.
        let mut state = TuiState::new(app.clone(), SessionId::new("another").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("/new");
        let outcome = state.handle_key(KeyAction::Enter).await;
        apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(state.input(), "");
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn rejected_typed_new_keeps_editable_command() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("old").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        state.handle_paste("/new");
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
                panic!("expected Home selection")
            };
            ack.send(Err(CoreError::Shutdown)).unwrap();
        });
        let outcome = state.handle_key(KeyAction::Enter).await;
        apply_outcome(&app, &mut state, &mut deck, outcome, true).await;
        assert_eq!(state.input(), "/new");
        assert_eq!(deck.tabs.len(), 1);
        assert!(deck.home.is_none());
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn cards_cursor_follows_parked_view_and_pages_from_oldest_loaded_row() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("cards-a").unwrap());
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        let worker = tokio::spawn(async move {
            for (before, ids, has_older) in
                [(None, vec![30, 20], true), (Some(20), vec![10], false)]
            {
                let Some(InboxMsg::ToolOps {
                    before_rowid, ack, ..
                }) = inbox.recv().await
                else {
                    panic!("expected cards query")
                };
                assert_eq!(before_rowid, before);
                ack.send(Ok(ToolOpPage {
                    rows: ids
                        .into_iter()
                        .map(|rowid| ToolOpView {
                            op: format!("op-{rowid}"),
                            rowid,
                            name: "read".into(),
                            state: "completed".into(),
                            input: None,
                            output: None,
                            output_bytes: 0,
                            output_truncated: false,
                        })
                        .collect(),
                    total: 3,
                    has_older,
                }))
                .unwrap();
            }
        });
        apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadCards)
            .await
            .unwrap();
        assert_eq!(deck.cards_before, Some(20));
        assert!(state.cards_need_older());
        let next = TuiState::new(app.clone(), SessionId::new("cards-b").unwrap());
        deck.tabs[0] = Some(std::mem::replace(&mut state, next));
        deck.tab_cards_before[0] = deck.cards_before;
        deck.active_tab = Some(1);
        deck.tabs.push(None);
        deck.tab_cards_before.push(None);
        deck.cards_before = None;
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(deck.cards_before, Some(20));
        assert!(state.cards_need_older());
        apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadCards)
            .await
            .unwrap();
        assert_eq!(deck.cards_before, Some(10));
        assert!(!state.cards_need_older());
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn v03_catalog_failure_is_not_an_empty_usable_session() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::Create { ack, .. }) = inbox.recv().await else {
                panic!("create")
            };
            ack.send(Ok(())).unwrap();
            let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                panic!("history")
            };
            ack.send(Ok(Default::default())).unwrap();
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("catalog")
            };
            ack.send(Err(oc_core::session::CoreError::Shutdown))
                .unwrap();
        });
        let result = initial_state(&app, Some(SessionId::new("catalog-failure").unwrap())).await;
        assert!(matches!(result, Err(StartupFailure::Query)));
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn bare_home_queries_selection_without_creating_or_reading_history() {
        use oc_core::core_app::InboxMsg;
        let (app, mut inbox, _) = CoreApp::channel(4);
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
                panic!("Home must query selection first")
            };
            assert_eq!(action, SelectionAction::Current);
            ack.send(Err(CoreError::Shutdown)).unwrap();
            assert!(inbox.try_recv().is_err(), "no root or history query");
        });
        assert!(matches!(
            initial_state(&app, None).await,
            Err(StartupFailure::Query)
        ));
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn home_refuses_session_scoped_queries() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app.clone());
        let mut loop_state = LoopState::default();
        for intent in [
            PanelIntent::LoadCards,
            PanelIntent::LoadCardOutput {
                op: "op".into(),
                offset: 0,
            },
            PanelIntent::LoadOlder,
            PanelIntent::LoadNewer,
            PanelIntent::Compress {
                focus: String::new(),
            },
        ] {
            assert!(
                apply_intent(&app, &mut state, &mut loop_state, intent)
                    .await
                    .unwrap_err()
                    .contains("no active session")
            );
            assert!(
                inbox.try_recv().is_err(),
                "no session-scoped query reached worker"
            );
        }
    }

    #[tokio::test]
    async fn pending_submission_refuses_session_and_location_switches() {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let session = SessionId::new("pending").unwrap();
        let mut state = TuiState::new(app.clone(), session.clone());
        state.handle_paste("immutable draft");
        state.handle_key(KeyAction::Enter).await;
        let _request = inbox.recv().await.unwrap(); // retain acceptance sender
        let mut loop_state = LoopState::default();
        for intent in [
            PanelIntent::SwitchSession { id: "other".into() },
            PanelIntent::SwitchLocation {
                path: "/fixture/other".into(),
            },
        ] {
            let error = apply_intent(&app, &mut state, &mut loop_state, intent)
                .await
                .unwrap_err();
            assert!(error.contains("switch refused"));
            assert_eq!(state.session(), &session);
            assert_eq!(state.input(), "immutable draft");
            assert!(inbox.try_recv().is_err(), "switch reached runtime");
        }
        // Session-scoped events cannot match even a coincident turn id.
        state.begin_compress_turn(WorkerTurnId("same-id".into()));
        handle_worker_event(
            &app,
            &mut state,
            &mut loop_state,
            &session,
            CoreEvent::TextDelta {
                session: SessionId::new("other").unwrap(),
                turn: WorkerTurnId("same-id".into()),
                delta: "wrong session".into(),
            },
        )
        .await
        .unwrap();
        assert!(!state.viewport().join("\n").contains("wrong session"));
    }
}
