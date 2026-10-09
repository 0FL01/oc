use super::*;
use oc_core::core_app::{InboxMsg, UserShellSelection};
use ratatui::{Terminal, backend::TestBackend};
use std::sync::atomic::Ordering;
use unicode_width::UnicodeWidthStr;

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
        assert!(text.contains("Shell"));
        let example = state.prompt_shell_example();
        if home {
            let example = example.expect("Home Shell example");
            let index = ["ls -la", "git status", "pwd"]
                .iter()
                .position(|candidate| *candidate == example)
                .unwrap();
            assert_eq!(
                state.home_example, HOME_EXAMPLES[index],
                "mode changes share the existing placeholder selection"
            );
            assert!(text.contains(&format!("Run a command… \"{example}\"")));
        } else {
            assert!(example.is_none());
            assert!(!text.contains("Run a command…"));
        }
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert_eq!(
            state.prompt_shell_example(),
            example,
            "redraw is not reselection"
        );
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
        assert_eq!(state.window.rows().last().unwrap().role, "shell_input");
        assert!(state.window.rows().last().unwrap().message_id.is_none());
        assert_eq!(
            state
                .window
                .rows()
                .last()
                .unwrap()
                .tool
                .as_ref()
                .unwrap()
                .op,
            "user-shell:owned"
        );
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn shell_mode_footer_replaces_normal_hints_and_has_no_hidden_live_status_action() {
    use oc_core::queries::ShellJob;
    for home in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = if home {
            TuiState::new_home(app)
        } else {
            TuiState::new(app, sid("existing-shell"))
        };
        state.chrome.location = Some("/fixture".into());
        state.apply_shell_jobs(vec![ShellJob {
            session: sid("existing-shell"),
            shell_id: "real-existing-operation".into(),
            location: "/fixture".into(),
            generation: 1,
            turn: String::new(),
            provider: "fixture".into(),
            model: "fixture/model".into(),
            command: "existing command".into(),
            pid: Some(123),
            background: true,
        }]);
        state.handle_key(KeyAction::Char('!')).await;
        let example = state.prompt_shell_example();
        for width in [43, 44, 80, 120] {
            let height = if width == 120 { 40 } else { 24 };
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for command in ["", "printf literal-output"] {
                state.restore_prompt(command.into());
                terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
                let buffer = terminal.backend().buffer();
                let rows: Vec<String> = (0..height)
                    .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                    .collect();
                let suffix = if width < 44 {
                    "esc shell"
                } else {
                    "esc exit shell mode"
                };
                let (y, row) = rows
                    .iter()
                    .enumerate()
                    .find(|(_, row)| row.contains(suffix))
                    .unwrap();
                let start = row.find(suffix).unwrap() as u16;
                let theme = crate::theme::Theme::dark();
                assert_eq!(buffer[(start, y as u16)].fg, theme.text());
                assert_eq!(buffer[(start + 4, y as u16)].fg, theme.text_muted());
                assert!(
                    !row.contains("agents")
                        && !row.contains("commands")
                        && !row.contains("1 shell")
                );
                assert!(state.shells.footer_hit.get().is_none());
                assert_eq!(state.prompt_shell_example(), example);
                assert_eq!(
                    state.input(),
                    command,
                    "placeholder/footer never edits input"
                );
                assert_eq!(state.prompt_layout(width as usize).1.1, command.len());
                let (metadata_y, metadata) = rows
                    .iter()
                    .enumerate()
                    .find(|(_, row)| row.contains("Shell"))
                    .unwrap();
                let metadata_x = metadata.find("Shell").unwrap() as u16;
                assert!(
                    !buffer[(metadata_x, metadata_y as u16)]
                        .modifier
                        .contains(ratatui::style::Modifier::BOLD)
                );
                if command.is_empty() && home {
                    let placeholder = rows
                        .iter()
                        .find(|row| row.contains("Run a command…"))
                        .unwrap();
                    let start = UnicodeWidthStr::width(
                        &placeholder[..placeholder.find("Run a command…").unwrap()],
                    ) as u16;
                    let placeholder_y =
                        rows.iter().position(|row| row == placeholder).unwrap() as u16;
                    assert_eq!(buffer[(start, placeholder_y)].fg, theme.text_muted());
                } else {
                    assert!(!rows.iter().any(|row| row.contains("Run a command…")));
                }
            }
        }
        state.handle_key(KeyAction::Leader).await;
        let mut pending = Terminal::new(TestBackend::new(80, 24)).unwrap();
        pending.draw(|f| crate::shell::render(f, &state)).unwrap();
        let buffer = pending.backend().buffer();
        let shell = buffer
            .content
            .windows(5)
            .position(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == "Shell")
            .unwrap();
        assert_eq!(
            buffer.content[shell].fg,
            crate::theme::Theme::dark().border()
        );
        state.handle_key(KeyAction::Cancel).await;
        assert!(
            !state.leader_pending(),
            "the existing sequence owner consumes its cancellation"
        );
        assert!(state.prompt_shell_mode());
        state.handle_key(KeyAction::Cancel).await;
        assert!(!state.prompt_shell_mode());
        assert_eq!(state.input(), "printf literal-output");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!text.contains("exit shell mode"));
        assert!(text.contains("ctrl+p commands"));
        assert!(
            inbox.try_recv().is_err(),
            "mode/presentation does not query or execute"
        );
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
