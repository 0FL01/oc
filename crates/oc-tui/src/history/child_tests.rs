use super::*;
use oc_core::{
    domain::SessionId,
    queries::{ChildHistory, ChildJob, ChildState, HistoryTurn},
    session::MessageId,
};

fn job(state: ChildState) -> ChildJob {
    ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "actual-launch".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: "Inspect Ω界".into(),
        delivery_id: "actual-delivery".into(),
        state,
        background: true,
        turn: Some("actual-child-turn".into()),
        result: None,
        message_id: Some("actual-notice".into()),
    }
}
fn message(state: ChildState) -> HistoryMessage {
    HistoryMessage {
        id: MessageId("actual-notice".into()),
        seq: 9,
        role: Role::User,
        text: "RAW technical notice is immutable data, not its UI label".into(),
        turn: None,
        model_switch: None,
        user_shell: None,
        child: Some(ChildHistory::Notice(Box::new(job(state)))),
        shell_notice: None,
    }
}
fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
    HistoryPage {
        total: rows.len(),
        rows,
        ..Default::default()
    }
}

#[test]
fn child_notice_and_task_are_typed_bounded_projection_with_matching_indexed_cells() {
    let theme = crate::theme::Theme::dark();
    for (state, label, color) in [
        (ChildState::Completed, "↳ Helper finished", theme.info()),
        (ChildState::Error, "! Helper failed", theme.error()),
        (ChildState::Cancelled, "! Helper cancelled", theme.warning()),
        (ChildState::Unknown, "! Helper unknown", theme.info()),
    ] {
        let mut source = message(state);
        if let Some(ChildHistory::Notice(job)) = &mut source.child {
            job.description = "untrusted\n\x1b[31mΩ界".repeat(20);
        }
        let saved = source.clone();
        let rows = rows_from_page(&source);
        assert_eq!(source, saved);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id.as_deref(), Some(&source.id));
        assert!(rows[0].tool.is_none() && rows[0].meta.is_none() && rows[0].text.is_empty());
        for width in [43, 80, 120, 160] {
            let painted = crate::messages::transcript(&rows, theme, width, width, |_| theme.text());
            assert_eq!(painted.len(), 1, "notice never wraps");
            assert!(painted[0].plain_text().contains(label));
            assert!(!painted[0].plain_text().chars().any(char::is_control));
            assert_eq!(painted[0].spans()[1].style().fg, Some(color));
            assert_eq!(painted[0].spans()[2].style().fg, Some(theme.text_muted()));
            let hovered = crate::messages::hover_child_notice(
                &painted[0],
                theme,
                rows[0].child_notice.as_deref().unwrap(),
            );
            assert_eq!(hovered.plain_text(), painted[0].plain_text());
            assert_eq!(
                hovered.spans()[1].style().fg,
                Some(
                    if matches!(state, ChildState::Error | ChildState::Cancelled) {
                        color
                    } else {
                        theme.text()
                    }
                )
            );
            assert_eq!(hovered.spans()[2].style().fg, Some(theme.text_muted()));
            let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
            let (visible, total, targets) = crate::messages::visible_transcript_user_targets(
                &rows,
                theme,
                (width, width),
                (24, 0, None),
                |_| theme.text(),
                &cache,
                &|_| false,
            );
            assert_eq!(total, 2);
            assert!(
                targets.iter().all(Option::is_none),
                "not Revert/Fork user data"
            );
            let paint = |lines| {
                use ratatui::widgets::Widget;
                let rect = ratatui::layout::Rect::new(0, 0, width, 24);
                let mut buffer = ratatui::buffer::Buffer::empty(rect);
                ratatui::widgets::Paragraph::new(crate::styled::Lines::from(lines).into_text())
                    .render(rect, &mut buffer);
                buffer
            };
            let mut expected = vec![crate::styled::Line::plain("")];
            expected.extend(painted);
            assert_eq!(paint(visible), paint(expected));
        }
        source.child = None;
        assert_eq!(
            rows_from_page(&source)[0].role,
            "user",
            "JSON-looking prose is never classified"
        );
    }
    let mut task = message(ChildState::Completed);
    task.id = MessageId("actual-task".into());
    task.seq = 8;
    task.child = Some(ChildHistory::Task {
        text: "original task [parent_context_pack] Ω界".into(),
        limited: true,
    });
    let rows = rows_from_page(&task);
    assert_eq!(
        rows[0].text,
        "original task [parent_context_pack] Ω界\n[Task preview]"
    );
    assert_eq!(rows[0].role, "user");
    assert!(rows[0].child_notice.is_none());

    let mut window = HistoryWindow::default();
    window.reset(&page(vec![task.clone(), message(ChildState::Completed)]));
    let before = window.rows().to_vec();
    assert!(!window.refresh_owner_notices(&page(vec![task, message(ChildState::Completed)])));
    assert_eq!(window.rows(), before);
    let mut disjoint = message(ChildState::Cancelled);
    disjoint.id = MessageId("later".into());
    disjoint.seq = 30;
    assert!(window.refresh_owner_notices(&page(vec![disjoint])));
    assert!(window.has_newer());
    assert_eq!(
        window.rows(),
        before,
        "disjoint exact notice cannot jump unseen history"
    );
}

