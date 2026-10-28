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
async fn incident_native_compact_with_shell_and_child_completions() {
    use oc_core::compaction::{CompactionSnapshot, CompactionState};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join("config")).unwrap();
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(project.join("opencode.json"), json!({
        "model":"fixture/local", "compaction":{"auto":false,"keep":{"tokens":0}},
        "agents":{"title":{"disabled":true},"helper":{"mode":"subagent","model":"fixture/child"}},
        "providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"apiKey":"fixture-only","baseURL":format!("http://{}/v1",provider.local_addr().unwrap())},"models":{
            "local":{"limit":{"context":100000,"output":2048}},"child":{"limit":{"context":100000,"output":2048}}
        }}},
        "permissions":[{"action":"shell","resource":"*","effect":"allow"},{"action":"subagent","resource":"*","effect":"allow"}]
    }).to_string()).unwrap();
    let parent_release = Arc::new(tokio::sync::Notify::new());
    let child_release = Arc::new(tokio::sync::Notify::new());
    let summary_release = Arc::new(tokio::sync::Notify::new());
    let (posted, mut requests) = tokio::sync::mpsc::channel(16);
    let peer = {
        let parent_release = parent_release.clone();
        let child_release = child_release.clone();
        let summary_release = summary_release.clone();
        tokio::spawn(async move {
            let parents = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut clients = tokio::task::JoinSet::new();
            loop {
                let (mut socket, _) = provider.accept().await.unwrap();
                let parents = parents.clone();
                let posted = posted.clone();
                let parent_release = parent_release.clone();
                let child_release = child_release.clone();
                let summary_release = summary_release.clone();
                clients.spawn(async move {
                    let request: serde_json::Value = {
                        let mut reader = tokio::io::BufReader::new(&mut socket);
                        let mut length = None;
                        let mut header_bytes = 0;
                        loop {
                            let mut line = String::new();
                            assert!(reader.read_line(&mut line).await.unwrap() > 0);
                            header_bytes += line.len();
                            assert!(header_bytes < 16 * 1024);
                            if line == "\r\n" { break; }
                            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                                length = Some(value.trim().parse::<usize>().unwrap());
                            }
                        }
                        let length = length.unwrap();
                        assert!(length <= 1 << 20);
                        let mut bytes = vec![0; length];
                        reader.read_exact(&mut bytes).await.unwrap();
                        serde_json::from_slice(&bytes).unwrap()
                    };
                    let output = if request["model"] == "child" {
                        posted.send(("child".to_owned(), request)).await.unwrap();
                        child_release.notified().await;
                        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"CHILD_COMPLETED_ONCE"}]}])
                    } else if request["tools"].as_array().unwrap().is_empty() {
                        posted.send(("summary".to_owned(), request)).await.unwrap();
                        summary_release.notified().await;
                        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Objective\nContinue the task; background shell and child results are pending."}]}])
                    } else {
                        let index = parents.fetch_add(1, Ordering::AcqRel);
                        posted.send((format!("parent-{index}"), request)).await.unwrap();
                        if index == 0 {
                            json!([
                                {"type":"function_call","id":"shell-item","call_id":"shell-launch","name":"shell","arguments":json!({
                                    "command":"n=0; while [ ! -f release-shell ] && [ $n -lt 1000 ]; do n=$((n+1)); sleep .01; done; [ -f release-shell ] || exit 124; printf x >> effect; printf SHELL_COMPLETED_ONCE",
                                    "background":true,"timeout":0
                                }).to_string()},
                                {"type":"function_call","id":"child-item","call_id":"child-launch","name":"subagent","arguments":json!({"agent":"helper","description":"independent work","prompt":"child task","background":true}).to_string()}
                            ])
                        } else {
                            if index == 1 { parent_release.notified().await; }
                            json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
                        }
                    };
                    let body = format!("data: {}\n\n", json!({"type":"response.completed","response":{"status":"completed","output":output,"usage":{"input_tokens":1000,"output_tokens":100}}}));
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                });
            }
        })
    };
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
    let session = SessionId("compact-jobs".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Compaction fixture".into())
        .await
        .unwrap();
    let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
    let root = owner
        .begin(location.clone(), session.0.clone(), origin, 0, false, None)
        .unwrap();
    let mut events = owner.events.subscribe();
    let turn = root
        .app
        .submit(session.clone(), "Launch both independent jobs".into())
        .await
        .unwrap();
    let endpoint = Endpoint::bind(&data).await.unwrap();
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
    let connection = tokio::spawn(connection(
        server,
        1,
        endpoint.registration.clone(),
        owner.clone(),
        state,
        budget.clone(),
        closed,
    ));
    let mut stage = "active root and jobs";
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        framing::write(&mut client, &Message::encode(&Hello {
            version: endpoint::VERSION, instance: endpoint.registration.instance.clone(),
            lease: Lease::Run, project: Some(project.display().to_string()),
        }, &budget).unwrap()).await.unwrap();
        framing::read(&mut client, &budget).await.unwrap().decode::<Result<Welcome, SpawnDiagnostic>>().unwrap().unwrap();
        let mut seen = Vec::new();
        while !seen.iter().any(|(kind, _)| kind == "child") || !seen.iter().any(|(kind, _)| kind == "parent-1") {
            seen.push(requests.recv().await.unwrap());
        }
        assert_eq!(seen.len(), 3, "no unrelated auxiliary generation in this fixture");
        assert_eq!(owner.resources.shell.running(&session.0).unwrap().len(), 1);
        assert!(matches!(owner.db.child_tool_jobs(&session.0).unwrap()[0].state,
            oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running));
        send_client(&mut client, &budget, Client::Request {
            id: 20, location_epoch: 0, expected: None,
            request: Box::new(Request::CompactSession { session: session.clone() }),
        }).await;
        let (id, active) = next_reply(&mut client, &budget).await;
        assert_eq!(id, 20);
        let active = active.and_then(|value| serde_json::from_value::<Result<CompactionSnapshot, CoreError>>(value).unwrap());
        eprintln!("incident native compact while shell+child+root active: {active:?}");
        parent_release.notify_one();
        // Still exercise completion delivery when the busy-ingress defect
        // refuses the first command; assert that refusal only after cleanup.
        let compact = if let Ok(snapshot) = &active {
            snapshot.clone()
        } else {
            stage = "initial parent settlement";
            loop {
                let event = match events.recv().await.unwrap() { CoreEvent::Projection { event, .. } => *event, event => event };
                if matches!(event, CoreEvent::TurnFinished { turn: ref id, .. } if *id == turn) { break; }
                if matches!(event, CoreEvent::TurnFailed { .. }) { panic!("initial parent failed: {event:?}"); }
            }
            owner.next_root().await.unwrap();
            assert!(owner.root(&session.0).is_none());
            stage = "idle compact acknowledgement";
            send_client(&mut client, &budget, Client::Request {
                id: 21, location_epoch: 0, expected: None,
                request: Box::new(Request::CompactSession { session: session.clone() }),
            }).await;
            let (_, idle) = next_reply(&mut client, &budget).await;
            serde_json::from_value::<Result<CompactionSnapshot, CoreError>>(idle.unwrap()).unwrap().unwrap()
        };
        assert_eq!(compact.state, CompactionState::Queued);
        stage = "summary request";
        loop {
            let request = requests.recv().await.unwrap();
            let summary = request.0 == "summary";
            seen.push(request);
            if summary { break; }
        }
        std::fs::write(project.join("release-shell"), "").unwrap();
        stage = "shell notice during summary";
        let shell_notice = loop {
            let event = match events.recv().await.unwrap() { CoreEvent::Projection { event, .. } => *event, event => event };
            if let CoreEvent::ShellNotice(notice) = event { break notice; }
        };
        assert_eq!(shell_notice.state, "completed");
        assert!(!owner.db.parent_notice_assigned(&session.0, &shell_notice.message_id).unwrap(), "summary in flight cannot consume the new notice");
        summary_release.notify_one();
        stage = "compaction completion";
        loop {
            let event = match events.recv().await.unwrap() { CoreEvent::Projection { event, .. } => *event, event => event };
            if let CoreEvent::Compaction(snapshot) = event
                && snapshot.id == compact.id && snapshot.state == CompactionState::Completed { break; }
        }
        stage = "compaction root release";
        owner.next_root().await.unwrap();
        child_release.notify_one();
        stage = "post-compaction parent continuations";
        let mut child_notice = None;
        loop {
            let event = match events.recv().await.unwrap() { CoreEvent::Projection { event, .. } => *event, event => event };
            match event {
                CoreEvent::ChildNotice(notice) => child_notice = Some(notice),
                CoreEvent::TurnFinished { session: completed_session, turn: id, .. } if completed_session == session && id != turn => {
                    owner.next_root().await.unwrap();
                    if let Some(notice) = &child_notice
                        && owner.db.parent_notice_assigned(&session.0, notice.job.message_id.as_deref().unwrap()).unwrap()
                        && owner.db.parent_notice_assigned(&session.0, &shell_notice.message_id).unwrap()
                        && owner.root(&session.0).is_none()
                    { break; }
                }
                CoreEvent::ParentContinuationRefused { diagnostic, .. } => panic!("continuation refused: {diagnostic}"),
                CoreEvent::TurnFailed { error, .. } => panic!("continuation failed: {error}"),
                _ => {}
            }
        }
        let child_notice = child_notice.unwrap();
        assert_eq!(child_notice.job.state, oc_core::queries::ChildState::Completed);
        assert!(owner.db.parent_notice_assigned(&session.0, &shell_notice.message_id).unwrap());
        assert!(owner.db.parent_notice_assigned(&session.0, child_notice.job.message_id.as_deref().unwrap()).unwrap());
        while let Ok(request) = requests.try_recv() { seen.push(request); }
        assert_eq!(seen.iter().filter(|(kind, _)| kind == "child").count(), 1);
        assert_eq!(owner.db.list_tool_ops(&session.0).unwrap().len(), 2);
        assert_eq!(std::fs::read(project.join("effect")).unwrap(), b"x");
        for marker in ["SHELL_COMPLETED_ONCE", "CHILD_COMPLETED_ONCE"] {
            assert!(seen.iter().filter(|(kind, _)| kind.starts_with("parent-")).any(|(_, request)| request["input"].to_string().contains(marker)), "result was never delivered: {marker}");
        }
        eprintln!("incident manual compact: shell completed during summary, child after summary; both assigned; one shell effect and one child request");
        active
    }).await;
    if outcome.is_err() {
        while let Ok((kind, _)) = requests.try_recv() {
            eprintln!("incident pending request: {kind}");
        }
        eprintln!(
            "incident stalled at {stage}; root_active={}; compactions={:?}; shell_running={}; child_states={:?}",
            owner.root(&session.0).is_some(),
            owner
                .db
                .compaction_history(&session.0)
                .unwrap()
                .iter()
                .map(|snapshot| (&snapshot.state, &snapshot.error))
                .collect::<Vec<_>>(),
            owner.resources.shell.running(&session.0).unwrap().len(),
            owner
                .db
                .child_tool_jobs(&session.0)
                .unwrap()
                .iter()
                .map(|job| &job.state)
                .collect::<Vec<_>>()
        );
    }
    parent_release.notify_one();
    child_release.notify_one();
    summary_release.notify_one();
    std::fs::write(project.join("release-shell"), "").unwrap();
    drop(client);
    assert!(connection.await.unwrap().is_none());
    owner.shutdown().await.unwrap();
    peer.abort();
    let _ = peer.await;
    assert!(
        outcome
            .expect("native compact lifecycle finishes within 20 seconds")
            .is_ok(),
        "native /compact must queue on the active root, not return TurnBusy"
    );
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
    let endpoint = Endpoint::bind(&data).await.unwrap();
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
    let endpoint = Endpoint::bind(&data).await.unwrap();
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
