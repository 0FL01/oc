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
use oc_core::queries::SessionSelectionAction as SelectionAction;
use oc_core::queries::StartupNotice;
use oc_core::session::{CoreError, LocationSwitchFailure};
use oc_tui::app::{KeyOutcome, PanelIntent, TuiPanel, TuiState, TuiStatus};
use oc_tui::dcp_panel::DcpOutcome;
use oc_tui::events::{UiEvent, map_event};
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
    let home = session_opt.is_none();
    let session = match session_opt {
        Some(raw) => SessionId::new(raw).ok_or_else(|| "invalid session id".to_string())?,
        None => SessionId::new(format!("s-tui-{}", nanos())).ok_or("id".to_string())?,
    };
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
    let result = drive_ui(&app, session, home).await;
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
}

async fn drive_ui(app: &CoreApp, session: SessionId, home: bool) -> Result<u8, String> {
    let _term = enter()?;
    if std::env::var_os(PANIC_PROBE_ENV).is_some() {
        panic!("{PANIC_PROBE_ENV} probe");
    }
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;
    let mut state = match initial_state(app, session, home).await {
        Ok(state) => state,
        Err(failure) => return startup_failure(&mut terminal, failure).map(|_| 1),
    };
    let mut rx = app.subscribe();
    let mut loop_state = LoopState::default();
    let mut frame_metrics = std::env::var_os(METRICS_ENV).map(|_| FrameMetrics::default());

    loop {
        state.poll_submission();
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
    session: SessionId,
    home: bool,
) -> Result<TuiState, StartupFailure> {
    app.create_session(session.clone())
        .await
        .map_err(|_| StartupFailure::Query)?;
    let page = app
        .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await
        .map_err(|_| StartupFailure::Query)?;
    let mut state = TuiState::new(app.clone(), session);
    state.attach_page(&page);
    state.home = home && page.total == 0;
    // A failed catalog is an initialization error, never a usable empty snapshot.
    state.apply_catalog(
        app.session_selection(
            state.session().clone(),
            state.home,
            SelectionAction::Current,
        )
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
            let outcome = if *state.panel() == TuiPanel::None {
                state.handle_key(action).await
            } else {
                state.handle_panel_key(action)
            };
            apply_outcome(app, state, loop_state, outcome).await;
        }
        Some(UiEvent::Paste(text)) => {
            let outcome = state.handle_paste(&text);
            if let Some(note) = outcome.note {
                state.push_note(&note);
            }
        }
        Some(UiEvent::Mouse(mouse)) => {
            let outcome = if *state.panel() == TuiPanel::None
                && matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) {
                state.scroll_transcript(mouse.kind == MouseEventKind::ScrollUp)
            } else {
                let (cols, rows) =
                    crossterm::terminal::size().map_err(|e| format!("mouse terminal size: {e}"))?;
                state.handle_mouse(mouse, ratatui::layout::Rect::new(0, 0, cols, rows))
            };
            apply_outcome(app, state, loop_state, outcome).await;
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
    let consumes = matches!(intent, PanelIntent::SwitchLocation { .. });
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
            let snapshot = app
                .session_selection(session, state.home, SelectionAction::Current)
                .await
                .map_err(|e| e.to_string())?;
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
        PanelIntent::LoadCardOutput { op, offset } => {
            let page = app
                .tool_output_page(session, op.clone(), offset, 240)
                .await
                .map_err(|e| e.to_string())?;
            state.apply_card_output(op, offset, page);
        }
        PanelIntent::SelectModel { id } => {
            let snapshot = app
                .session_selection(session, state.home, SelectionAction::Model(id))
                .await
                .map_err(|e| e.to_string())?;
            let note = format!("model: {}", snapshot.model_id);
            state.model_choice_applied(snapshot);
            state.push_note(&note);
        }
        PanelIntent::ChooseModel { variant, .. } => {
            let snapshot = app
                .session_selection(session, state.home, SelectionAction::Variant(variant))
                .await
                .map_err(|e| e.to_string())?;
            state.model_choice_applied(snapshot);
        }
        PanelIntent::NewSession => {
            if state.is_busy() {
                return Err("turn active; action unavailable".into());
            }
            let target = SessionId::new(format!("tui-{}", nanos())).ok_or("bad session id")?;
            app.create_session(target.clone())
                .await
                .map_err(|e| e.to_string())?;
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            let snapshot = app
                .session_selection(
                    target.clone(),
                    true,
                    SelectionAction::New(state.active_agent().map(str::to_string)),
                )
                .await
                .map_err(|e| e.to_string())?;
            state.set_session(target);
            state.attach_page(&page);
            state.apply_catalog(snapshot);
            state.home = true;
            loop_state.cards_before = None;
            loop_state.dcp_seen = false;
            let session = state.session().clone();
            refresh_dcp(app, state, &session).await;
        }
        PanelIntent::SelectAgent { id } => {
            let snapshot = app
                .session_selection(session, state.home, SelectionAction::Agent(id))
                .await
                .map_err(|e| e.to_string())?;
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
            let snapshot = app
                .session_selection(target.clone(), false, SelectionAction::Current)
                .await
                .map_err(|e| e.to_string())?;
            let page = app
                .history_page(target.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            state.set_session(target);
            state.attach_page(&page);
            state.apply_catalog(snapshot);
            state.close_panel();
            loop_state.cards_before = None;
        }
        PanelIntent::SwitchLocation { path } => {
            // The application refuses a switch during a turn; the view-model
            // keeps the current Location and session until it is published.
            if state.is_busy() {
                return Err("turn active; location switch refused".to_string());
            }
            let snapshot = app.switch_location(path).await.map_err(|error| match error {
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
            })?;
            let target = SessionId::new(snapshot.session.clone())
                .ok_or_else(|| "bad session id".to_string())?;
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
            state.reset_workspace();
            state.set_session(target);
            state.attach_page(&page);
            state.apply_catalog(catalog);
            state.close_panel();
            loop_state.cards_before = None;
            state.push_note(&format!("location: {}", snapshot.location));
            for notice in snapshot.notices {
                state.push_note(&format!("warning: {}", startup_notice(notice)));
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
            state.request_compress(focus).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
    if owner != state.session() {
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
        "session": state.session().0,
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

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::core_app::WorkerTurnId;
    use oc_tui::events::KeyAction;

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
        let result = initial_state(&app, SessionId::new("catalog-failure").unwrap(), true).await;
        assert!(matches!(result, Err(StartupFailure::Query)));
        worker.await.unwrap();
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
