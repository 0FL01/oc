//! DCP12 explicit owner admission, immutable generations and real wire effects.
use super::*;
use oc_core::dcp_view::DcpUnavailable;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn config(base: &str) -> serde_json::Value {
    serde_json::json!({"model":"fixture/m", "compaction":{"auto":false},
        "permission":{"compress":"allow"}, "agent":{"title":{"disable":true}},
        "provider":{"fixture":{"options":{"baseURL":base,"apiKey":"synthetic"},
        "models":{"m":{"limit":{"context":100000,"output":2048}}}}}})
}

fn env(root: &std::path::Path) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "HOME".into(),
            root.join("home").to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ])
}

async fn settled(events: &mut tokio::sync::broadcast::Receiver<CoreEvent>) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let CoreEvent::TurnFinished { .. } = events.recv().await.unwrap() {
                break;
            }
        }
    })
    .await
    .unwrap();
}

async fn request(stream: &mut tokio::net::TcpStream) -> serde_json::Value {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let boundary = loop {
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() <= 1_048_576);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let length = String::from_utf8_lossy(&bytes[..boundary])
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
    serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap()
}

async fn respond(stream: &mut tokio::net::TcpStream, output: serde_json::Value) {
    let body = format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})
    );
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn dcp12_explicit_manual_compress_is_real_scoped_and_ask_stays_captured() {
    for ask in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut raw = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
        raw["dcp"] = serde_json::json!({"manualMode":{"enabled":true},"compress":{"permission":if ask {"ask"}else{"allow"},"minContextLimit":1,"maxContextLimit":1}});
        std::fs::write(project.join("opencode.json"), raw.to_string()).unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for step in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let req = request(&mut stream).await;
                let output = if step == 1 {
                    let anchors=req["input"].as_array().unwrap().iter().find_map(|item| {
                        let text=item["content"].as_array()?.first()?["text"].as_str()?;
                        text.strip_prefix("DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: ").map(|v|serde_json::from_str::<serde_json::Value>(v).unwrap())
                    }).unwrap();
                    let closed = anchors
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|a| a["closed"] == true)
                        .collect::<Vec<_>>();
                    assert!(closed.len() >= 2);
                    serde_json::json!([{"type":"function_call","id":"fc","call_id":"compress-call","name":"compress","arguments":serde_json::json!({"topic":"manual atomic","content":[{"startId":closed.first().unwrap()["id"],"endId":closed.last().unwrap()["id"],"summary":"SELECTED_FACT retained; next work"}]}).to_string()}])
                } else {
                    serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":if step==0 {format!("SELECTED_FACT {}", "closed filler ".repeat(500))}else{"done".into()}}]}])
                };
                requests.push(req);
                respond(&mut stream, output).await;
            }
            requests
        });
        let (app, guard, _) = spawn_with_env(&project, &data, env(root.path()))
            .await
            .unwrap();
        if ask {
            app.register_approval_consumer(false).await.unwrap();
        }
        let s = SessionId::new("controls").unwrap();
        app.create_session(s.clone()).await.unwrap();
        app.rename_session(s.clone(), "fixed".into()).await.unwrap();
        let mut events = app.subscribe();
        app.submit(
            s.clone(),
            "Manual context compression request. Call the compress tool (ordinary text).".into(),
        )
        .await
        .unwrap();
        settled(&mut events).await;
        let facts = app.dcp_snapshot(s.clone()).await.unwrap();
        assert_eq!(
            facts.availability.ordinary_refusal,
            Some(DcpUnavailable::ManualOnly)
        );
        assert_eq!(facts.availability.manual_refusal, None);
        app.compress(s.clone(), "preserve SELECTED_FACT".into())
            .await
            .unwrap();
        if ask {
            let approval = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(a) = app.pending_approvals().await.unwrap().into_iter().next() {
                        break a;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(approval.action, "compress");
            assert!(
                app.reload_location().await.is_err(),
                "captured Ask prevents reload"
            );
            app.reply_approval(oc_core::approval::ApprovalReply {
                id: approval.id,
                binding: approval.binding,
                decision: oc_core::approval::ApprovalDecision::Once,
            })
            .await
            .unwrap();
        }
        settled(&mut events).await;
        let compressed = app.dcp_snapshot(s.clone()).await.unwrap();
        assert_eq!(compressed.compressions, 1);
        assert_eq!(compressed.blocks, 1);
        app.submit(s.clone(), "ordinary next turn".into())
            .await
            .unwrap();
        settled(&mut events).await;
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 4);
        for (index, req) in requests.iter().enumerate() {
            let available = matches!(index, 1 | 2);
            assert_eq!(
                req["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == "compress"),
                available
            );
            let input = req["input"].to_string();
            assert_eq!(input.contains("DCP context anchors"), available);
            assert_eq!(
                input.contains("Explicit manual DCP compression admitted"),
                available
            );
            assert!(!input.contains("DCP reminder ("));
            if !available {
                assert!(input.contains("Stable text-message IDs"));
            }
        }
        assert!(!requests[3]["input"].to_string().contains("closed filler"));
        let db = Db::open(&data).unwrap();
        assert_eq!(db.dcp_block_count("controls").unwrap(), 1);
        let ops = db.list_tool_ops_page("controls", 100, None).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].state, "completed");
    }
}

