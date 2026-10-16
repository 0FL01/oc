use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{ChildJob, ChildState};

#[tokio::test]
async fn term01_child_composer_closes_to_parent_before_captured_create_error() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut parent = TuiState::new(app.clone(), SessionId("parent".into()));
    parent.restore_prompt("parked parent draft".into());
    parent.chrome.location = Some("/parent".into());
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    state.chrome.session_terminal = true;
    state.chrome.terminal_generation = 19;
    state.attach_linked_child(ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "child-operation".into(),
        generation: 7,
        location: "/child".into(),
        agent: "build".into(),
        model: "fixture/model".into(),
        description: "child".into(),
        delivery_id: "delivery".into(),
        state: ChildState::Running,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    });
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        read_only: true,
        ..Default::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::Terminal {
            session,
            action,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured terminal create");
        };
        assert_eq!(session, SessionId("child".into()));
        assert!(
            matches!(action,TerminalAction::CreateChild { source,generation:19,.. }
                if source.child.0 == "child" && source.parent.0 == "parent"
                    && source.location == "/child" && source.generation == 7
                    && source.operation == "child-operation")
        );
        ack.send(Err(CoreError::Application(
            "captured admission refused".into(),
        )))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    state.handle_key(KeyAction::TerminalSelect).await;
    state.handle_key(KeyAction::Up).await;
    let captured = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    assert!(!state.terminals_open());
    apply_intent(&app, &mut state, &mut deck, captured)
        .await
        .unwrap();
    worker.await.unwrap();
    assert_eq!(state.session(), &SessionId("parent".into()));
    assert_eq!(state.input(), "parked parent draft");
    assert!(deck.child_views.contains_key(&SessionId("child".into())));
    assert!(!deck.read_only);
    assert_eq!(state.note(), Some("Unable to load terminal"));
    assert!(
        deck.child_views[&SessionId("child".into())]
            .chrome
            .session_terminal
    );
}
