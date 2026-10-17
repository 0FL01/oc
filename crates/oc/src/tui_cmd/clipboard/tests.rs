use super::*;
use std::sync::Arc;

#[tokio::test]
async fn clipboard_pending_enter_and_stale_edit_cancel_and_join() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("clipboard".into()));
    state.handle_paste("draft");
    let mut deck = LoopState::default();
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    let joined = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done = joined.clone();
    deck.clipboard_job = Some(ClipboardJob {
        target: state.clipboard_target().unwrap(),
        location: None,
        tab: None,
        cancel,
        task: tokio::spawn(async move {
            let _ = cancelled.await;
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::clipboard::Content::Text("obsolete clipboard".into()))
        }),
    });
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
    )
    .await
    .unwrap();
    assert!(
        inbox.try_recv().is_err(),
        "Enter must not submit a partial draft"
    );
    assert_eq!(state.input(), "draft");
    state.handle_paste(" changed");
    assert!(sync_clipboard(&mut state, &mut deck).await);
    assert!(deck.clipboard_job.is_none());
    assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        state.input(),
        "draft changed",
        "obsolete result is discarded"
    );
}
