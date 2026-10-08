use super::*;

#[tokio::test]
async fn vis12_history_port_is_global_bounded_available_while_active_and_fails_closed() {
    let (app, mut inbox, _) = CoreApp::channel(1);
    let (ack, _) = tokio::sync::oneshot::channel();
    app.inbox
        .try_send(InboxMsg::PromptHistory { append: None, ack })
        .unwrap();
    assert_eq!(app.prompt_history(None).await, Err(CoreError::QueueFull));
    assert_eq!(
        app.prompt_history(Some("x".repeat(MAX_INPUT_BYTES + 1)))
            .await,
        Err(CoreError::InputTooLarge)
    );
    inbox.recv().await.unwrap();
    drop(inbox);
    assert_eq!(app.prompt_history(None).await, Err(CoreError::Shutdown));

    let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["finished".into()], 50));
    let mut events = app.subscribe();
    let a = SessionId::new("history-a").unwrap();
    let b = SessionId::new("history-b").unwrap();
    app.create_session(a.clone()).await.unwrap();
    app.create_session(b.clone()).await.unwrap();
    let turn = app.submit(a, "accepted Ω界".into()).await.unwrap();
    assert_eq!(app.prompt_history(None).await.unwrap(), ["accepted Ω界"]);
    loop {
        if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { turn: event, .. } if event == turn)
        {
            break;
        }
    }
    for n in 0..65 {
        app.prompt_history(Some(format!("prompt {n}")))
            .await
            .unwrap();
    }
    let bounded = app.prompt_history(None).await.unwrap();
    assert_eq!(bounded.len(), crate::queries::MAX_PROMPT_HISTORY_ENTRIES);
    assert_eq!(bounded.first().unwrap(), "prompt 15");
    assert_eq!(bounded.last().unwrap(), "prompt 64");
    let turn = app.submit(b, "prompt 64".into()).await.unwrap();
    assert_eq!(
        app.prompt_history(None).await.unwrap(),
        bounded,
        "accepted consecutive exact duplicate is shared across sessions"
    );
    loop {
        if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { turn: event, .. } if event == turn)
        {
            break;
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
