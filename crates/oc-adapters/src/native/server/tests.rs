use super::*;
use oc_core::core_app::UserShellSelection;
use serde_json::json;

async fn send_client(client: &mut UnixStream, budget: &Budget, frame: Client) {
    framing::write(client, &Message::encode(&frame, budget).unwrap())
        .await
        .unwrap();
}

async fn next_reply(
    client: &mut UnixStream,
    budget: &Budget,
) -> (u64, Result<serde_json::Value, CoreError>) {
    loop {
        if let Server::Reply { id, result, .. } = framing::read(client, budget)
            .await
            .unwrap()
            .decode::<Server>()
            .unwrap()
        {
            return (id, result);
        }
    }
}

#[tokio::test]
async fn srv04_running_shell_receipt_cancellation_and_bound_pending_root_control_are_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join("config")).unwrap();
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(project.join("opencode.json"),json!({
        "model":"fixture/local",
        "agents":{"title":{"disabled":true}},
        "providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"apiKey":"fixture-only","baseURL":format!("http://{}/v1",provider.local_addr().unwrap())},"models":{"local":{"limit":{"context":65536,"output":4096}}}}},
        "permissions":[{"action":"shell","resource":"*","effect":"ask"}]
    }).to_string()).unwrap();
    let owner = Owner::new(
        Db::open(&data).unwrap(),
        BTreeMap::from([
            ("HOME".into(), home.display().to_string()),
            (
                "XDG_CONFIG_HOME".into(),
                home.join("config").display().to_string(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("OC_ALLOW_LOOPBACK_TEST_PROVIDER".into(), "1".into()),
        ]),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("running-shell".into());
    location.app.create_session(session.clone()).await.unwrap();
    let root = owner
        .begin(
            location,
            session.0.clone(),
            Arc::new(Origin::new(Kind::Run, false)),
            1,
            false,
            None,
        )
        .unwrap();
    let mut events = root.app.subscribe();
    let turn = root
        .app
        .submit(session.clone(), "held model".into())
        .await
        .unwrap();
    let socket = tokio::time::timeout(Duration::from_secs(5), provider.accept())
        .await
        .unwrap()
        .unwrap()
        .0;
    let endpoint = Endpoint::bind(&data).unwrap();
    let (disconnect, closed) = watch::channel(false);
    let state = Arc::new(Mutex::new(State {
        closing: false,
        startup: false,
        sequence: 1,
        revision: 0,
        viewers: BTreeMap::from([(
            1,
            Viewer {
                ready: false,
                lease: None,
                output: None,
                disconnect,
            },
        )]),
        operations: BTreeMap::new(),
        terminals: BTreeMap::new(),
        cleanup_failed: false,
    }));
    let (mut client, server) = UnixStream::pair().unwrap();
    let budget = Budget::default();
    let task = tokio::spawn(connection(
        server,
        1,
        endpoint.registration.clone(),
        owner.clone(),
        state,
        budget.clone(),
        closed,
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        framing::write(
            &mut client,
            &Message::encode(
                &Hello {
                    version: endpoint::VERSION,
                    instance: endpoint.registration.instance.clone(),
                    lease: Lease::Run,
                    project: Some(project.display().to_string()),
                },
                &budget,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        framing::read(&mut client, &budget)
            .await
            .unwrap()
            .decode::<Result<Welcome, SpawnDiagnostic>>()
            .unwrap()
            .unwrap();
        for id in [1, 2] {
            send_client(
                &mut client,
                &budget,
                Client::Request {
                    id,
                    location_epoch: 0,
                    expected: None,
                    request: Box::new(Request::UserShell {
                        session: session.clone(),
                        command: format!("printf unexpected > shell-{id}.effect"),
                        selection: UserShellSelection::Existing(None),
                        cancel: Arc::new(AtomicBool::new(false)),
                    }),
                },
            )
            .await;
            loop {
                if let CoreEvent::PermissionAsked(request) = events.recv().await.unwrap()
                    && request.binding.operation.starts_with("user-shell:")
                {
                    break;
                }
            }
            if id == 1 {
                send_client(&mut client, &budget, Client::CancelRequest { id }).await;
                let (reply, result) = next_reply(&mut client, &budget).await;
                assert_eq!(reply, id);
                assert!(
                    serde_json::from_value::<Result<String, CoreError>>(result.unwrap())
                        .unwrap()
                        .is_err()
                );
                assert!(!root.cancel.load(Ordering::Acquire));
            } else {
                send_client(
                    &mut client,
                    &budget,
                    Client::Control {
                        id: 100,
                        location_epoch: 0,
                        expected: Some(Expected::Pending(id)),
                        session: session.clone(),
                        turn: Some(turn.clone()),
                    },
                )
                .await;
                let mut replies = BTreeMap::new();
                while replies.len() < 2 {
                    let (id, result) = next_reply(&mut client, &budget).await;
                    replies.insert(id, result.unwrap());
                }
                assert!(
                    serde_json::from_value::<Result<(), CoreError>>(replies.remove(&100).unwrap())
                        .unwrap()
                        .is_ok()
                );
                assert!(
                    serde_json::from_value::<Result<String, CoreError>>(
                        replies.remove(&id).unwrap()
                    )
                    .unwrap()
                    .is_err()
                );
                assert!(root.cancel.load(Ordering::Acquire));
            }
        }
        owner.next_root().await.unwrap();
    })
    .await
    .unwrap();
    drop(client);
    drop(socket);
    assert!(task.await.unwrap().is_none());
    owner.shutdown().await.unwrap();
    assert!(!project.join("shell-1.effect").exists());
    assert!(!project.join("shell-2.effect").exists());
}

#[tokio::test]
async fn srv04_request_cancellation_overtakes_queued_shell_before_admission() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let home = root.path().join("home");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        json!({
            "model":"fixture/local",
            "providers":{"fixture":{"package":"@ai-sdk/openai",
                "settings":{"apiKey":"fixture-only","baseURL":"http://127.0.0.1:9/v1"},
                "models":{"local":{"limit":{"context":65536,"output":4096}}}}},
            "permissions":[{"action":"shell","resource":"*","effect":"allow"}]
        })
        .to_string(),
    )
    .unwrap();
    let owner = Owner::new(
        Db::open(&data).unwrap(),
        BTreeMap::from([
            ("HOME".into(), home.display().to_string()),
            (
                "XDG_CONFIG_HOME".into(),
                home.join("config").display().to_string(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("OC_ALLOW_LOOPBACK_TEST_PROVIDER".into(), "1".into()),
        ]),
    )
    .unwrap();
    let endpoint = Endpoint::bind(&data).unwrap();
    let (disconnect, closed) = watch::channel(false);
    let state = Arc::new(Mutex::new(State {
        closing: false,
        startup: false,
        sequence: 1,
        revision: 0,
        viewers: BTreeMap::from([(
            1,
            Viewer {
                ready: false,
                lease: None,
                output: None,
                disconnect,
            },
        )]),
        operations: BTreeMap::new(),
        terminals: BTreeMap::new(),
        cleanup_failed: false,
    }));
    let (mut client, server) = UnixStream::pair().unwrap();
    let budget = Budget::default();
    let task = tokio::spawn(connection(
        server,
        1,
        endpoint.registration.clone(),
        owner.clone(),
        state,
        budget.clone(),
        closed,
    ));
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        framing::write(
            &mut client,
            &Message::encode(
                &Hello {
                    version: endpoint::VERSION,
                    instance: endpoint.registration.instance.clone(),
                    lease: Lease::Run,
                    project: Some(project.display().to_string()),
                },
                &budget,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        framing::read(&mut client, &budget)
            .await
            .unwrap()
            .decode::<Result<Welcome, SpawnDiagnostic>>()
            .unwrap()
            .unwrap();
        // This is the legal priority-lane order when the bulk writer has not
        // yet sent the queued request. The wire reconstructs a fresh cancel flag.
        for frame in [
            Client::CancelRequest { id: 1 },
            Client::Request {
                id: 1,
                location_epoch: 0,
                expected: None,
                request: Box::new(Request::UserShell {
                    session: SessionId("overtaken-shell".into()),
                    command: "printf unexpected > overtaken.effect".into(),
                    selection: UserShellSelection::Fresh(None),
                    cancel: Arc::new(AtomicBool::new(false)),
                }),
            },
        ] {
            framing::write(&mut client, &Message::encode(&frame, &budget).unwrap())
                .await
                .unwrap();
        }
        loop {
            if let Server::Reply { id: 1, result, .. } = framing::read(&mut client, &budget)
                .await
                .unwrap()
                .decode::<Server>()
                .unwrap()
            {
                break result.and_then(|value| {
                    serde_json::from_value::<Result<String, CoreError>>(value).unwrap()
                });
            }
        }
    })
    .await;
    drop(client);
    assert!(task.await.unwrap().is_none());
    owner.shutdown().await.unwrap();
    assert_eq!(
        result.unwrap(),
        Err(CoreError::Application(
            "user shell admission cancelled".into()
        ))
    );
    assert!(!project.join("overtaken.effect").exists());
    assert!(matches!(
        owner.db.session_meta("overtaken-shell"),
        Err(StorageError::SessionNotFound)
    ));
}
