use super::*;
use oc_core::core_app::InboxMsg;

#[tokio::test]
async fn clipboard_images_atomic_edit_restore_receipt_chips_and_plain_history() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), sid("images"));
    let png: std::sync::Arc<[u8]> = b"fixture-opaque-editor-payload".as_slice().into();
    state.handle_paste(" Ω ");
    assert!(state.paste_clipboard_image(png.clone()).note.is_none());
    assert_eq!(state.input(), " Ω [Image 1] ");
    state.handle_key(KeyAction::Backspace).await; // trailing space
    state.handle_key(KeyAction::Char('\u{301}')).await; // grapheme crossing label end
    assert_eq!(state.editor.prompt(state.input()).images.len(), 1);
    let prompt = state.editor.prompt(state.input());
    assert_eq!(
        &prompt.text[prompt.images[0].mark.start..prompt.images[0].mark.end],
        "[Image 1]"
    );
    assert!(
        !state
            .editor
            .expand_chip(&prompt.text, prompt.images[0].mark.start)
    );
    state.handle_key(KeyAction::Backspace).await;
    assert!(
        state.editor.prompt(state.input()).images.is_empty(),
        "entire accented marker deleted"
    );
    state.handle_key(KeyAction::Undo).await;
    assert_eq!(state.editor.prompt(state.input()).images.len(), 1);
    state.handle_key(KeyAction::Redo).await;
    state.handle_key(KeyAction::Undo).await;
    state.paste_clipboard_image(png.clone());
    assert!(state.input().contains("[Image 2]"));
    let saved = state.composer_draft();
    let mut restored = TuiState::new(app, sid("images"));
    restored.restore_composer_draft(saved);
    assert_eq!(
        restored.editor.prompt(restored.input()),
        state.editor.prompt(state.input())
    );
    restored.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { text, ack, .. }) = inbox.recv().await else {
        panic!("submit")
    };
    assert_eq!(text.images.len(), 2);
    assert_eq!(text.images[0].png, png);
    assert_eq!(
        &text.text[text.images[0].mark.start..text.images[0].mark.end],
        "[Image 1]"
    );
    restored.handle_paste("!"); // receipt cannot clear the independent next draft
    assert!(restored.clipboard_target().is_some());
    restored.paste_clipboard_image(b"independent-next-image".as_slice().into());
    let next_prompt = restored.editor.prompt(restored.input());
    let next_cursor = restored.editor.cursor;
    assert_eq!(next_prompt.images.len(), 1);
    ack.send(Ok(WorkerTurnId("accepted".into()))).unwrap();
    restored.poll_submission();
    assert_eq!(restored.history().rows()[0].chips.len(), 2);
    assert_eq!(restored.editor.prompt(restored.input()), next_prompt);
    assert_eq!(restored.editor.cursor, next_cursor);
    let mut row = msg(1, Role::User, &text.text);
    row.images = text.images.iter().map(|i| i.mark.clone()).collect();
    restored.attach_page(&page(vec![row], 1, false, false));
    assert_eq!(restored.history().rows()[0].chips.len(), 2);
    let mut editor = crate::editor::Editor::default();
    let mut draft = text.text.clone();
    assert!(editor.recall(&mut draft, true, vec![text.text]));
    assert!(
        editor.prompt(&draft).images.is_empty(),
        "ordinary durable Up history is text-only"
    );
}

#[tokio::test]
async fn clipboard_images_effective_binding_and_non_chat_guards() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let key = |code| KeyEvent::new(code, KeyModifiers::CONTROL);
    assert_eq!(
        state.terminal_key(key(KeyCode::Char('v'))),
        Some(KeyAction::Paste)
    );
    state.chrome.prompt_paste_shortcut = Some("ctrl+g p".into());
    let leader = state.terminal_key(key(KeyCode::Char('g'))).unwrap();
    state.handle_key(leader).await;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
        Some(KeyAction::Paste)
    );
    state.chrome.prompt_paste_shortcut = Some(String::new());
    assert_ne!(
        state.terminal_key(key(KeyCode::Char('v'))),
        Some(KeyAction::Paste)
    );
    state.paste_clipboard_image(b"opaque".as_slice().into());
    state.editor.cursor = 0;
    state.handle_key(KeyAction::Char('/')).await;
    assert!(state.handle_key(KeyAction::Enter).await.note.is_some());
    assert!(state.editor.has_images());
    assert!(state.request_compress(String::new()).is_err());
    assert!(
        state.editor.has_images(),
        "command refusal preserves attachments"
    );
    state.panel = TuiPanel::Model;
    assert!(state.clipboard_target().is_none());
}
