//! DCP10 actual ChildLane requests, causal calls and durable session isolation.
use super::*;
use std::thread::JoinHandle;

struct OwnedPeer {
    base: String,
    stop: Arc<AtomicBool>,
    requests: CapturedRequests,
    join: Option<JoinHandle<()>>,
}

impl OwnedPeer {
    fn start(script: impl Fn(&serde_json::Value) -> String + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests: CapturedRequests = Default::default();
        let (closing, captured) = (stop.clone(), requests.clone());
        let script = Arc::new(script);
        let join = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            while !closing.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (script, captured) = (script.clone(), captured.clone());
                        handlers.push(std::thread::spawn(move || {
                            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                            stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                            let mut reader = BufReader::new(stream);
                            let mut size = 0;
                            loop {
                                let mut line = String::new();
                                if reader.read_line(&mut line).unwrap() == 0 { return; }
                                if line.trim().is_empty() { break; }
                                if let Some((name, value)) = line.split_once(':')
                                    && name.eq_ignore_ascii_case("content-length") {
                                    size = value.trim().parse::<usize>().unwrap();
                                }
                            }
                            assert!(size > 0 && size < 1_048_576);
                            let mut bytes = vec![0; size];
                            reader.read_exact(&mut bytes).unwrap();
                            let request = serde_json::from_slice(&bytes).unwrap();
                            captured.lock().unwrap().push(serde_json::Value::clone(&request));
                            let body = script(&request);
                            write!(reader.get_mut(), "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("owned peer accept: {e}"),
                }
            }
            let failures = handlers
                .into_iter()
                .map(|handler| handler.join())
                .filter(Result::is_err)
                .count();
            assert_eq!(failures, 0, "owned peer handler failures");
        });
        Self {
            base,
            stop,
            requests,
            join: Some(join),
        }
    }
}

impl Drop for OwnedPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let joined = self.join.take().unwrap().join();
        if !std::thread::panicking() {
            joined.unwrap();
        }
    }
}

