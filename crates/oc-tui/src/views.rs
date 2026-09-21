//! Bounded Ratatui rendering for the minimal TUI.
//!
//! The viewport never renders the whole transcript: at most
//! [`crate::app::VIEWPORT_LINES`] history lines plus the panel and prompt
//! panes. PTY paste/resize and terminal restoration are qualified in T26;
//! here we assert viewport bounds, Unicode width and state transitions on a
//! `TestBackend`.

use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{TuiPanel, TuiState};

/// Render the whole state into one Ratatui frame.
pub fn render_frame(frame: &mut Frame<'_>, state: &TuiState) {
    let area = frame.area();
    let panel = panel_lines(state);
    // +2 for the panel block borders; zero height hides the panel.
    let panel_height = if panel.is_empty() {
        0
    } else {
        ((panel.len() + 2) as u16).min(12)
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(panel_height),
            Constraint::Length(3),
        ])
        .split(area);
    let visible = state.viewport();
    // Transient status: the intent note and the DCP notice stay visible
    // without ever entering history.
    let mut title = format!("oc {:?}", state.status());
    if let Some(note) = state.note() {
        title.push_str(&format!(" — {note}"));
    }
    if let Some(notice) = state.dcp.notice() {
        title.push_str(&format!(" — {notice}"));
    }
    // Bottom-align the window in the pane: `viewport()` returns the tail
    // window (scroll-aware) but `Paragraph` top-aligns and would clip the
    // newest lines on small screens.
    let pane_rows = chunks[0].height.saturating_sub(2) as usize;
    let skip = visible.len().saturating_sub(pane_rows.max(1));
    let history = Paragraph::new(visible.join("\n"))
        .scroll((skip as u16, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(history, chunks[0]);
    if panel_height > 0 {
        let panel_widget = Paragraph::new(panel.join("\n"))
            .block(Block::default().borders(Borders::ALL).title("panel"));
        frame.render_widget(panel_widget, chunks[1]);
    }
    let prompt =
        Paragraph::new(state.input()).block(Block::default().borders(Borders::ALL).title("prompt"));
    frame.render_widget(prompt, chunks[2]);
}

/// Render state to a test backend; returns text lines for assertions.
pub fn render_test(state: &TuiState, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| render_frame(frame, state))
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

/// Bounded panel lines (8 rows) for the active panel (empty when no panel).
pub fn panel_lines(state: &TuiState) -> Vec<String> {
    const ROWS: usize = 8;
    match state.panel() {
        TuiPanel::None => Vec::new(),
        TuiPanel::Model => match &state.picker {
            Some(picker) => {
                let mut out = vec![format!("model | {}", picker.status_line())];
                if let Some(variant) = picker.pending_variant() {
                    out.push(format!("variant: {variant}"));
                }
                out.extend(picker.window().into_iter().take(ROWS));
                if let Some(error) = picker.last_error() {
                    out.push(format!("note: {error}"));
                }
                out
            }
            None => vec!["model | loading catalog…".to_string()],
        },
        TuiPanel::Agents => {
            let mut out = vec!["agents | enter selects".to_string()];
            for (i, agent) in state.agents.iter().take(ROWS).enumerate() {
                let mark = if i == state.agents_cursor { ">" } else { " " };
                let model = match &agent.model {
                    Some(model) => format!(" [{model}]"),
                    None => String::new(),
                };
                out.push(format!(
                    "{mark} {} — {}{model}",
                    agent.id, agent.description
                ));
            }
            out
        }
        TuiPanel::Sessions => {
            let mut out = vec!["sessions | enter resumes, esc closes".to_string()];
            for (i, id) in state.sessions.iter().take(ROWS).enumerate() {
                let mark = if i == state.sessions_cursor { ">" } else { " " };
                out.push(format!("{mark} {id}"));
            }
            out
        }
        TuiPanel::Skills => {
            let mut out = vec!["skills | bodies stay behind the native tool".to_string()];
            for card in state.skills.iter().take(ROWS) {
                out.push(format!("{} — {}: {}", card.id, card.name, card.description));
            }
            out
        }
        TuiPanel::Cards => {
            let mut out = vec!["cards | newest first, up pages older".to_string()];
            for (i, row) in state.cards.iter().take(ROWS).enumerate() {
                let mark = if i == state.cards_cursor { ">" } else { " " };
                out.push(format!("{mark} {}", row.text));
            }
            out
        }
        TuiPanel::Help(topic) => match topic {
            Some(topic) => vec![format!("help | {topic}"), help_topic(topic.as_str())],
            None => vec![
                "help | commands".to_string(),
                "/model /agents /sessions /skills /cards /dcp-compress /help /quit".to_string(),
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
        "agents" => "pick the primary agent profile; model pins stay explicit".to_string(),
        "sessions" => "switch session; history pages load oldest-first".to_string(),
        "skills" => "catalog cards only; bodies load via the skill tool".to_string(),
        _ => "unknown topic".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{panel_lines, render_test};
    use crate::app::{TuiPanel, TuiState, VIEWPORT_LINES};
    use crate::dcp_panel::DcpOutcome;
    use crate::events::KeyAction;
    use oc_core::core_app::{CoreApp, MockProvider, WorkerTurnId};
    use oc_core::domain::SessionId;
    use oc_core::queries::{
        AgentEntry, CatalogSnapshot, DcpSnapshot, HistoryMessage, HistoryPage, ModelEntry,
        VariantEntry,
    };
    use oc_core::session::Role;

    fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
        let total = rows.len();
        HistoryPage {
            rows,
            total,
            has_older: false,
            has_newer: false,
        }
    }

    async fn view_state(name: &str) -> TuiState {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        let id = SessionId::new(name).expect("id");
        app.create_session(id.clone()).await.expect("create");
        TuiState::new(app, id)
    }

    async fn open(state: &mut TuiState, command: &str) {
        for c in command.chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        state.handle_key(KeyAction::Enter).await;
    }

    fn catalog() -> CatalogSnapshot {
        CatalogSnapshot {
            provider: "ludka2".to_string(),
            models: vec![ModelEntry {
                id: "a".to_string(),
                variants: vec![VariantEntry {
                    name: "low".to_string(),
                    disabled: false,
                    reasoning_effort: Some("low".to_string()),
                }],
                context: 1000,
                output: 100,
            }],
            model_id: "a".to_string(),
            variant: None,
            agents: vec![AgentEntry {
                id: "x".to_string(),
                description: "first profile".to_string(),
                model: Some("a".to_string()),
                variant: None,
            }],
            agent_id: Some("x".to_string()),
            commands: Vec::new(),
        }
    }

    #[tokio::test]
    async fn render_is_bounded_with_unicode() {
        let mut state = view_state("s-r").await;
        state.attach_page(&page(vec![
            msg(1, Role::User, "привет 🌍"),
            msg(2, Role::Assistant, "ok"),
        ]));
        for c in "next…".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        let lines = render_test(&state, 40, 10);
        assert_eq!(lines.len(), 10);
        let joined = lines.join("\n");
        assert!(joined.contains("привет"), "unicode must render: {joined}");
        assert!(joined.contains("prompt"), "input pane: {joined}");
        // Viewport constant is the product contract for T06.
        assert_eq!(VIEWPORT_LINES, 20);
    }

    #[tokio::test]
    async fn panels_render_bounded() {
        let mut state = view_state("s-p").await;
        state.apply_catalog(catalog());

        open(&mut state, "/model").await;
        assert_eq!(state.panel(), &TuiPanel::Model);
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("model |")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "a"), "{lines:?}");
        let frame = render_test(&state, 60, 24);
        assert!(frame.join("\n").contains("model |"), "panel pane renders");

        open(&mut state, "/agents").await;
        let lines = panel_lines(&state);
        assert!(
            lines.iter().any(|l| l.contains("agents | enter selects")),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "> x — first profile [a]"),
            "{lines:?}"
        );

        state.apply_sessions(vec!["s-p".to_string(), "s-q".to_string()]);
        open(&mut state, "/sessions").await;
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("> s-p")), "{lines:?}");

        open(&mut state, "/skills").await;
        state.apply_skills(vec![oc_core::queries::SkillCard {
            id: "sk".to_string(),
            name: "Skill".to_string(),
            description: "does things".to_string(),
        }]);
        let lines = panel_lines(&state);
        assert!(
            lines.iter().any(|l| l == "sk — Skill: does things"),
            "{lines:?}"
        );
        state.accept_intent();

        open(&mut state, "/help").await;
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("/model")), "{lines:?}");

        state.handle_panel_key(KeyAction::Cancel);
        assert!(panel_lines(&state).is_empty());
    }

    #[tokio::test]
    async fn dcp_panel_renders_snapshot_and_notice() {
        let mut state = view_state("s-d").await;
        state.apply_dcp_snapshot(DcpSnapshot {
            estimated_tokens: 900,
            max_context: 1000,
            turns_since_compress: 3,
            blocks: 2,
            compressions: 1,
            nudges: 4,
            prunes: 0,
        });
        state.dcp.request_compress("draft").expect("request");
        state.begin_compress_turn(WorkerTurnId("t-dcp".to_string()));
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
