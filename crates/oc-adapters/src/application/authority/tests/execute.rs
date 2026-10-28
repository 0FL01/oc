//! Real owner, provider and MCP exercise the detached program and wakeup path.
use super::*;

const MCP: &str = r#"import json, os, sys, time
for line in sys.stdin:
    req = json.loads(line)
    method = req.get('method')
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'}}
    elif method == 'tools/list':
        result = {'tools':[{'name':'wait','description':'wait for release','inputSchema':{'type':'object'}}]}
    elif method == 'tools/call':
        with open(sys.argv[1], 'a') as f: f.write('effect\n')
        while not os.path.exists(sys.argv[2]): time.sleep(.01)
        result = {'content':[{'type':'text','text':'EXECUTE_REPORT_SENTINEL'}], 'isError':False}
    else:
        continue
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}), flush=True)
"#;

#[tokio::test]
async fn execute_background_reports_and_wakes_idle_parent_once() {
    episode(Case::Background).await;
}

#[tokio::test]
async fn execute_foreground_conversion_keeps_original_worker_and_wake() {
    episode(Case::Converted).await;
}

#[tokio::test]
async fn execute_nested_approval_survives_the_launch_turn() {
    episode(Case::Approval).await;
}

#[tokio::test]
async fn execute_live_receipt_survives_step_compression_and_wakes_parent() {
    episode(Case::Compression).await;
}

#[tokio::test]
async fn execute_notice_joins_a_busy_parent_without_a_second_root() {
    episode(Case::Busy).await;
}

#[tokio::test]
async fn execute_notice_waits_for_redo_after_undo() {
    episode(Case::UndoRedo).await;
}

#[tokio::test]
async fn execute_notice_before_branch_commit_is_retired() {
    episode(Case::UndoCommit).await;
}

#[tokio::test]
async fn execute_notice_after_branch_commit_wakes_new_branch() {
    episode(Case::UndoLate).await;
}

#[tokio::test]
async fn execute_deletion_after_delivery_releases_origin_without_wakeup() {
    episode(Case::Delete).await;
}

#[tokio::test]
async fn execute_session_cancellation_suppresses_the_wakeup() {
    episode(Case::SessionCancel).await;
}

#[tokio::test]
async fn execute_origin_cancellation_joins_inflight_mcp_without_replay() {
    episode(Case::OriginCancel).await;
}

#[tokio::test]
async fn shutdown_joins_a_detached_execute_before_closing_its_mcp() {
    episode(Case::Shutdown).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Background,
    Converted,
    Approval,
    Compression,
    Busy,
    UndoRedo,
    UndoCommit,
    UndoLate,
    Delete,
    SessionCancel,
    OriginCancel,
    Shutdown,
}