#[tokio::test]
async fn dcp12_zero_effect_command_refusals_query_and_reload_location() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    let data = root.path().join("data");
    for p in [&a, &b] {
        std::fs::create_dir(p).unwrap();
        std::fs::write(
            p.join("opencode.json"),
            config("http://127.0.0.1:1/v1").to_string(),
        )
        .unwrap();
    }
    let (app, guard, _) = spawn_with_env(&a, &data, env(root.path())).await.unwrap();
    let s = SessionId::new("refusals").unwrap();
    app.create_session(s.clone()).await.unwrap();
    for (fragment, reason) in [
        (
            serde_json::json!({"enabled":false}),
            DcpUnavailable::GlobalOff,
        ),
        (
            serde_json::json!({"compress":{"enabled":false,"permission":"ask"}}),
            DcpUnavailable::CompressOff,
        ),
        (
            serde_json::json!({"compress":{"permission":"deny"}}),
            DcpUnavailable::Denied,
        ),
        (
            serde_json::json!({"manualMode":{"enabled":true},"compress":{"permission":"ask"}}),
            DcpUnavailable::ApprovalConsumerRequired,
        ),
        (
            serde_json::json!({"commands":{"enabled":false}}),
            DcpUnavailable::CommandsOff,
        ),
    ] {
        std::fs::write(a.join("dcp.jsonc"), fragment.to_string()).unwrap();
        app.reload_location().await.unwrap();
        let before = app.history_page(s.clone(), None, None, 100).await.unwrap();
        assert!(app.compress(s.clone(), "refuse".into()).await.is_err());
        let after = app.history_page(s.clone(), None, None, 100).await.unwrap();
        assert_eq!(before, after);
        let facts = app.dcp_snapshot(s.clone()).await.unwrap();
        if reason == DcpUnavailable::CommandsOff {
            assert!(facts.availability.ordinary_refusal.is_none());
            assert!(!app.catalog().await.unwrap().chrome.dcp.commands_enabled);
            assert!(app.submit(s.clone(), "/dcp-compress".into()).await.is_err());
        } else {
            assert_eq!(facts.availability.manual_refusal, Some(reason));
        }
    }
    let switched = app
        .switch_location_home(b.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert!(switched.catalog.chrome.dcp.commands_enabled);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert_eq!(db.dcp_block_count("refusals").unwrap(), 0);
    assert_eq!(db.tool_ops_len("refusals").unwrap(), 0);
    assert!(
        db.read_history_page("refusals", 100, None)
            .unwrap()
            .is_empty()
    );
}
