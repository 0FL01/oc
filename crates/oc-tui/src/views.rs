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

use crate::app::{TuiPanel, TuiState};

/// Render state to a test backend; returns text lines for assertions.
pub fn render_test(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| {
            let area = frame.area();
            let panel = panel_lines(state);
            // +2 for the panel block borders; zero height hides the panel.
            let panel_height = ((panel.len() + 2) as u16).min(12);
            let panel_height = if panel.is_empty() { 0 } else { panel_height };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(panel_height),
                    Constraint::Length(3),
                ])
                .split(area);
            let visible = state.viewport().join("\n");
            let title = match state.dcp.notice() {
                Some(notice) => format!("oc {:?} — {notice}", state.status),
                None => format!("oc {:?}", state.status),
            };
            let history =
                Paragraph::new(visible).block(Block::default().borders(Borders::ALL).title(title));
            frame.render_widget(history, chunks[0]);
            if panel_height > 0 {
                let panel_widget = Paragraph::new(panel.join("\n"))
                    .block(Block::default().borders(Borders::ALL).title("panel"));
                frame.render_widget(panel_widget, chunks[1]);
            }
            let prompt = Paragraph::new(state.input.as_str())
                .block(Block::default().borders(Borders::ALL).title("prompt"));
            frame.render_widget(prompt, chunks[2]);
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

/// Bounded panel lines for the active panel (empty when no panel).
pub fn panel_lines(state: &TuiState) -> Vec<String> {
    const ROWS: usize = 8;
    match &state.panel {
        TuiPanel::None => Vec::new(),
        TuiPanel::Model => match &state.picker {
            Some(picker) => {
                let mut out = vec![format!("model | {}", picker.status_line())];
                out.extend(picker.window().into_iter().take(ROWS));
                if let Some(error) = picker.last_error() {
                    out.push(format!("note: {error}"));
                }
                out
            }
            None => vec!["model | loading catalog…".to_string()],
        },
        TuiPanel::Sessions => {
            let mut out = vec!["sessions | enter resumes, esc closes".to_string()];
            for (i, id) in state.sessions.iter().take(ROWS).enumerate() {
                let mark = if i == state.sessions_cursor { ">" } else { " " };
                out.push(format!("{mark} {id}"));
            }
            out
        }
        TuiPanel::Skills => match &state.workspace {
            Some(workspace) => {
                let mut out = vec!["skills | bodies stay behind the native tool".to_string()];
                for (id, name, description) in workspace.skill_cards().into_iter().take(ROWS) {
                    out.push(format!("{id} — {name}: {description}"));
                }
                out
            }
            None => vec!["skills | no workspace registry".to_string()],
        },
        TuiPanel::Help(topic) => match topic {
            Some(topic) => vec![format!("help | {topic}"), help_topic(topic)],
            None => vec![
                "help | commands".to_string(),
                "/model /sessions /skills /dcp-compress /help /quit".to_string(),
            ],
        },
        TuiPanel::Dcp => {
            let mut out =
                vec!["dcp | /dcp-compress [focus] requests, runtime executes".to_string()];
            out.extend(state.dcp.panel_rows());
            out
        }
    }
}

fn help_topic(topic: &str) -> String {
    match topic {
        "model" => "pick the exact model id; retired ids never fall back".to_string(),
        "sessions" => "switch session; history pages load oldest-first".to_string(),
        "skills" => "catalog cards only; bodies load via the skill tool".to_string(),
        _ => "unknown topic".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{panel_lines, render_test};
    use crate::app::{TuiPanel, TuiState};
    use oc_adapters::models::ModelCatalog;
    use oc_adapters::storage::Db;
    use oc_core::core_app::{CoreApp, MockProvider};
    use oc_core::domain::SessionId;

    fn test_db(name: &str) -> Db {
        let root = std::env::temp_dir().join(format!("oc-tui-views-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Db::open(&root).expect("db")
    }

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

    #[tokio::test]
    async fn panels_render_bounded() {
        let (app, _guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(_guard);
        let sid = SessionId::new("s-p").expect("id");
        app.create_session(sid.clone()).await.expect("create");
        let db = test_db("panels");
        let mut state = TuiState::new(app, sid);

        let catalog = ModelCatalog {
            provider: "ludka2".to_string(),
            models: [("a".to_string(), serde_json::json!({}))]
                .into_iter()
                .collect(),
        };
        state.open_picker(catalog, &db).expect("picker");
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("model |")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "a"), "{lines:?}");
        let frame = render_test(&state, 60, 24);
        assert!(frame.join("\n").contains("model |"), "panel pane renders");

        state.open_sessions(vec!["s-p".to_string(), "s-q".to_string()]);
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("> s-p")), "{lines:?}");

        state.panel = TuiPanel::Help(None);
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("/model")), "{lines:?}");

        state.close_panel();
        assert!(panel_lines(&state).is_empty());
    }

    #[tokio::test]
    async fn dcp_panel_renders_snapshot_and_notice() {
        use crate::dcp_panel::{DcpContextSnapshot, DcpOutcome};

        let (app, _guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(_guard);
        let sid = SessionId::new("s-d").expect("id");
        app.create_session(sid.clone()).await.expect("create");
        let mut state = TuiState::new(app, sid);
        state.dcp.set_snapshot(DcpContextSnapshot {
            estimated_tokens: 900,
            max_context: 1000,
            turns_since_compress: 3,
            blocks: 2,
            compressions: 1,
            nudges: 4,
            prunes: 0,
        });
        state.dcp.request_compress("draft").expect("request");
        state.panel = TuiPanel::Dcp;
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("900/1000")), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("pending focus: draft")),
            "{lines:?}"
        );

        state.notify_dcp(DcpOutcome::Failed {
            reason: "span open".to_string(),
        });
        let frame = render_test(&state, 70, 24).join("\n");
        assert!(frame.contains("dcp failed: span open"), "notice in title");
    }
}
