//! Real-terminal consumer of the shared native application.
//!
//! The view-model (`oc-tui`) is storage-free: this loop answers its
//! [`PanelIntent`] values through the application API, drains worker events
//! into turn-scoped view state, and owns terminal setup/restore. UI never
//! persists input or outcomes independently of application acceptance.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use crossterm::event::{self, Event as CEvent};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use oc_adapters::application::{HISTORY_PAGE_LIMIT, TOOL_OPS_PAGE_LIMIT};
use oc_core::core_app::{CoreApp, CoreEvent, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_tui::app::{KeyOutcome, PanelIntent, TuiPanel, TuiState, TuiStatus};
use oc_tui::dcp_panel::DcpOutcome;
use oc_tui::events::{UiEvent, map_event};
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

/// Max key events drained per frame (paste bursts stay fast; a flooding
/// input still yields to the worker drain below each frame).
const MAX_KEYS_PER_FRAME: usize = 256;

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
    if !at_tty() {
        return Err(
            "no TTY for interactive TUI; use `oc run \"<prompt>\"` for headless use".to_string(),
        );
    }
    let project = std::env::current_dir().map_err(|e| e.to_string())?;
    let session = match session_opt {
        Some(raw) => SessionId::new(raw).ok_or_else(|| "invalid session id".to_string())?,
        None => SessionId::new(format!("s-tui-{}", nanos())).ok_or("id".to_string())?,
    };
    let (app, guard, diagnostics) = oc_adapters::application::spawn(&project, data_dir).await?;
    for diagnostic in diagnostics {
        eprintln!("warning: {diagnostic}");
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
    compress_turn: Option<WorkerTurnId>,
    /// Cursor for paging older tool cards.
    cards_before: Option<i64>,
    /// DCP snapshot was fetched for the currently open panel.
    dcp_seen: bool,
}

async fn drive_ui(app: &CoreApp, session: SessionId) -> Result<ExitCode, String> {
    let _term = enter()?;
    if std::env::var_os(PANIC_PROBE_ENV).is_some() {
        panic!("{PANIC_PROBE_ENV} probe");
    }
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;
    app.create_session(session.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut rx = app.subscribe();
    let mut loop_state = LoopState::default();
    // Seed the viewport from the newest durable page (resume shows prior
    // turns without ever loading the whole transcript).
    let page = app
        .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await
        .map_err(|e| e.to_string())?;
    state.attach_page(&page);
    // The catalog is cheap (no provider call) and tells the view which
    // workspace commands the application owns.
    if let Ok(snapshot) = app.catalog().await {
        state.apply_catalog(snapshot);
    }

    loop {
        terminal
            .draw(|frame| render_frame(frame, &state))
            .map_err(|e| format!("draw: {e}"))?;
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
            }
        }
        // Worker events, non-blocking drain.
        while let Ok(event) = rx.try_recv() {
            let current = state.session().clone();
            handle_worker_event(app, &mut state, &mut loop_state, &current, event).await?;
        }
        // The DCP panel shows runtime counters: refresh when it opens.
        if *state.panel() == TuiPanel::Dcp && !loop_state.dcp_seen {
            let current = state.session().clone();
            refresh_dcp(app, &mut state, &current).await;
            loop_state.dcp_seen = true;
        } else if *state.panel() != TuiPanel::Dcp {
            loop_state.dcp_seen = false;
        }
    }
    write_metrics(&state);
    drop(_term);
    Ok(ExitCode::SUCCESS)
}

fn at_tty() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
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
            let outcome = if *state.panel() == TuiPanel::None {
                state.handle_key(action).await
            } else {
                state.handle_panel_key(action)
            };
            apply_outcome(app, state, loop_state, outcome).await;
        }
        Some(UiEvent::Paste(text)) => {
            if *state.panel() == TuiPanel::None {
                let outcome = state.handle_paste(&text);
                if let Some(note) = outcome.note {
                    state.push_note(&note);
                }
            }
        }
        Some(UiEvent::Resize) | None => {}
    }
    Ok(())
}

