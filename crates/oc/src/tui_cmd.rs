//! Real-terminal `oc tui` loop for T06.
//!
//! Same `CoreApp` worker headless uses, plus `Db` persistence wired in the
//! binary (keeps `oc-tui -> oc-core` direction). Alternate screen + raw mode
//! are always restored via a scope guard, including on error; panic
//! restoration beyond the guard is qualified with a real PTY in T26.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use crossterm::event::{self, Event as CEvent};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};

use oc_adapters::storage::Db;
use oc_core::core_app::{CoreApp, CoreEvent, MockProvider};
use oc_core::domain::SessionId;
use oc_tui::app::{TuiState, TuiStatus};
use oc_tui::events::map_key;
use oc_tui::terminal::{enter, install_panic_hook};

/// Qualification probe (T26): when set, panic right after entering the
/// terminal so PTY tests can verify panic-path restoration. Never set in
/// normal use.
const PANIC_PROBE_ENV: &str = "OC_TUI_TEST_PANIC";

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
        return Err("no TTY for interactive TUI; use `oc run` headless".to_string());
    }
    let db = Db::open(data_dir).map_err(|e| format!("storage: {e}"))?;
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    let session = match session_opt {
        Some(raw) => SessionId::new(raw).ok_or_else(|| "invalid session id".to_string())?,
        None => SessionId::new(format!("s-tui-{}", nanos())).ok_or("id".to_string())?,
    };
    match db.create_session(&session.0) {
        Ok(()) => {}
        Err(oc_adapters::storage::StorageError::Sqlite(_)) => {}
        Err(e) => return Err(format!("storage: {e}")),
    }
    app.create_session(session.clone())
        .await
        .map_err(|_| "worker unavailable".to_string())?;

    let _term = enter()?;
    if std::env::var_os(PANIC_PROBE_ENV).is_some() {
        panic!("{PANIC_PROBE_ENV} probe");
    }
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut rx = app.subscribe();
    // Seed viewport from durable history (resume shows prior turns).
    if let Ok(history) = db.read_history(&session.0) {
        for (role, text) in history {
            state.lines.push(format!("{role}: {text}"));
        }
    }

    loop {
        draw(&mut terminal, &state).map_err(|e| format!("draw: {e}"))?;
        if state.status == TuiStatus::Quit {
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
                handle_crossterm(cev, &mut state, &db, &session).await?;
            }
        }
        // Worker events, non-blocking drain.
        while let Ok(ev) = rx.try_recv() {
            match ev {
                CoreEvent::TurnStarted { .. } => {}
                CoreEvent::TextDelta { turn, delta, .. } => {
                    state.apply_delta(&turn, &delta);
                }
                CoreEvent::TurnFinished { turn, text, .. } => {
                    state.apply_finished(&turn, &text);
                    let _ = db.append_message(&session.0, "assistant", &text);
                }
                CoreEvent::TurnInterrupted { turn, .. } => {
                    state.apply_interrupted(&turn);
                }
            }
        }
    }
    drop(_term);
    let _ = app.shutdown().await;
    let _ = guard.join().await;
    Ok(ExitCode::SUCCESS)
}

fn at_tty() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

/// Max key events drained per frame (paste bursts stay fast; a flooding
/// input still yields to the worker drain below each frame).
const MAX_KEYS_PER_FRAME: usize = 256;

/// Handle one Crossterm event: keys drive `TuiState`, anything else is
/// ignored (resize is picked up by the next draw, which re-queries size).
async fn handle_crossterm(
    cev: CEvent,
    state: &mut TuiState,
    db: &Db,
    session: &SessionId,
) -> Result<(), String> {
    if let CEvent::Key(key) = cev
        && let Some(action) = map_key(key)
    {
        // Capture input before Enter clears it for durable persist.
        let pending = if matches!(action, oc_tui::events::KeyAction::Enter) {
            Some(state.input.trim().to_string())
        } else {
            None
        };
        if let Some(note) = state.handle_key(action).await {
            state.lines.push(format!("({note})"));
        }
        if let Some(text) = pending
            && !text.is_empty()
            && text != "/quit"
        {
            let _ = db.append_message(&session.0, "user", &text);
        }
    }
    Ok(())
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &TuiState,
) -> Result<(), std::io::Error> {
    terminal.draw(|frame| {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([ratatui::layout::Constraint::Min(1), Constraint::Length(3)])
            .split(area);
        let visible = state.viewport();
        // Bottom-align the window in the pane: `viewport()` returns the
        // tail window (scroll-aware), but `Paragraph` top-aligns and would
        // clip the newest lines on small screens (T26 PTY find: after a few
        // turns a 40x8 pane froze on the first three lines forever).
        let pane_rows = chunks[0].height.saturating_sub(2) as usize;
        let skip = visible.len().saturating_sub(pane_rows.max(1));
        let history = Paragraph::new(visible.join("\n"))
            .scroll((skip as u16, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("oc {:?}", state.status)),
            );
        frame.render_widget(history, chunks[0]);
        let prompt = Paragraph::new(state.input.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("prompt (/quit)"),
        );
        frame.render_widget(prompt, chunks[1]);
    })?;
    Ok(())
}
