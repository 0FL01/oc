use super::*;
use oc_core::{
    core_app::WorkerTurnId,
    domain::SessionId,
    queries::{LiveTurnSnapshot, TabDeckSnapshot, WorkspaceSnapshot},
    session::MAX_QUEUE_ITEMS,
};

fn pair() -> (
    CoreApp,
    tokio::task::JoinHandle<Result<(), ()>>,
    UnixStream,
    Budget,
) {
    let (client, server) = UnixStream::pair().unwrap();
    let (app, inbox, controls, events, admissions) = CoreApp::native_channel(MAX_QUEUE_ITEMS, 7);
    let death = process_death(std::process::id()).unwrap();
    let task = tokio::spawn(bridge(Bridge {
        stream: client,
        inbox,
        controls,
        admissions,
        lease: Lease::Tui,
        events,
        budget: Budget::default(),
        epoch: 0,
        death,
    }));
    (app, task, server, Budget::default())
}

async fn frame(stream: &mut UnixStream, budget: &Budget, value: &Server) {
    framing::write(stream, &Message::encode(value, budget).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn srv04_native_cancel_priority_crosses_full_ipc_pending_window() {
    let (app, task, mut server, budget) = pair();
    let mut receipts = Vec::new();
    let session = SessionId("held".into());
    let (ready, entered) = tokio::sync::oneshot::channel();
    let peer = async {
        let mut first = 0;
        for index in 0..MAX_QUEUE_ITEMS {
            let request: Client = framing::read(&mut server, &budget)
                .await
                .unwrap()
                .decode()
                .unwrap();
            let Client::Request { id, request, .. } = request else {
                panic!("normal submission");
            };
            assert!(matches!(*request, wire::Request::Submit { .. }));
            if index == 0 {
                first = id;
            }
        }
        ready.send(()).unwrap();
        // No ordinary request is acknowledged: the entire existing pending
        // window is still full when cancellation crosses the actual UDS.
        let control: Client = framing::read(&mut server, &budget)
            .await
            .unwrap()
            .decode()
            .unwrap();
        let Client::Control {
            id,
            session,
            expected: Some(Expected::Pending(owned)),
            ..
        } = control
        else {
            panic!("priority cancellation");
        };
        assert_eq!(session.0, "held");
        assert_eq!(owned, first);
        frame(
            &mut server,
            &budget,
            &Server::Reply {
                id,
                location_epoch: 0,
                result: Ok(serde_json::to_value(Ok::<_, oc_core::session::CoreError>(())).unwrap()),
            },
        )
        .await;
    };
    let caller = async {
        for _ in 0..MAX_QUEUE_ITEMS {
            receipts.push(
                app.request_submit(session.clone(), "held input".into())
                    .unwrap(),
            );
        }
        // The peer cannot read this control until it has read all work frames;
        // waiting for its observed final request avoids testing enqueue order.
        entered.await.unwrap();
        app.cancel(session).await.unwrap();
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(peer, caller);
    })
    .await
    .unwrap();
    assert!(
        receipts
            .iter_mut()
            .all(|receipt| receipt.try_result().is_none())
    );
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn srv02_gap_resnapshot_discards_covered_deltas_without_replaying_work() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let session = SessionId("live".into());
    let turn = WorkerTurnId("T1".into());
    let delta = |text: &str| CoreEvent::TextDelta {
        session: session.clone(),
        turn: turn.clone(),
        delta: text.into(),
    };
    for (revision, text) in [(1, "a"), (3, "c")] {
        frame(
            &mut server,
            &budget,
            &Server::Event {
                revision,
                origin: Some(9),
                event: Box::new(wire::Event::from(delta(text))),
            },
        )
        .await;
    }
    assert!(matches!(
        events.recv().await.unwrap(),
        CoreEvent::Projection { revision: 1, .. }
    ));
    assert_eq!(events.recv().await.unwrap(), CoreEvent::ProjectionGap);
    assert!(matches!(
        events.recv().await.unwrap(),
        CoreEvent::Projection { revision: 3, .. }
    ));
    let snapshot = WorkspaceSnapshot {
        revision: 3,
        deck: TabDeckSnapshot {
            location: "project".into(),
            revision: None,
            sessions: vec![session.clone()],
            new_session_titles: Vec::new(),
            active: Some(session.clone()),
            rail_width: Some(28),
        },
        turns: vec![LiveTurnSnapshot {
            session: session.clone(),
            turn: turn.clone(),
            origin: Some(9),
            events: vec![delta("abc")],
            display_name: None,
            truncated: false,
        }],
    };
    let query = app.workspace(Some(session.clone()));
    let answer = async {
        let Client::Request { id, request, .. } = framing::read(&mut server, &budget)
            .await
            .unwrap()
            .decode::<Client>()
            .unwrap()
        else {
            panic!("snapshot request");
        };
        assert!(
            matches!(*request, wire::Request::Workspace { .. }),
            "repair never resubmits a mutation"
        );
        frame(
            &mut server,
            &budget,
            &Server::Reply {
                id,
                location_epoch: 0,
                result: Ok(serde_json::to_value(Ok::<_, oc_core::session::CoreError>(
                    snapshot.clone(),
                ))
                .unwrap()),
            },
        )
        .await;
    };
    let (actual, ()) = tokio::join!(query, answer);
    assert_eq!(actual.unwrap(), snapshot);
    for (revision, text) in [(2, "old"), (3, "covered"), (4, "d")] {
        frame(
            &mut server,
            &budget,
            &Server::Event {
                revision,
                origin: Some(9),
                event: Box::new(wire::Event::from(delta(text))),
            },
        )
        .await;
    }
    let next = tokio::time::timeout(Duration::from_secs(3), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        next,
        CoreEvent::Projection {
            revision: 4,
            event: Box::new(delta("d"))
        }
    );
    assert!(
        events.try_recv().is_err(),
        "covered snapshot events are not appended twice"
    );
    task.abort();
    let _ = task.await;
}