#[test]
fn model_shell_notice_is_plain_typed_data_without_a_child_link_or_user_action() {
    let theme = crate::theme::Theme::dark();
    for (state, label, color) in [
        ("completed", "↳ Shell finished", theme.info()),
        ("failed", "! Shell failed", theme.error()),
        ("cancelled", "! Shell cancelled", theme.warning()),
        ("timed_out", "! Shell timed out", theme.warning()),
        ("unknown", "! Shell unknown", theme.info()),
    ] {
        let mut source = message(ChildState::Completed);
        source.child = None;
        source.shell_notice = Some(oc_core::queries::ShellHistoryNotice {
            operation: "model-op".into(),
            state: state.into(),
            command: "printf\n  Ω界\t\x1b[31m literal cancelled".into(),
        });
        let saved = source.clone();
        let rows = rows_from_page(&source);
        assert_eq!(source, saved);
        assert_eq!(rows[0].role, "shell_notice");
        assert!(
            rows[0].child_notice.is_none() && rows[0].tool.is_none() && rows[0].text.is_empty()
        );
        for width in [43, 80, 120, 160] {
            let painted = crate::messages::transcript(&rows, theme, width, width, |_| theme.text());
            assert_eq!(painted.len(), 1);
            assert!(painted[0].plain_text().contains(label));
            assert!(!painted[0].plain_text().chars().any(char::is_control));
            assert_eq!(painted[0].spans()[1].style().fg, Some(color));
            assert_eq!(painted[0].spans()[2].style().fg, Some(theme.text_muted()));
            let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
            let (indexed, total, targets) = crate::messages::visible_transcript_user_targets(
                &rows,
                theme,
                (width, width),
                (24, 0, None),
                |_| theme.text(),
                &cache,
                &|_| false,
            );
            assert_eq!(total, 2);
            assert!(targets.iter().all(Option::is_none));
            assert_eq!(
                indexed.last().unwrap().plain_text(),
                painted[0].plain_text()
            );
        }
        let mut window = HistoryWindow::default();
        window.reset(&page(vec![source.clone()]));
        assert!(window.retained_bytes() >= source.shell_notice.as_ref().unwrap().command.len());
        assert!(!window.refresh_owner_notices(&page(vec![source.clone()])));
        source.shell_notice = None;
        assert_eq!(
            rows_from_page(&source)[0].role,
            "user",
            "RAW wording is not classification"
        );
    }
}

#[tokio::test]
async fn child_notice_pointer_uses_current_paint_and_captured_owner_not_prose_or_bare_release() {
    use crate::{
        app::{PanelIntent, TuiState},
        events::KeyAction,
    };
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    for (width, height) in [(80, 24), (120, 40), (160, 48)] {
        let (app, mut inbox, _) = oc_core::core_app::CoreApp::channel(8);
        let mut state = TuiState::new(app, SessionId("parent".into()));
        state.attach_page(&page(vec![message(ChildState::Completed)]));
        state.restore_prompt("unfinished root draft".into());
        let frame = Rect::new(0, 0, width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let buffer = terminal.backend().buffer();
        let y = (0..height)
            .find(|&y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .contains("Helper finished")
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
            matches!(outcome.intent, Some(PanelIntent::OpenChild { selected }) if selected == job(ChildState::Completed))
        );
        assert_eq!(state.input(), "unfinished root draft");
        assert!(matches!(state.panel(), crate::app::TuiPanel::None));
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
        state.handle_key(KeyAction::Left).await;
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
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left)), frame);
        state.attach_page(&page(Vec::new()));
        assert!(
            state
                .handle_mouse(event(MouseEventKind::Up(MouseButton::Left)), frame)
                .intent
                .is_none(),
            "a removed notice cannot activate the previously painted owner"
        );
        assert_eq!(state.input(), "unfinished root draft");
        assert!(
            inbox.try_recv().is_err(),
            "UI only emits captured navigation; no executor or new query"
        );
    }
}

#[test]
fn child_notice_refresh_correlates_only_current_prompt_without_executing_parts() {
    let mut window = HistoryWindow::default();
    window.push_synthetic("user", "accepted prompt", None, None);
    let mut accepted = message(ChildState::Completed);
    accepted.id = MessageId("accepted".into());
    accepted.seq = 8;
    accepted.child = None;
    accepted.turn = Some(HistoryTurn {
        id: "current".into(),
        status: "executing".into(),
        ..Default::default()
    });
    let current = page(vec![accepted, message(ChildState::Completed)]);
    window.correlate_live_prompt(&current, "other");
    assert!(window.rows()[0].message_id.is_none());
    window.correlate_live_prompt(&current, "current");
    assert!(window.refresh_owner_notices(&current));
    assert_eq!(window.rows().len(), 2);
    assert_eq!(window.rows()[0].text, "accepted prompt");
    assert_eq!(window.rows()[0].seq, 8);
    assert_eq!(window.rows()[1].role, "child_notice");
}
