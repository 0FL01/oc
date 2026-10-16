use super::*;

#[tokio::test]
async fn vis12_user_shell_port_bounds_queue_and_cancels_pending_admission_without_dispatch() {
    let (app, mut inbox, _) = CoreApp::channel(1);
    let session = SessionId::new("user-shell-port").unwrap();
    let mut receipt = app
        .request_user_shell(
            session.clone(),
            "printf 'Ω界'".into(),
            UserShellSelection::Fresh(None),
        )
        .unwrap();
    assert!(receipt.try_result().is_none());
    assert!(matches!(
        app.request_user_shell(
            session.clone(),
            "next".into(),
            UserShellSelection::Fresh(None)
        ),
        Err(CoreError::QueueFull)
    ));
    assert!(matches!(
        app.request_user_shell(
            session.clone(),
            "x".repeat(MAX_INPUT_BYTES + 1),
            UserShellSelection::Fresh(None)
        ),
        Err(CoreError::InputTooLarge)
    ));
    receipt.cancel();
    let InboxMsg::UserShell {
        command,
        selection,
        cancel,
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("one typed user-command admission, not a model submit");
    };
    assert_eq!(command, "printf 'Ω界'");
    assert_eq!(selection, UserShellSelection::Fresh(None));
    assert!(cancel.load(Ordering::Acquire));
    ack.send(Err(CoreError::Application("cancelled".into())))
        .unwrap();
    assert_eq!(
        receipt.wait().await,
        Err(CoreError::Application("cancelled".into()))
    );
    assert!(inbox.try_recv().is_err());
    drop(inbox);
    assert!(matches!(
        app.request_user_shell(session, "next".into(), UserShellSelection::Existing(None)),
        Err(CoreError::Shutdown)
    ));
}

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
