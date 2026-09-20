//! Minimal Ratatui smoke: render one frame to a test backend.
//!
//! The full daily TUI (prompt/stream/history/cancel/exit) arrives in M1/M5.
//! This only proves Ratatui + Crossterm link and the `oc-core` application
//! handle is reachable without pulling storage/providers.

use oc_core::application::AppHandle;
use ratatui::{
    Terminal,
    backend::TestBackend,
    widgets::{Block, Borders, Paragraph},
};

/// TUI crate identity for diagnostics.
pub fn tui_name() -> &'static str {
    "oc-tui"
}

/// Render a single smoke frame showing the pending session count.
///
/// Returns the rendered buffer as text lines for assertion.
pub fn render_smoke_frame(app: &AppHandle) -> Vec<String> {
    let backend = TestBackend::new(40, 5);
    let mut terminal = Terminal::new(backend).expect("test backend");
    let text = format!("oc smoke pending={}", app.pending_sessions());
    terminal
        .draw(|frame| {
            let area = frame.area();
            let block = Block::default().borders(Borders::ALL).title(tui_name());
            let para = Paragraph::new(text.clone()).block(block);
            frame.render_widget(para, area);
        })
        .expect("draw smoke frame");
    let buffer = terminal.backend().buffer().clone();
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::render_smoke_frame;
    use oc_core::application::AppHandle;

    #[test]
    fn smoke_frame_contains_pending() {
        let app = AppHandle::smoke();
        let lines = render_smoke_frame(&app);
        assert_eq!(lines.len(), 5);
        let joined = lines.join("\n");
        assert!(joined.contains("pending=0"), "frame={joined}");
        assert!(joined.contains("oc-tui"), "frame={joined}");
    }

    #[test]
    fn crossterm_event_kind_is_linked() {
        // Prove crossterm links without opening a real terminal.
        let code = crossterm::event::KeyCode::Char('q');
        assert_eq!(code, crossterm::event::KeyCode::Char('q'));
    }
}