/// Report a note, then apply the intent (or its typed failure).
async fn apply_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    outcome: KeyOutcome,
) {
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else {
        return;
    };
    // Scrolling intents never consume typed input; commands do.
    let consumes = !matches!(intent, PanelIntent::LoadOlder | PanelIntent::LoadNewer);
    match apply_intent(app, state, loop_state, intent).await {
        Ok(()) => {
            if consumes {
                state.accept_intent();
            }
        }
        Err(message) => state.apply_intent_error(message),
    }
}

async fn apply_intent(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    intent: PanelIntent,
) -> Result<(), String> {
    let session = state.session().clone();
    match intent {
        PanelIntent::LoadCatalog => {
            let snapshot = app.catalog().await.map_err(|e| e.to_string())?;
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
        PanelIntent::ChooseModel { id, variant } => {
            let snapshot = app
                .select_model(id, variant)
                .await
                .map_err(|e| e.to_string())?;
            let note = format!("model: {}", snapshot.model_id);
            state.apply_catalog(snapshot);
            state.close_panel();
            state.push_note(&note);
        }
        PanelIntent::SelectAgent { id } => {
            let snapshot = app.select_agent(id).await.map_err(|e| e.to_string())?;
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
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            state.set_session(target);
            state.attach_page(&page);
            state.close_panel();
        }
        PanelIntent::SwitchLocation { path } => {
            // The application refuses a switch during a turn; the view-model
            // keeps the current Location and session until it is published.
            if state.is_busy() {
                return Err("turn active; location switch refused".to_string());
            }
            let snapshot = app.switch_location(path).await.map_err(|e| e.to_string())?;
            let target = SessionId::new(snapshot.session.clone())
                .ok_or_else(|| "bad session id".to_string())?;
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            state.reset_workspace();
            state.set_session(target);
            state.attach_page(&page);
            state.apply_catalog(snapshot.catalog);
            state.close_panel();
            loop_state.cards_before = None;
            state.push_note(&format!("location: {}", snapshot.location));
            for diagnostic in snapshot.diagnostics {
                state.push_note(&format!("warning: {diagnostic}"));
            }
        }
        PanelIntent::LoadOlder => {
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
            let turn = app
                .compress(session.clone(), focus)
                .await
                .map_err(|e| e.to_string())?;
            state.begin_compress_turn(turn.clone());
            loop_state.compress_turn = Some(turn);
            let snapshot = app.dcp_snapshot(session).await.map_err(|e| e.to_string())?;
            state.apply_dcp_snapshot(snapshot);
        }
    }
    Ok(())
}

async fn handle_worker_event(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    session: &SessionId,
    event: CoreEvent,
) -> Result<(), String> {
    match event {
        CoreEvent::TurnStarted { .. } => {}
        CoreEvent::TextDelta { turn, delta, .. } => state.apply_delta(&turn, &delta),
        CoreEvent::TurnFinished { turn, text, .. } => {
            let compress = loop_state.compress_turn.as_ref() == Some(&turn);
            state.apply_finished(&turn, &text);
            if compress {
                loop_state.compress_turn = None;
                report_compress_outcome(app, state, session).await?;
            }
        }
        CoreEvent::TurnInterrupted { turn, .. } => {
            let compress = loop_state.compress_turn.as_ref() == Some(&turn);
            state.apply_interrupted(&turn);
            if compress {
                loop_state.compress_turn = None;
                state.notify_dcp(DcpOutcome::Failed {
                    reason: "compress turn cancelled".to_string(),
                });
            }
        }
        CoreEvent::TurnFailed { turn, error, .. } => {
            let compress = loop_state.compress_turn.as_ref() == Some(&turn);
            state.apply_failed(&turn, &error);
            if compress {
                loop_state.compress_turn = None;
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
fn write_metrics(state: &TuiState) {
    let Some(path) = std::env::var_os(METRICS_ENV) else {
        return;
    };
    let metrics = serde_json::json!({
        "session": state.session().0,
        "retained_bytes": state.retained_bytes(),
        "window_rows": state.history().len(),
        "window_total": state.history().total(),
        "panel": format!("{:?}", state.panel()),
        "status": format!("{:?}", state.status()),
    });
    let _ = std::fs::write(path, metrics.to_string());
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
