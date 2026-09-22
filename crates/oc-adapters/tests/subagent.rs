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
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = worker_queue.clone();
                let hits = worker_hits.clone();
                let requests = worker_requests.clone();
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
        (base, hits, requests)
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
        providers: BTreeMap::new(),
        mcp: BTreeMap::new(),
        permissions,
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
        max_rounds: 4,
    }
}

static NO_CANCEL: AtomicBool = AtomicBool::new(false);

fn agent(id: &str, primary: bool, model: Option<&str>) -> SubagentAgent {
    SubagentAgent {
        id: id.to_string(),
        description: format!("{id} description"),
        primary,
        model: model.map(str::to_string),
        variant: None,
        prompt: format!("You are {id}."),
        permissions: BTreeMap::new(),
        digest: Some(format!("{id}-digest")),
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
async fn unknown_primary_and_background_calls_fail_closed() {
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
        subagent_call(
            "call-bg",
            serde_json::json!({
                "agent": "helper", "description": "Background", "prompt": "go", "background": true
            }),
        ) + &sse_completed(),
        sse_delta("after background") + &sse_completed(),
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
    let background = runtime
        .run_turn(params("parent", "three", &harness, provider, &NO_CANCEL))
        .await
        .expect("turn");
    assert!(
        background.calls[0]
            .output
            .contains("background subagents are not supported yet"),
        "got: {}",
        background.calls[0].output
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
