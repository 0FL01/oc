use super::*;
use crate::app::{PanelIntent, TuiState};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use oc_core::core_app::{CoreApp, WorkerTurnId};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn job() -> ChildJob {
    ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "launch".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/inherited".into(),
        description: "inspect configs".into(),
        delivery_id: "delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    }
}

#[tokio::test]
async fn captured_subagent_live_background_and_terminal_keep_one_graph_and_original_family() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("parent".into()));
    state.restore_prompt("original root draft".into());
    let turn = WorkerTurnId("parent-turn".into());
    state.begin_linked_turn(turn.clone());
    state.apply_delta(&turn, "one text part");
    let mut captured = job();
    state.apply_child_jobs(vec![captured.clone()]);
    assert_eq!(
        state.transcript_rows().len(),
        1,
        "inventory never creates a tool part"
    );
    let input = r#"{"agent":"helper","description":"inspect configs","prompt":"real task"}"#;
    state.apply_tool_started(&turn, "launch", "subagent", input);
    let rows = state.transcript_rows();
    let card = rows[1].tool.as_ref().unwrap();
    assert_eq!(card.child_job.as_deref(), Some(&captured));
    assert_eq!(card.state, "started");
    let crate::tools::ToolRender::Subagent(render) = &card.render else {
        panic!("subagent")
    };
    assert!(
        render.live_running && render.model_label.is_none(),
        "inherited model is not an explicit caption"
    );
    let lines = crate::tools::tool_block(card, crate::theme::Theme::dark(), 120);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].plain_text(),
        "   ⋯ Helper Subagent — inspect configs"
    );
    assert!(!lines[0].plain_text().contains("inherited"));

    for field in [
        "parent",
        "operation",
        "child",
        "generation",
        "location",
        "delivery",
    ] {
        let mut foreign = captured.clone();
        match field {
            "parent" => foreign.parent = SessionId("foreign".into()),
            "operation" => foreign.operation = "foreign".into(),
            "child" => foreign.child = SessionId("foreign".into()),
            "generation" => foreign.generation += 1,
            "location" => foreign.location = "/moved".into(),
            _ => foreign.delivery_id = "foreign".into(),
        }
        state.apply_child_jobs(vec![foreign]);
        assert_eq!(
            state.transcript_rows()[1]
                .tool
                .as_ref()
                .unwrap()
                .child_job
                .as_deref(),
            Some(&captured)
        );
    }
    captured.background = true;
    state.apply_child_jobs(vec![captured.clone()]);
    let raw = "<subagent sessionID=\"foreign\" state=\"failed\">result is data</subagent>";
    state.apply_tool_finished(
        &turn,
        "launch",
        "subagent",
        "running",
        raw,
        raw.len() as i64,
        false,
    );
    let rows = state.transcript_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].text, "one text part");
    let card = rows[1].tool.as_ref().unwrap();
    assert_eq!(card.op, "launch");
    assert_eq!(
        card.state, "running",
        "the real native returned-background state is not rewritten"
    );
    assert_eq!(card.output_preview, raw);
    assert_eq!(card.child_job.as_deref(), Some(&captured));
    let caption = crate::tools::tool_block(card, crate::theme::Theme::dark(), 120);
    assert_eq!(caption.len(), 1);
    assert!(
        caption[0]
            .plain_text()
            .contains("⋯ Helper Subagent — inspect configs  Background ")
    );
    let badge = caption[0].spans().last().unwrap();
    assert_eq!(badge.content(), " Background ");
    assert_eq!(
        badge.style().fg,
        Some(crate::theme::Theme::dark().text_muted())
    );
    assert_eq!(
        badge.style().bg,
        Some(crate::theme::Theme::dark().decrease(crate::theme::Theme::dark().background()))
    );

    captured.state = ChildState::Completed;
    state.apply_child_jobs(vec![captured.clone()]);
    let card = state.transcript_rows()[1].tool.clone().unwrap();
    assert!(
        crate::tools::tool_block(&card, crate::theme::Theme::dark(), 120)[0]
            .plain_text()
            .contains("✓ Helper Subagent — inspect configs  Background ")
    );
    let mut stale = captured.clone();
    stale.state = ChildState::Running;
    state.apply_child_jobs(vec![stale]);
    assert_eq!(
        state.transcript_rows()[1]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_deref(),
        Some(&captured),
        "terminal generation cannot revive"
    );
    let mut continued = captured.clone();
    continued.operation = "continuation".into();
    continued.generation += 1;
    continued.delivery_id = "continuation-delivery".into();
    continued.location = "/new-location".into();
    continued.state = ChildState::Running;
    continued.background = false;
    state.apply_child_jobs(vec![captured.clone(), continued.clone()]);
    let card = state.transcript_rows()[1].tool.clone().unwrap();
    assert_eq!(
        card.child_job.as_deref(),
        Some(&captured),
        "a continuation does not retarget the old link"
    );
    assert!(
        matches!(&card.render,crate::tools::ToolRender::Subagent(render) if render.live_running)
    );
    assert!(
        crate::tools::tool_block(&card, crate::theme::Theme::dark(), 120)[0]
            .plain_text()
            .contains("⋯ Helper Subagent")
    );
    continued.child = SessionId("unrelated-child".into());
    state.apply_child_jobs(vec![captured.clone(), continued]);
    assert!(
        matches!(&state.transcript_rows()[1].tool.as_ref().unwrap().render,crate::tools::ToolRender::Subagent(render) if !render.live_running)
    );
    state.apply_finished(&turn, "done", 1);
    assert_eq!(
        state
            .history()
            .rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        1
    );
    assert_eq!(state.input(), "original root draft");
    assert!(
        inbox.try_recv().is_err(),
        "passive projection cannot execute or query"
    );
}

