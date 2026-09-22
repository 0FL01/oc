//! Bounded view-model producers and the frame entry point.
//!
//! [`render_frame`] delegates to the upstream v2.0.12 shell
//! ([`crate::shell`]): tab strip, session area with the sticky-bottom
//! transcript, status row, prompt box, devtools bar and the toast overlay.
//! This module keeps the bounded `Vec<String>` panel producers
//! ([`panel_lines`], at most [`crate::app::VIEWPORT_LINES`] history lines)
//! and the `TestBackend` helper used by the unit and golden tests.
//!
//! PTY paste/resize and terminal restoration are qualified in T26; here we
//! assert viewport bounds, Unicode width and state transitions on a
//! `TestBackend`.

use ratatui::{Frame, Terminal, backend::TestBackend};

use crate::app::{TuiPanel, TuiState};

/// Render the whole state into one Ratatui frame.
pub fn render_frame(frame: &mut Frame<'_>, state: &TuiState) {
    crate::shell::render(frame, state);
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
                out.extend(picker.window().into_iter().take(ROWS).map(|id|picker.display_label(&id)));
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
                "/model /agents /sessions /skills /cards /location <path> /dcp-compress /help /quit".to_string(),
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
    use crate::theme::Theme;
    use oc_core::core_app::{CoreApp, MockProvider, WorkerTurnId};
    use oc_core::domain::SessionId;
    use oc_core::queries::{
        AgentEntry, CatalogSnapshot, DcpSnapshot, HistoryMessage, HistoryPage, ModelEntry,
        VariantEntry,
    };
    use oc_core::session::Role;

    fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            turn: None,
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
        let total = rows.len();
        HistoryPage {
            parent_id: None,
            title: None,
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
            chrome: oc_core::queries::TuiChrome {
                devtools: Some(true),
                ..Default::default()
            },
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            provider: "ludka2".to_string(),
            models: vec![ModelEntry {
                display_name: String::new(),
                provider_name: String::new(),
                price: None,
                id: "a".to_string(),
                variants: vec![VariantEntry {
                    name: "low".to_string(),
                    disabled: false,
                    reasoning_effort: Some("low".to_string()),
                }],
                context: 1000,
                context_known: true,
                output_known: true,
                output: 100,
            }],
            model_id: "a".to_string(),
            variant: None,
            agents: vec![AgentEntry {
                color_index: 0,
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
        // Iteration 2 (upstream shell): the transcript has no border or title
        // (`routes/session/index.tsx:1273-1300`); the prompt box renders the
        // upstream `┃`/`╹`/`▀` shape (`component/prompt/index.tsx:1650-1870`).
        let narrow = render_test(&state, 40, 10);
        assert_eq!(narrow.len(), 10);
        let joined = narrow.join("\n");
        assert!(joined.contains("next…"), "input renders: {joined}");
        assert!(joined.contains('┃'), "prompt left border: {joined}");
        assert!(joined.contains('╹'), "prompt underline: {joined}");
        assert!(joined.contains('▀'), "prompt underline: {joined}");

        let tall = render_test(&state, 40, 24).join("\n");
        assert!(tall.contains("привет"), "unicode must render: {tall}");
        assert!(tall.contains("🌍"), "wide unicode must render: {tall}");
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
        assert!(lines.iter().any(|l| l == "a · ludka2"), "{lines:?}");
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
        let turn = WorkerTurnId("t-dcp".to_string());
        state.begin_compress_turn(turn.clone());
        let lines = panel_lines(&state);
        assert!(lines.iter().any(|l| l.contains("900/1000")), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("pending focus: draft")),
            "{lines:?}"
        );

        // Iteration 2: the DCP notice renders in the prompt footer status slot
        // once the turn is idle (the running slot shows `esc interrupt`);
        // completion is what the binary reports the outcome after.
        state.apply_finished(&turn, "", 0);
        state.notify_dcp(DcpOutcome::Failed {
            reason: "span open".to_string(),
        });
        let frame = render_test(&state, 70, 24).join("\n");
        assert!(frame.contains("dcp failed: span open"), "{frame}");
    }

    #[tokio::test]
    async fn status_text_uses_theme_colors() {
        // Iteration 2 (upstream shell): the old `oc <status> — note`
        // history-pane title is gone; upstream has no transcript border or
        // title (`routes/session/index.tsx:1273-1300`). The running state now
        // renders the upstream `esc interrupt` prompt-footer hint
        // (`component/prompt/index.tsx:139-142`) and transient notes render as
        // the upstream toast (`ui/toast.tsx:48-90`). This test keeps the
        // colour guarantees on the frame buffer of the new surfaces.
        use ratatui::{Terminal, backend::TestBackend};

        let theme = Theme::dark();
        let mut state = view_state("s-title").await;
        let turn = WorkerTurnId("t-title".to_string());
        state.begin_compress_turn(turn.clone());
        state.push_note("something happened");

        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| super::render_frame(frame, &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        // Toast: warning side border, raised-high surface, text on the first
        // content row (70 wide: maxWidth = min(60, 64) = 60, right = 2).
        assert_eq!(buffer[(8, 1)].fg, theme.warning());
        assert_eq!(buffer[(11, 2)].bg, theme.background_raised_high());
        assert_eq!(buffer[(11, 2)].fg, theme.text());
        // Prompt footer: `esc` in base text, `interrupt` muted.
        assert_eq!(buffer[(2, 22)].fg, theme.text());
        assert_eq!(buffer[(6, 22)].fg, theme.text_muted());

        // A paused/streaming scroll shows the upstream jump affordance in the
        // status row (`routes/session/index.tsx:1344-1348`).
        state.attach_page(&page(
            (0..VIEWPORT_LINES + 5)
                .map(|i| msg(i as i64, Role::User, "line"))
                .collect(),
        ));
        for _ in 0..4 {
            state.handle_key(KeyAction::Up).await;
        }
        let backend = TestBackend::new(70, 24);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| super::render_frame(frame, &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        // Content is 2..67 wide at 70 columns; the affordance is right-aligned.
        assert_eq!(buffer[(67, 16)].fg, theme.action_secondary());
    }

    #[tokio::test]
    async fn rendered_frame_carries_theme_styles() {
        // Iteration 2: border/title styles moved from the bordered panes to
        // the upstream shell surfaces; the former pane-title cells now assert
        // the tab surface, prompt glyphs and devtools bar instead. The
        // coordinates come from the layout module, so allocation changes
        // cannot silently invalidate the assertions.
        use crate::layout;
        use ratatui::{Terminal, backend::TestBackend};

        let theme = Theme::dark();
        let state = view_state("s-styles").await;
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| super::render_frame(frame, &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let shell = layout::configured_shell_regions(buffer.area, false, 0);
        let regions = layout::session_regions(shell.session, 0);
        // Tab strip surface: decrease(background.raised.base).
        assert_eq!(buffer[(0, 0)].bg, theme.decrease(theme.background_panel()));
        // Prompt left border on the first prompt body row.
        assert_eq!(
            buffer[(regions.prompt.x, regions.prompt.y)].fg,
            theme.border()
        );
        // Prompt underline `▀` carries decrease(background.raised.base).
        assert_eq!(
            buffer[(regions.underline.x + 1, regions.underline.y)].fg,
            theme.decrease(theme.background_panel())
        );
        // Production mode does not allocate a debug row.
        assert_eq!(shell.devtools.height, 0);
    }
}
