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
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph};

use oc_adapters::storage::Db;
use oc_core::core_app::{CoreApp, CoreEvent, MockProvider};
use oc_core::domain::SessionId;
use oc_tui::app::{TuiState, TuiStatus};
use oc_tui::events::map_key;

/// Guard that restores the terminal on drop.
struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), LeaveAlternateScreen);
    }
}

/// Launch the interactive TUI; returns process exit code.
pub async fn run_tui(data_dir: &Path, session_opt: Option<String>) -> ExitCode {
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

    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    let mut stderr = std::io::stderr();
    crossterm::execute!(stderr, EnterAlternateScreen).map_err(|e| format!("screen: {e}"))?;
    let _term = TermGuard;
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
        // Keys (50 ms poll keeps streaming responsive without busy loop).
        if event::poll(Duration::from_millis(50)).map_err(|e| format!("input: {e}"))? {
            let cev = event::read().map_err(|e| format!("input: {e}"))?;
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
        }
        // Worker events, non-blocking drain.
        while let Ok(ev) = rx.try_recv() {
            match ev {
                CoreEvent::TurnStarted { .. } => {}
                CoreEvent::TextDelta { turn, delta, .. } => {
                    push_live(&mut state, &turn.0, &delta);
                }
                CoreEvent::TurnFinished { turn, text, .. } => {
                    finish_live(&mut state, &turn.0, &text);
                    let _ = db.append_message(&session.0, "assistant", &text);
                }
                CoreEvent::TurnInterrupted { turn, .. } => {
                    cut_live(&mut state, &turn.0);
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

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn push_live(state: &mut TuiState, _turn: &str, delta: &str) {
    if let Some(last) = state.lines.last_mut()
        && last.starts_with("ai: ")
    {
        last.push_str(delta);
        return;
    }
    state.lines.push(format!("ai: {delta}"));
}

fn finish_live(state: &mut TuiState, _turn: &str, text: &str) {
    if let Some(last) = state.lines.last_mut() {
        if last.starts_with("ai: ") {
            *last = format!("ai: {text}");
        } else {
            state.lines.push(format!("ai: {text}"));
        }
    } else {
        state.lines.push(format!("ai: {text}"));
    }
}

fn cut_live(state: &mut TuiState, _turn: &str) {
    if let Some(last) = state.lines.last()
        && last.starts_with("ai: ")
    {
        state.lines.pop();
    }
    state.lines.push("(cancelled)".to_string());
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
        let visible = state.viewport().join("\n");
        let history = Paragraph::new(visible).block(
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
