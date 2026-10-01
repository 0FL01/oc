use super::*;
use oc_core::{
    core_app::{CoreApp, MockProvider},
    domain::SessionId,
};

fn job(id: &str, source: &str) -> ShellJob {
    ShellJob {
        session: SessionId(source.into()),
        shell_id: id.into(),
        location: "/original".into(),
        generation: 42,
        turn: "issuing-turn".into(),
        model: "gpt-original".into(),
        provider: "fixture".into(),
        command: id.into(),
        pid: Some(123),
        background: false,
    }
}

#[tokio::test]
async fn tool13_open_viewer_and_kill_keep_original_child_identity_after_list_removal() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("parent".into()));
    let child = job("selected", "child");
    state.restore_prompt("draft remains ordinary editor state".into());
    state.apply_shell_jobs(vec![child.clone(), job("sibling", "parent")]);
    state.handle_key(KeyAction::Shells).await;
    state.handle_key(KeyAction::Enter).await;
    state.apply_shell_jobs(vec![job("sibling", "parent")]);
    state.apply_shell_output(
        &child,
        ToolOutputPage {
            text: "status: completed\nfinal-child".into(),
            total_bytes: 35,
            next_offset: None,
        },
    );
    let control = state.handle_key(KeyAction::DeleteOrQuit).await;
    let viewer = state.shell_viewer().cloned();
    let output = state.shells.output.as_ref().unwrap().text.clone();
    let draft = state.input().to_owned();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        matches!(control.intent, Some(PanelIntent::CancelShell { session, shell_id }) if session.0 == "child" && shell_id == "selected")
    );
    assert_eq!(viewer, Some(child));
    assert!(output.contains("final-child"));
    assert_eq!(draft, "draft remains ordinary editor state");
}

#[tokio::test]
async fn tool13_same_id_location_adoption_keeps_only_original_shell_capture() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    let mut original = TuiState::new(app.clone(), SessionId("parent".into()));
    let child = job("selected", "child");
    original.apply_shell_jobs(vec![child.clone()]);
    original.handle_key(KeyAction::Shells).await;
    original.handle_key(KeyAction::Enter).await;
    original.restore_prompt("old-location editor".into());
    let mut moved = TuiState::new(app.clone(), SessionId("parent".into()));
    moved.chrome.location = Some("/destination".into());
    moved.inherit_shell_view(&mut original);
    let key = moved.terminal_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::CONTROL,
    ));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(moved.shell_viewer(), Some(&child));
    assert_eq!(moved.chrome.location.as_deref(), Some("/destination"));
    assert!(moved.input().is_empty());
    assert_eq!(key, Some(KeyAction::ShellBackground));
    assert!(!original.shells_open());
}