#[tokio::test]
async fn subagent_card_pointer_uses_painted_generation_and_failed_card_expands_instead_of_navigating()
 {
    for (width, height) in [(80, 24), (120, 40), (160, 48)] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app, SessionId("parent".into()));
        let turn = WorkerTurnId("parent-turn".into());
        state.begin_linked_turn(turn.clone());
        let mut captured = job();
        captured.background = true;
        state.apply_child_jobs(vec![captured.clone()]);
        state.apply_tool_started(
            &turn,
            "launch",
            "subagent",
            &serde_json::json!({"agent":"helper","description":format!("inspect configs {}","WRAPPED-CAPTION ".repeat(9))}).to_string(),
        );
        state.apply_tool_finished(
            &turn,
            "launch",
            "subagent",
            "completed",
            "raw result",
            10,
            false,
        );
        state.restore_prompt("unchanged draft".into());
        let frame = Rect::new(0, 0, width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let y = (0..height)
            .find(|&y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains("Helper Subagent")
            })
            .unwrap();
        let x = (0..width)
            .find(|&x| buffer[(x, y)].symbol() == "H")
            .unwrap();
        let event = |kind| MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        assert!(
            state
                .handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), frame)
                .intent
                .is_none()
        );
        state.handle_mouse(event(MouseEventKind::Moved), frame);
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(x, y)].fg,
            crate::theme::Theme::dark().text()
        );
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), frame);
        let outcome = state.handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), frame);
        assert!(
            matches!(outcome.intent,Some(PanelIntent::OpenChild {selected}) if selected==captured)
        );
        if width == 80 {
            terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
            let buffer = terminal.backend().buffer();
            let wrapped_y = (y + 1..height)
                .find(|&row| {
                    (0..width)
                        .map(|column| buffer[(column, row)].symbol())
                        .collect::<String>()
                        .contains("WRAPPED-CAPTION")
                })
                .unwrap();
            let wrapped_x = (0..width)
                .find(|&column| !buffer[(column, wrapped_y)].symbol().trim().is_empty())
                .unwrap();
            // Every continuation stays within the label column, not at x=0.
            assert_eq!(wrapped_x, x);
            let wrapped = |kind| MouseEvent {
                kind,
                column: wrapped_x,
                row: wrapped_y,
                modifiers: KeyModifiers::NONE,
            };
            state.handle_mouse(wrapped(MouseEventKind::Down(MouseButton::Left)), frame);
            assert!(
                matches!(state.handle_mouse(wrapped(MouseEventKind::Up(MouseButton::Left)),frame).intent,Some(PanelIntent::OpenChild{selected}) if selected==captured)
            );
        }
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), frame);
        state.handle_mouse(event(MouseEventKind::Drag(MouseButton::Left)), frame);
        assert!(
            state
                .handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), frame)
                .intent
                .is_none()
        );
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), frame);
        assert!(
            state
                .handle_mouse(
                    event(MouseEventKind::Up(MouseButton::Left)),
                    Rect::new(0, 0, width + 1, height)
                )
                .intent
                .is_none()
        );
        assert_eq!(state.input(), "unchanged draft");
        assert!(inbox.try_recv().is_err());
    }
    for scenario in ["resize", "replacement", "valid"] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app, SessionId("parent".into()));
        let turn = WorkerTurnId("parent-turn".into());
        state.begin_linked_turn(turn.clone());
        let mut captured = job();
        captured.state = ChildState::Error;
        state.apply_child_jobs(vec![captured]);
        let input = r#"{"agent":"helper","description":"failed captured child"}"#;
        state.apply_tool_started(&turn, "launch", "subagent", input);
        state.apply_tool_finished(
            &turn,
            "launch",
            "subagent",
            "failed",
            "ACTUAL-FAILURE-DETAIL",
            21,
            false,
        );
        state.restore_prompt("unchanged draft".into());
        let frame = Rect::new(0, 0, 80, 24);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let y = (0..24)
            .find(|&y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains("Helper Subagent")
            })
            .unwrap();
        let x = (0..80).find(|&x| buffer[(x, y)].symbol() == "H").unwrap();
        assert!(
            !state
                .viewport()
                .join("\n")
                .contains("ACTUAL-FAILURE-DETAIL")
        );
        let event = |kind| MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), frame);
        if scenario == "replacement" {
            state.apply_tool_started(
                &turn,
                "launch",
                "subagent",
                r#"{"agent":"helper","description":"replacement header"}"#,
            );
            state.apply_tool_finished(
                &turn,
                "launch",
                "subagent",
                "failed",
                "ACTUAL-FAILURE-DETAIL",
                21,
                false,
            );
        }
        let released = if scenario == "resize" {
            Rect::new(0, 0, 81, 24)
        } else {
            frame
        };
        assert!(
            state
                .handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), released)
                .intent
                .is_none(),
            "error detail is never navigation"
        );
        assert_eq!(
            state
                .viewport()
                .join("\n")
                .contains("ACTUAL-FAILURE-DETAIL"),
            scenario == "valid",
            "{scenario} paint fence"
        );
        assert_eq!(state.input(), "unchanged draft");
        assert!(inbox.try_recv().is_err());
    }
}

