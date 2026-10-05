//! Subagent slices S3/S4: foreground `subagent` tool against the fake
//! Responses server — child persistence, fresh history, permission denial,
//! agent/model resolution, depth gating, continuation, and cancellation.
//!
//! The child turn runs through the inner turn path without taking the
//! single-flight lease and shares the parent cancel flag, so one cancel
//! stops both turns.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oc_adapters::config::{Generation, Permission};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{
    Runtime, SESSION_LOCATION_PREFIX, SubagentAgent, SubagentCatalog, TurnParams, TurnStatus,
};
use oc_adapters::storage::Db;

#[path = "fixtures/child_dcp.rs"]
mod child_dcp;

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n".to_string()
}

fn sse_tool_call(call_id: &str, name: &str, args: &serde_json::Value) -> String {
    let item_id = format!("fc_{call_id}");
    let added = serde_json::json!({"type": "response.output_item.added", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": "", "status": "in_progress"
    }});
    let delta = serde_json::json!({"type": "response.function_call_arguments.delta",
        "item_id": item_id, "delta": args.to_string()});
    let done = serde_json::json!({"type": "response.output_item.done", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": args.to_string(), "status": "completed"
    }});
    format!("data: {added}\n\ndata: {delta}\n\ndata: {done}\n\n")
}

fn subagent_call(call_id: &str, args: serde_json::Value) -> String {
    sse_tool_call(call_id, "subagent", &args)
}

type CapturedRequests = Arc<Mutex<Vec<serde_json::Value>>>;

/// Scripted fake: serve queued SSE bodies in order, then repeat the last.
struct Fake;

impl Fake {
    fn start(script: Vec<String>) -> (String, CapturedRequests) {
        let (base, _, requests) = Self::start_recording(script);
        (base, requests)
    }

    fn start_recording(script: Vec<String>) -> (String, Arc<Mutex<usize>>, CapturedRequests) {
        let (base, hits, requests, _) = Self::start_with_headers(script);
        (base, hits, requests)
    }

    #[allow(clippy::type_complexity)]
    fn start_with_headers(
        script: Vec<String>,
    ) -> (
        String,
        Arc<Mutex<usize>>,
        CapturedRequests,
        Arc<Mutex<Vec<BTreeMap<String, String>>>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let hits = Arc::new(Mutex::new(0usize));
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let worker_queue = queue.clone();
        let worker_hits = hits.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker_requests = requests.clone();
        let headers = Arc::new(Mutex::new(Vec::new()));
        let worker_headers = headers.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = worker_queue.clone();
                let hits = worker_hits.clone();
                let requests = worker_requests.clone();
                let headers = worker_headers.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    let mut seen = BTreeMap::new();
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(_) => return,
                        }
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':') {
                            if name.trim().eq_ignore_ascii_case("content-length") {
                                content_length = value.trim().parse().unwrap_or(0);
                            }
                            seen.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
                        }
                    }
                    let mut body = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    headers.lock().expect("headers").push(seen);
                    requests
                        .lock()
                        .expect("requests")
                        .push(serde_json::from_slice(&body).expect("JSON request"));
                    let payload = {
                        let mut queue = queue.lock().expect("queue");
                        *hits.lock().expect("hits") += 1;
                        if queue.len() > 1 {
                            queue.pop_front().expect("script")
                        } else {
                            queue.front().cloned().unwrap_or_default()
                        }
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    let _ = reader.get_mut().write_all(response.as_bytes());
                });
            }
        });
        (base, hits, requests, headers)
    }

    /// Serve `first` immediately; every later request stalls with heartbeats
    /// (a child stream that only ends on cancellation).
    fn start_cancelable(first: String, stall: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let hits = Arc::new(Mutex::new(0usize));
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let hits = hits.clone();
                let first = first.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(_) => return,
                        }
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':')
                            && name.trim().eq_ignore_ascii_case("content-length")
                        {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    let index = {
                        let mut hits = hits.lock().expect("hits");
                        let index = *hits;
                        *hits += 1;
                        index
                    };
                    let stream = reader.get_mut();
                    if index == 0 {
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            first.len(),
                            first
                        );
                        let _ = stream.write_all(response.as_bytes());
                        return;
                    }
                    let _ = stream.write_all(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                            .as_bytes(),
                    );
                    let _ = stream.flush();
                    let start = Instant::now();
                    while start.elapsed() < stall {
                        let _ = stream.write_all(b": hb\n\n");
                        let _ = stream.flush();
                        std::thread::sleep(Duration::from_millis(100));
                    }
                });
            }
        });
        base
    }
}

struct Harness {
    _project: tempfile::TempDir,
    _data: tempfile::TempDir,
    db: Db,
    catalog: ModelCatalog,
}

fn allow_all() -> BTreeMap<String, Permission> {
    [
        "read",
        "apply_patch",
        "bash",
        "webfetch",
        "skill",
        "compress",
        "subagent",
    ]
    .into_iter()
    .map(|name| (name.to_string(), Permission::Allow))
    .collect()
}

fn make_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let models = [
        (
            "m",
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        ),
        (
            "agent-model",
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        ),
        (
            "tiny",
            serde_json::json!({"limit": {"context": 6_000, "output": 200}}),
        ),
        (
            "gpt-parent",
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        ),
        (
            "small",
            serde_json::json!({
                "limit": {"context": 1_000_000, "output": 100_000},
                "variants": {"fast": {"reasoningEffort": "low"}},
            }),
        ),
    ];
    let catalog = ModelCatalog {
        provider: "test".to_string(),
        models: models
            .into_iter()
            .map(|(id, spec)| (id.to_string(), spec))
            .collect(),
    };
    let generation = Generation {
        tool_output: Default::default(),
        compaction: Default::default(),
        config_diagnostics: Vec::new(),
        animations: None,
        providers: BTreeMap::new(),
        public_go_enabled: false,
        mcp: BTreeMap::new(),
        permissions,
        permission_rules: Default::default(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    (
        Harness {
            _project: project,
            _data: data,
            db,
            catalog,
        },
        generation,
    )
}

fn runtime_of<'a>(harness: &'a Harness, generation: Generation) -> Runtime<'a> {
    let project = harness._project.path();
    let files = oc_adapters::files::Files::new(project, harness._data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(project).expect("shell");
    Runtime::new(
        &harness.db,
        "work",
        generation,
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        files,
        shell,
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .expect("runtime")
}

fn provider_of(base: &str) -> ResponsesConfig {
    ResponsesConfig {
        headers: BTreeMap::new(),
        set_cache_key: true,
        wire: Default::default(),
        base_url: base.to_string(),
        api_key: "test-key".to_string(),
        timeout: Some(false),
        chunk_timeout_ms: 5_000,
        connect_timeout: Duration::from_secs(5),
        allow_private: true,
    }
}

fn params<'c>(
    session: &str,
    prompt: &str,
    harness: &'c Harness,
    provider: ResponsesConfig,
    cancel: &'c AtomicBool,
) -> TurnParams<'c> {
    TurnParams {
        session: session.to_string(),
        prompt: prompt.to_string(),
        invocation: None,
        catalog: &harness.catalog,
        model_id: "m".to_string(),
        variant: None,
        max_output: 1_000,
        provider,
        cancel,
    }
}

