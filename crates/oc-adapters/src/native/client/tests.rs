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

fn snapshot(revision: u64, session: &str, origin: Option<u64>) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        revision,
        deck: TabDeckSnapshot {
            location: "project".into(),
            revision: None,
            sessions: vec![SessionId(session.into())],
            new_session_titles: Vec::new(),
            active: Some(SessionId(session.into())),
            rail_width: Some(28),
        },
        turns: origin
            .map(|origin| LiveTurnSnapshot {
                session: SessionId(session.into()),
                turn: WorkerTurnId(format!("T{origin}")),
                origin: Some(origin),
                events: Vec::new(),
                display_name: None,
                truncated: false,
            })
            .into_iter()
            .collect(),
    }
}

async fn request(stream: &mut UnixStream, budget: &Budget) -> (u64, wire::Request) {
    let Client::Request { id, request, .. } = framing::read(stream, budget)
        .await
        .unwrap()
        .decode()
        .unwrap()
    else {
        panic!("ordinary request");
    };
    (id, *request)
}

async fn answer<T: serde::Serialize>(
    stream: &mut UnixStream,
    budget: &Budget,
    id: u64,
    epoch: u64,
    value: T,
) {
    frame(
        stream,
        budget,
        &Server::Reply {
            id,
            location_epoch: epoch,
            result: Ok(serde_json::to_value(Ok::<_, oc_core::session::CoreError>(value)).unwrap()),
        },
    )
    .await;
}

async fn cancel_expected(
    app: &CoreApp,
    stream: &mut UnixStream,
    budget: &Budget,
    session: &str,
    expected: Option<u64>,
) {
    let cancel = app.cancel_turn(SessionId(session.into()), WorkerTurnId("painted".into()));
    let peer = async {
        let Client::Control {
            id,
            expected: actual,
            ..
        } = framing::read(stream, budget)
            .await
            .unwrap()
            .decode()
            .unwrap()
        else {
            panic!("captured control");
        };
        match (actual, expected) {
            (Some(Expected::Origin(actual)), Some(expected)) => assert_eq!(actual, expected),
            (None, None) => {}
            _ => panic!("wrong control provenance"),
        }
        answer(stream, budget, id, 0, ()).await;
    };
    let (result, ()) = tokio::join!(cancel, peer);
    result.unwrap();
}