fn text(request: &serde_json::Value) -> String {
    request["input"].to_string()
}
fn names(request: &serde_json::Value) -> Vec<&str> {
    request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn dcp10_actual_child_disabled_manual_deny_and_no_consumer_ask_keep_ids_and_pair_refusal() {
    for (fragment, central, profile, root_available) in [
        (
            serde_json::json!({"experimental":{"allowSubAgents":false}}),
            Permission::Allow,
            false,
            true,
        ),
        (
            serde_json::json!({"enabled":false}),
            Permission::Allow,
            false,
            false,
        ),
        (
            serde_json::json!({"compress":{"enabled":false}}),
            Permission::Allow,
            false,
            false,
        ),
        (
            serde_json::json!({"manualMode":{"enabled":true}}),
            Permission::Allow,
            false,
            false,
        ),
        (
            serde_json::json!({"compress":{"permission":"deny"}}),
            Permission::Allow,
            false,
            false,
        ),
        (
            serde_json::json!({"compress":{"permission":"ask"}}),
            Permission::Allow,
            false,
            false,
        ),
        (serde_json::json!({}), Permission::Deny, false, false),
        (serde_json::json!({}), Permission::Allow, true, true),
    ] {
        let mut permissions = allow_all();
        permissions.insert("compress".into(), central);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_of(&harness, generation);
        runtime
            .reload_dcp(oc_adapters::dcp_auto::load_config(&fragment).unwrap().0)
            .unwrap();
        let mut child = agent("child", false, Some("test/agent-model"));
        if profile {
            child.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
                &serde_json::json!({"permission":{"*":"deny"}}),
            )
            .unwrap();
        }
        runtime
            .publish_subagents(Some(catalog(1, vec![child])))
            .unwrap();
        runtime.create_session("parent").unwrap();
        let rounds: Arc<Mutex<BTreeMap<String, usize>>> = Default::default();
        let peer = OwnedPeer::start(move |request| {
            let model = request["model"].as_str().unwrap();
            let mut rounds = rounds.lock().unwrap();
            let round = rounds.entry(model.into()).or_default();
            *round += 1;
            if model == "m" {
                assert_eq!(names(request).contains(&"compress"), root_available);
                if *round == 1 {
                    return subagent_call(
                        "delegate",
                        serde_json::json!({"agent":"child","description":"gate","prompt":"Call the compress tool explicit ordinary text is not manual admission"}),
                    ) + &sse_completed();
                }
            } else {
                assert!(!names(request).contains(&"compress"));
                let wire = text(request);
                assert!(wire.contains("Stable text-message IDs"));
                for forbidden in [
                    "DCP context anchors",
                    "closed=true",
                    "DCP reminder (",
                    "Explicit manual DCP compression admitted",
                ] {
                    assert!(!wire.contains(forbidden));
                }
                if *round == 1 {
                    return sse_tool_call(
                        "stale-compress",
                        "compress",
                        &serde_json::json!({"topic":"stale","content":[{"startId":"alien","endId":"alien","summary":"never committed"}]}),
                    ) + &sse_completed();
                }
                let paired = request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|i| {
                        i["type"] == "function_call_output" && i["call_id"] == "stale-compress"
                    })
                    .collect::<Vec<_>>();
                assert_eq!(paired.len(), 1);
                assert!(
                    paired[0]["output"]
                        .as_str()
                        .unwrap()
                        .contains("excluded by issuing request")
                );
            }
            sse_delta("terminal") + &sse_completed()
        });
        tokio::time::timeout(
            Duration::from_secs(10),
            runtime.run_turn(params(
                "parent",
                "gate",
                &harness,
                provider_of(&peer.base),
                &NO_CANCEL,
            )),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(peer.requests.lock().unwrap().len(), 4);
        let children = harness.db.children_of("parent").unwrap();
        assert_eq!(children.len(), 1);
        assert!(
            harness
                .db
                .load_compression_blocks(&children[0])
                .unwrap()
                .is_empty()
        );
        assert!(runtime.pending_approvals().is_empty());
        let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
        for table in [
            "compression_blocks",
            "permission_grants",
            "dcp_tool_projection",
            "dcp_tool_projection_v2",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
        let started: i64 = conn
            .query_row(
                "SELECT count(*) FROM tool_operations WHERE name='compress' AND state='started'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(started, 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn dcp10_actual_own_model_children_compress_continue_reopen_with_isolated_raw_and_sibling() {
    own_model_children(false).await;
}

#[tokio::test]
async fn dcp10_genuine_child_compress_ask_consumes_typed_once_without_saved_grant() {
    own_model_children(true).await;
}

async fn run_owned(runtime: &Runtime<'_>, params: TurnParams<'_>, db: &Db, expected_asks: usize) {
    use oc_core::{
        approval::{ApprovalDecision, ApprovalReply},
        core_app::CoreEvent,
    };
    let (tx, mut events) = tokio::sync::broadcast::channel(256);
    runtime.set_approval_events(&tx);
    runtime.register_approval_consumer(false);
    let run = tokio::time::timeout(Duration::from_secs(10), runtime.run_turn(params));
    tokio::pin!(run);
    let mut asks = 0;
    loop {
        tokio::select! {
            report = &mut run => { report.unwrap().unwrap(); break; }
            event = events.recv() => {
                if let CoreEvent::PermissionAsked(request) = event.unwrap() {
                    assert_eq!(request.action, "compress");
                    assert_ne!(request.binding.session, "parent");
                    assert!(db.load_compression_blocks(&request.binding.session).unwrap().is_empty());
                    asks += 1;
                    runtime.reply_approval(ApprovalReply { id: request.id, binding: request.binding, decision: ApprovalDecision::Once }).unwrap();
                }
            }
        }
    }
    assert_eq!(asks, expected_asks);
}

async fn own_model_children(ask: bool) {
    let (mut harness, generation) = make_harness(allow_all());
    std::fs::write(
        harness._project.path().join("seed"),
        "own closed read ".repeat(200),
    )
    .unwrap();
    for (model, context) in [
        ("general-model", 200_000),
        ("explore-model", 300_000),
        ("custom-model", 400_000),
        ("spectator-model", 500_000),
    ] {
        harness.catalog.models.insert(
            model.into(),
            serde_json::json!({"limit":{"context":context,"output":4096}}),
        );
    }
    let agents = ["general", "explore", "custom", "spectator"]
        .into_iter()
        .map(|id| {
            let mut a = agent(id, false, None);
            a.model = Some(format!("test/{id}-model"));
            a.prompt = format!("OWN_{id}_SYSTEM");
            if id == "explore" {
                a.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
                    &serde_json::json!({"permission":[
                        {"action":"*","resource":"*","effect":"deny"},
                        {"action":"read","resource":"*","effect":"allow"},
                        {"action":"compress","resource":"*","effect":"allow"}
                    ]}),
                )
                .unwrap();
            }
            a
        })
        .collect::<Vec<_>>();
    let ids: Arc<Mutex<BTreeMap<String, String>>> = Default::default();
    let child_ids = ids.clone();
    let rounds: Arc<Mutex<BTreeMap<String, usize>>> = Default::default();
    let peer = OwnedPeer::start(move |request| {
        let model = request["model"].as_str().unwrap();
        let mut rounds = rounds.lock().unwrap();
        let round = rounds.entry(model.to_owned()).or_default();
        *round += 1;
        let round = *round;
        drop(rounds);
        let wire = text(request);
        if model == "m" {
            if round.is_multiple_of(2) {
                return sse_delta("root terminal") + &sse_completed();
            }
            let mut body = String::new();
            for id in ["general", "explore", "custom", "spectator"] {
                if id == "spectator" && round > 1 {
                    continue;
                }
                let mut args = serde_json::json!({"agent":id,"description":id,"prompt":format!("OWN_{id}_TASK {}", if round==1 {"seed"} else {"continue"})});
                if round > 1 {
                    args["sessionID"] = child_ids.lock().unwrap()[id].clone().into();
                }
                body += &subagent_call(&format!("delegate-{id}-{round}"), args);
            }
            return body + &sse_completed();
        }
        let id = model.strip_suffix("-model").unwrap();
        assert!(wire.contains(&format!("OWN_{id}_SYSTEM")));
        assert!(!wire.contains("ROOT_PRIVATE"));
        assert!(names(request).contains(&"compress"));
        assert!(wire.contains("DCP context anchors") || wire.contains("Stable text-message IDs"));
        if id == "spectator" {
            return sse_delta("SPECTATOR_UNCHANGED") + &sse_completed();
        }
        match round {
            1 => {
                sse_tool_call(
                    &format!("read-{id}"),
                    "read",
                    &serde_json::json!({"path":"seed"}),
                ) + &sse_completed()
            }
            2 => {
                assert!(
                    request["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["call_id"] == format!("read-{id}")
                            && i["type"] == "function_call_output")
                );
                sse_delta(&format!(
                    "OLD_{id}_SENTINEL {}",
                    "discard closed work ".repeat(200)
                )) + &sse_completed()
            }
            3 => {
                let anchor = request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|i| {
                        let content = i["content"][0]["text"].as_str()?;
                        content.contains("DCP context anchors").then_some(content)
                    })
                    .unwrap();
                let anchors: serde_json::Value =
                    serde_json::from_str(&anchor[anchor.find("[{").unwrap()..]).unwrap();
                let closed = anchors
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|a| a["closed"] == true)
                    .collect::<Vec<_>>();
                sse_tool_call(
                    &format!("compress-{id}"),
                    "compress",
                    &serde_json::json!({"topic":format!("own {id}"),"content":[{
                        "startId":closed[0]["id"],"endId":closed.last().unwrap()["id"],"summary":format!("OWN_{id}_KEPT short working fact")
                    }]}),
                ) + &sse_completed()
            }
            4 | 5 => {
                assert!(wire.contains(&format!("OWN_{id}_KEPT")), "{wire}");
                assert!(!wire.contains(&format!("OLD_{id}_SENTINEL")));
                // Existing selected producer retention is a separate R9 seam;
                // an eligible-child switch must preserve its causal pair intact.
                let group = request["input"].as_array().unwrap();
                let calls = group
                    .iter()
                    .filter(|i| {
                        i["call_id"] == format!("read-{id}") && i["type"] == "function_call"
                    })
                    .count();
                let results = group
                    .iter()
                    .filter(|i| {
                        i["call_id"] == format!("read-{id}") && i["type"] == "function_call_output"
                    })
                    .count();
                assert_eq!(calls, results);
                assert!(calls <= 1);
                if round == 4 {
                    assert!(
                        request["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|i| i["call_id"] == format!("compress-{id}")
                                && i["type"] == "function_call_output"
                                && i["output"].as_str().unwrap().contains("compressed"))
                    );
                }
                sse_delta(&format!("OWN_{id}_FINAL")) + &sse_completed()
            }
            _ => panic!("unexpected child replay"),
        }
    });
    let runtime = runtime_of(&harness, generation.clone());
    if ask {
        runtime
            .reload_dcp(DcpConfig {
                compress_permission: Some(Permission::Ask),
                ..Default::default()
            })
            .unwrap();
    }
    runtime
        .publish_subagents(Some(catalog(1, agents.clone())))
        .unwrap();
    runtime.create_session("parent").unwrap();
    run_owned(
        &runtime,
        params(
            "parent",
            "ROOT_PRIVATE seed",
            &harness,
            provider_of(&peer.base),
            &NO_CANCEL,
        ),
        &harness.db,
        0,
    )
    .await;
    let children = harness
        .db
        .children_of("parent")
        .unwrap()
        .into_iter()
        .map(|id| {
            let meta = harness.db.session_meta(&id).unwrap();
            (id, meta)
        })
        .collect::<Vec<_>>();
    assert_eq!(children.len(), 4);
    for (id, child) in &children {
        ids.lock()
            .unwrap()
            .insert(child.agent.clone().unwrap(), id.clone());
    }
    let raw = children
        .iter()
        .map(|(id, _)| (id.clone(), harness.db.read_history_full(id).unwrap()))
        .collect::<BTreeMap<_, _>>();
    let parent_raw = harness.db.read_history_full("parent").unwrap();
    run_owned(
        &runtime,
        params(
            "parent",
            "ROOT_PRIVATE compress",
            &harness,
            provider_of(&peer.base),
            &NO_CANCEL,
        ),
        &harness.db,
        if ask { 3 } else { 0 },
    )
    .await;
    assert!(
        harness
            .db
            .load_compression_blocks("parent")
            .unwrap()
            .is_empty()
    );
    assert!(
        harness
            .db
            .read_history_full("parent")
            .unwrap()
            .starts_with(&parent_raw)
    );
    for (id, child) in &children {
        assert!(
            harness
                .db
                .read_history_full(id)
                .unwrap()
                .starts_with(&raw[id])
        );
        assert_eq!(
            harness.db.load_compression_blocks(id).unwrap().len(),
            if child.agent.as_deref() == Some("spectator") {
                0
            } else {
                1
            }
        );
    }
    let spectator = ids.lock().unwrap()["spectator"].clone();
    assert_eq!(
        harness.db.read_history_full(&spectator).unwrap(),
        raw[&spectator]
    );
    drop(runtime);
    let reopened = runtime_of(&harness, generation);
    if ask {
        reopened
            .reload_dcp(DcpConfig {
                compress_permission: Some(Permission::Ask),
                ..Default::default()
            })
            .unwrap();
    }
    reopened
        .publish_subagents(Some(catalog(1, agents)))
        .unwrap();
    run_owned(
        &reopened,
        params(
            "parent",
            "ROOT_PRIVATE reopen",
            &harness,
            provider_of(&peer.base),
            &NO_CANCEL,
        ),
        &harness.db,
        0,
    )
    .await;
    let requests = peer.requests.lock().unwrap();
    assert_eq!(requests.len(), 22);
    for id in ["general", "explore", "custom"] {
        let child = &ids.lock().unwrap()[id];
        assert_eq!(harness.db.load_compression_blocks(child).unwrap().len(), 1);
        assert_eq!(harness.db.tool_ops_len(child).unwrap(), 2);
    }
    let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
    let grants: i64 = conn
        .query_row("SELECT count(*) FROM permission_grants", [], |r| r.get(0))
        .unwrap();
    assert_eq!(grants, 0, "Once is not a durable grant");
}
