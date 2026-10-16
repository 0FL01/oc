use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oc_core::core_app::InboxMsg;
use ratatui::{Terminal, backend::TestBackend};

#[tokio::test]
async fn vis12_shared_history_recall_undo_draft_and_effective_keys_do_not_use_session_rows() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(sid("shared-a")).await.unwrap();
    app.create_session(sid("shared-b")).await.unwrap();
    let mut events = app.subscribe();
    let mut accepted = TuiState::new(app.clone(), sid("shared-a"));
    submit_echo(&mut accepted, &mut events, "FIRST @src/demo.rs Ω界").await;
    let latest = "SECOND first line\nsecond Ω界";
    submit_echo(&mut accepted, &mut events, latest).await;
    assert_eq!(
        app.prompt_history(None).await.unwrap(),
        ["FIRST @src/demo.rs Ω界", latest]
    );

    let draft = "unfinished Ω界";
    for home in [true, false] {
        let mut state = if home {
            TuiState::new_home(app.clone())
        } else {
            TuiState::new(app.clone(), sid("shared-b"))
        };
        assert!(
            state.history().rows().is_empty(),
            "no conversation rows to recall"
        );
        state.restore_prompt(draft.into());
        state.editor.move_to(0, false);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.input(), latest);
        state.handle_key(KeyAction::Undo).await;
        assert_eq!(
            state.input(),
            draft,
            "recall is one undoable editor operation"
        );
        state.restore_prompt(draft.into());
        state.editor.move_to(0, false);
        state.handle_key(KeyAction::Up).await;
        state.handle_key(KeyAction::Up).await;
        assert_eq!(state.input(), "FIRST @src/demo.rs Ω界");
        state.handle_key(KeyAction::Down).await;
        assert_eq!(
            state.input(),
            "FIRST @src/demo.rs Ω界",
            "raw end precedes recall"
        );
        state.handle_key(KeyAction::Down).await;
        assert_eq!(state.input(), latest);
        state.handle_key(KeyAction::Down).await;
        assert_eq!(
            state.input(),
            draft,
            "native returns the pre-browse unfinished draft"
        );
        assert_eq!(state.editor.cursor, draft.len());
        state.editor.move_to(0, false);
        state.handle_key(KeyAction::Up).await;
        state.handle_key(KeyAction::Char('X')).await;
        state.editor.move_to(0, false);
        state.handle_key(KeyAction::Up).await;
        assert_eq!(
            state.input(),
            format!("X{latest}"),
            "edited copies refuse older history"
        );
    }

    let mut state = TuiState::new_home(app.clone());
    let mut catalog = snapshot();
    catalog.chrome.prompt_history_shortcuts.previous = "f2,ctrl+g p".into();
    catalog.chrome.prompt_history_shortcuts.next = "f3".into();
    state.apply_catalog(catalog.clone());
    state.restore_prompt(latest.into());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    // Donor command handlers (not just arrow keys) preserve visual/raw-edge
    // navigation before history, so the same rule applies to effective remaps.
    for expected_history in [false, false, true] {
        let previous = state
            .terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
            .unwrap();
        state.handle_key(previous).await;
        assert_eq!(state.input(), latest);
        assert_eq!(state.editor.browsing_history(), expected_history);
    }
    state.restore_prompt(draft.into());
    state.editor.move_to(0, false);
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let old_up = state
        .terminal_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
        .unwrap();
    state.handle_key(old_up).await;
    assert_eq!(state.input(), draft, "overridden physical Up cannot recall");
    let previous = state
        .terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(previous, KeyAction::PromptHistoryPrevious);
    state.handle_key(previous).await;
    assert_eq!(state.input(), latest);
    state.editor.move_to(state.input.len(), false);
    let next = state
        .terminal_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    state.handle_key(next).await;
    assert_eq!(state.input(), draft);
    state.editor.move_to(0, false);
    let leader = state
        .terminal_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .unwrap();
    state.handle_key(leader).await;
    assert!(state.leader_pending());
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
        Some(KeyAction::PromptHistoryPrevious)
    );
    assert!(!state.leader_pending());
    catalog.chrome.prompt_history_shortcuts.previous.clear();
    catalog.chrome.prompt_history_shortcuts.next.clear();
    state.apply_catalog(catalog);
    assert!(
        state
            .terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
            .is_none()
    );
    state.status = TuiStatus::Streaming;
    state.restore_prompt(draft.into());
    state.editor.move_to(0, false);
    state.chrome.prompt_history_shortcuts.previous = "up".into();
    state.handle_key(KeyAction::Up).await;
    assert_eq!(
        state.input(),
        latest,
        "busy root still owns an editable unsent draft"
    );
    assert!(state.pending.is_none());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn vis12_clear_retention_slash_admission_and_history_failure_preserve_authority() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    let mut state = TuiState::new_home(app.clone());
    state.restore_prompt("界".repeat(19));
    state.handle_key(KeyAction::Interrupt).await;
    assert!(
        app.prompt_history(None).await.unwrap().is_empty(),
        "not UTF-8 byte length"
    );
    let retained = "😀".repeat(10);
    state.restore_prompt(retained.clone());
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(app.prompt_history(None).await.unwrap(), [retained]);
    state.restore_prompt(format!("\u{feff}{}\u{a0}", "a".repeat(19)));
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(
        app.prompt_history(None).await.unwrap().len(),
        1,
        "ECMAScript trim then UTF-16 length"
    );
    state.handle_paste("a\nb\nc");
    let pasted = state.input().to_owned();
    state.handle_key(KeyAction::Interrupt).await;
    assert_eq!(
        app.prompt_history(None).await.unwrap().last(),
        Some(&pasted),
        "pasted parts retain a short cleared draft"
    );
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        app.prompt_history(None).await.unwrap().len(),
        2,
        "empty Enter is not input"
    );
    state.restore_prompt("/mcps".into());
    let opened = state.handle_key(KeyAction::Enter).await;
    assert!(matches!(opened.intent, Some(PanelIntent::LoadMcps)));
    assert_eq!(
        app.prompt_history(None).await.unwrap().last().unwrap(),
        "/mcps"
    );
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(state.panel, TuiPanel::None);
    assert!(state.pending.is_none());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    // Both accepted slash and retained clear fail closed before local effects.
    for slash in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(2);
        let mut state = TuiState::new_home(app);
        let text: String = if slash {
            "/mcps".into()
        } else {
            "retained unfinished input".into()
        };
        state.restore_prompt(text.clone());
        let action = if slash {
            KeyAction::Enter
        } else {
            KeyAction::Interrupt
        };
        let owner = async {
            let Some(InboxMsg::PromptHistory { append, ack }) = inbox.recv().await else {
                panic!("history admission")
            };
            assert_eq!(append, Some(text.clone()));
            ack.send(Err(CoreError::Application(
                "history fixture refusal".into(),
            )))
            .unwrap();
        };
        let (outcome, ()) = tokio::join!(state.handle_key(action), owner);
        assert!(outcome.note.unwrap().contains("history fixture refusal"));
        assert!(outcome.intent.is_none());
        assert_eq!(state.input(), text);
        assert_eq!(state.panel, TuiPanel::None);
        assert!(state.pending.is_none());
        assert!(inbox.try_recv().is_err());
    }
}