#[tokio::test]
async fn workspace_ahead_of_wire_preserves_other_sessions_and_monotonic_controls() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let start = |session: &str, origin: u64| CoreEvent::TurnStarted {
        session: SessionId(session.into()),
        turn: WorkerTurnId(format!("T{origin}")),
        model_switch: None,
    };
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 1,
            origin: Some(11),
            event: Box::new(wire::Event::from(start("child", 11))),
        },
    )
    .await;
    events.recv().await.unwrap();
    let query = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (id, _) = request(&mut server, &budget).await;
        answer(&mut server, &budget, id, 0, snapshot(5, "root", Some(22))).await;
    };
    let (result, ()) = tokio::join!(query, peer);
    result.unwrap();
    cancel_expected(&app, &mut server, &budget, "child", Some(11)).await;
    let child_delta = CoreEvent::TextDelta {
        session: SessionId("child".into()),
        turn: WorkerTurnId("T11".into()),
        delta: "not covered by the root snapshot".into(),
    };
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 2,
            origin: Some(11),
            event: Box::new(wire::Event::from(child_delta.clone())),
        },
    )
    .await;
    // A covered old root start must still be forwarded for per-view filtering,
    // but cannot roll the bridge's captured control back from origin 22 to 10.
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 4,
            origin: Some(10),
            event: Box::new(wire::Event::from(start("root", 10))),
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(3), async {
        assert_eq!(
            events.recv().await.unwrap(),
            CoreEvent::Projection {
                revision: 2,
                event: Box::new(child_delta),
            }
        );
        assert!(matches!(
            events.recv().await.unwrap(),
            CoreEvent::ProjectionGap
        ));
        assert!(matches!(
            events.recv().await.unwrap(),
            CoreEvent::Projection { revision: 4, .. }
        ));
        cancel_expected(&app, &mut server, &budget, "child", None).await;
        cancel_expected(&app, &mut server, &budget, "root", Some(22)).await;
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn delayed_workspace_replies_cannot_revive_idle_or_replace_a_newer_origin() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let first = app.workspace(Some(SessionId("root".into())));
    let second = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (old, _) = request(&mut server, &budget).await;
        let (new, _) = request(&mut server, &budget).await;
        answer(&mut server, &budget, new, 0, snapshot(5, "root", None)).await;
        answer(&mut server, &budget, old, 0, snapshot(4, "root", Some(11))).await;
    };
    let (first, second, ()) = tokio::join!(first, second, peer);
    first.unwrap();
    second.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", None).await;
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 6,
            origin: Some(12),
            event: Box::new(wire::Event::from(CoreEvent::TurnStarted {
                session: SessionId("root".into()),
                turn: WorkerTurnId("T12".into()),
                model_switch: None,
            })),
        },
    )
    .await;
    events.recv().await.unwrap();
    let query = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (id, _) = request(&mut server, &budget).await;
        answer(&mut server, &budget, id, 0, snapshot(5, "root", Some(11))).await;
    };
    let (result, ()) = tokio::join!(query, peer);
    result.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", Some(12)).await;
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 7,
            origin: Some(11),
            event: Box::new(wire::Event::from(CoreEvent::TurnInterrupted {
                session: SessionId("root".into()),
                turn: WorkerTurnId("T11".into()),
                partial: String::new(),
                duration_ms: 1,
            })),
        },
    )
    .await;
    events.recv().await.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", Some(12)).await;
    // The wire caught up with the idle snapshot before the old reply arrived:
    // its pending request still has to retain the tombstone's ordering barrier.
    let first = app.workspace(Some(SessionId("idle".into())));
    let second = app.workspace(Some(SessionId("idle".into())));
    let peer = async {
        let (old, _) = request(&mut server, &budget).await;
        let (new, _) = request(&mut server, &budget).await;
        answer(&mut server, &budget, new, 0, snapshot(7, "idle", None)).await;
        answer(&mut server, &budget, old, 0, snapshot(6, "idle", Some(13))).await;
    };
    let (first, second, ()) = tokio::join!(first, second, peer);
    first.unwrap();
    second.unwrap();
    cancel_expected(&app, &mut server, &budget, "idle", None).await;
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn workspace_preserves_owned_bound_and_a_failed_new_input_does_not_erase_it() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let mut receipt = app
        .request_submit(SessionId("root".into()), "first input".into())
        .unwrap();
    let (owned, _) = request(&mut server, &budget).await;
    frame(
        &mut server,
        &budget,
        &Server::Bound {
            id: owned,
            session: SessionId("root".into()),
            origin: 99,
        },
    )
    .await;
    let query = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (id, _) = request(&mut server, &budget).await;
        answer(&mut server, &budget, id, 0, snapshot(2, "root", None)).await;
        answer(&mut server, &budget, owned, 0, WorkerTurnId("T99".into())).await;
    };
    let (result, ()) = tokio::join!(query, peer);
    result.unwrap();
    receipt.wait().await.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", Some(99)).await;
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 3,
            origin: Some(99),
            event: Box::new(wire::Event::from(CoreEvent::TurnStarted {
                session: SessionId("root".into()),
                turn: WorkerTurnId("T99".into()),
                model_switch: None,
            })),
        },
    )
    .await;
    events.recv().await.unwrap();
    let mut refused = app
        .request_submit(SessionId("root".into()), "overlapping input".into())
        .unwrap();
    let (id, _) = request(&mut server, &budget).await;
    frame(
        &mut server,
        &budget,
        &Server::Reply {
            id,
            location_epoch: 0,
            result: Ok(serde_json::to_value(Err::<WorkerTurnId, _>(
                oc_core::session::CoreError::TurnBusy,
            ))
            .unwrap()),
        },
    )
    .await;
    assert_eq!(
        refused.wait().await,
        Err(oc_core::session::CoreError::TurnBusy)
    );
    cancel_expected(&app, &mut server, &budget, "root", Some(99)).await;
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn an_unobserved_terminal_fences_a_previously_issued_workspace_reply() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let query = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (id, _) = request(&mut server, &budget).await;
        frame(
            &mut server,
            &budget,
            &Server::Event {
                revision: 11,
                origin: Some(11),
                event: Box::new(wire::Event::from(CoreEvent::TurnInterrupted {
                    session: SessionId("root".into()),
                    turn: WorkerTurnId("T11".into()),
                    partial: String::new(),
                    duration_ms: 1,
                })),
            },
        )
        .await;
        answer(&mut server, &budget, id, 0, snapshot(10, "root", Some(11))).await;
    };
    let (result, ()) = tokio::join!(query, peer);
    result.unwrap();
    events.recv().await.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", None).await;
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn settled_title_owner_cannot_override_the_next_turn_control() {
    let (app, task, mut server, budget) = pair();
    let mut events = app.subscribe();
    let title = app.regenerate_title(SessionId("root".into()));
    let peer = async {
        let (id, _) = request(&mut server, &budget).await;
        frame(
            &mut server,
            &budget,
            &Server::Bound {
                id,
                session: SessionId("root".into()),
                origin: 1,
            },
        )
        .await;
        answer(&mut server, &budget, id, 0, "title").await;
    };
    let (result, ()) = tokio::join!(title, peer);
    assert_eq!(result.unwrap(), "title");
    frame(
        &mut server,
        &budget,
        &Server::Event {
            revision: 1,
            origin: Some(2),
            event: Box::new(wire::Event::from(CoreEvent::TurnStarted {
                session: SessionId("root".into()),
                turn: WorkerTurnId("T2".into()),
                model_switch: None,
            })),
        },
    )
    .await;
    events.recv().await.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", Some(2)).await;
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn workspace_sent_before_route_change_cannot_install_old_location_controls() {
    let (app, task, mut server, budget) = pair();
    let old = app.workspace(Some(SessionId("root".into())));
    let peer = async {
        let (old_id, _) = request(&mut server, &budget).await;
        let changed = app.workspace(None);
        let route_reply = async {
            let (id, _) = request(&mut server, &budget).await;
            // Replies report the attachment epoch at send time, not necessarily
            // the epoch captured by an earlier concurrent workspace query.
            answer(&mut server, &budget, id, 1, snapshot(8, "root", None)).await;
        };
        let (result, ()) = tokio::join!(changed, route_reply);
        result.unwrap();
        let current = app.workspace(Some(SessionId("root".into())));
        let current_reply = async {
            let (id, _) = request(&mut server, &budget).await;
            answer(&mut server, &budget, id, 1, snapshot(10, "root", Some(88))).await;
        };
        let (result, ()) = tokio::join!(current, current_reply);
        result.unwrap();
        answer(
            &mut server,
            &budget,
            old_id,
            1,
            snapshot(99, "root", Some(77)),
        )
        .await;
    };
    let (result, ()) = tokio::join!(old, peer);
    result.unwrap();
    cancel_expected(&app, &mut server, &budget, "root", Some(88)).await;
    task.abort();
    let _ = task.await;
}

#[test]
fn snapshot_retires_only_turn_receipts_acknowledged_before_its_query() {
    let mut operation = Operation::default();
    for (id, completion, acknowledged_at) in [
        (1, OwnedCompletion::Turn, Some(5)),
        (2, OwnedCompletion::Turn, Some(6)),
        (3, OwnedCompletion::Turn, None),
        (4, OwnedCompletion::Compaction, Some(5)),
    ] {
        operation.owned.insert(
            id,
            OwnedOperation {
                origin: Some(id),
                completion,
                acknowledged_at,
            },
        );
    }
    operation.snapshot(10, None, 8, 6);
    assert_eq!(
        operation.owned.keys().copied().collect::<Vec<_>>(),
        [2, 3, 4]
    );
    operation.snapshot(11, None, 9, 7);
    assert_eq!(operation.owned.keys().copied().collect::<Vec<_>>(), [3, 4]);
    assert!(matches!(operation.expected(), Some(Expected::Origin(4))));
    operation.owned.insert(
        5,
        OwnedOperation {
            origin: Some(5),
            completion: OwnedCompletion::Turn,
            acknowledged_at: Some(9),
        },
    );
    operation.snapshot(10, None, 11, 11);
    assert!(
        operation.owned.contains_key(&5),
        "an obsolete reply cannot resolve ownership"
    );
    operation.snapshot(12, Some(100), 12, 12);
    assert!(
        !operation.owned.contains_key(&5),
        "a post-ACK successor snapshot replaces the old provisional owner"
    );
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
