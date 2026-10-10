use super::*;
use crate::app::ForegroundWork;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use oc_core::queries::{ChildJob, ChildState, ShellJob};

fn child() -> ChildJob {
    ChildJob {
        parent: sid("foreground"),
        child: sid("child"),
        operation: "child-op".into(),
        generation: 17,
        location: "/captured/original".into(),
        agent: "helper".into(),
        model: "provider/original".into(),
        description: "held child".into(),
        delivery_id: "captured-delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    }
}
fn shell(turn: &WorkerTurnId) -> ShellJob {
    ShellJob {
        session: sid("foreground"),
        shell_id: "shell-op".into(),
        location: "/captured/shell".into(),
        generation: 23,
        turn: turn.0.clone(),
        model: "captured-model".into(),
        provider: "captured-provider".into(),
        command: "held command".into(),
        pid: Some(1234),
        background: false,
        output: None,
    }
}
fn key(code: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL)
}

#[tokio::test]
async fn foreground_hint_and_root_handoff_are_delayed_bounded_and_exact_current_work() {
    let (app, mut owner, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, sid("foreground"));
    state.chrome.animations = Some(false);
    let turn = WorkerTurnId("parent-turn".into());
    state.begin_linked_turn(turn.clone());
    state.apply_child_jobs(vec![child()]);
    state.apply_shell_jobs(vec![shell(&turn)]);
    assert!(
        !state.foreground_available(),
        "inventory alone cannot create an executing part"
    );
    state.apply_tool_started(
        &turn,
        "child-op",
        "subagent",
        r#"{"agent":"helper","description":"held child"}"#,
    );
    state.apply_tool_started(
        &turn,
        "shell-op",
        "shell",
        r#"{"command":"immutable command"}"#,
    );
    state.restore_prompt("unsent draft".into());
    let now = Instant::now();
    for part in &mut state.live_parts {
        if let LivePart::Tool { card, .. } = part {
            card.started_at = Some(now);
        }
    }
    state.sync_foreground_hint(now);
    assert_eq!(
        state.foreground_hint_deadline(),
        Some(now + Duration::from_secs(3))
    );
    assert_eq!(state.background_hint_key(), None);
    assert!(!state.sync_foreground_hint(now + Duration::from_millis(2999)));
    assert!(state.tick_ui(now + Duration::from_secs(3)));
    assert_eq!(state.background_hint_key(), Some("ctrl+b"));
    assert_eq!(
        state.foreground_hint_deadline(),
        None,
        "one delay, not a permanent timer"
    );
    let parts = state.live_parts.clone();
    for width in [43, 80, 120, 160] {
        let text = state
            .rendered_transcript(width, width)
            .iter()
            .map(|line| line.plain_text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Press ctrl+b"));
        assert!(
            !state
                .history()
                .rows()
                .iter()
                .any(|row| row.role == "background_hint")
        );
        assert_eq!(
            state.live_parts, parts,
            "hint is disposable presentation, not tool/provider graph"
        );
    }
    let action = state.terminal_key(key('b')).unwrap();
    assert_eq!(action, KeyAction::BackgroundSession);
    let outcome = state.handle_key(action).await;
    let Some(PanelIntent::BackgroundSession {
        session,
        turn: captured_turn,
        work,
    }) = outcome.intent
    else {
        panic!("missing handoff")
    };
    assert_eq!(session, sid("foreground"));
    assert_eq!(captured_turn, turn);
    assert_eq!(
        work,
        vec![
            ForegroundWork::Child(Box::new(child())),
            ForegroundWork::Shell(Box::new(shell(&turn)))
        ]
    );
    assert_eq!(state.input(), "unsent draft");
    assert_eq!(state.live_parts, parts);
    assert!(
        owner.try_recv().is_err(),
        "only the binary dispatches captured effects"
    );

    state.chrome.background_shortcut = Some("ctrl+y".into());
    assert_eq!(state.terminal_key(key('b')), Some(KeyAction::Left));
    assert_eq!(
        state.terminal_key(key('y')),
        Some(KeyAction::BackgroundSession)
    );
    state.chrome.background_shortcut = Some("ctrl+x b".into());
    let leader = state.terminal_key(key('x')).unwrap();
    state.handle_key(leader).await;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)),
        Some(KeyAction::BackgroundSession)
    );
    let mut repeat = key('b');
    repeat.kind = KeyEventKind::Repeat;
    assert_eq!(state.terminal_key(repeat), None);
    state.chrome.background_shortcut = Some("none".into());
    state.sync_foreground_hint(now + Duration::from_secs(4));
    assert_eq!(state.background_hint_key(), None);
    assert_eq!(state.foreground_hint_deadline(), None);
    assert_eq!(state.terminal_key(key('b')), Some(KeyAction::Left));

    state.apply_tool_finished(
        &turn,
        "child-op",
        "subagent",
        "running",
        "published background handle",
        27,
        false,
    );
    state.apply_child_jobs(vec![child()]);
    let Some(PanelIntent::BackgroundSession { work, .. }) = state.background_session().intent
    else {
        panic!("remaining foreground shell")
    };
    assert_eq!(
        work,
        vec![ForegroundWork::Shell(Box::new(shell(&turn)))],
        "published native child handle cannot regain a blocker from stale foreground metadata"
    );

    // Source and turn filters reject same-operation foreign/direct-user records.
    let mut foreign = shell(&turn);
    foreign.turn = "other-turn".into();
    let mut converted = child();
    converted.background = true;
    state.apply_shell_jobs(vec![foreign]);
    state.apply_child_jobs(vec![converted]);
    assert!(!state.foreground_available());
    let mut direct = shell(&turn);
    direct.turn.clear();
    state.apply_shell_jobs(vec![direct]);
    assert!(!state.foreground_available());
    state.active_turn = None;
    state.apply_shell_jobs(vec![shell(&turn)]);
    state.apply_child_jobs(vec![child()]);
    assert!(
        !state.foreground_available(),
        "retained parts without an active request grant no controls"
    );
    assert_eq!(state.background_session().intent, None);
    assert_eq!(state.foreground_hint_deadline(), None);
    assert_eq!(state.input(), "unsent draft");
    assert!(owner.try_recv().is_err());
}
