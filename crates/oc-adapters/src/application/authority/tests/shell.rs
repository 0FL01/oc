use super::*;

#[tokio::test]
async fn idle_shell_report_wakes_once_with_current_parent_selection() {
    episode(Case::Idle).await;
}

#[tokio::test]
async fn converted_shell_keeps_original_parent_wake() {
    episode(Case::Converted).await;
}

#[tokio::test]
async fn idle_session_cancel_suppresses_completed_shell_handoff() {
    episode(Case::SessionCancel).await;
}

#[tokio::test]
async fn origin_release_suppresses_completed_shell_handoff() {
    episode(Case::OriginCancel).await;
}

#[tokio::test]
async fn busy_parent_consumes_shell_report_without_an_extra_root() {
    episode(Case::Busy).await;
}

#[tokio::test]
async fn active_undo_holds_shell_report_until_redo_without_replaying_shell() {
    episode(Case::UndoRedo).await;
}

#[tokio::test]
async fn undo_new_prompt_discards_already_queued_shell_report() {
    episode(Case::UndoCommit).await;
}

#[tokio::test]
async fn undo_new_prompt_keeps_shell_that_completes_after_branch_commit() {
    episode(Case::UndoLate).await;
}

#[tokio::test]
async fn worker_shutdown_still_suppresses_detached_shell_handoff() {
    episode(Case::WorkerShutdown).await;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Idle,
    Converted,
    SessionCancel,
    OriginCancel,
    Busy,
    UndoRedo,
    UndoCommit,
    UndoLate,
    WorkerShutdown,
}