static NO_CANCEL: AtomicBool = AtomicBool::new(false);

#[tokio::test]
async fn approval_structural_invalid_child_never_asks_saves_or_starts() {
    use oc_core::core_app::CoreEvent;
    let cases = [
        (serde_json::json!({"agent":"ghost"}), 1, "Unknown agent"),
        (
            serde_json::json!({"agent":"boss"}),
            1,
            "cannot run as a subagent",
        ),
        (serde_json::json!({"agent":"helper"}), 0, "depth limit"),
        (
            serde_json::json!({"agent":"helper","sessionID":"foreign"}),
            1,
            "not a child",
        ),
        (
            serde_json::json!({"agent":"helper","sessionID":"missing"}),
            1,
            "session not found",
        ),
        (
            serde_json::json!({"agent":"helper","model":"test/missing"}),
            1,
            "not available",
        ),
        (
            serde_json::json!({"agent":"helper","model":"test/m#missing"}),
            1,
            "no variants",
        ),
        (serde_json::json!({"agent":"bad-model"}), 1, "not available"),
    ];
    for (permission, auto_once) in [
        (Permission::Ask, false),
        (Permission::Ask, true),
        (Permission::Allow, false),
    ] {
        for (mut args, depth, expected) in cases.clone() {
            args["description"] = serde_json::json!("Invalid child");
            args["prompt"] = serde_json::json!("Must not execute");
            let mut permissions = allow_all();
            permissions.insert("subagent".into(), permission);
            let (harness, generation) = make_harness(permissions);
            let runtime = runtime_of(&harness, generation);
            runtime
                .publish_subagents(Some(catalog(
                    depth,
                    vec![
                        agent("helper", false, None),
                        agent("boss", true, None),
                        agent("bad-model", false, Some("test/missing")),
                    ],
                )))
                .unwrap();
            runtime.create_session("parent").unwrap();
            runtime.create_session("foreign").unwrap();
            let (tx, mut events) = tokio::sync::broadcast::channel(128);
            runtime.set_approval_events(&tx);
            runtime.register_approval_consumer(auto_once);
            let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
            // A transient INSERT of execution intent is forbidden too, even if
            // it would subsequently be overwritten with a failed outcome.
            conn.execute_batch("CREATE TRIGGER forbid_child_intent BEFORE INSERT ON tool_operations WHEN NEW.state='started' BEGIN SELECT RAISE(FAIL,'structurally invalid child intent'); END;").unwrap();
            let (base, requests) = Fake::start(vec![
                subagent_call("invalid", args) + &sse_completed(),
                sse_delta("recovered") + &sse_completed(),
            ]);
            let report = tokio::time::timeout(
                Duration::from_secs(5),
                runtime.run_turn(params(
                    "parent",
                    "invalid child",
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL,
                )),
            )
            .await
            .expect("invalid child cannot wait for approval")
            .unwrap();
            assert_eq!(report.status, TurnStatus::Completed);
            assert!(
                report.calls[0].output.contains(expected),
                "{}",
                report.calls[0].output
            );
            assert!(runtime.pending_approvals().is_empty());
            while let Ok(event) = events.try_recv() {
                assert!(!matches!(
                    event,
                    CoreEvent::PermissionAsked(_)
                        | CoreEvent::PermissionResolved { .. }
                        | CoreEvent::ToolCallStarted { .. }
                ));
            }
            assert_eq!(
                conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                conn.query_row(
                    "SELECT count(*) FROM sessions WHERE parent_id IS NOT NULL",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert_eq!(requests.lock().unwrap().len(), 2);
        }
    }
}

#[tokio::test]
async fn approval_child_feedback_keeps_typed_graph_and_root_cancel_clears_child_wait() {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    for cancel_child in [false, true] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation);
        let mut child = agent("review", false, None);
        child.permissions.insert("bash".into(), Permission::Ask);
        child.digest = Some("review-digest".into());
        runtime
            .publish_subagents(Some(catalog(2, vec![child])))
            .unwrap();
        let (events, _) = tokio::sync::broadcast::channel(16);
        runtime.set_approval_events(&events);
        runtime.register_approval_consumer(false);
        runtime.create_session("parent").unwrap();
        let cancel = AtomicBool::new(false);
        let (base, requests) = Fake::start(vec![
            subagent_call(
                "spawn",
                serde_json::json!({"agent":"review", "description":"review", "prompt":"child"}),
            ) + &sse_completed(),
            sse_tool_call(
                "child-effect",
                "bash",
                &serde_json::json!({"argv":["/usr/bin/touch","child-marker"]}),
            ) + &sse_completed(),
            sse_delta("corrected child") + &sse_completed(),
            sse_delta("parent done") + &sse_completed(),
        ]);
        let running = runtime.run_turn(params(
            "parent",
            "delegate",
            &harness,
            provider_of(&base),
            &cancel,
        ));
        let replying = async {
            let request = tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    if let Some(r) = runtime.pending_approvals().into_iter().next() {
                        break r;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_ne!(request.binding.session, "parent");
            assert_eq!(request.binding.agent.as_deref(), Some("review"));
            assert_eq!(
                request.binding.agent_digest.as_deref(),
                Some("review-digest")
            );
            assert!(
                harness
                    .db
                    .list_tool_ops(&request.binding.session)
                    .unwrap()
                    .is_empty()
            );
            assert!(!harness._project.path().join("child-marker").exists());
            if cancel_child {
                cancel.store(true, Ordering::Release);
            } else {
                runtime
                    .reply_approval(ApprovalReply {
                        id: request.id,
                        binding: request.binding.clone(),
                        decision: ApprovalDecision::Reject {
                            feedback: Some("do not touch files; answer from context".into()),
                        },
                    })
                    .unwrap();
            }
            request
        };
        let (result, request) = tokio::join!(running, replying);
        let report = result.unwrap();
        assert!(runtime.pending_approvals().is_empty());
        assert!(!harness._project.path().join("child-marker").exists());
        assert!(
            runtime
                .reply_approval(ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: ApprovalDecision::Always
                })
                .is_err()
        );
        if cancel_child {
            assert_eq!(report.status, TurnStatus::Cancelled);
        } else {
            assert_eq!(report.status, TurnStatus::Completed);
            assert!(report.calls[0].output.contains("corrected child"));
            let requests = requests.lock().unwrap();
            assert!(requests[2]["input"].as_array().unwrap().iter().any(|item| {
                item["type"] == "function_call_output"
                    && item["call_id"] == "child-effect"
                    && item["output"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("do not touch files; answer from context")
            }));
        }
    }
}

#[tokio::test]
async fn nested_resource_constraints_and_filtered_catalog_remain_inherited() {
    let (harness, mut generation) = make_harness(allow_all());
    generation.permission_rules =
        oc_adapters::permissions::PermissionRules::from_config(&serde_json::json!({
            "permission":{"*":"allow", "subagent":{"*":"allow", "blocked":"deny"}}
        }))
        .unwrap();
    let runtime = runtime_of(&harness, generation);
    let mut restricted = agent("restricted", false, None);
    restricted.permission_rules =
        oc_adapters::permissions::PermissionRules::from_config(&serde_json::json!({
            "permission":{"bash":{"*":"deny", "/bin/echo hi":"allow"}}
        }))
        .unwrap();
    let mut free = agent("free", false, None);
    free.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission":"allow"}),
    )
    .unwrap();
    let mut hidden = agent("hidden", false, None);
    hidden.hidden = true;
    runtime
        .publish_subagents(Some(catalog(
            3,
            vec![restricted, free, hidden, agent("blocked", false, None)],
        )))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "parent-child",
            serde_json::json!({"agent":"restricted", "description":"Child", "prompt":"child"}),
        ) + &sse_completed(),
        subagent_call(
            "child-grandchild",
            serde_json::json!({"agent":"free", "description":"Grandchild", "prompt":"grandchild"}),
        ) + &sse_completed(),
        sse_tool_call(
            "grandchild-ok",
            "bash",
            &serde_json::json!({"argv":["/bin/echo","hi"]}),
        ) + &sse_tool_call(
            "grandchild-no",
            "bash",
            &serde_json::json!({"argv":["/bin/touch","marker"]}),
        ) + &sse_completed(),
        sse_delta("grandchild done") + &sse_completed(),
        sse_delta("child done") + &sse_completed(),
        sse_delta("parent done") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "spawn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert!(!harness._project.path().join("marker").exists());
    let captured = child_requests(&requests);
    assert!(
        function_output(&captured[3], "grandchild-ok")
            .unwrap()
            .contains("hi")
    );
    assert_eq!(
        function_output(&captured[3], "grandchild-no"),
        Some("error: denied bash")
    );
    let tools = captured[0]["tools"].as_array().unwrap();
    let description = tools
        .iter()
        .find(|tool| tool["name"] == "subagent")
        .unwrap()["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("- restricted:"));
    assert!(description.contains("- free:"));
    assert!(!description.contains("- hidden:"));
    assert!(!description.contains("- blocked:"));
}

