use super::*;
use oc_core::core_app::{InboxMsg, UserShellSelection};
use ratatui::{Terminal, backend::TestBackend};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn vis12_user_shell_mode_captures_receipt_without_model_turn_and_preserves_newer_draft() {
    for home in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = if home {
            TuiState::new_home(app)
        } else {
            TuiState::new(app, sid("user-command"))
        };
        state.restore_prompt("ordinary".into());
        state.handle_key(KeyAction::Char('!')).await;
        assert!(!state.prompt_shell_mode());
        assert_eq!(state.input(), "ordinary!");
        state.handle_key(KeyAction::Home).await;
        state.handle_key(KeyAction::Char('!')).await;
        assert!(state.prompt_shell_mode());
        assert_eq!(state.input(), "ordinary!");
        state.handle_key(KeyAction::Backspace).await;
        assert!(
            !state.prompt_shell_mode(),
            "visual-start Backspace exits without editing"
        );
        assert_eq!(state.input(), "ordinary!");
        state.handle_key(KeyAction::Char('!')).await;
        state.restore_prompt(String::new());
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("Run a command…") && text.contains("Shell"));
        state.handle_key(KeyAction::Interrupt).await;
        assert!(!state.prompt_shell_mode());
        assert_ne!(
            state.status,
            TuiStatus::Quit,
            "empty Shell Ctrl+C only exits mode"
        );

        state.handle_key(KeyAction::Char('!')).await;
        state.restore_prompt("/bin/echo @literal".into());
        assert!(state.slash_options().is_none() && state.mention_request().is_none());
        let command = " printf 'Ω界' ";
        state.restore_prompt(command.into());
        let outcome = state.handle_key(KeyAction::Enter).await;
        assert!(outcome.intent.is_none());
        assert_eq!(
            state.input(),
            command,
            "admission does not clear optimistically"
        );
        assert!(state.pending.is_none() && state.active_turn.is_none());
        let InboxMsg::UserShell {
            session,
            command: captured,
            selection,
            ack,
            ..
        } = inbox.try_recv().unwrap()
        else {
            panic!("structured Shell port");
        };
        assert_eq!(captured, command);
        assert_eq!(state.user_shell_admission_session(), Some(&session));
        assert!(state.has_pending_submission() && state.is_busy());
        assert!(matches!(selection, UserShellSelection::Fresh(_)) == home);
        assert!(
            inbox.try_recv().is_err(),
            "no model or optimistic history request"
        );
        state.handle_key(KeyAction::Char('X')).await;
        ack.send(Ok("user-shell:owned".into())).unwrap();
        state.poll_submission();
        assert_eq!(state.session.as_ref(), Some(&session));
        assert_eq!(state.input(), format!("{command}X"));
        assert!(!state.prompt_shell_mode());
        assert_eq!(state.status, TuiStatus::Idle);
        assert!(!state.has_pending_submission() && !state.is_busy());
        assert!(state.pending.is_none() && state.active_turn.is_none());
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn vis12_user_shell_cancel_refusal_and_overlay_keep_authority_and_editable_command() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    state.panel = TuiPanel::Commands;
    state.handle_key(KeyAction::Char('!')).await;
    assert!(!state.prompt_shell_mode() && state.input().is_empty());
    state.close_panel();
    state.handle_key(KeyAction::Char('!')).await;
    state.restore_prompt("printf no-effect".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::UserShell { cancel, ack, .. } = inbox.try_recv().unwrap() else {
        panic!("structured request");
    };
    assert!(!cancel.load(Ordering::Acquire));
    state.handle_key(KeyAction::Cancel).await;
    assert!(cancel.load(Ordering::Acquire));
    ack.send(Err(CoreError::Application(
        "Shell admission cancelled".into(),
    )))
    .unwrap();
    state.poll_submission();
    assert_eq!(state.input(), "printf no-effect");
    assert!(state.prompt_shell_mode());
    assert!(state.session.is_none() && state.pending.is_none() && state.active_turn.is_none());
    assert_eq!(state.status, TuiStatus::Idle);
    assert!(inbox.try_recv().is_err());
    state.handle_key(KeyAction::Cancel).await;
    assert!(!state.prompt_shell_mode());
    assert_eq!(state.input(), "printf no-effect");
}