async fn episode(case: Case) {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("report"), "SHELL_REPORT_SENTINEL").unwrap();
    std::fs::write(project.join("note"), "independent work").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ORIGINAL_PARENT_PROFILE"});
    source["agents"]["beta"] =
        json!({"system":"CURRENT_PARENT_PROFILE","model":"fixture/beta-model"});
    source["default_agent"] = json!("alpha");
    source["providers"]["fixture"]["models"]["beta-model"] =
        json!({"limit":{"context":100000,"output":2048}});
    source["permissions"] = json!([
        {"action":"shell","resource":"*","effect":"allow"},
        {"action":"read","resource":"*","effect":"allow"}
    ]);
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let continue_busy = Arc::new(tokio::sync::Notify::new());
    let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (entered, mut requests) = mpsc::channel(4);
    let peer = {
        let continue_busy = continue_busy.clone();
        let posts = posts.clone();
        tokio::spawn(async move {
            let mut held = Vec::new();
            for index in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let body = request(&mut socket).await;
                posts.fetch_add(1, Ordering::AcqRel);
                entered.send(body).await.unwrap();
                if index == 1 && matches!(case, Case::UndoRedo | Case::WorkerShutdown) {
                    held.push(socket);
                    continue;
                }
                let output = if index == 0 {
                    json!([{"type":"function_call","id":"shell-item","call_id":"shell-launch","name":"shell","arguments":json!({
                        "command":"touch entered; while [ ! -f release ]; do sleep .01; done; printf x >> effects; cat report",
                        "background":case != Case::Converted
                    }).to_string()}])
                } else if index == 1 && case == Case::Busy {
                    continue_busy.notified().await;
                    json!([{"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"note\"}"}])
                } else {
                    json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
                };
                respond(&mut socket, output).await;
            }
        })
    };
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("parent".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Named parent".into())
        .await
        .unwrap();
    let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
    let running = owner
        .begin(
            location.clone(),
            session.0.clone(),
            origin.clone(),
            0,
            false,
            None,
        )
        .unwrap();
    let mut events = owner.events.subscribe();
    let first = running
        .app
        .submit(session.clone(), "ORIGINAL_PARENT_TASK".into())
        .await
        .unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(5), async {
        while !project.join("entered").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let job = owner.resources.shell.running("parent").unwrap().remove(0);
        if case == Case::Converted {
            assert!(!job.background);
            assert!(
                owner
                    .resources
                    .shell
                    .background("parent", &job.shell_id)
                    .unwrap()
            );
        }
        let mut seen = vec![
            requests.recv().await.unwrap(),
            requests.recv().await.unwrap(),
        ];
        let mut automatic = None;
        if case == Case::UndoRedo {
            let undone = running
                .app
                .change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            assert!(undone.can_redo);
            assert_eq!(undone.draft.as_deref(), Some("ORIGINAL_PARENT_TASK"));
            assert!(origin.foreground.load(Ordering::Acquire));
        } else if case == Case::WorkerShutdown {
            running.app.shutdown().await.unwrap();
        }
        if case != Case::Busy {
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                if match event {
                    CoreEvent::TurnInterrupted { turn, .. }
                        if matches!(case, Case::UndoRedo | Case::WorkerShutdown) =>
                    {
                        turn == first
                    }
                    CoreEvent::TurnFinished { turn, .. }
                        if !matches!(case, Case::UndoRedo | Case::WorkerShutdown) =>
                    {
                        turn == first
                    }
                    _ => false,
                } {
                    break;
                }
            }
            owner.next_root().await.unwrap();
            assert!(
                owner.root("parent").is_none(),
                "must be idle before shell completes"
            );
            assert_eq!(
                owner.resources.shell.running("parent").unwrap()[0].shell_id,
                job.shell_id
            );
            if matches!(case, Case::UndoCommit | Case::UndoLate) {
                let blocked = owner.reserve_mutation("parent").unwrap();
                location
                    .app
                    .change_conversation(session.clone(), ConversationAction::Undo)
                    .await
                    .unwrap();
                drop(blocked);
            }
            location
                .app
                .session_selection(
                    session.clone(),
                    false,
                    SessionSelectionAction::Agent("beta".into()),
                )
                .await
                .unwrap();
        } else {
            assert!(owner.root("parent").is_some());
        }
        let mut blocked = (case != Case::Busy).then(|| owner.reserve_mutation("parent").unwrap());
        if case == Case::UndoLate {
            drop(blocked.take());
            seen.push(
                replace_parent(&owner, &location, &session, &mut events, &mut requests).await,
            );
            blocked = Some(owner.reserve_mutation("parent").unwrap());
        }
        std::fs::write(project.join("release"), "").unwrap();
        let notice = loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if let CoreEvent::ShellNotice(notice) = event {
                break notice;
            }
        };
        assert_eq!(notice.shell_id, job.shell_id);
        if case == Case::Busy {
            continue_busy.notify_one();
        } else if matches!(case, Case::UndoRedo | Case::UndoCommit) {
            assert!(owner.resources.shell.parent_batches().unwrap().is_empty());
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
            assert!(
                owner.root("parent").is_none(),
                "staged Undo must not start or refuse a continuation"
            );
            assert_eq!(posts.load(Ordering::Acquire), 2);
            if case == Case::UndoRedo {
                blocked = Some(owner.reserve_mutation("parent").unwrap());
                let redo = location
                    .app
                    .change_conversation(session.clone(), ConversationAction::Redo)
                    .await
                    .unwrap();
                assert!(!redo.can_redo && redo.reverted.is_none());
            } else {
                seen.push(
                    replace_parent(&owner, &location, &session, &mut events, &mut requests).await,
                );
                owner.resources.shell.parent_batches().unwrap();
            }
        } else if case == Case::WorkerShutdown {
            assert!(owner.resources.shell.parent_batches().unwrap().is_empty());
        } else {
            // A reaped handoff, not a running process, must keep finite-run
            // settlement open until admission or explicit suppression.
            while owner.resources.shell.parent_batches().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let settle = origin.settle();
            tokio::pin!(settle);
            assert!(futures_util::poll!(&mut settle).is_pending());
            match case {
                Case::SessionCancel => owner.cancel_session(&location, "parent", None).unwrap(),
                Case::OriginCancel => owner.cancel_origin(&origin),
                _ => {}
            }
        }
        drop(blocked);
        let suppressed = matches!(
            case,
            Case::SessionCancel | Case::OriginCancel | Case::UndoCommit | Case::WorkerShutdown
        );
        if !suppressed {
            seen.push(requests.recv().await.unwrap());
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                if let CoreEvent::TurnFinished { turn, .. } = event
                    && (case == Case::Busy || turn != first)
                {
                    automatic = Some(turn);
                    break;
                }
            }
            owner.next_root().await.unwrap();
        }
        origin.settle().await;
        assert!(owner.root("parent").is_none());
        (seen, notice, automatic)
    })
    .await;
    owner.shutdown().await.unwrap();
    peer.abort();
    let _ = peer.await;
    let (seen, notice, automatic) = completed.unwrap_or_else(|error| {
        panic!("{case:?}: {error}; posts={}", posts.load(Ordering::Acquire))
    });
    while let Ok(event) = events.try_recv() {
        let event = match event {
            CoreEvent::Projection { event, .. } => *event,
            event => event,
        };
        assert!(!matches!(
            event,
            CoreEvent::ParentContinuationRefused { .. }
        ));
    }
    let suppressed = matches!(
        case,
        Case::SessionCancel | Case::OriginCancel | Case::UndoCommit | Case::WorkerShutdown
    );
    assert_eq!(
        posts.load(Ordering::Acquire),
        match case {
            Case::UndoCommit => 3,
            Case::UndoLate => 4,
            _ if suppressed => 2,
            _ => 3,
        }
    );
    assert_eq!(
        std::fs::read(project.join("effects")).unwrap(),
        b"x",
        "no shell replay"
    );
    assert_eq!(
        owner
            .db
            .parent_notice_assigned("parent", &notice.message_id)
            .unwrap(),
        !suppressed
    );
    let history = owner.db.read_history_full("parent").unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|(_, role, text)| role == "user" && text.is_empty())
            .count(),
        usize::from(!suppressed && case != Case::Busy)
    );
    assert!(
        owner
            .db
            .change_conversation(
                "parent",
                oc_core::queries::ConversationAction::Revert {
                    message: oc_core::session::MessageId(notice.message_id.clone())
                }
            )
            .is_err(),
        "native data is not a human prompt boundary"
    );
    if suppressed {
        if case == Case::UndoCommit {
            let input = seen.last().unwrap()["input"].to_string();
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(
                !input.contains("SHELL_REPORT_SENTINEL") && !input.contains("ORIGINAL_PARENT_TASK")
            );
        }
        return;
    }
    let request = seen.last().unwrap();
    let input = request["input"].to_string();
    assert_eq!(input.matches("SHELL_REPORT_SENTINEL").count(), 1, "{input}");
    assert!(input.contains(if case == Case::UndoLate {
        "REPLACEMENT_TASK"
    } else {
        "ORIGINAL_PARENT_TASK"
    }));
    if case == Case::UndoLate {
        assert!(!input.contains("ORIGINAL_PARENT_TASK"));
    }
    if case != Case::Busy {
        assert_eq!(request["model"], "beta-model");
        assert!(input.contains("CURRENT_PARENT_PROFILE"));
        let (_, result) = owner.db.turn_result(&automatic.unwrap().0).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(saved["native_input"]["kind"], "shell_notices");
        assert_eq!(
            saved["native_input"]["messages"],
            json!([notice.message_id])
        );
    }
}

