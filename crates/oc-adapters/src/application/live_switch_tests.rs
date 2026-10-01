use super::*;
use oc_core::queries::SessionSelectionAction as Action;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn tool12_busy_owner_commit_reloads_next_request_without_another_prompt() {
    for ask in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        std::fs::write(project.join("opencode.json"),serde_json::json!({
        "model":"fixture/gpt-fixture", "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":base,"apiKey":"dummy"},
        "models":{"gpt-fixture":{"limit":{"context":100000,"output":10000}},"text-fixture":{"limit":{"context":50000,"output":2000}}}}},
        "permission":{"*":"allow","apply_patch":if ask {"ask"} else {"allow"}}
    }).to_string()).unwrap();
        let (arrived, arrival) = tokio::sync::oneshot::channel();
        let (release, gate) = tokio::sync::oneshot::channel();
        let (second, second_arrival) = tokio::sync::oneshot::channel();
        let (release_second, second_gate) = tokio::sync::oneshot::channel();
        let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let posted = posts.clone();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            let mut arrived = Some(arrived);
            let mut gate = Some(gate);
            let mut second = Some(second);
            let mut second_gate = Some(second_gate);
            for step in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                let boundary = loop {
                    let n = stream.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..boundary]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                assert!(length <= 1_048_576);
                while bytes.len() < boundary + length {
                    let n = stream.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                requests.push(
                    serde_json::from_slice::<serde_json::Value>(
                        &bytes[boundary..boundary + length],
                    )
                    .unwrap(),
                );
                posted.fetch_add(1, Ordering::SeqCst);
                if step == 0 {
                    arrived.take().unwrap().send(()).unwrap();
                    gate.take().unwrap().await.unwrap();
                }
                if step == 1 {
                    second.take().unwrap().send(()).unwrap();
                    second_gate.take().unwrap().await.unwrap();
                }
                let output = if step == 0 {
                    serde_json::json!([{"type":"reasoning","id":"a-reasoning","encrypted_content":"A-opaque","summary":[]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"A prepared"}]},{"type":"function_call","id":"a-item","call_id":"a-call","name":"apply_patch","arguments":serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: file\n+once\n*** End Patch"}).to_string()}])
                } else if step == 1 {
                    serde_json::json!([{"type":"reasoning","id":"b-reasoning","encrypted_content":"B-opaque","summary":[]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"B prepared"}]},{"type":"function_call","id":"b-item","call_id":"b-call","name":"edit","arguments":serde_json::json!({"path":"file","oldString":"once","newString":"twice"}).to_string()},{"type":"function_call","id":"b-job-item","call_id":"b-job","name":"shell","arguments":serde_json::json!({"command":"printf effect > background-provenance","background":true}).to_string()}])
                } else {
                    serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}])
                };
                let event = serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}});
                let body = format!("data: {event}\n\n");
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        let env = BTreeMap::from([
            (
                "HOME".into(),
                root.path().join("home").to_string_lossy().into_owned(),
            ),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        if ask {
            app.register_approval_consumer(false).await.unwrap();
        }
        let session = SessionId::new("busy-switch").unwrap();
        app.create_session(session.clone()).await.unwrap();
        app.rename_session(session.clone(), "fixed".into())
            .await
            .unwrap();
        let mut events = app.subscribe();
        let _turn = app
            .submit(session.clone(), "one task".into())
            .await
            .unwrap();
        arrival.await.unwrap();
        let mut release = Some(release);
        let approval = if ask {
            release.take().unwrap().send(()).unwrap();
            Some(next_approval(&app).await)
        } else {
            None
        };
        let committed = app
            .session_selection(session.clone(), false, Action::Model("text-fixture".into()))
            .await;
        if let Some(approval) = approval {
            assert_eq!(posts.load(Ordering::SeqCst), 1);
            assert!(!project.join("file").exists());
            assert_eq!(approval.resources, ["file"]);
            app.reply_approval(oc_core::approval::ApprovalReply {
                id: approval.id,
                binding: approval.binding,
                decision: oc_core::approval::ApprovalDecision::Once,
            })
            .await
            .unwrap();
        } else {
            release.take().unwrap().send(()).unwrap();
        }
        second_arrival.await.unwrap();
        let returned = app
            .session_selection(session.clone(), false, Action::Model("gpt-fixture".into()))
            .await
            .unwrap();
        assert_eq!(returned.model_id, "gpt-fixture");
        release_second.send(()).unwrap();
        if ask {
            let approval = next_approval(&app).await;
            assert_eq!(posts.load(Ordering::SeqCst), 2);
            assert_eq!(std::fs::read(project.join("file")).unwrap(), b"once\n");
            app.reply_approval(oc_core::approval::ApprovalReply {
                id: approval.id,
                binding: approval.binding,
                decision: oc_core::approval::ApprovalDecision::Once,
            })
            .await
            .unwrap();
        }
        let requests = server.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match events.recv().await.unwrap() {
                    CoreEvent::TurnFinished { .. } => break,
                    CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !project.join("background-provenance").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        assert_eq!(committed.unwrap().model_id, "text-fixture");
        assert_eq!(requests[0]["model"], "gpt-fixture");
        assert_eq!(requests[1]["model"], "text-fixture");
        assert_eq!(requests[2]["model"], "gpt-fixture");
        assert_eq!(std::fs::read(project.join("file")).unwrap(), b"twice\n");
        assert!(
            requests[1]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["call_id"] == "a-call" && i["type"] == "function_call_output")
        );
        assert!(!requests[1].to_string().contains("A-opaque"));
        assert!(!requests[2].to_string().contains("B-opaque"));
        assert!(
            requests[2]["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["call_id"] == "b-call" && i["type"] == "function_call_output")
        );
        let db = Db::open(&data).unwrap();
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        assert_eq!(
            std::fs::read(project.join("background-provenance")).unwrap(),
            b"effect"
        );
        let provenance: String = conn
            .query_row(
                "SELECT provenance ->> '$.model' FROM shell_jobs",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            provenance, "text-fixture",
            "issued B work remains B after A commits"
        );
        let raw: String = conn
            .query_row(
                "SELECT result FROM turns WHERE session_id='busy-switch' AND status='completed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
        assert_eq!(
            log.requests
                .iter()
                .map(|r| r.model.id.as_str())
                .collect::<Vec<_>>(),
            ["gpt-fixture", "text-fixture", "gpt-fixture"]
        );
        assert_eq!(
            log.requests
                .iter()
                .map(|r| r.output_limit)
                .collect::<Vec<_>>(),
            [4096, 2000, 4096]
        );
        assert_ne!(
            log.requests[0].tool_fingerprint,
            log.requests[1].tool_fingerprint
        );
        assert_eq!(
            log.requests[0].tool_fingerprint,
            log.requests[2].tool_fingerprint
        );
        let view = db
            .turn_presentation("busy-switch", &log.turn_id)
            .unwrap()
            .unwrap();
        assert_eq!(view.parts.len(), 6);
        assert!(view.part_states.iter().all(|p| p.model_label.is_some()));
    }
}

async fn next_approval(app: &CoreApp) -> oc_core::approval::ApprovalRequest {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(request) = app.pending_approvals().await.unwrap().into_iter().next() {
                return request;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn tool12_exact_commit_scope_noop_retired_variant_and_atomic_storage_refusal() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"),serde_json::json!({"model":"fixture/gpt-fixture","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"http://127.0.0.1:1/v1","apiKey":"synthetic"},"models":{"gpt-fixture":{},"text-fixture":{"variants":{"low":{"reasoningEffort":"low"}}}}}}}).to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(
        &project,
        &data,
        BTreeMap::from([
            (
                "HOME".into(),
                root.path().join("home").to_string_lossy().into_owned(),
            ),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let session = SessionId::new("scope").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let current = app
        .session_selection(session.clone(), false, Action::Current)
        .await
        .unwrap();
    let mut commit = oc_core::queries::ModelCommit {
        caller: 0,
        binding: oc_core::queries::SelectionBinding {
            location: current.chrome.location,
            generation: current.chrome.selection_generation,
            provider: current.provider,
            agent_id: current.agent_id,
        },
        model_id: "text-fixture".into(),
        variant: None,
        draft_revision: 1,
    };
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    app.session_selection(session.clone(), false, Action::Commit(commit.clone()))
        .await
        .unwrap();
    let snapshot: Vec<(String, String, String)> = conn
        .prepare("SELECT key,value,updated_at FROM prefs ORDER BY key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    commit.variant = Some("default".into());
    app.session_selection(session.clone(), false, Action::Commit(commit.clone()))
        .await
        .unwrap();
    for invalid in ["generation", "agent", "location", "variant", "model"] {
        let mut rejected = commit.clone();
        match invalid {
            "generation" => rejected.binding.generation += 1,
            "agent" => rejected.binding.agent_id = Some("other".into()),
            "location" => rejected.binding.location = Some("/other".into()),
            "variant" => rejected.variant = Some("retired".into()),
            _ => rejected.model_id = "retired".into(),
        };
        assert!(
            app.session_selection(session.clone(), false, Action::Commit(rejected))
                .await
                .is_err(),
            "{invalid}"
        );
    }
    conn.execute_batch("CREATE TRIGGER refuse_model BEFORE INSERT ON events WHEN NEW.kind='session_model_selected' BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    commit.model_id = "gpt-fixture".into();
    commit.variant = None;
    assert!(
        app.session_selection(session.clone(), false, Action::Commit(commit))
            .await
            .is_err()
    );
    let after: Vec<(String, String, String)> = conn
        .prepare("SELECT key,value,updated_at FROM prefs ORDER BY key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let counts:(i64,i64,i64)=conn.query_row("SELECT (SELECT count(*) FROM events WHERE kind='session_model_selected'),(SELECT count(*) FROM events WHERE kind='generation_dispatched'),(SELECT count(*) FROM messages)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(snapshot, after);
    assert_eq!(counts, (1, 0, 0));
}