#[test]
fn explicit_subagent_model_caption_and_continuation_are_not_current_selection_or_output_prose() {
    let picker = crate::picker::ModelPicker::new(oc_adapters::models::ModelCatalog {
        provider: "fixture".into(),
        models: std::collections::BTreeMap::from([(
            "m".into(),
            serde_json::json!({"name":"Human Model"}),
        )]),
    });
    assert_eq!(
        crate::tools::subagent_model_label("fixture/m#deep", Some(&picker)).as_deref(),
        Some("Human Model (deep)")
    );
    assert_eq!(
        crate::tools::subagent_model_label("other/m#deep", Some(&picker)).as_deref(),
        Some("other/m (deep)")
    );
    assert_eq!(
        crate::tools::subagent_model_label("fixture/missing", Some(&picker)).as_deref(),
        Some("fixture/missing")
    );
    assert_eq!(crate::tools::subagent_model_label("", Some(&picker)), None);
    let render = crate::tools::ToolRender::parse(
        "subagent",
        Some(
            r#"{"sessionID":"child","subagent_type":"explore","description":"continue","model":"fixture/m#deep"}"#,
        ),
        Some("foreign running cancelled"),
        "completed",
    );
    let crate::tools::ToolRender::Subagent(render) = render else {
        panic!("subagent")
    };
    assert!(render.continuation);
    assert_eq!(render.agent, "explore");
    assert!(
        render.child_session.is_none() && render.child_state.is_none() && render.result.is_empty()
    );
}