async fn episode(case: Case) {
    let convert = case == Case::Converted;
    let ask = case == Case::Approval;
    let busy = case == Case::Busy;
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let script = project.join("mcp.py");
    let effect = project.join("effect");
    let release = project.join("release");
    std::fs::write(&script, MCP).unwrap();
    let note = if case == Case::Compression {
        "COMPRESSIBLE_READ_RESULT ".repeat(200)
    } else {
        "independent work".into()
    };
    std::fs::write(project.join("note"), note).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ORIGINAL_PARENT_PROFILE"});
    source["agents"]["beta"] =
        json!({"system":"CURRENT_PARENT_PROFILE","model":"fixture/beta-model"});
    source["default_agent"] = json!("alpha");
    source["providers"]["fixture"]["models"]["beta-model"] =
        json!({"limit":{"context":100000,"output":2048}});
    source["permissions"] = json!([
        {"action":"execute","resource":"*","effect":"allow"},
        {"action":"fixture__wait","resource":"*","effect":if ask {"ask"} else {"allow"}},
        {"action":"read","resource":"*","effect":"allow"},
        {"action":"compress","resource":"*","effect":"allow"}
    ]);
    source["mcp"] = json!({"servers":{"fixture":{"type":"local",
        "command":["/usr/bin/python3",script,effect,release],
        "timeout":{"startup":3000,"catalog":3000,"execution":10000}}}});
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let (entered, mut requests) = mpsc::channel(4);
    let gate = Arc::new(tokio::sync::Notify::new());
    let peer_gate = gate.clone();
    let peer = tokio::spawn(async move {
        let count = match case {
            Case::Compression => 5,
            Case::UndoLate => 4,
            _ => 3,
        };
        for index in 0..count {
            let (mut socket, _) = listener.accept().await.unwrap();
            let body = request(&mut socket).await;
            entered.send(body.clone()).await.unwrap();
            let output = if index == 0 {
                json!([{"type":"function_call","id":"execute-item","call_id":"execute-call","name":"execute",
                    "arguments":json!({"code":"return await tools.fixture.wait({});","background":!convert}).to_string()}])
            } else if case == Case::Compression && index == 1 {
                json!([{"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"note\"}"}])
            } else if case == Case::Compression && index == 2 {
                let markers = body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|item| {
                        let text = item["content"].as_array()?.first()?["text"].as_str()?;
                        let id = text
                            .strip_prefix("<dcp-message-id>tassistant-")?
                            .strip_suffix("</dcp-message-id>")?;
                        (item["role"] == "developer").then(|| format!("tassistant-{id}"))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(markers.len(), 2);
                json!([{"type":"function_call","id":"compress-item","call_id":"compress-call","name":"compress",
                    "arguments":json!({"topic":"closed steps","content":[{"startId":markers[0],"endId":markers[1],"summary":"EXECUTE_STEPS_SUMMARY"}]}).to_string()}])
            } else if case == Case::Compression && index == 3 {
                let input = body["input"].as_array().unwrap();
                assert!(input.iter().any(
                    |item| item["type"] == "function_call" && item["call_id"] == "execute-call"
                ));
                assert!(input.iter().any(|item| {
                    item["type"] == "function_call_output"
                        && item["call_id"] == "execute-call"
                        && item["output"]
                            .as_str()
                            .is_some_and(|output| output.contains("running"))
                }));
                let text = body["input"].to_string();
                assert!(text.contains("EXECUTE_STEPS_SUMMARY"), "{text}");
                assert!(!text.contains("COMPRESSIBLE_READ_RESULT"), "{text}");
                assert!(!text.contains("was NOT applied"), "{text}");
                json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
            } else if index == 1 && busy {
                peer_gate.notified().await;
                json!([{"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"note\"}"}])
            } else {
                json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
            };
            respond(&mut socket, output).await;
        }
    });
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = location.app.mcp_status().await.unwrap();
            if snapshot
                .servers
                .iter()
                .any(|server| server.status == oc_core::queries::McpStatus::Connected)
            {
                break;
            }
            assert!(
                snapshot
                    .servers
                    .iter()
                    .all(|server| !matches!(server.status, oc_core::queries::McpStatus::Failed)),
                "fixture MCP failed: {snapshot:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture MCP did not connect");
    let session = SessionId("parent".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Parent".into())
        .await
        .unwrap();
    let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
    let running = owner
        .begin(
            location.clone(),
            session.0.clone(),
            origin.clone(),
            u8::from(ask),
            false,
            None,
        )
        .unwrap();
    let mut events = owner.events.subscribe();
    let first = running
        .app
        .submit(session.clone(), "ORIGINAL_TASK".into())
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let initial = requests.recv().await.unwrap();
        assert!(
            initial["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "execute")
        );
        if !ask {
            while !effect.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        let job = loop {
            if let Some(job) = location
                .app
                .execute_jobs(session.clone())
                .await
                .unwrap()
                .into_iter()
                .next()
            {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        assert!(
            owner
                .db
                .live_background_call_ids(&job.turn)
                .unwrap()
                .contains(&"execute-call".to_string())
        );
        assert!(
            owner
                .db
                .live_execute_call_ids("parent")
                .unwrap()
                .contains(&(job.turn.clone(), "execute-call".into()))
        );
        assert_eq!(job.background, !convert);
        if convert {
            assert!(
                tokio::time::timeout(Duration::from_millis(100), requests.recv())
                    .await
                    .is_err()
            );
            location
                .app
                .background_execute(session.clone(), job.turn.clone(), job.execute_id.clone())
                .await
                .unwrap();
        }
        let receipt = requests.recv().await.unwrap();
        let output = receipt["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| {
                item["type"] == "function_call_output" && item["call_id"] == "execute-call"
            })
            .and_then(|item| item["output"].as_str())
            .expect("execute receipt");
        let output: serde_json::Value = serde_json::from_str(output).unwrap();
        assert_eq!(output["status"], "running");
        assert_eq!(output["executeID"], job.execute_id);
        assert!(!release.exists());
        if case == Case::Compression {
            requests.recv().await.unwrap();
            requests.recv().await.unwrap();
        }
        if !busy {
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                if matches!(event, CoreEvent::TurnFinished { turn, .. } if turn == first) {
                    break;
                }
            }
            owner.next_root().await.unwrap();
            assert!(owner.root("parent").is_none());
        } else {
            assert!(owner.root("parent").is_some());
        }
        if case == Case::Shutdown {
            owner.shutdown().await.unwrap();
            assert!(owner.root("parent").is_none());
            assert_eq!(owner.db.tool_state(&job.execute_id).unwrap(), "completed");
            assert!(matches!(
                owner
                    .db
                    .execute_job_outcome("parent", &job.execute_id)
                    .unwrap()
                    .state
                    .as_str(),
                "cancelled" | "unknown"
            ));
            assert_eq!(std::fs::read_to_string(&effect).unwrap(), "effect\n");
            origin.settle().await;
            return;
        }
        if case == Case::Background {
            location
                .app
                .session_selection(
                    session.clone(),
                    false,
                    SessionSelectionAction::Agent("beta".into()),
                )
                .await
                .unwrap();
        }
        if ask {
            assert!(!effect.exists(), "Ask must not grant the nested call");
            let request = loop {
                if let Some(request) = location
                    .app
                    .pending_approvals()
                    .await
                    .unwrap()
                    .into_iter()
                    .next()
                {
                    break request;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            };
            assert_eq!(request.binding.operation, job.execute_id);
            location
                .app
                .reply_approval(oc_core::approval::ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: oc_core::approval::ApprovalDecision::Once,
                })
                .await
                .unwrap();
            while !effect.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        let mut blocked = if matches!(case, Case::UndoRedo | Case::UndoCommit | Case::UndoLate) {
            let blocked = owner.reserve_mutation("parent").unwrap();
            let undo = location
                .app
                .change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            assert!(undo.can_redo);
            Some(blocked)
        } else {
            None
        };
        if case == Case::Delete {
            blocked = Some(owner.reserve_mutation("parent").unwrap());
            assert!(location.app.delete_session(session.clone()).await.is_err());
        }
        if case == Case::UndoLate {
            drop(blocked.take());
            let replacement =
                replace_parent(&owner, &location, &session, &mut events, &mut requests).await;
            let input = replacement["input"].to_string();
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(!input.contains("ORIGINAL_TASK"));
            blocked = Some(owner.reserve_mutation("parent").unwrap());
        }
        if case == Case::SessionCancel {
            owner.cancel_session(&location, "parent", None).unwrap();
        }
        if case == Case::OriginCancel {
            owner.cancel_origin(&origin);
        } else {
            std::fs::write(&release, "").unwrap();
        }
        let notice = loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if let CoreEvent::ExecuteNotice(notice) = event {
                break notice;
            }
        };
        assert_eq!(notice.execute_id, job.execute_id);
        assert!(
            !owner
                .db
                .live_background_call_ids(&job.turn)
                .unwrap()
                .contains(&"execute-call".to_string())
        );
        assert!(owner.db.live_execute_call_ids("parent").unwrap().is_empty());
        if case == Case::Delete {
            location.app.delete_session(session.clone()).await.unwrap();
            drop(blocked.take());
            tokio::time::timeout(Duration::from_secs(3), origin.settle())
                .await
                .expect("deleted session retained execute handoff");
            assert!(owner.root("parent").is_none());
            assert_eq!(std::fs::read_to_string(&effect).unwrap(), "effect\n");
            return;
        }
        if matches!(case, Case::SessionCancel | Case::OriginCancel) {
            assert!(
                tokio::time::timeout(Duration::from_millis(100), requests.recv())
                    .await
                    .is_err()
            );
            assert!(owner.root("parent").is_none());
            assert!(
                !owner
                    .db
                    .parent_notice_assigned("parent", &notice.message_id)
                    .unwrap()
            );
            if case == Case::OriginCancel {
                let outcome = owner
                    .db
                    .execute_job_outcome("parent", &job.execute_id)
                    .unwrap();
                assert_eq!(outcome.state, "unknown");
                assert!(!release.exists());
                assert_eq!(std::fs::read_to_string(&effect).unwrap(), "effect\n");
            }
            tokio::time::timeout(Duration::from_secs(3), origin.settle())
                .await
                .expect("cancelled origin did not settle");
            return;
        }
        if case == Case::UndoRedo {
            assert!(owner.resources.execute.parent_batches().unwrap().is_empty());
            assert!(owner.root("parent").is_none());
            let redo = location
                .app
                .change_conversation(session.clone(), ConversationAction::Redo)
                .await
                .unwrap();
            assert!(!redo.can_redo);
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
        }
        if case == Case::UndoCommit {
            assert!(owner.resources.execute.parent_batches().unwrap().is_empty());
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
            assert!(owner.root("parent").is_none());
            let replacement =
                replace_parent(&owner, &location, &session, &mut events, &mut requests).await;
            let input = replacement["input"].to_string();
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(!input.contains("ORIGINAL_TASK"));
            assert!(!input.contains("EXECUTE_REPORT_SENTINEL"));
            owner.resources.execute.parent_batches().unwrap();
            tokio::time::timeout(Duration::from_secs(3), origin.settle())
                .await
                .expect("retired origin did not settle");
            assert!(
                !owner
                    .db
                    .parent_notice_assigned("parent", &notice.message_id)
                    .unwrap()
            );
            assert_eq!(std::fs::read_to_string(&effect).unwrap(), "effect\n");
            return;
        }
        if case == Case::UndoLate {
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
        }
        if busy {
            gate.notify_one();
        }
        let resumed = requests.recv().await.unwrap();
        let input = resumed["input"].to_string();
        assert_eq!(
            input.matches("EXECUTE_REPORT_SENTINEL").count(),
            1,
            "{input}"
        );
        if case == Case::UndoLate {
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(!input.contains("ORIGINAL_TASK"));
        } else {
            assert!(input.contains("ORIGINAL_TASK"));
        }
        if case == Case::Background {
            assert_eq!(resumed["model"], "beta-model");
            assert!(input.contains("CURRENT_PARENT_PROFILE"));
        }
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if matches!(event, CoreEvent::TurnFinished { turn, .. } if busy || turn != first) {
                break;
            }
        }
        owner.next_root().await.unwrap();
        origin.settle().await;
        assert!(
            owner
                .db
                .parent_notice_assigned("parent", &notice.message_id)
                .unwrap()
        );
        let snapshot = location
            .app
            .execute_snapshot(session.clone(), job.execute_id)
            .await
            .unwrap();
        assert_eq!(snapshot.state, "completed");
        assert_eq!(
            snapshot.message_id.as_deref(),
            Some(notice.message_id.as_str())
        );
        assert_eq!(std::fs::read_to_string(&effect).unwrap(), "effect\n");
    })
    .await;
    if case != Case::Shutdown {
        owner.shutdown().await.unwrap();
    }
    peer.abort();
    let _ = peer.await;
    result.unwrap();
}