fn agent(id: &str, primary: bool, model: Option<&str>) -> SubagentAgent {
    SubagentAgent {
        id: id.to_string(),
        description: format!("{id} description"),
        primary,
        model: model.map(str::to_string),
        variant: None,
        prompt: format!("You are {id}."),
        permissions: BTreeMap::new(),
        permission_rules: Default::default(),
        hidden: false,
        digest: Some(format!("{id}-digest")),
        request: Default::default(),
        color: None,
    }
}

fn catalog(depth_limit: u32, agents: Vec<SubagentAgent>) -> SubagentCatalog {
    SubagentCatalog {
        agents: agents
            .into_iter()
            .map(|agent| (agent.id.clone(), agent))
            .collect(),
        depth_limit,
    }
}

fn function_output<'a>(request: &'a serde_json::Value, call_id: &str) -> Option<&'a str> {
    request["input"].as_array()?.iter().find_map(|item| {
        (item["type"] == "function_call_output" && item["call_id"].as_str() == Some(call_id))
            .then(|| item["output"].as_str())
            .flatten()
    })
}

fn child_requests(requests: &CapturedRequests) -> Vec<serde_json::Value> {
    requests.lock().expect("requests").clone()
}

fn turn_status(db: &Db, session: &str) -> String {
    let sql = rusqlite::Connection::open(db.root().join("oc.sqlite")).expect("sqlite");
    sql.query_row(
        "SELECT status FROM turns WHERE session_id = ?1 ORDER BY rowid DESC LIMIT 1",
        [session],
        |row| row.get(0),
    )
    .expect("turn row")
}

fn messages(db: &Db, session: &str) -> Vec<(String, String)> {
    db.read_history(session).expect("history")
}

#[tokio::test]
async fn t47_child_unknown_limits_keep_fallback_warning_in_tool_result() {
    let (mut harness, generation) = make_harness(allow_all());
    harness.catalog.models.insert(
        "agent-model".into(),
        serde_json::json!({
            "variants":{"low":{"reasoningEffort":"low"}}
        }),
    );
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![agent("helper", false, Some("test/agent-model"))],
        )))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-sub",
            serde_json::json!({"agent":"helper", "description":"Check", "prompt":"check"}),
        ) + &sse_completed(),
        sse_delta("child answer") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    let captured = child_requests(&requests);
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[1]["model"], "agent-model");
    assert_eq!(captured[1]["max_output_tokens"], 4096);
    assert!(captured[1].get("reasoning").is_none());
    let output = function_output(&captured[2], "call-sub").unwrap();
    assert!(output.contains("native fallback caps (context=32768, output=4096)"));
    assert!(output.contains("child answer"));
    assert!(
        report.calls[0]
            .output
            .contains("unknown context and output limits")
    );
}

#[tokio::test]
async fn spawn_returns_child_text_and_persists_fresh_child_row() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![agent("helper", false, None), agent("boss", true, None)],
        )))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-sub",
            serde_json::json!({"agent": "helper", "description": "Say hi", "prompt": "say hi"}),
        ) + &sse_completed(),
        sse_delta("child says hi") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");

    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "parent final");
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].name, "subagent");
    assert_eq!(report.calls[0].state, "completed");

    let children = harness.db.children_of("parent").expect("children");
    assert_eq!(children.len(), 1, "exactly one child row");
    let child = &children[0];
    let meta = harness.db.session_meta(child).expect("meta");
    assert_eq!(meta.parent_id.as_deref(), Some("parent"));
    assert_eq!(meta.agent.as_deref(), Some("helper"));
    assert_eq!(meta.model.as_deref(), Some("test/m"));
    assert_eq!(meta.title.as_deref(), Some("Say hi"));
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let pinned:Option<i64>=sql.query_row("SELECT json_extract(result,'$.display.agent_color_index') FROM turns WHERE session_id=?1",[child],|r|r.get(0)).unwrap();
    assert_eq!(
        pinned,
        Some(1),
        "boss then helper: child pins its owning generation's slot, not the parent slot"
    );
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .unwrap();
    let after:Option<i64>=sql.query_row("SELECT json_extract(result,'$.display.agent_color_index') FROM turns WHERE session_id=?1",[child],|r|r.get(0)).unwrap();
    assert_eq!(
        after, pinned,
        "later catalog ordering must not recolor child history"
    );

    // Fresh child history: prefixed prompt + child answer only.
    assert_eq!(
        messages(&harness.db, child),
        vec![
            (
                "user".to_string(),
                "You are a subagent spawned by another session.\nsay hi".to_string()
            ),
            ("assistant".to_string(), "child says hi".to_string()),
        ]
    );
    assert_eq!(
        messages(&harness.db, "parent"),
        vec![
            ("user".to_string(), "work".to_string()),
            ("assistant".to_string(), "parent final".to_string()),
        ]
    );

    let captured = child_requests(&requests);
    assert_eq!(captured.len(), 3, "parent round 1, child, parent round 2");
    let output = function_output(&captured[2], "call-sub").expect("tool output");
    assert!(
        output.starts_with(&format!(
            "<subagent sessionID=\"{child}\" state=\"completed\">"
        )),
        "got: {output}"
    );
    assert!(output.ends_with("</subagent>"), "got: {output}");
    assert!(output.contains("child says hi"), "got: {output}");
    // The child lane starts from the prefix, never from the parent transcript.
    let child_input = captured[1]["input"].to_string();
    assert!(
        child_input.contains("You are a subagent spawned by another session."),
        "got: {child_input}"
    );
    assert!(!child_input.contains("\"work\""), "got: {child_input}");
    assert_eq!(captured[1]["model"], "m");
    // The advertised tool lists selectable subagents, not primary-only ones.
    let tools = captured[0]["tools"].as_array().expect("tools");
    let definition = tools
        .iter()
        .find(|tool| tool["name"] == "subagent")
        .expect("subagent tool definition");
    let description = definition["description"].as_str().expect("description");
    assert!(
        description.contains("Available subagents:"),
        "{description}"
    );
    assert!(
        description.contains("- helper: helper description"),
        "{description}"
    );
    assert!(!description.contains("- boss"), "{description}");
    assert_eq!(
        definition["parameters"]["required"],
        serde_json::json!(["agent", "description", "prompt"])
    );
    assert_eq!(definition["parameters"]["additionalProperties"], false);
}

