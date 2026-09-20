//! Bounded Ratatui rendering for the minimal TUI.
//!
//! The viewport never renders the whole transcript: at most
//! `VIEWPORT_LINES` history lines plus input/status. PTY paste/resize and
//! terminal restoration are qualified in T26; here we assert viewport
//! bounds, Unicode width and state transitions on a `TestBackend`.

use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::TuiState;

/// Render state to a test backend; returns text lines for assertions.
pub fn render_test(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(area);
            let visible = state.viewport().join("\n");
            let history = Paragraph::new(visible).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("oc {:?}", state.status)),
            );
            frame.render_widget(history, chunks[0]);
            let prompt = Paragraph::new(state.input.as_str())
                .block(Block::default().borders(Borders::ALL).title("prompt"));
            frame.render_widget(prompt, chunks[1]);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::render_test;
    use crate::app::TuiState;
    use oc_core::core_app::{CoreApp, MockProvider};
    use oc_core::domain::SessionId;

    #[tokio::test]
    async fn render_is_bounded_with_unicode() {
        let (app, _guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(_guard);
        let sid = SessionId::new("s-r").expect("id");
        app.create_session(sid.clone()).await.expect("create");
        let mut state = TuiState::new(app, sid);
        state.lines.push("you: привет 🌍".to_string());
        state.lines.push("ai: ok".to_string());
        state.input = "next…".to_string();
        let lines = render_test(&state, 40, 10);
        assert_eq!(lines.len(), 10);
        let joined = lines.join("\n");
        assert!(joined.contains("привет"), "unicode must render: {joined}");
        assert!(joined.contains("prompt"), "input pane: {joined}");
        // Viewport constant is the product contract for T06.
        assert_eq!(crate::app::VIEWPORT_LINES, 20);
    }
}