#[tokio::test]
async fn advertised_dcp_range_does_not_strand_large_context() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["providers"]["fixture"]["models"]["m"]["limit"] =
        json!({"context":500000,"input":450000,"output":128000});
    source["compaction"]["auto"] = json!(true);
    source["dcp"] = json!({"compress":{
        "permission":"allow", "minContextLimit":270000, "maxContextLimit":350000
    }});
    source["permissions"] = json!([
        {"action":"shell","resource":"*","effect":"allow"},
        {"action":"compress","resource":"*","effect":"allow"}
    ]);
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peer = {
        let posts = posts.clone();
        tokio::spawn(async move {
            for index in 0..9 {
                let (mut socket, _) = listener.accept().await.unwrap();
                // This incident-sized history exceeds the shared small-peer
                // helper's 1 MiB request bound. Keep this peer bounded too.
                let body: serde_json::Value = {
                    let mut reader = tokio::io::BufReader::new(&mut socket);
                    let mut length = None;
                    let mut header_bytes = 0;
                    loop {
                        let mut line = String::new();
                        assert!(reader.read_line(&mut line).await.unwrap() > 0);
                        header_bytes += line.len();
                        assert!(header_bytes <= 16 * 1024);
                        if line == "\r\n" {
                            break;
                        }
                        if let Some(value) =
                            line.to_ascii_lowercase().strip_prefix("content-length:")
                        {
                            length = Some(value.trim().parse::<usize>().unwrap());
                        }
                    }
                    let length = length.unwrap();
                    assert!(length <= 2 * 1024 * 1024);
                    let mut bytes = vec![0; length];
                    reader.read_exact(&mut bytes).await.unwrap();
                    serde_json::from_slice(&bytes).unwrap()
                };
                posts.fetch_add(1, Ordering::AcqRel);
                let mut output = match index {
                    2 => {
                        let anchors: serde_json::Value = body["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .rev()
                            .flat_map(|item| item["content"].as_array().into_iter().flatten())
                            .filter_map(|part| part["text"].as_str())
                            .find_map(|text| text.strip_prefix("DCP context anchors in order. Compress only closed=true spans (except current-task renewal); the final anchor is unfinished: "))
                            .map(|text| serde_json::from_str(text).unwrap())
                            .expect("provider-visible DCP anchors");
                        let closed = anchors
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|anchor| anchor["closed"] == true)
                            .collect::<Vec<_>>();
                        let first = closed.first().unwrap();
                        let last = closed
                            .iter()
                            .rev()
                            .find(|anchor| anchor["role"] == "assistant")
                            .unwrap();
                        eprintln!(
                            "replayed advertised compress range: {}..{} (start role={})",
                            first["id"], last["id"], first["role"]
                        );
                        json!([{"type":"function_call","id":"compress-item","call_id":"compress-call","name":"compress","arguments":json!({
                        "topic":"closed eval history", "content":[{"startId":first["id"],"endId":last["id"],"summary":"The closed synthetic seed turns completed. Continue the current task."}]
                    }).to_string()}])
                    }
                    3 => {
                        json!([{"type":"function_call","id":"failed-item","call_id":"failed-call","name":"shell","arguments":json!({
                        "command":"printf SYNTHETIC_FOREGROUND_FAILURE; exit 101", "timeout":120000
                    }).to_string()}])
                    }
                    4 => {
                        json!([{"type":"function_call","id":"passed-item","call_id":"passed-call","name":"shell","arguments":json!({
                        "command":"printf SYNTHETIC_FOREGROUND_SUCCESS", "timeout":600000
                    }).to_string()}])
                    }
                    5 => {
                        json!([{"type":"function_call","id":"background-item","call_id":"background-call","name":"shell","arguments":json!({
                        "command":"n=0; while [ ! -f release ] && [ $n -lt 1000 ]; do n=$((n+1)); sleep .01; done; [ -f release ] || exit 124; printf x >> effects; printf SYNTHETIC_BACKGROUND_FAILURE; exit 101",
                        "background":true, "timeout":0
                    }).to_string()}])
                    }
                    _ => {
                        if index == 7 {
                            assert!(
                                body["input"]
                                    .to_string()
                                    .contains("SYNTHETIC_BACKGROUND_FAILURE")
                            );
                        }
                        if index == 8 {
                            assert!(body["input"].to_string().contains("continue"));
                        }
                        json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"CONTROL_SETTLED"}]}])
                    }
                };
                // The real journal retained about 604k bytes of encrypted
                // reasoning in the current profile. These opaque response
                // items preserve that workload class without private payloads.
                if matches!(index, 1 | 5) {
                    output.as_array_mut().unwrap().insert(
                        0,
                        json!({
                            "type":"reasoning", "id":format!("reasoning-{index}"),
                            "summary":[], "content":[],
                            "encrypted_content":"x".repeat(if index == 1 {600_000} else {100_000})
                        }),
                    );
                }
                let input_tokens = if index == 0 { 175_000 } else { 306_105 };
                let body = format!(
                    "data: {}\n\n",
                    json!({"type":"response.completed", "response":{
                        "status":"completed", "output":output,
                        "usage":{"input_tokens":input_tokens,"output_tokens":1942,"total_tokens":input_tokens+1942}
                    }})
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        })
    };
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("parent".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Reproduction".into())
        .await
        .unwrap();
    // The copied incident starts with a raw system row before its first user
    // row. Keep that reachable historical shape inside the sole fixture owner.
    owner
        .db
        .append_message(&session.0, "system", "Replayed instruction prefix")
        .unwrap();
    let mut events = owner.events.subscribe();
    let completed = tokio::time::timeout(Duration::from_secs(20), async {
        // Both prompts pass the real public input limit; their combined
        // history reaches the incident's 270k..350k DCP band without seeding Db.
        for prompt in ["a".repeat(700_000), "b".repeat(400_000)] {
            let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
            let running = owner
                .begin(location.clone(), session.0.clone(), origin, 0, false, None)
                .unwrap();
            let turn = running.app.submit(session.clone(), prompt).await.unwrap();
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                match event {
                    CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
                    CoreEvent::TurnFailed { error, .. } => panic!("seed failed: {error}"),
                    _ => {}
                }
            }
            owner.next_root().await.unwrap();
        }
        let facts = location.app.dcp_snapshot(session.clone()).await.unwrap();
        assert!((270_000..350_000).contains(&facts.estimated_tokens));
        eprintln!(
            "reproduction projected_tokens={}; context=500000; output_capacity=128000",
            facts.estimated_tokens
        );
        let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
        let running = owner
            .begin(
                location.clone(),
                session.0.clone(),
                origin.clone(),
                0,
                false,
                None,
            )
            .unwrap();
        let first = running
            .app
            .submit(session.clone(), "run the synthetic checks".into())
            .await
            .unwrap();
        let mut shell_id = None;
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            match event {
                CoreEvent::ToolCallFinished { name, state, output, .. } if name == "compress" => {
                    eprintln!("replayed compress result: state={state}; {output}");
                }
                CoreEvent::ToolCallFinished {
                    name,
                    op,
                    state,
                    output,
                    ..
                } if name == "shell" => {
                    if op.ends_with("-failed-call") {
                        assert_eq!(state, "failed");
                        assert!(output.contains("SYNTHETIC_FOREGROUND_FAILURE"), "{output}");
                    } else if op.ends_with("-passed-call") {
                        assert_eq!(state, "completed");
                        assert!(output.contains("SYNTHETIC_FOREGROUND_SUCCESS"), "{output}");
                    } else {
                        assert!(op.ends_with("-background-call"), "{op}");
                        assert_eq!(state, "completed");
                        // Only the background launch receipt is JSON.
                        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
                        assert_eq!(value["status"], "running");
                        shell_id = Some(value["shellID"].as_str().unwrap().to_owned());
                    }
                }
                CoreEvent::TurnFailed { turn, .. } | CoreEvent::TurnFinished { turn, .. }
                    if turn == first => break,
                _ => {}
            }
        }
        owner.next_root().await.unwrap();
        let (status, raw) = owner.db.turn_result(&first.0).unwrap();
        let saved: serde_json::Value = serde_json::from_str(raw.as_deref().unwrap()).unwrap();
        if status == "failed" {
            let retry_origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
            let retry = owner
                .begin(location.clone(), session.0.clone(), retry_origin, 0, false, None)
                .unwrap();
            let receipt = retry.app.submit(session.clone(), "continue".into()).await;
            return Err(format!(
                "native replay stranded the session: terminal={}; next submit={receipt:?}; physical requests={}",
                saved["display"]["terminal_diagnostic"],
                posts.load(Ordering::Acquire)
            ));
        }
        assert_eq!(status, "completed");
        assert_eq!(
            posts.load(Ordering::Acquire),
            7,
            "unexpected native replay request sequence: {}",
            saved["display"]["terminal_diagnostic"]
        );
        let shell_id = shell_id.expect("background receipt");
        assert!(owner.root(&session.0).is_none());
        std::fs::write(project.join("release"), "").unwrap();
        let notice = loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if let CoreEvent::ShellNotice(notice) = event {
                break notice;
            }
        };
        assert_eq!(notice.shell_id, shell_id);
        let snapshot = owner
            .resources
            .shell
            .snapshot(&session.0, &shell_id)
            .unwrap();
        assert_eq!(snapshot.state, "failed");
        assert_eq!(snapshot.exit, Some(101));
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            match event {
                CoreEvent::TurnFinished { turn, .. } if turn != first => break,
                CoreEvent::TurnFailed { error, .. } => panic!("shell handoff failed: {error}"),
                CoreEvent::ParentContinuationRefused { diagnostic, .. } => {
                    panic!("shell handoff refused: {diagnostic}")
                }
                _ => {}
            }
        }
        owner.next_root().await.unwrap();
        origin.settle().await;
        let next_origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
        let next = owner
            .begin(
                location.clone(),
                session.0.clone(),
                next_origin,
                0,
                false,
                None,
            )
            .unwrap();
        let receipt = next.app.submit(session.clone(), "continue".into()).await;
        eprintln!("next submit after failed parent/background exit 101: {receipt:?}");
        let turn = receipt.expect("the same session must admit its next short request");
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            match event {
                CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
                CoreEvent::TurnFailed { error, .. } => panic!("next turn failed: {error}"),
                _ => {}
            }
        }
        owner.next_root().await.unwrap();
        assert!(owner.root(&session.0).is_none());
        assert!(
            owner
                .resources
                .shell
                .running(&session.0)
                .unwrap()
                .is_empty()
        );
        assert_eq!(owner.db.dcp_block_count(&session.0).unwrap(), 1);
        assert_eq!(
            std::fs::read(project.join("effects")).unwrap(),
            b"x",
            "no shell replay"
        );
        assert_eq!(posts.load(Ordering::Acquire), 9);
        Ok(())
    })
    .await;
    owner.shutdown().await.unwrap();
    peer.abort();
    let _ = peer.await;
    completed
        .expect("bounded native-owner reproduction")
        .expect("advertised closed DCP spans must be usable before admission is exhausted");
}