#[tokio::test]
async fn denied_subagent_creates_no_child_row() {
    let mut permissions = allow_all();
    permissions.insert("subagent".to_string(), Permission::Deny);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-denied",
            serde_json::json!({"agent": "helper", "description": "Denied", "prompt": "go"}),
        ) + &sse_completed(),
        sse_delta("denied handled") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls[0].state, "failed");
    assert!(
        report.calls[0].output.contains("error: denied subagent"),
        "got: {}",
        report.calls[0].output
    );
    assert!(
        harness
            .db
            .children_of("parent")
            .expect("children")
            .is_empty(),
        "a denied call must not create a child row"
    );
}

#[tokio::test]
async fn unknown_and_primary_calls_fail_closed() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![agent("helper", false, None), agent("boss", true, None)],
        )))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-ghost",
            serde_json::json!({"agent": "ghost", "description": "Ghost", "prompt": "go"}),
        ) + &sse_completed(),
        sse_delta("after ghost") + &sse_completed(),
        subagent_call(
            "call-boss",
            serde_json::json!({"agent": "boss", "description": "Boss", "prompt": "go"}),
        ) + &sse_completed(),
        sse_delta("after boss") + &sse_completed(),
    ]);
    let provider = provider_of(&base);
    let unknown = runtime
        .run_turn(params(
            "parent",
            "one",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        unknown.calls[0]
            .output
            .contains("error: Unknown agent: ghost"),
        "got: {}",
        unknown.calls[0].output
    );
    let primary = runtime
        .run_turn(params(
            "parent",
            "two",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        primary.calls[0]
            .output
            .contains("error: Agent boss cannot run as a subagent"),
        "got: {}",
        primary.calls[0].output
    );
    assert!(
        harness
            .db
            .children_of("parent")
            .expect("children")
            .is_empty(),
        "no failing call may create a child row"
    );
}

#[tokio::test]
async fn depth_limit_blocks_nesting_and_higher_depth_allows_it() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-child",
            serde_json::json!({"agent": "helper", "description": "Child", "prompt": "child task"}),
        ) + &sse_completed(),
        sse_delta("child answer") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
        subagent_call(
            "call-nested",
            serde_json::json!({"agent": "helper", "description": "Nested", "prompt": "nested"}),
        ) + &sse_completed(),
        sse_delta("nested refused") + &sse_completed(),
        subagent_call(
            "call-grandchild",
            serde_json::json!({"agent": "helper", "description": "Grand", "prompt": "grand"}),
        ) + &sse_completed(),
        sse_delta("grandchild answer") + &sse_completed(),
        sse_delta("child final") + &sse_completed(),
    ]);
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "parent",
            "spawn",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("spawn turn");
    let child = harness
        .db
        .children_of("parent")
        .expect("children")
        .remove(0);

    // Depth 1: the child may not spawn another subagent.
    let refused = runtime
        .run_turn(params(
            &child,
            "nested",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("nested turn");
    assert!(
        refused.calls[0].output.contains(
            "error: Subagent depth limit reached (1). Increase \"experimental.subagent_depth\" to allow nested subagents."
        ),
        "got: {}",
        refused.calls[0].output
    );
    assert!(
        harness.db.children_of(&child).expect("children").is_empty(),
        "depth-refused call must not create a grandchild"
    );

    // Depth 2: the same child may spawn once more.
    runtime
        .publish_subagents(Some(catalog(2, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime
        .run_turn(params(
            &child,
            "nested again",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .expect("nested turn");
    let grandchildren = harness.db.children_of(&child).expect("children");
    assert_eq!(grandchildren.len(), 1);
    let grandchild = &grandchildren[0];
    let meta = harness.db.session_meta(grandchild).expect("meta");
    assert_eq!(meta.parent_id.as_deref(), Some(child.as_str()));
    assert_eq!(meta.agent.as_deref(), Some("helper"));
}

#[tokio::test]
async fn explicit_model_overrides_child_agent_model() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![agent("helper", false, Some("test/agent-model"))],
        )))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-agent-model",
            serde_json::json!({"agent": "helper", "description": "Agent model", "prompt": "go"}),
        ) + &sse_completed(),
        sse_delta("first child") + &sse_completed(),
        sse_delta("first parent") + &sse_completed(),
        subagent_call(
            "call-explicit",
            serde_json::json!({
                "agent": "helper", "description": "Explicit model", "prompt": "go",
                "model": "test/small#fast"
            }),
        ) + &sse_completed(),
        sse_delta("second child") + &sse_completed(),
        sse_delta("second parent") + &sse_completed(),
    ]);
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "parent",
            "one",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    runtime
        .run_turn(params("parent", "two", &harness, provider, &NO_CANCEL))
        .await
        .expect("turn");

    let children = harness.db.children_of("parent").expect("children");
    assert_eq!(children.len(), 2);
    let first = harness.db.session_meta(&children[0]).expect("meta");
    assert_eq!(first.model.as_deref(), Some("test/agent-model"));
    let second = harness.db.session_meta(&children[1]).expect("meta");
    assert_eq!(second.model.as_deref(), Some("test/small#fast"));

    let captured = child_requests(&requests);
    assert_eq!(captured[1]["model"], "agent-model");
    assert_eq!(captured[1]["reasoning"]["effort"], serde_json::Value::Null);
    assert_eq!(captured[4]["model"], "small");
    assert_eq!(captured[4]["reasoning"]["effort"], "low");
}

