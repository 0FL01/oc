use super::*;
use oc_core::core_app::UserShellSelection;
use serde_json::json;

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