#[tokio::test]
async fn invalid_model_and_variant_fail_before_child_creation() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-unknown-model",
            serde_json::json!({
                "agent": "helper", "description": "Unknown", "prompt": "go", "model": "other/x"
            }),
        ) + &sse_completed(),
        sse_delta("after unknown") + &sse_completed(),
        subagent_call(
            "call-no-variants",
            serde_json::json!({
                "agent": "helper", "description": "No variants", "prompt": "go", "model": "test/m#fast"
            }),
        ) + &sse_completed(),
        sse_delta("after no variants") + &sse_completed(),
        subagent_call(
            "call-bad-variant",
            serde_json::json!({
                "agent": "helper", "description": "Bad variant", "prompt": "go",
                "model": "test/small#nope"
            }),
        ) + &sse_completed(),
        sse_delta("after bad variant") + &sse_completed(),
    ]);
    let provider = provider_of(&base);
    let unknown = runtime
        .run_turn(params(
            "parent",
            "one",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        unknown.calls[0].output.contains(
            "error: Model \"other/x\" is not available. Use the models tool to see what is available."
        ),
        "got: {}",
        unknown.calls[0].output
    );
    let no_variants = runtime
        .run_turn(params(
            "parent",
            "two",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        no_variants.calls[0]
            .output
            .contains("error: Model \"test/m\" has no variants. Omit the variant."),
        "got: {}",
        no_variants.calls[0].output
    );
    let bad_variant = runtime
        .run_turn(params("parent", "three", &harness, provider, &NO_CANCEL))
        .await
        .expect("turn");
    assert!(
        bad_variant.calls[0].output.contains(
            "error: Variant \"nope\" is not available for \"test/small\". Available: fast."
        ),
        "got: {}",
        bad_variant.calls[0].output
    );
    assert!(
        harness
            .db
            .children_of("parent")
            .expect("children")
            .is_empty(),
        "model resolution failures must not create child rows"
    );
}

#[tokio::test]
async fn session_continuation_only_accepts_a_child_of_the_caller() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    runtime.create_session("other").expect("session");
    harness
        .db
        .create_child_session(
            "other",
            "other-kid",
            Some("helper"),
            Some("test/m"),
            Some("Other"),
        )
        .expect("other child");
    harness
        .db
        .set_pref(&format!("{SESSION_LOCATION_PREFIX}other-kid"), "work")
        .expect("pref");

    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-first",
            serde_json::json!({"agent": "helper", "description": "First", "prompt": "first"}),
        ) + &sse_completed(),
        sse_delta("first answer") + &sse_completed(),
        sse_delta("parent first") + &sse_completed(),
    ]);
    runtime
        .run_turn(params(
            "parent",
            "spawn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("spawn turn");
    let child = harness
        .db
        .children_of("parent")
        .expect("children")
        .remove(0);

    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-continue",
            serde_json::json!({
                "agent": "helper", "description": "Continue", "prompt": "again", "sessionID": child
            }),
        ) + &sse_completed(),
        sse_delta("second answer") + &sse_completed(),
        sse_delta("parent second") + &sse_completed(),
    ]);
    runtime
        .run_turn(params(
            "parent",
            "continue",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("continuation turn");
    assert_eq!(
        harness.db.children_of("parent").expect("children"),
        vec![child.clone()],
        "continuation must not create a second child"
    );
    let continued = messages(&harness.db, &child);
    assert_eq!(continued.len(), 4, "got: {continued:?}");
    assert_eq!(continued[2], ("user".to_string(), "again".to_string()));

    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-ghost",
            serde_json::json!({
                "agent": "helper", "description": "Ghost", "prompt": "again", "sessionID": "ghost"
            }),
        ) + &sse_completed(),
        sse_delta("after ghost") + &sse_completed(),
    ]);
    let ghost = runtime
        .run_turn(params(
            "parent",
            "ghost",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("ghost turn");
    assert!(
        ghost.calls[0]
            .output
            .contains("error: Subagent session not found: ghost"),
        "got: {}",
        ghost.calls[0].output
    );

    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-foreign",
            serde_json::json!({
                "agent": "helper", "description": "Foreign", "prompt": "again",
                "sessionID": "other-kid"
            }),
        ) + &sse_completed(),
        sse_delta("after foreign") + &sse_completed(),
    ]);
    let foreign = runtime
        .run_turn(params(
            "parent",
            "foreign",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("foreign turn");
    assert!(
        foreign.calls[0]
            .output
            .contains("error: Session other-kid is not a child of the current session"),
        "got: {}",
        foreign.calls[0].output
    );
    assert_eq!(
        harness.db.children_of("parent").expect("children"),
        vec![child],
        "no rejected call may add a child"
    );
}

#[tokio::test]
async fn empty_child_text_uses_no_text_fallback() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![
        subagent_call(
            "call-empty",
            serde_json::json!({"agent": "helper", "description": "Empty", "prompt": "go"}),
        ) + &sse_completed(),
        sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        report.calls[0]
            .output
            .contains("Subagent completed without a text response."),
        "got: {}",
        report.calls[0].output
    );
}

#[tokio::test]
async fn child_agent_permissions_narrow_the_child_lane_only() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    let mut restricted = agent("restricted", false, None);
    restricted.permissions = [
        ("bash".to_string(), Permission::Deny),
        ("subagent".to_string(), Permission::Deny),
    ]
    .into_iter()
    .collect();
    runtime
        .publish_subagents(Some(catalog(2, vec![restricted])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let child_bash = sse_tool_call(
        "call-child-bash",
        "bash",
        &serde_json::json!({"argv": ["echo", "hi"]}),
    );
    let child_sub = subagent_call(
        "call-child-sub",
        serde_json::json!({"agent": "restricted", "description": "Nested", "prompt": "nested"}),
    );
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-spawn",
            serde_json::json!({"agent": "restricted", "description": "Child", "prompt": "child"}),
        ) + &sse_completed(),
        child_bash + &child_sub + &sse_completed(),
        sse_delta("child final") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
        sse_tool_call(
            "call-parent-bash",
            "bash",
            &serde_json::json!({"argv": ["echo", "hi"]}),
        ) + &sse_completed(),
        sse_delta("parent final two") + &sse_completed(),
    ]);
    let provider = provider_of(&base);
    let spawn = runtime
        .run_turn(params(
            "parent",
            "spawn",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(
        spawn.calls[0].state, "completed",
        "the parent lane allows subagent: {}",
        spawn.calls[0].output
    );
    let child = harness
        .db
        .children_of("parent")
        .expect("children")
        .remove(0);
    assert!(
        harness.db.children_of(&child).expect("children").is_empty(),
        "the child lane must not inherit the parent's subagent authority"
    );

    let captured = child_requests(&requests);
    assert_eq!(
        function_output(&captured[2], "call-child-bash"),
        Some("error: denied bash")
    );
    assert_eq!(
        function_output(&captured[2], "call-child-sub"),
        Some("error: denied subagent")
    );

    // The same parent lane still runs bash normally.
    let parent_bash = runtime
        .run_turn(params("parent", "bash", &harness, provider, &NO_CANCEL))
        .await
        .expect("turn");
    assert_eq!(parent_bash.calls[0].state, "completed");
    assert!(
        parent_bash.calls[0].output.contains("exit 0"),
        "got: {}",
        parent_bash.calls[0].output
    );
}

#[tokio::test]
async fn cancel_mid_child_stream_cancels_child_and_parent() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let first = subagent_call(
        "call-cancel",
        serde_json::json!({"agent": "helper", "description": "Slow", "prompt": "slow"}),
    ) + &sse_completed();
    let base = Fake::start_cancelable(first, Duration::from_secs(30));
    let cancel = AtomicBool::new(false);
    let mut active_during_child = false;
    let result = {
        let turn = runtime.run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &cancel,
        ));
        tokio::pin!(turn);
        loop {
            tokio::select! {
                value = &mut turn => break value,
                _ = tokio::time::sleep(Duration::from_millis(300)), if !cancel.load(Ordering::Relaxed) => {
                    active_during_child = runtime.turn_active();
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    };
    assert!(
        active_during_child,
        "the child turn must not take the single-flight lease"
    );
    let report = result.expect("turn");
    assert_eq!(report.status, TurnStatus::Cancelled);
    let children = harness.db.children_of("parent").expect("children");
    assert_eq!(children.len(), 1);
    let child = &children[0];
    assert_eq!(turn_status(&harness.db, child), "cancelled");
    assert_eq!(turn_status(&harness.db, "parent"), "cancelled");
    assert_eq!(
        messages(&harness.db, child).len(),
        1,
        "a cancelled child has no assistant commit"
    );
    assert_eq!(messages(&harness.db, "parent").len(), 1);
    assert!(!runtime.turn_active(), "lease released after cancellation");
}
#[path = "fixtures/background_children.rs"]
mod background_children;
#[path = "fixtures/command_routing.rs"]
mod command_routing;
#[path = "fixtures/foreground_children.rs"]
mod foreground_children;

#[tokio::test]
async fn r6_profile_request_overlays_reach_root_and_child_requests_only() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    let root_request = oc_adapters::provider::RequestOverlay {
        headers: BTreeMap::from([("x-profile".to_string(), "root-value".to_string())]),
        body: serde_json::json!({"temperature":0.2,"text":{"verbosity":"low"}})
            .as_object()
            .unwrap()
            .clone(),
    };
    runtime
        .publish_workspace(
            Some("ROOT PROFILE"),
            "",
            Vec::new(),
            BTreeMap::new(),
            Some("root-digest".into()),
            Some("tuned".into()),
            None,
            BTreeMap::new(),
            Default::default(),
            root_request,
        )
        .expect("workspace");
    let mut helper = agent("helper", false, None);
    helper.request = oc_adapters::provider::RequestOverlay {
        headers: BTreeMap::from([("x-profile".to_string(), "child-value".to_string())]),
        body: serde_json::json!({"top_p":0.5})
            .as_object()
            .unwrap()
            .clone(),
    };
    runtime
        .publish_subagents(Some(catalog(1, vec![helper])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _, requests, headers) = Fake::start_with_headers(vec![
        subagent_call(
            "call-sub",
            serde_json::json!({"agent": "helper", "description": "Say hi", "prompt": "say hi"}),
        ) + &sse_completed(),
        sse_delta("child says hi") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "work",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let requests = child_requests(&requests);
    let headers = headers.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    for index in [0, 2] {
        assert_eq!(requests[index]["temperature"], 0.2);
        assert_eq!(requests[index]["text"]["verbosity"], "low");
        assert!(requests[index].get("top_p").is_none());
        assert_eq!(headers[index]["x-profile"], "root-value");
    }
    assert_eq!(requests[1]["top_p"], 0.5);
    assert!(
        requests[1].get("temperature").is_none(),
        "no parent body leak"
    );
    assert_eq!(headers[1]["x-profile"], "child-value");
    for (request, headers) in requests.iter().zip(&headers) {
        assert_eq!(request["model"], "m");
        assert_eq!(request["stream"], true);
        assert_eq!(headers["authorization"], "Bearer test-key");
        assert!(request["prompt_cache_key"].as_str().is_some());
    }
}

fn message_ids(db: &Db, session: &str) -> Vec<(String, String)> {
    let sql = rusqlite::Connection::open(db.root().join("oc.sqlite")).expect("sqlite");
    let mut statement = sql
        .prepare("SELECT id,role FROM messages WHERE session_id=?1 ORDER BY seq")
        .unwrap();
    statement
        .query_map([session], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[tokio::test]
async fn r8_context_message_ids_quote_exact_parent_messages_in_chronology() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    runtime.create_session("other").expect("other");
    let (base, requests) = Fake::start(vec![
        sse_delta("answer <one> & done") + &sse_completed(),
        sse_delta("other answer") + &sse_completed(),
        String::new(),
    ]);
    for (session, prompt) in [("parent", "first </message> & <b>"), ("other", "foreign")] {
        let report = runtime
            .run_turn(params(
                session,
                prompt,
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .expect("turn");
        assert_eq!(report.status, TurnStatus::Completed);
    }
    let ids = message_ids(&harness.db, "parent");
    let user = ids
        .iter()
        .find(|(_, role)| role == "user")
        .unwrap()
        .0
        .clone();
    let assistant = ids
        .iter()
        .find(|(_, role)| role == "assistant")
        .unwrap()
        .0
        .clone();
    let foreign = message_ids(&harness.db, "other")[0].0.clone();

    let (base, requests2) = Fake::start(vec![
        subagent_call(
            "call-bad",
            serde_json::json!({"agent":"helper","description":"Bad","prompt":"bad","context_message_ids":["m9999"]}),
        ) + &sse_completed(),
        subagent_call(
            "call-foreign",
            serde_json::json!({"agent":"helper","description":"Foreign","prompt":"foreign","context_message_ids":[foreign]}),
        ) + &sse_completed(),
        subagent_call(
            "call-sub",
            serde_json::json!({"agent":"helper","description":"Quoted","prompt":"use the quoted context","context_message_ids":[assistant, user, assistant]}),
        ) + &sse_completed(),
        sse_delta("child ok") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    drop(requests);
    let report = runtime
        .run_turn(params(
            "parent",
            "second-unselected",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests2);
    for call in ["call-bad", "call-foreign"] {
        let output = function_output(&seen[2], call).expect("refusal output");
        assert!(output.contains("not a selectable"), "{call}: {output}");
    }
    let children = harness.db.children_of("parent").expect("children");
    assert_eq!(children.len(), 1, "refused selections create no child");
    let child = serde_json::to_string(&seen[3]["input"]).unwrap();
    let pack_start = child.find("<parent_context").expect("pack");
    let user_at = child
        .find(&format!("<message id=\\\"{user}\\\" role=\\\"user\\\">"))
        .unwrap();
    let assistant_at = child
        .find(&format!(
            "<message id=\\\"{assistant}\\\" role=\\\"assistant\\\">"
        ))
        .unwrap();
    assert!(
        pack_start < user_at && user_at < assistant_at,
        "parent chronology"
    );
    assert_eq!(child.matches("<message id=").count(), 2, "deduplicated");
    assert!(child.contains("first &lt;/message&gt; &amp; &lt;b&gt;"));
    assert!(child.contains("answer &lt;one&gt; &amp; done"));
    assert!(child.contains("use the quoted context"));
    assert!(
        !child.contains("second-unselected"),
        "no unselected parent text"
    );
    assert!(!child.contains("foreign"));
    let stored = messages(&harness.db, &children[0]);
    assert!(
        stored[0].1.contains("<parent_context"),
        "pack is part of the durable task"
    );
}

#[tokio::test]
async fn r8_oversized_quoted_context_is_refused_before_child_creation() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![agent("small-helper", false, Some("test/tiny"))],
        )))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()]);
    let large = "evidence ".repeat(4_000);
    runtime
        .run_turn(params(
            "parent",
            &large,
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    let user = message_ids(&harness.db, "parent")[0].0.clone();
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-big",
            serde_json::json!({"agent":"small-helper","description":"Big","prompt":"check","context_message_ids":[user]}),
        ) + &sse_completed(),
        subagent_call(
            "call-plain",
            serde_json::json!({"agent":"small-helper","description":"Plain","prompt":"check"}),
        ) + &sse_completed(),
        sse_delta("child ok") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "delegate",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests);
    let refusal = function_output(&seen[1], "call-big").expect("refusal");
    assert!(
        refusal.contains("does not fit the small-helper subagent request"),
        "{refusal}"
    );
    assert!(refusal.contains("Select fewer messages"), "{refusal}");
    let children = harness.db.children_of("parent").expect("children");
    assert_eq!(children.len(), 1, "only the plain call created a child");
    assert!(
        seen.iter().all(
            |request| !request.to_string().contains("evidence evidence evidence")
                || request["model"] == "m"
        ),
        "no oversized quoted request reached the tiny child model"
    );
}

#[tokio::test]
async fn r8_subagent_guidance_separates_automatic_assembly_from_caller_context() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, requests) = Fake::start(vec![sse_delta("ok") + &sse_completed()]);
    runtime
        .run_turn(params(
            "parent",
            "hi",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    let seen = child_requests(&requests);
    let tool = seen[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "subagent")
        .expect("subagent tool");
    let description = tool["description"].as_str().unwrap();
    for expected in [
        "automatically receives its own profile prompt, environment, applicable AGENTS instructions",
        "Your conversation and findings are not shared",
        "context_message_ids",
        "workspace is shared rather than a separate sandbox",
    ] {
        assert!(description.contains(expected), "{expected}: {description}");
    }
    let ids = &tool["parameters"]["properties"]["context_message_ids"];
    assert_eq!(ids["type"], "array");
    assert_eq!(ids["maxItems"], 64);
}

#[tokio::test]
async fn r10_skill_preview_is_ordered_and_filtered_per_lane_policy() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    let skills = [
        ("zeta", "---\ndescription: Z skill\n---\nZETA_BODY"),
        ("alpha", "---\ndescription: A skill\n---\nALPHA_BODY"),
        ("secret", "---\ndescription: S skill\n---\nSECRET_BODY"),
        (
            "manual",
            "---\ndescription: M skill\nmetadata:\n  opencode/autoinvoke: \"FALSE\"\n---\nMANUAL_BODY",
        ),
        ("bare", "---\nname: Bare\n---\nBARE_BODY"),
    ]
    .map(|(id, text)| (id.to_string(), text.to_string()))
    .to_vec();
    let rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission": {"skill": {"secret": "deny"}}}),
    )
    .unwrap();
    runtime
        .publish_workspace(
            None,
            "",
            skills,
            BTreeMap::new(),
            None,
            Some("build".into()),
            None,
            BTreeMap::new(),
            rules,
            Default::default(),
        )
        .expect("workspace");
    let mut helper = agent("helper", false, None);
    helper.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission": {"skill": {"zeta": "deny"}}}),
    )
    .unwrap();
    runtime
        .publish_subagents(Some(catalog(1, vec![helper])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-sub",
            serde_json::json!({"agent": "helper", "description": "Skills", "prompt": "list"}),
        ) + &sse_completed(),
        sse_delta("child ok") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "go",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests);
    let preview = |request: &serde_json::Value| {
        request["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.to_string())
            .find(|text| text.contains("Available native skills"))
    };
    let parent = preview(&seen[0]).expect("parent preview");
    let child = preview(&seen[1]).expect("child preview");
    assert!(
        parent.find("alpha").unwrap() < parent.find("zeta").unwrap(),
        "{parent}"
    );
    for absent in ["secret", "manual", "bare", "_BODY"] {
        assert!(!parent.contains(absent), "{absent}: {parent}");
        assert!(!child.contains(absent), "{absent}: {child}");
    }
    assert!(
        child.contains("alpha") && !child.contains("zeta"),
        "{child}"
    );
    let whole = serde_json::to_string(&seen).unwrap();
    assert!(
        !whole.contains("ALPHA_BODY"),
        "bodies only through the skill tool"
    );
}

#[tokio::test]
async fn r10_file_tool_family_follows_each_lane_model_and_next_root_request() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            1,
            vec![
                agent("own", false, Some("test/m")),
                agent("inherit", false, None),
            ],
        )))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-own",
            serde_json::json!({"agent": "own", "description": "Own", "prompt": "own model"}),
        ) + &sse_completed(),
        sse_delta("own ok") + &sse_completed(),
        subagent_call(
            "call-inherit",
            serde_json::json!({"agent": "inherit", "description": "Inherit", "prompt": "parent model"}),
        ) + &sse_completed(),
        sse_delta("inherit ok") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
        sse_delta("switched final") + &sse_completed(),
    ]);
    let mut first = params(
        "parent",
        "delegate",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    first.model_id = "gpt-parent".into();
    let report = runtime.run_turn(first).await.expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    // User switches the root model between turns (TOOL12 owns busy commits).
    let report = runtime
        .run_turn(params(
            "parent",
            "after switch",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("switched turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests);
    assert_eq!(seen.len(), 6);
    let family = |request: &serde_json::Value| {
        let mut names: Vec<String> = request["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .filter(|name| matches!(*name, "apply_patch" | "edit" | "write"))
            .map(str::to_string)
            .collect();
        names.sort();
        let guidance: Vec<String> = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.to_string())
            .filter(|text| text.contains("Current request file-mutation tools"))
            .collect();
        assert_eq!(guidance.len(), 1, "one guidance item: {guidance:?}");
        (
            request["model"].as_str().unwrap().to_string(),
            names,
            guidance[0].clone(),
        )
    };
    let (model, names, guidance) = family(&seen[0]);
    assert_eq!(
        (model.as_str(), names.as_slice()),
        ("gpt-parent", ["apply_patch".to_string()].as_slice())
    );
    assert!(guidance.contains("tools: apply_patch."), "{guidance}");
    let (model, names, guidance) = family(&seen[1]);
    assert_eq!(model, "m", "own-model child");
    assert_eq!(names, ["edit", "write"]);
    assert!(
        guidance.contains("tools: write, edit.") && !guidance.contains("apply_patch"),
        "{guidance}"
    );
    let (model, names, guidance) = family(&seen[3]);
    assert_eq!(model, "gpt-parent", "inheriting child");
    assert_eq!(names, ["apply_patch"]);
    assert!(guidance.contains("tools: apply_patch."), "{guidance}");
    let (model, names, guidance) = family(&seen[5]);
    assert_eq!(model, "m", "next root request after the switch");
    assert_eq!(names, ["edit", "write"]);
    assert!(!guidance.contains("apply_patch"), "{guidance}");
    for request in &seen {
        let fixed = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| {
                item.to_string().contains("Available subagents")
                    || item.to_string().contains("You are opencode")
            })
            .count();
        assert!(fixed <= 1, "no duplicated fixed lane");
    }
}

#[tokio::test]
async fn r9_child_renews_own_task_pack_hot_before_final_and_keeps_raw() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(1, vec![agent("helper", false, None)])))
        .expect("catalog");
    runtime.create_session("parent").expect("session");
    let (base, _) = Fake::start(vec![sse_delta("PACK_FACT answer") + &sse_completed()]);
    runtime
        .run_turn(params(
            "parent",
            "PACK_FACT source",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("seed");
    let user = message_ids(&harness.db, "parent")[0].0.clone();
    std::fs::write(harness._project.path().join("note.txt"), "note").unwrap();
    // Parent user m0003 → child task m0004 (one global message sequence).
    let renew = |call: &str, summary: &str| {
        sse_tool_call(
            call,
            "compress",
            &serde_json::json!({"topic":"renew","content":[{"startId":"m0004","endId":"m0004","summary":summary}]}),
        ) + &sse_completed()
    };
    let (base, requests) = Fake::start(vec![
        subagent_call(
            "call-sub",
            serde_json::json!({"agent":"helper","description":"Renew","prompt":"TASK_DETAIL_OBSOLETE do work","context_message_ids":[user]}),
        ) + &sse_completed(),
        sse_tool_call(
            "child-read",
            "read",
            &serde_json::json!({"path":"note.txt"}),
        ) + &sse_completed(),
        renew("child-renew-1", "WORKING_SUMMARY_ONE keep PACK_FACT"),
        renew("child-renew-2", "WORKING_SUMMARY_TWO keep PACK_FACT"),
        sse_delta("child final") + &sse_completed(),
        sse_delta("parent final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "parent",
            "delegate",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests);
    assert_eq!(seen.len(), 6);
    let first = seen[1]["input"].to_string();
    assert!(
        first.contains("TASK_DETAIL_OBSOLETE") && first.contains("parent_context"),
        "exact first delivery"
    );
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let outputs: Vec<String> = sql
        .prepare("SELECT output FROM tool_operations WHERE name='compress' ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(outputs.len(), 2);
    assert!(
        outputs.iter().all(|o| o.contains("task_renewal_accepted")),
        "{outputs:?}"
    );
    assert!(
        function_output(&seen[3], "child-renew-1").is_none(),
        "renewal group stays in RAW only"
    );
    for (index, current, previous) in [
        (3, "WORKING_SUMMARY_ONE", None),
        (4, "WORKING_SUMMARY_TWO", Some("WORKING_SUMMARY_ONE")),
    ] {
        let wire = seen[index]["input"].to_string();
        assert!(wire.contains(current), "renewed HOT in request {index}");
        assert!(
            !wire.contains("TASK_DETAIL_OBSOLETE") && !wire.contains("parent_context"),
            "task/pack replaced in request {index}"
        );
        if let Some(previous) = previous {
            assert!(
                !wire.contains(previous),
                "successive renewals replace, not accumulate"
            );
        }
        assert!(
            function_output(&seen[index], "child-read").is_some(),
            "closed groups retained"
        );
        assert!(
            !seen[index]["input"].as_array().unwrap().iter().any(|item| {
                matches!(item["role"].as_str(), Some("developer" | "system"))
                    && item.to_string().contains(current)
            }),
            "summary keeps user-level authority"
        );
    }
    let child = harness.db.children_of("parent").unwrap()[0].clone();
    let stored = messages(&harness.db, &child);
    assert!(
        stored[0].1.contains("TASK_DETAIL_OBSOLETE"),
        "RAW message unchanged"
    );
    let (turn, hot): (String, String) = sql
        .query_row(
            "SELECT id,result FROM turns WHERE session_id=?1",
            [&child],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        hot.contains("WORKING_SUMMARY_TWO") && !hot.contains("TASK_DETAIL_OBSOLETE"),
        "latest HOT committed"
    );
    let raw = harness
        .db
        .raw_turn_segment(&turn, 1, 1 << 20)
        .unwrap()
        .expect("sealed RAW segment");
    assert!(
        raw.contains("TASK_DETAIL_OBSOLETE"),
        "original task sealed in RAW"
    );
}

#[tokio::test]
async fn r9_root_task_renewal_and_spanning_range_keeps_tail_refusal() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    std::fs::write(harness._project.path().join("note.txt"), "note").unwrap();
    runtime.create_session("root").expect("session");
    let (base, requests) = Fake::start(vec![
        sse_tool_call("root-read", "read", &serde_json::json!({"path":"note.txt"}))
            + &sse_completed(),
        sse_tool_call(
            "root-renew",
            "compress",
            &serde_json::json!({"topic":"renew","content":[{"startId":"m0001","endId":"m0001","summary":"ROOT_SUMMARY still needed"}]}),
        ) + &sse_completed(),
        sse_delta("root final") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "root",
            "ROOT_TASK_OBSOLETE long task",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let seen = child_requests(&requests);
    let last = seen[2]["input"].to_string();
    assert!(
        last.contains("ROOT_SUMMARY") && !last.contains("ROOT_TASK_OBSOLETE"),
        "{last}"
    );
    assert!(function_output(&seen[2], "root-read").is_some());
    assert!(
        messages(&harness.db, "root")[0]
            .1
            .contains("ROOT_TASK_OBSOLETE")
    );

    runtime.create_session("span").expect("span session");
    let (base, _) = Fake::start(vec![sse_delta("old answer") + &sse_completed()]);
    runtime
        .run_turn(params(
            "span",
            "OLD question",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("seed");
    let ids = message_ids(&harness.db, "span");
    let (base, requests) = Fake::start(vec![
        sse_tool_call(
            "span-compress",
            "compress",
            &serde_json::json!({"topic":"span","content":[{"startId":ids[0].0,"endId":"m0005","summary":"spanning"}]}),
        ) + &sse_completed(),
        sse_delta("span final") + &sse_completed(),
    ]);
    assert_eq!(message_ids(&harness.db, "span").len(), 2);
    runtime
        .run_turn(params(
            "span",
            "SPAN_TASK",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("span turn");
    assert_eq!(message_ids(&harness.db, "span")[2].0, "m0005");
    let seen = child_requests(&requests);
    let output = function_output(&seen[1], "span-compress").expect("output");
    assert!(output.contains("unfinished tail"), "{output}");
    assert!(seen[1]["input"].to_string().contains("SPAN_TASK"));
}
