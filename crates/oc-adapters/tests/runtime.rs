//! T24 (STORE05/TOOL09/DCP08): runtime turn loop against a fake Responses
//! server — completion/drain, unified permission path, MCP fail-fast,
//! Location binding, reload, compress, commands, AUD11/AUD12 outcomes.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::config::{Generation, McpEntry, Permission};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{
    COMMAND_BYTES_CAP, Runtime, ToolCallEvent, TurnParams, TurnStatus, expand_command,
};
use oc_adapters::storage::Db;
use oc_core::context_plan::ProtectedSpec;

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n".to_string()
}

fn sse_completed_usage(input_tokens: u64, output_tokens: u64) -> String {
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"usage\":{{\"input_tokens\":{input_tokens},\"output_tokens\":{output_tokens}}}}}}}\n\n"
    )
}

/// Reasoning summary delta (`response.reasoning_summary_text.delta`).
fn sse_reasoning(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.reasoning_summary_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
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

/// Scripted fakes: serve queued SSE bodies in order, then repeat the last.
struct Fake;

type CapturedRequests = Arc<Mutex<Vec<serde_json::Value>>>;

impl Fake {
    /// Stall the body `stall` after sending headers immediately: models a
    /// hung stream where cancel must land without waiting for bytes.
    fn start_stalled(script: Vec<String>, stall: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = queue.clone();
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
                    let payload = {
                        let mut queue = queue.lock().expect("queue");
                        if queue.len() > 1 {
                            queue.pop_front().expect("script")
                        } else {
                            queue.front().cloned().unwrap_or_default()
                        }
                    };
                    let stream = reader.get_mut();
                    let _ = stream.write_all(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                            .as_bytes(),
                    );
                    let _ = stream.flush();
                    // Drip heartbeats so a cancelled stream observes the flag
                    // promptly instead of sitting inside one chunk wait.
                    let start = std::time::Instant::now();
                    while start.elapsed() < stall {
                        let _ = stream.write_all(b": hb\n\n");
                        let _ = stream.flush();
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    let _ = stream.write_all(payload.as_bytes());
                });
            }
        });
        base
    }

    fn start(script: Vec<String>, delay: Duration) -> (String, Arc<Mutex<usize>>) {
        let (base, hits, _) = Self::start_recording(script, delay);
        (base, hits)
    }

    fn start_recording(
        script: Vec<String>,
        delay: Duration,
    ) -> (String, Arc<Mutex<usize>>, CapturedRequests) {
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
                    std::thread::sleep(delay);
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
        let _ = &queue;
        (base, hits, requests)
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
    ]
    .into_iter()
    .map(|name| (name.to_string(), Permission::Allow))
    .collect()
}

fn make_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let catalog = ModelCatalog {
        provider: "test".to_string(),
        models: [(
            "m".to_string(),
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        )]
        .into_iter()
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

fn runtime_of<'a>(
    harness: &'a Harness,
    generation: Generation,
    protected: Vec<String>,
) -> Runtime<'a> {
    runtime_with_dcp(harness, generation, protected, DcpConfig::default())
}

fn runtime_with_dcp<'a>(
    harness: &'a Harness,
    generation: Generation,
    protected: Vec<String>,
    dcp_config: DcpConfig,
) -> Runtime<'a> {
    let project = harness._project.path();
    let files = oc_adapters::files::Files::new(project, harness._data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(project).expect("shell");
    Runtime::new(
        &harness.db,
        "work",
        generation,
        ProtectedGlobs {
            patterns: protected,
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
        dcp_config,
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

fn dcp_nudge_count(request: &serde_json::Value) -> usize {
    request["input"]
        .to_string()
        .matches("exceeds soft limit")
        .count()
}

fn function_call<'a>(
    request: &'a serde_json::Value,
    call_id: &str,
) -> Option<&'a serde_json::Value> {
    request["input"]
        .as_array()?
        .iter()
        .find(|item| item["type"] == "function_call" && item["call_id"].as_str() == Some(call_id))
}

fn function_output<'a>(request: &'a serde_json::Value, call_id: &str) -> Option<&'a str> {
    request["input"].as_array()?.iter().find_map(|item| {
        (item["type"] == "function_call_output" && item["call_id"].as_str() == Some(call_id))
            .then(|| item["output"].as_str())
            .flatten()
    })
}

fn function_item_count(request: &serde_json::Value, kind: &str, call_id: &str) -> usize {
    request["input"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == kind && item["call_id"].as_str() == Some(call_id))
        .count()
}

#[tokio::test]
async fn aud06_intent_failure_prevents_patch() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_intent BEFORE INSERT ON tool_operations BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END;").unwrap();
    let tool = sse_tool_call(
        "call-patch",
        "apply_patch",
        &serde_json::json!({"patchText": "*** Begin Patch\n*** Add File: sentinel\n+must not exist\n*** End Patch"}),
    );
    let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    let result = runtime
        .run_turn(params(
            "s",
            "patch",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert!(
        !harness._project.path().join("sentinel").exists(),
        "mutation ran before durable intent"
    );
}

#[tokio::test]
async fn aud07_rejected_input_has_no_turn_or_event() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_input BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'injected input failure'); END;").unwrap();
    let result = runtime
        .run_turn_with_events(
            params(
                "s",
                "input",
                &harness,
                provider_of("http://127.0.0.1:9"),
                &NO_CANCEL,
            ),
            |_| panic!("rejected input acknowledged"),
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    let turns: i64 = sql
        .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0, "unaccepted input left a started turn");
    let events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind != 'session_created'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(events, 0);
    assert!(harness.db.read_history("s").unwrap().is_empty());
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert!(reopened.read_history("s").unwrap().is_empty());
}

#[tokio::test]
async fn aud07_terminal_failure_does_not_commit_assistant() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_terminal BEFORE UPDATE ON turns WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'injected terminal failure'); END;").unwrap();
    let (base, _) = Fake::start(
        vec![sse_delta("must not commit") + &sse_completed()],
        Duration::ZERO,
    );
    let result = runtime
        .run_turn(params(
            "s",
            "input",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert_eq!(
        harness.db.read_history("s").unwrap(),
        [("user".to_string(), "input".to_string())]
    );
    let terminal: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'turn_finished'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(terminal, 0);
    let (status, checkpoint): (String, Option<String>) = sql
        .query_row("SELECT status, result FROM turns", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(status, "started");
    assert!(
        checkpoint.is_some(),
        "generation checkpoint must succeed before the terminal commit fails"
    );
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert_eq!(
        reopened.read_history("s").unwrap(),
        [("user".to_string(), "input".to_string())]
    );
}

#[tokio::test]
async fn aud07_mixed_order_and_mcp_storage_failures() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("mcp.py");
    let order = harness._project.path().join("order");
    std::fs::write(&script, r#"import json, sys
for line in sys.stdin:
    r = json.loads(line)
    method = r.get('method')
    if method == 'initialize':
        result = {'protocolVersion': '2025-11-25', 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'fixture', 'version': '1'}}
    elif method == 'tools/list':
        result = {'tools': [{'name': 'mark', 'description': 'mark', 'inputSchema': {'type': 'object'}}]}
    elif method == 'tools/call':
        with open(sys.argv[1], 'a') as f: f.write('M\n')
        result = {'content': [{'type': 'text', 'text': 'marked'}], 'isError': False}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': r['id'], 'result': result}), flush=True)
"#).unwrap();
    generation
        .permissions
        .insert("fixture__mark".to_string(), Permission::Allow);
    generation.mcp.insert(
        "fixture".to_string(),
        McpEntry {
            kind: "local".to_string(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".to_string(),
                script.to_string_lossy().into_owned(),
                order.to_string_lossy().into_owned(),
            ],
            timeout: Some(2000),
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let first = sse_tool_call(
        "builtin-first",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'B1\\n' >> order"]}),
    );
    let middle = sse_tool_call("mcp-middle", "fixture__mark", &serde_json::json!({}));
    let last = sse_tool_call(
        "builtin-last",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'B2\\n' >> order"]}),
    );
    let (base, _) = Fake::start(
        vec![first + &middle + &last + &sse_completed()],
        Duration::ZERO,
    );
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    for fault in ["", "intent", "outcome"] {
        std::fs::write(&order, "").unwrap();
        match fault {
            "intent" => sql.execute_batch("CREATE TRIGGER fail_mcp BEFORE INSERT ON tool_operations WHEN NEW.name = 'fixture__mark' BEGIN SELECT RAISE(ABORT, 'injected MCP intent'); END;").unwrap(),
            "outcome" => sql.execute_batch("CREATE TRIGGER fail_mcp BEFORE UPDATE ON tool_operations WHEN NEW.name = 'fixture__mark' BEGIN SELECT RAISE(ABORT, 'injected MCP outcome'); END;").unwrap(),
            _ => {},
        }
        let mut p = params(
            "s",
            "ordered tools",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        );
        p.max_rounds = 1;
        let result = runtime.run_turn(p).await;
        if fault.is_empty() {
            let report = result.unwrap();
            assert_eq!(
                report
                    .calls
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                ["bash", "fixture__mark", "bash"]
            );
            assert_eq!(std::fs::read_to_string(&order).unwrap(), "B1\nM\nB2\n");
            let ops = harness.db.list_tool_ops("s").unwrap();
            assert!(ops[1].op.ends_with("mcp-middle"));
            assert_eq!(ops[1].turn.as_deref(), Some(report.turn_id.as_str()));
        } else {
            assert_eq!(
                result.unwrap_err(),
                oc_adapters::runtime::RuntimeError::Storage
            );
            assert_eq!(
                std::fs::read_to_string(&order).unwrap(),
                if fault == "intent" { "B1\n" } else { "B1\nM\n" }
            );
            sql.execute_batch("DROP TRIGGER fail_mcp").unwrap();
        }
    }
}

#[tokio::test]
async fn text_turn_completes_and_drains() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let (base, _) = Fake::start(vec![sse_delta("hi") + &sse_completed()], Duration::ZERO);

    let report = runtime
        .run_turn(params(
            "s",
            "hello",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "hi");
    assert_eq!(report.rounds, 1);
    assert_eq!(report.usage, Some((10, 5)));

    let history = harness.db.read_history("s").expect("history");
    assert_eq!(
        history,
        [
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "hi".to_string())
        ]
    );
    let (status, _) = harness.db.turn_result(&report.turn_id).expect("turn row");
    assert_eq!(status, "completed");

    // No retained per-turn state: a second turn runs cleanly.
    let report2 = runtime
        .run_turn(params(
            "s",
            "again",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn2");
    assert_eq!(report2.status, TurnStatus::Completed);
}

#[tokio::test]
async fn aud11_text_without_successful_terminal_never_completes() {
    for (terminal, expected, stored) in [
        ("", TurnStatus::Incomplete, "incomplete"),
        (
            "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\"}}}\n\n",
            TurnStatus::Failed,
            "failed",
        ),
        (
            "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n",
            TurnStatus::Incomplete,
            "incomplete",
        ),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let (base, hits) = Fake::start(vec![sse_delta("partial") + terminal], Duration::ZERO);
        let mut observed = String::new();
        let report = runtime
            .run_turn_with_events(
                params("s", "hello", &harness, provider_of(&base), &NO_CANCEL),
                |_| {},
                |_, delta| observed.push_str(delta),
                |_, _| {},
            )
            .await
            .unwrap();
        assert_eq!(
            observed, "partial",
            "fixture must deliver a valid text delta"
        );
        assert_eq!(report.status, expected);
        assert!(report.calls.is_empty());
        assert_eq!(harness.db.turn_result(&report.turn_id).unwrap().0, stored);
        assert_eq!(
            harness.db.read_history("s").unwrap(),
            [("user".to_string(), "hello".to_string())],
            "partial assistant must not be committed as a completed answer"
        );
        assert_eq!(*hits.lock().unwrap(), 1, "no hidden generation retry");
    }
}

#[tokio::test]
async fn aud11_incomplete_call_never_executes() {
    for terminal in [
        "".to_string(),
        "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\"}}\n\n"
            .to_string(),
        "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\n\n".to_string(),
        sse_completed(),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let args = serde_json::json!({"argv": ["/bin/sh", "-c", "printf unexpected >> sentinel"]});
        let added = serde_json::json!({"type": "response.output_item.added", "item": {
            "type": "function_call", "id": "fc_partial", "call_id": "call_partial",
            "name": "bash", "arguments": "", "status": "in_progress"
        }});
        let delta = serde_json::json!({"type": "response.function_call_arguments.delta",
            "item_id": "fc_partial", "delta": args.to_string()});
        // Even valid JSON arguments cannot substitute for output_item.done.
        let (base, hits) = Fake::start(
            vec![format!("data: {added}\n\ndata: {delta}\n\n{terminal}")],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params("s", "run", &harness, provider_of(&base), &NO_CANCEL))
            .await
            .unwrap();
        assert_ne!(report.status, TurnStatus::Completed, "{terminal}");
        assert!(report.calls.is_empty(), "unfinished batch executed");
        assert!(harness.db.list_tool_ops("s").unwrap().is_empty());
        assert!(!harness._project.path().join("sentinel").exists());
        assert_ne!(
            harness.db.turn_result(&report.turn_id).unwrap().0,
            "completed"
        );
        assert_eq!(*hits.lock().unwrap(), 1, "no hidden generation retry");
    }
}

#[tokio::test]
async fn aud11_round_exhaustion_retains_output_without_replaying_effect_after_restart() {
    let (mut harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").unwrap();
    let tool = sse_tool_call(
        "call_effect",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'once\\n' >> effects; printf durable-output"]}),
    );
    let (base, hits, requests) = Fake::start_recording(
        vec![
            tool + &sse_completed(),
            sse_delta("resumed") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut turn = params(
        "s",
        "record effect",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    turn.max_rounds = 1;
    let report = runtime.run_turn(turn).await.unwrap();
    assert_eq!(report.status, TurnStatus::Incomplete);
    assert_eq!(report.rounds, 1);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 1, "round budget must stop requests");
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("effects")).unwrap(),
        "once\n"
    );
    let (status, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    assert_eq!(status, "incomplete");
    let journal: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    let input = journal["input"].as_array().unwrap();
    let output = input
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .unwrap()
        .clone();
    assert_eq!(output["call_id"], "call_effect");
    assert!(
        output["output"]
            .as_str()
            .unwrap()
            .contains("durable-output")
    );
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .count(),
        1
    );
    assert!(input.iter().any(|item| item["type"] == "function_call"
        && item["id"] == "fc_call_effect"
        && item["call_id"] == "call_effect"));

    drop(runtime);
    drop(harness.db);
    harness.db = Db::open(harness._data.path()).unwrap();
    assert_eq!(
        harness.db.turn_result(&report.turn_id).unwrap().0,
        "incomplete"
    );
    let reopened: serde_json::Value =
        serde_json::from_str(&harness.db.turn_result(&report.turn_id).unwrap().1.unwrap()).unwrap();
    assert_eq!(
        reopened, journal,
        "recovery must preserve the durable wire journal"
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.open_session("s").unwrap();
    let resumed = runtime
        .run_turn(params(
            "s",
            "continue",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(resumed.status, TurnStatus::Completed);
    assert!(resumed.calls.is_empty());
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("effects")).unwrap(),
        "once\n"
    );
    let ops = harness.db.list_tool_ops("s").unwrap();
    assert_eq!(
        ops.len(),
        1,
        "restart must not execute the prior effect again"
    );
    assert_eq!(ops[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 2);
    let requests = requests.lock().unwrap();
    let continuation = requests[1]["input"].as_array().unwrap();
    assert_eq!(
        continuation.iter().filter(|item| **item == output).count(),
        1
    );
    assert!(
        continuation
            .iter()
            .any(|item| item["type"] == "function_call"
                && item["id"] == "fc_call_effect"
                && item["call_id"] == "call_effect")
    );
}

#[tokio::test]
async fn tool_rounds_execute_and_record() {
    let (harness, generation) = make_harness(allow_all());
    std::fs::write(harness._project.path().join("note.txt"), "file-bytes").expect("seed");
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let tool = sse_tool_call("i1", "read", &serde_json::json!({"path": "note.txt"}));
    let (base, _) = Fake::start(
        vec![
            tool + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );

    let report = runtime
        .run_turn(params(
            "s",
            "read it",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.rounds, 2);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].name, "read");
    assert_eq!(report.calls[0].state, "completed");
    assert!(
        report.calls[0].output.contains("file-bytes"),
        "{}",
        report.calls[0].output
    );

    let ops = harness.db.list_tool_ops("s").expect("ops");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].state, "completed");
}

#[tokio::test]
async fn denied_and_ask_tools_fail_visibly() {
    for permission in [Permission::Deny, Permission::Ask] {
        let mut permissions = allow_all();
        permissions.insert("bash".to_string(), permission);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").expect("create");
        let tool = sse_tool_call("i1", "bash", &serde_json::json!({"argv": ["echo", "x"]}));
        let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
        let report = runtime
            .run_turn(params("s", "run", &harness, provider_of(&base), &NO_CANCEL))
            .await
            .expect("turn");
        assert_eq!(report.calls[0].state, "failed");
        assert!(
            report.calls[0].output.contains("denied"),
            "{}",
            report.calls[0].output
        );
        let ops = harness.db.list_tool_ops("s").expect("ops");
        assert_eq!(ops[0].state, "failed");
    }
}

#[tokio::test]
async fn protected_patch_never_reaches_disk() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, vec!["*.secret".to_string()]);
    runtime.create_session("s").expect("create");
    let patch = "*** Begin Patch\n*** Add File: x.secret\n+boe\n*** End Patch\n";
    let tool = sse_tool_call(
        "i1",
        "apply_patch",
        &serde_json::json!({"patchText": patch}),
    );
    let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    let report = runtime
        .run_turn(params(
            "s",
            "patch",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.calls[0].state, "failed");
    assert!(
        report.calls[0].output.contains("protected"),
        "{}",
        report.calls[0].output
    );
    assert!(
        !harness._project.path().join("x.secret").exists(),
        "legacy deny wins"
    );
}

#[tokio::test]
async fn cancel_drains_to_records() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let base = Fake::start_stalled(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(30),
    );
    let cancel = AtomicBool::new(false);
    let provider = provider_of(&base);
    let (_, report) = tokio::join!(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel.store(true, Ordering::Relaxed);
        },
        runtime.run_turn(params("s", "slow", &harness, provider, &cancel))
    );
    let report = report.expect("cancelled turn");
    assert_eq!(report.status, TurnStatus::Cancelled);
    let history = harness.db.read_history("s").expect("history");
    assert_eq!(history.len(), 1, "user kept, no partial assistant");
    let (status, _) = harness.db.turn_result(&report.turn_id).expect("turn row");
    assert_eq!(status, "cancelled");
}

#[tokio::test]
async fn aud12_cancel_during_mcp_initialize_reaps_child_before_acceptance() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("stall_initialize.py");
    let pid_file = harness._project.path().join("mcp.pid");
    std::fs::write(
        &script,
        r#"import json, os, sys, time
request = json.loads(sys.stdin.readline())
assert request['method'] == 'initialize'
with open(sys.argv[1], 'w') as f:
    f.write(str(os.getpid()))
time.sleep(30)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "stall".to_string(),
        McpEntry {
            kind: "local".to_string(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".to_string(),
                script.to_string_lossy().into_owned(),
                pid_file.to_string_lossy().into_owned(),
            ],
            timeout: Some(30_000),
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, hits) = Fake::start(
        vec![sse_delta("unexpected") + &sse_completed()],
        Duration::ZERO,
    );
    let cancel = AtomicBool::new(false);
    let accepted = AtomicBool::new(false);
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
                // Synchronize on initialize received, not an assumed startup delay.
                loop {
                    if std::fs::read_to_string(&pid_file)
                        .is_ok_and(|text| text.parse::<u32>().is_ok())
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let at = std::time::Instant::now();
                cancel.store(true, Ordering::Relaxed);
                at
            },
            runtime.run_turn_with_events(
                params("s", "cancel attach", &harness, provider_of(&base), &cancel),
                |_| {
                    accepted.store(true, Ordering::Relaxed);
                },
                |_, _| panic!("provider started before MCP attached"),
                |_, _| {},
            )
        )
    })
    .await;
    let pid: u32 = std::fs::read_to_string(&pid_file)
        .expect("child received initialize")
        .parse()
        .unwrap();
    let process = std::path::PathBuf::from(format!("/proc/{pid}"));
    let cleanup = tokio::time::timeout(Duration::from_secs(1), async {
        while process.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    if cleanup.is_err() {
        // A failing regression must not leave this fixture running for 30 seconds.
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
    let (cancelled_at, result) = outcome.expect("attach ignored cancellation for five seconds");
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(1),
        "cancel/child cleanup exceeded one second"
    );
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Cancelled
    );
    assert!(cleanup.is_ok(), "stdio child {pid} survived cancellation");
    assert!(!accepted.load(Ordering::Relaxed));
    assert_eq!(*hits.lock().unwrap(), 0);
    assert!(harness.db.read_history("s").unwrap().is_empty());
    assert!(harness.db.list_tool_ops("s").unwrap().is_empty());
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let turns: i64 = sql
        .query_row("SELECT COUNT(*) FROM turns", [], |row| row.get(0))
        .unwrap();
    assert_eq!(turns, 0, "cancelled attach must not begin a turn");
    let events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind != 'session_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(events, 0, "cancelled attach must not acknowledge input");
}

#[tokio::test]
async fn aud23_generation_reuse_then_reload_disable_reaps_the_single_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("generation_mcp.py");
    let lifecycle = harness._project.path().join("generation.log");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r = json.loads(line); method = r.get('method'); result = None
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'generation','version':'1'}}
    elif method == 'tools/list':
        result = {'tools':[{'name':'ping','description':'ping','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "counted".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                lifecycle.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    for prompt in ["first", "second"] {
        assert_eq!(
            runtime
                .run_turn(params(
                    "s",
                    prompt,
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL,
                ))
                .await
                .unwrap()
                .status,
            TurnStatus::Completed
        );
    }
    let log = std::fs::read_to_string(&lifecycle).unwrap();
    let pids = log
        .lines()
        .filter_map(|line| line.strip_prefix("spawn "))
        .map(|pid| pid.parse::<libc::pid_t>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 1, "one child per generation: {log}");
    // SAFETY: signal 0 only probes the fixture child recorded by that child.
    assert_eq!(unsafe { libc::kill(pids[0], 0) }, 0);

    runtime
        .reload(Generation {
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: allow_all(),
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        })
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pids[0], 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final liveness probe only.
    let alive = unsafe { libc::kill(pids[0], 0) };
    assert_ne!(alive, 0, "reload left MCP child");
    assert_eq!(
        runtime
            .run_turn(params(
                "s",
                "disabled",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap()
            .status,
        TurnStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(&lifecycle)
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("spawn "))
            .count(),
        1,
        "disabled generation respawned the server"
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn aud23_tool_list_changed_relists_only_the_dirty_server() {
    let (harness, mut generation) = make_harness(allow_all());
    let changed_script = harness._project.path().join("list_changed.py");
    let changed_log = harness._project.path().join("list_changed.log");
    // The dirty server announces a change after every list, so a notification
    // arriving during a relist must stay pending for the next turn.
    std::fs::write(
        &changed_script,
        r#"import json, os, sys
count = 0
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        with open(sys.argv[1], 'a') as f: f.write('initialize\n')
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{'listChanged':True}},'serverInfo':{'name':'changed','version':'1'}}
    elif method == 'tools/list':
        count += 1
        with open(sys.argv[1], 'a') as f: f.write('list\n')
        result={'tools':[{'name':('old' if count % 2 == 1 else 'new'),'description':'changed','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
        if method == 'tools/list':
            print(json.dumps({'jsonrpc':'2.0','method':'notifications/tools/list_changed'}), flush=True)
"#,
    )
    .unwrap();
    let stable_script = harness._project.path().join("stable.py");
    let stable_log = harness._project.path().join("stable.log");
    std::fs::write(
        &stable_script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        with open(sys.argv[1], 'a') as f: f.write('initialize\n')
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'stable','version':'1'}}
    elif method == 'tools/list':
        with open(sys.argv[1], 'a') as f: f.write('list\n')
        result={'tools':[{'name':'ping','description':'stable','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    for (id, script, log) in [
        ("changed", &changed_script, &changed_log),
        ("stable", &stable_script, &stable_log),
    ] {
        generation.mcp.insert(
            id.into(),
            McpEntry {
                kind: "local".into(),
                url: None,
                enabled: true,
                oauth: false,
                headers: BTreeMap::new(),
                command: vec![
                    "/usr/bin/python3".into(),
                    script.to_string_lossy().into_owned(),
                    log.to_string_lossy().into_owned(),
                ],
                timeout: Some(2_000),
                codemode: None,
            },
        );
    }
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _, requests) =
        Fake::start_recording(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    for prompt in ["first", "second", "third"] {
        assert_eq!(
            runtime
                .run_turn(params(
                    "s",
                    prompt,
                    &harness,
                    provider_of(&base),
                    &NO_CANCEL,
                ))
                .await
                .unwrap()
                .status,
            TurnStatus::Completed
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    {
        let requests = requests.lock().unwrap();
        let names = |request: &serde_json::Value| {
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert!(names(&requests[0]).contains(&"changed__old".to_string()));
        assert!(names(&requests[1]).contains(&"changed__new".to_string()));
        assert!(names(&requests[2]).contains(&"changed__old".to_string()));
        for request in requests.iter() {
            assert!(names(request).contains(&"stable__ping".to_string()));
        }
    }
    let changed = std::fs::read_to_string(&changed_log).unwrap();
    assert_eq!(
        changed.lines().filter(|line| *line == "initialize").count(),
        1
    );
    assert_eq!(
        changed.lines().filter(|line| *line == "list").count(),
        3,
        "notification during relist was lost: {changed}"
    );
    let stable = std::fs::read_to_string(&stable_log).unwrap();
    assert_eq!(
        stable.lines().filter(|line| *line == "initialize").count(),
        1
    );
    assert_eq!(
        stable.lines().filter(|line| *line == "list").count(),
        1,
        "dirty server forced an unrelated relist: {stable}"
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn aud23_aborted_turn_releases_lease_and_shutdown_reaps_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("aborted_turn.py");
    let lifecycle = harness._project.path().join("aborted_turn.log");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'a') as f: f.write('spawn %d\n' % os.getpid())
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'aborted','version':'1'}}
    elif method == 'tools/list':
        result={'tools':[{'name':'ping','description':'ping','inputSchema':{'type':'object'}}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "counted".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                lifecycle.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (slow, _) = Fake::start(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(3),
    );
    {
        let pending = runtime.run_turn(params(
            "s",
            "aborted",
            &harness,
            provider_of(&slow),
            &NO_CANCEL,
        ));
        tokio::pin!(pending);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let spawned = std::fs::read_to_string(&lifecycle)
                .map(|log| log.contains("spawn "))
                .unwrap_or(false);
            if spawned {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "child never spawned");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
                _ = &mut pending => panic!("turn finished before the abort"),
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        // `pending` is dropped here while the provider stream is still open.
    }
    // The single-flight lease must be released by the dropped future.
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    assert_eq!(
        runtime
            .run_turn(params(
                "s",
                "after the abort",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .expect("lease released after abort")
            .status,
        TurnStatus::Completed
    );
    runtime.shutdown_mcp().await.unwrap();
    let pid = std::fs::read_to_string(&lifecycle)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("spawn "))
        .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        .expect("recorded child pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final fixture liveness probe only.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "shutdown left the generation child alive");
}

#[tokio::test]
async fn aud23_server_cap_blocks_spawn_before_first_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let marker = harness._project.path().join("cap-spawn.log");
    for index in 0..(oc_adapters::runtime::MAX_MCP_SERVERS + 1) {
        generation.mcp.insert(
            format!("server-{index}"),
            McpEntry {
                kind: "local".into(),
                url: None,
                enabled: true,
                oauth: false,
                headers: BTreeMap::new(),
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    format!("printf 'spawned\\n' >> {}", marker.display()),
                ],
                timeout: Some(2_000),
                codemode: None,
            },
        );
    }
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let error = runtime
        .run_turn(params(
            "s",
            "must not attach",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect_err("server cap must refuse the generation");
    let text = error.to_string();
    assert!(
        text.contains("too many enabled MCP servers"),
        "actionable cap diagnostic: {text}"
    );
    assert!(!marker.exists(), "server cap spawned a child anyway");
    assert_eq!(harness.db.history_len("s").unwrap(), 0, "turn was accepted");
}

#[tokio::test]
async fn aud23_partial_attach_failure_reaps_previously_connected_child() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("partial_attach.py");
    let pid_file = harness._project.path().join("partial.pid");
    std::fs::write(
        &script,
        r#"import json, os, sys
with open(sys.argv[1], 'w') as f: f.write(str(os.getpid()))
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'partial','version':'1'}}
    elif method == 'tools/list': result={'tools':[]}
    if result is not None: print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    generation.mcp.insert(
        "a-good".into(),
        McpEntry {
            kind: "local".into(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: vec![
                "/usr/bin/python3".into(),
                script.to_string_lossy().into_owned(),
                pid_file.to_string_lossy().into_owned(),
            ],
            timeout: Some(2_000),
            codemode: None,
        },
    );
    generation.mcp.insert(
        "b-bad".into(),
        McpEntry {
            kind: "remote".into(),
            url: Some("http://127.0.0.1:9/v1/mcp".into()),
            enabled: true,
            oauth: false,
            headers: BTreeMap::from([(
                "Authorization".into(),
                "Bearer fixture-not-a-secret".into(),
            )]),
            command: Vec::new(),
            timeout: Some(200),
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let result = runtime
        .run_turn(params(
            "s",
            "must not start",
            &harness,
            provider_of("http://127.0.0.1:9"),
            &NO_CANCEL,
        ))
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::McpAttach {
            server: "b-bad".into(),
            stage: "DNS",
            safe_code: "private_host",
            retryable: false,
        }
    );
    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .parse::<libc::pid_t>()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final fixture liveness probe only.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "partial attach leaked child");
    assert_eq!(harness.db.history_len("s").unwrap(), 0);
}

#[tokio::test]
async fn mcp_attach_failure_is_loud() {
    let (harness, mut generation) = make_harness(allow_all());
    generation.mcp.insert(
        "codex".to_string(),
        McpEntry {
            kind: "remote".to_string(),
            url: Some("http://127.0.0.1:9/v1/mcp".to_string()),
            enabled: true,
            oauth: false,
            headers: [("authorization".to_string(), "Bearer k".to_string())]
                .into_iter()
                .collect(),
            command: Vec::new(),
            timeout: None,
            codemode: None,
        },
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let (base, _) = Fake::start(vec![sse_delta("hi") + &sse_completed()], Duration::ZERO);
    let error = runtime
        .run_turn(params("s", "hi", &harness, provider_of(&base), &NO_CANCEL))
        .await
        .expect_err("attach must fail");
    assert_eq!(
        error,
        oc_adapters::runtime::RuntimeError::McpAttach {
            server: "codex".to_string(),
            stage: "DNS",
            safe_code: "private_host",
            retryable: false,
        }
    );
    assert_eq!(
        harness.db.history_len("s").expect("len"),
        0,
        "no turn begun"
    );
}

#[test]
fn location_binding_holds() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").expect("create");
    runtime.open_session("s").expect("same location opens");
    let other = runtime_of(&harness, generation, Vec::new());
    // Same Location id ("work") reopens; a foreign one must fail.
    assert!(other.open_session("s").is_ok());
    assert!(other.open_session("ghost").is_err());
}

#[tokio::test]
async fn reload_applies_new_policy_and_guards_active_turn() {
    let (harness, generation) = make_harness(BTreeMap::new());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    // Default-deny: unlisted tools never run.
    let tool = sse_tool_call("i1", "read", &serde_json::json!({"path": "note.txt"}));
    let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    std::fs::write(harness._project.path().join("note.txt"), "file-bytes").expect("seed");
    let report = runtime
        .run_turn(params(
            "s",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        report.calls[0].output.contains("denied"),
        "{}",
        report.calls[0].output
    );

    // Reload between turns publishes id 2 with read allowed.
    let id = runtime
        .reload(Generation {
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: [("read".to_string(), Permission::Allow)]
                .into_iter()
                .collect(),
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        })
        .await
        .expect("reload");
    assert_eq!(id, 2);
    let report = runtime
        .run_turn(params(
            "s",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn2");
    assert_eq!(report.calls[0].state, "completed");

    // Reload during an active turn is refused (slow stream within chunk timeout).
    let (slow_base, _) = Fake::start(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(2),
    );
    let (reload_result, turn_result) = tokio::join!(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            runtime
                .reload(Generation {
                    providers: BTreeMap::new(),
                    mcp: BTreeMap::new(),
                    permissions: BTreeMap::new(),
                    provenance: BTreeMap::new(),
                    warnings: Vec::new(),
                })
                .await
        },
        runtime.run_turn(params(
            "s",
            "slow",
            &harness,
            provider_of(&slow_base),
            &NO_CANCEL
        ))
    );
    assert_eq!(
        reload_result.expect_err("reload during turn"),
        oc_adapters::runtime::RuntimeError::TurnActive
    );
    assert_eq!(
        turn_result.expect("slow turn").status,
        TurnStatus::Completed
    );
}

#[tokio::test]
async fn compress_blocks_compensate_and_stabilize() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    for (i, role) in [
        "user",
        "assistant",
        "user",
        "assistant",
        "user",
        "assistant",
    ]
    .iter()
    .enumerate()
    {
        harness
            .db
            .append_message("s", role, &format!("message {i} body"))
            .expect("msg");
    }
    let ids: Vec<(String, String, String)> = harness.db.read_history_full("s").expect("ids");
    // Binary startup applies the DCP schema; the harness mirrors that wiring.
    oc_adapters::dcp::apply_dcp_schema(&harness.db).expect("dcp schema");
    let spec = ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        file_globs: Vec::new(),
        ..ProtectedSpec::default()
    };
    let args = serde_json::json!({
        "topic": "t",
        "content": [{"startId": ids[0].0, "endId": ids[1].0, "summary": "first"}],
    });
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_compress_intent BEFORE INSERT ON tool_operations BEGIN SELECT RAISE(ABORT, 'injected compress intent'); END;").unwrap();
    assert_eq!(
        runtime.run_compress("s", &args, &spec).unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert!(harness.db.load_compression_blocks("s").unwrap().is_empty());
    sql.execute_batch("DROP TRIGGER fail_compress_intent")
        .unwrap();
    let report = runtime.run_compress("s", &args, &spec).expect("compress");
    assert_eq!(report.blocks, ["b0001".to_string()]);
    assert!(report.shrank);

    let args2 = serde_json::json!({
        "topic": "t",
        "content": [{"startId": ids[2].0, "endId": ids[3].0, "summary": "second"}],
    });
    let report2 = runtime.run_compress("s", &args2, &spec).expect("compress2");
    assert_eq!(
        report2.blocks,
        ["b0002".to_string()],
        "stable ids across calls"
    );

    // Invalid args store nothing.
    let before = harness
        .db
        .load_compression_blocks("s")
        .expect("blocks")
        .len();
    let bad = serde_json::json!({"topic": "t", "content": []});
    assert!(runtime.run_compress("s", &bad, &spec).is_err());
    let after = harness
        .db
        .load_compression_blocks("s")
        .expect("blocks")
        .len();
    assert_eq!(before, after);

    // Compress obeys the same permission path (default-deny without entry).
    let (harness2, generation2) = make_harness(BTreeMap::new());
    let runtime2 = runtime_of(&harness2, generation2, Vec::new());
    runtime2.create_session("s2").expect("create");
    assert!(runtime2.run_compress("s2", &args, &spec).is_err());
}

#[test]
fn command_expansion_is_single_bounded_pass() {
    let expanded = expand_command(
        "summarize $1 ($ARGUMENTS)",
        &["a".to_string(), "b".to_string()],
    )
    .expect("expand");
    assert_eq!(expanded, "summarize a (a b)");
    assert!(expand_command(&"x".repeat(COMMAND_BYTES_CAP + 1), &[]).is_err());
    // Upstream has no command size limit; a realistic 41 KiB command (owner
    // config shape) must expand instead of failing on a serving cap.
    let large = "x".repeat(41_000);
    assert_eq!(
        expand_command(&large, &[])
            .expect("large command expands")
            .len(),
        large.len()
    );
    assert!(
        expand_command("ok $9", &["only".to_string()])
            .expect("partial")
            .contains("$9")
    );
}

#[tokio::test]
async fn command_invocation_is_durable() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let expanded = expand_command("do $1", &["thing".to_string()]).expect("expand");
    let mut turn_params = params("s", &expanded, &harness, provider_of(&base), &NO_CANCEL);
    turn_params.invocation = Some("/cmd thing".to_string());
    runtime.run_turn(turn_params).await.expect("turn");
    let history = harness.db.read_history("s").expect("history");
    assert_eq!(history[0], ("user".to_string(), "/cmd thing".to_string()));
}

#[tokio::test]
async fn aud20_nudge_cadence_and_model_compress_are_session_scoped() {
    let (harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        nudge_frequency: 2,
        iteration_threshold: 100,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("a").unwrap();
    runtime.create_session("b").unwrap();
    std::fs::write(harness._project.path().join("note.txt"), "note").unwrap();

    let start = harness
        .db
        .append_message("a", "user", &format!("closed start {}", "x".repeat(8_192)))
        .unwrap();
    let end = harness
        .db
        .append_message(
            "a",
            "assistant",
            &format!("closed end {}", "y".repeat(8_192)),
        )
        .unwrap();
    harness
        .db
        .append_message("a", "user", "uncompressed tail")
        .unwrap();

    let mut a_read = sse_tool_call("a-read", "read", &serde_json::json!({"path": "note.txt"}));
    a_read.push_str(&sse_completed());
    let mut b_read = sse_tool_call("b-read", "read", &serde_json::json!({"path": "note.txt"}));
    b_read.push_str(&sse_completed());
    let mut a_compress = sse_tool_call(
        "a-compress",
        "compress",
        &serde_json::json!({
            "topic": "closed setup",
            "content": [{
                "startId": start,
                "endId": end,
                "summary": "closed setup is complete"
            }]
        }),
    );
    a_compress.push_str(&sse_completed());
    let (base, hits, requests) = Fake::start_recording(
        vec![
            a_read,
            sse_delta("a first complete") + &sse_completed(),
            b_read,
            sse_delta("b first complete") + &sse_completed(),
            a_compress,
            sse_delta("a compressed") + &sse_completed(),
            sse_delta("b cadence complete") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);

    let a_first = runtime
        .run_turn(params(
            "a",
            "advance A twice",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(a_first.rounds, 2);
    let b_first = runtime
        .run_turn(params(
            "b",
            "advance B twice",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(b_first.rounds, 2);
    let a_second = runtime
        .run_turn(params(
            "a",
            "compress A from the model",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(a_second.rounds, 2);
    assert_eq!(a_second.calls.len(), 1);
    assert_eq!(a_second.calls[0].name, "compress");
    assert_eq!(a_second.calls[0].state, "completed");
    runtime
        .run_turn(params(
            "b",
            "B remains independently due",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();

    assert_eq!(*hits.lock().unwrap(), 7);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests.iter().map(dcp_nudge_count).collect::<Vec<_>>(),
        [1, 0, 1, 0, 1, 0, 1],
        "A and B must keep independent frequency=2 cadence; only A enters cooldown after compress"
    );
    assert_eq!(harness.db.load_compression_blocks("a").unwrap().len(), 1);
    assert!(harness.db.load_compression_blocks("b").unwrap().is_empty());
}

#[tokio::test]
async fn aud20_nudge_cadence_survives_database_and_runtime_restart() {
    let (mut harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        nudge_frequency: 5,
        iteration_threshold: 100,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation.clone(), Vec::new(), dcp.clone());
    runtime.create_session("restart-nudge").unwrap();
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_delta("first") + &sse_completed(),
            sse_delta("after restart") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "restart-nudge",
            "first",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    drop(runtime);
    drop(harness.db);
    harness.db = Db::open(harness._data.path()).unwrap();
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.open_session("restart-nudge").unwrap();
    runtime
        .run_turn(params(
            "restart-nudge",
            "second",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(dcp_nudge_count)
            .collect::<Vec<_>>(),
        [1, 0],
        "restart must restore cadence rather than reset and emit immediately"
    );
}

#[tokio::test]
async fn aud19_denied_compress_has_no_schema_anchor_or_nudge() {
    let mut permissions = allow_all();
    permissions.insert("compress".to_string(), Permission::Deny);
    let (harness, generation) = make_harness(permissions);
    let dcp = DcpConfig {
        min_context: 1,
        max_context: 1,
        compress_permission: Some(Permission::Deny),
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("denied-compress").unwrap();
    let (base, _, requests) =
        Fake::start_recording(vec![sse_delta("done") + &sse_completed()], Duration::ZERO);
    runtime
        .run_turn(params(
            "denied-compress",
            "normal turn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "compress")
    );
    let input = requests[0]["input"].to_string();
    assert!(!input.contains("DCP context anchors"));
    assert!(!input.contains("DCP reminder"));
}

#[tokio::test]
async fn aud21_turn_protection_preserves_recent_completed_turn_verbatim() {
    let (harness, generation) = make_harness(allow_all());
    let dcp = DcpConfig {
        turn_protection: true,
        turn_protection_turns: 1,
        deduplication: false,
        purge_errors: false,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp);
    runtime.create_session("turn-protection").unwrap();
    let recent_user = format!("RECENT_USER_EXACT {}", "u".repeat(8_192));
    let recent_assistant = format!("RECENT_ASSISTANT_EXACT {}", "a".repeat(8_192));
    let start = harness
        .db
        .append_message("turn-protection", "user", &recent_user)
        .unwrap();
    let end = harness
        .db
        .append_message("turn-protection", "assistant", &recent_assistant)
        .unwrap();
    let mut compress = sse_tool_call(
        "turn-protection-compress",
        "compress",
        &serde_json::json!({
            "topic": "recent turn",
            "content": [{
                "startId": start, "endId": end,
                "summary": "recent turn summary"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let (base, _, requests) = Fake::start_recording(
        vec![compress, sse_delta("protected") + &sse_completed()],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "turn-protection",
            "attempt compression of recent completed turn",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.calls.len(), 1);
    assert!(matches!(
        report.calls[0].state.as_str(),
        "completed" | "no_gain"
    ));
    let requests = requests.lock().unwrap();
    let next = requests[1]["input"].to_string();
    assert!(next.contains("RECENT_USER_EXACT"));
    assert!(next.contains("RECENT_ASSISTANT_EXACT"));
}

#[tokio::test]
async fn aud20_summary_buffer_changes_effective_nudge_threshold() {
    let (harness, generation) = make_harness(allow_all());
    let mut dcp = DcpConfig {
        min_context: 500,
        max_context: 600,
        nudge_frequency: 1,
        summary_buffer: true,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp.clone());
    runtime.create_session("summary-buffer").unwrap();
    let first = harness
        .db
        .append_message("summary-buffer", "user", &"u".repeat(10_000))
        .unwrap();
    let second = harness
        .db
        .append_message("summary-buffer", "assistant", &"a".repeat(10_000))
        .unwrap();
    harness
        .db
        .append_message("summary-buffer", "user", "tail")
        .unwrap();
    oc_adapters::dcp::save_block(
        &harness.db,
        "summary-buffer",
        "buffer",
        &"s".repeat(4_000),
        &first,
        &second,
        &[first.clone(), second.clone()],
    )
    .unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_delta("buffered") + &sse_completed(),
            sse_delta("unbuffered") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    runtime
        .run_turn(params(
            "summary-buffer",
            "small request",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    dcp.summary_buffer = false;
    runtime.reload_dcp(dcp).unwrap();
    runtime
        .run_turn(params(
            "summary-buffer",
            "small request two",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    let first = requests[0]["input"].to_string();
    let second = requests[1]["input"].to_string();
    assert!(first.contains("DCP reminder (advisory)"));
    assert!(!first.contains("required before more work"));
    assert!(second.contains("DCP reminder (required before more work)"));
}

#[tokio::test]
async fn aud20_compress_commits_only_eligible_strategy_projection() {
    let (harness, generation) = make_harness(allow_all());
    let mut dcp = DcpConfig {
        deduplication: true,
        purge_errors: true,
        purge_after_turns: 1,
        protected_tools: vec!["bash".to_string()],
        protected_file_patterns: vec!["src/*.rs".to_string()],
        turn_protection: true,
        turn_protection_turns: 1,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&harness, generation, Vec::new(), dcp.clone());
    runtime.create_session("strategy").unwrap();
    std::fs::write(harness._project.path().join("note.txt"), "durable note").unwrap();

    let duplicate_args = serde_json::json!({"path": "note.txt"});
    let protected_args = serde_json::json!({"argv": ["/bin/true", "protected"]});
    let error_args = serde_json::json!({"path": format!("/{}", "e".repeat(5_000))});
    let recent_error_args = serde_json::json!({"path": format!("/{}", "r".repeat(5_000))});
    let large_success_args = serde_json::json!({"argv": ["/bin/true", "s".repeat(5_000)]});
    let protected_patch_args = serde_json::json!({
        "patchText": format!(
            "*** Begin Patch\n*** Update File: src/critical.rs\n@@\n-missing\n+{}\n*** End Patch\n",
            "p".repeat(5_000)
        )
    });
    assert!(error_args.to_string().len() > 4_096);
    assert!(large_success_args.to_string().len() > 4_096);
    assert!(protected_patch_args.to_string().len() > 4_096);

    let mut old_batch = String::new();
    old_batch.push_str(&sse_tool_call("dup-read", "read", &duplicate_args));
    old_batch.push_str(&sse_tool_call("protected-1", "bash", &protected_args));
    old_batch.push_str(&sse_tool_call("error-old", "read", &error_args));
    old_batch.push_str(&sse_tool_call("large-success", "bash", &large_success_args));
    old_batch.push_str(&sse_tool_call(
        "protected-file",
        "apply_patch",
        &protected_patch_args,
    ));
    old_batch.push_str(&sse_completed());
    let mut recent_batch = String::new();
    // Provider call IDs are opaque and may repeat in a later turn. Strategy
    // identity must not hide every occurrence merely because one is deduped.
    recent_batch.push_str(&sse_tool_call("dup-read", "read", &duplicate_args));
    recent_batch.push_str(&sse_tool_call("protected-2", "bash", &protected_args));
    recent_batch.push_str(&sse_tool_call("error-recent", "read", &recent_error_args));
    recent_batch.push_str(&sse_completed());
    let mut compress = sse_tool_call(
        "strategy-compress",
        "compress",
        &serde_json::json!({
            "topic": "old tool work",
            "content": [{
                "startId": "m0001", "endId": "m0004",
                "summary": "old tool work completed"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let mut third_duplicate = sse_tool_call("dup-read", "read", &duplicate_args);
    third_duplicate.push_str(&sse_completed());
    let mut second_compress = sse_tool_call(
        "strategy-compress-2",
        "compress",
        &serde_json::json!({
            "topic": "first strategy pass",
            "content": [{
                "startId": "m0005", "endId": "m0006",
                "summary": "first strategy pass completed"
            }]
        }),
    );
    second_compress.push_str(&sse_completed());
    let (base, hits, requests) = Fake::start_recording(
        vec![
            old_batch,
            sse_delta("old tools complete") + &sse_completed(),
            recent_batch,
            sse_delta("recent tools complete") + &sse_completed(),
            compress,
            sse_delta("strategy projection captured") + &sse_completed(),
            third_duplicate,
            sse_delta("third duplicate captured") + &sse_completed(),
            second_compress,
            sse_delta("second strategy projection captured") + &sse_completed(),
            sse_delta("manual projection captured") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);

    let old = runtime
        .run_turn(params(
            "strategy",
            "seed old typed tools",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(old.calls.len(), 5);
    let recent = runtime
        .run_turn(params(
            "strategy",
            "seed recent typed tools",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(recent.calls.len(), 3);
    let exact_error = harness
        .db
        .list_tool_ops("strategy")
        .unwrap()
        .into_iter()
        .find(|op| op.op.ends_with("-error-old"))
        .and_then(|op| op.output)
        .expect("durable old error output");
    assert!(exact_error.starts_with("error:"));

    runtime
        .run_turn(params(
            "strategy",
            &format!(
                "compress and commit automatic strategies {}",
                "strategy-padding ".repeat(1_000)
            ),
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    let third = runtime
        .run_turn(params(
            "strategy",
            "seed a third reused provider call id",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(third.calls.len(), 1);
    runtime
        .run_turn(params(
            "strategy",
            "compress again without occurrence drift",
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    dcp.manual_mode = true;
    dcp.automatic_strategies = false;
    runtime.reload_dcp(dcp).unwrap();
    runtime
        .run_turn(params(
            "strategy",
            "capture manual bypass",
            &harness,
            provider,
            &NO_CANCEL,
        ))
        .await
        .unwrap();

    assert_eq!(*hits.lock().unwrap(), 11);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 11);
    let projected = &requests[5];
    assert_eq!(
        function_item_count(projected, "function_call", "dup-read"),
        1
    );
    assert_eq!(
        function_item_count(projected, "function_call_output", "dup-read"),
        1
    );
    for call_id in [
        "protected-1",
        "error-old",
        "large-success",
        "protected-file",
        "protected-2",
        "error-recent",
    ] {
        assert!(
            function_call(projected, call_id).is_some(),
            "strategy removed eligible call {call_id}"
        );
        assert!(
            function_output(projected, call_id).is_some(),
            "strategy orphaned output {call_id}"
        );
    }
    assert_eq!(
        function_call(projected, "error-old").unwrap()["arguments"],
        serde_json::json!({"purged": "large error input"}).to_string()
    );
    assert_eq!(
        function_call(projected, "error-recent").unwrap()["arguments"],
        recent_error_args.to_string(),
        "turnProtection must prevent purge of recent typed tool input"
    );
    assert_eq!(
        function_output(projected, "error-old"),
        Some(exact_error.as_str())
    );
    assert_eq!(
        function_call(projected, "large-success").unwrap()["arguments"],
        large_success_args.to_string(),
        "large successful arguments must not be purged"
    );
    assert_eq!(
        function_call(projected, "protected-file").unwrap()["arguments"],
        protected_patch_args.to_string(),
        "protectedFilePatterns must inspect typed apply_patch paths"
    );
    assert_eq!(
        function_call(projected, "dup-read").unwrap()["arguments"],
        duplicate_args.to_string()
    );
    assert_eq!(
        function_call(projected, "protected-1").unwrap()["arguments"],
        protected_args.to_string()
    );
    assert_eq!(
        function_call(projected, "protected-2").unwrap()["arguments"],
        protected_args.to_string()
    );

    let projected_again = &requests[9];
    assert_eq!(
        function_item_count(projected_again, "function_call", "dup-read"),
        1,
        "a second strategy transaction must keep only the newest reused call ID occurrence"
    );
    assert_eq!(
        function_item_count(projected_again, "function_call_output", "dup-read"),
        1
    );

    let manual = &requests[10];
    assert_eq!(function_item_count(manual, "function_call", "dup-read"), 1);
    assert_eq!(
        function_item_count(manual, "function_call_output", "dup-read"),
        1
    );
    for (call_id, arguments) in [
        ("protected-1", protected_args.to_string()),
        (
            "error-old",
            serde_json::json!({"purged": "large error input"}).to_string(),
        ),
        ("large-success", large_success_args.to_string()),
        ("protected-file", protected_patch_args.to_string()),
        ("dup-read", duplicate_args.to_string()),
        ("protected-2", protected_args.to_string()),
        (
            "error-recent",
            serde_json::json!({"purged": "large error input"}).to_string(),
        ),
    ] {
        assert_eq!(
            function_call(manual, call_id).map(|call| &call["arguments"]),
            Some(&serde_json::Value::String(arguments)),
            "manual mode changed committed projection for {call_id}"
        );
        assert!(
            function_output(manual, call_id).is_some(),
            "manual mode removed output {call_id}"
        );
    }
    assert_eq!(
        function_output(manual, "error-old"),
        Some(exact_error.as_str())
    );
}

#[tokio::test]
async fn aud21_model_compress_preserves_complete_tool_graph_without_replay() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("graph").unwrap();
    let mut effect = sse_tool_call(
        "graph-effect",
        "bash",
        &serde_json::json!({
            "argv": ["/bin/sh", "-c", "printf 'once\\n' >> graph-effects"]
        }),
    );
    effect.push_str(&sse_completed());
    let (base, seed_hits, _) = Fake::start_recording(
        vec![
            effect,
            sse_delta(&format!("effect complete {}", "padding ".repeat(2_000))) + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let provider = provider_of(&base);
    let seeded = runtime
        .run_turn(params(
            "graph",
            &format!(
                "perform one durable effect {}",
                "request-padding ".repeat(2_000)
            ),
            &harness,
            provider.clone(),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(seeded.calls.len(), 1);
    assert_eq!(seeded.calls[0].state, "completed");
    let history = harness.db.read_history_full("graph").unwrap();
    assert_eq!(history.len(), 2);

    let mut compress = sse_tool_call(
        "compress-graph",
        "compress",
        &serde_json::json!({
            "topic": "unsafe graph range",
            "content": [{
                "startId": history[0].0,
                "endId": history[1].0,
                "summary": "the durable effect completed once"
            }]
        }),
    );
    compress.push_str(&sse_completed());
    let (compress_base, compress_hits, requests) = Fake::start_recording(
        vec![compress, sse_delta("refusal handled") + &sse_completed()],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "graph",
            "compress the completed tool turn",
            &harness,
            provider_of(&compress_base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls[0].name, "compress");
    assert_eq!(report.calls[0].state, "completed");
    let requests = requests.lock().unwrap();
    assert!(
        function_output(&requests[1], "compress-graph")
            .is_some_and(|output| output.contains("\"status\":\"compressed\""))
    );
    assert!(function_call(&requests[1], "graph-effect").is_some());
    assert!(function_output(&requests[1], "graph-effect").is_some());
    assert!(harness.db.load_compression_blocks("graph").unwrap().len() == 1);
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("graph-effects")).unwrap(),
        "once\n"
    );
    let operations = harness.db.list_tool_ops("graph").unwrap();
    assert_eq!(
        operations.iter().filter(|op| op.name == "bash").count(),
        1,
        "compression refusal replayed the prior side effect"
    );
    assert_eq!(*seed_hits.lock().unwrap(), 2);
    assert_eq!(
        *compress_hits.lock().unwrap(),
        2,
        "compression must return one structured output and then continue"
    );
}

/// DTO extension (iteration 3a): the runtime forwards provider reasoning
/// deltas and reports usage plus the provider-active streamed window, so the
/// TUI can render the reasoning block and the footer's `tok/s`.
#[tokio::test]
async fn dto_reasoning_deltas_and_usage_reach_the_event_callbacks() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s-dto").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("**Inspecting**\n\n")
                + &sse_reasoning("body")
                + &sse_delta("answer")
                + &sse_completed_usage(42, 7),
        ],
        Duration::from_millis(20),
    );
    let mut accepted = Vec::new();
    let mut reasoning = String::new();
    let mut text = String::new();
    let report = runtime
        .run_turn_with_events(
            params("s-dto", "hello", &harness, provider_of(&base), &NO_CANCEL),
            |turn| accepted.push(turn.to_string()),
            |_, delta| text.push_str(delta),
            |_, delta| reasoning.push_str(delta),
        )
        .await
        .expect("turn");
    assert_eq!(accepted.len(), 1, "one durable acceptance");
    assert_eq!(reasoning, "**Inspecting**\n\nbody");
    assert_eq!(text, "answer");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.usage, Some((42, 7)), "provider-reported usage");
    assert!(
        report.streamed_ms >= 20,
        "provider-active time must be measured: {}ms",
        report.streamed_ms
    );
    assert!(
        report.duration_ms >= 20,
        "turn wall time must be measured: {}ms",
        report.duration_ms
    );
    // Reasoning is never persisted as an assistant message.
    assert_eq!(
        harness.db.read_history("s-dto").unwrap(),
        [
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "answer".to_string())
        ]
    );
}

/// End to end through the real application worker: `application::spawn_with_env`
/// (project-local config, no process env mutation) broadcasts the new
/// `ReasoningDelta` and `TurnUsage` events next to `TurnFinished`.
#[tokio::test]
async fn dto_application_events_surface_reasoning_and_usage() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let home = tempfile::tempdir().expect("home");
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("**Planning**\n\n") + &sse_delta("visible") + &sse_completed_usage(9, 4),
        ],
        Duration::from_millis(20),
    );
    let config = serde_json::json!({
        "model": "fixture/fixture-model",
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": base, "apiKey": "test-key"},
            "models": {"fixture-model": {
                "name": "DTO fixture",
                "limit": {"context": 65536, "output": 4096},
            }},
        }},
        "permissions": {},
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).expect("config");
    let env: BTreeMap<String, String> = [
        ("HOME", home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK", "1".to_string()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    let (app, guard, _diagnostics) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .expect("application");
    let session = SessionId::new("s-app-dto").expect("session id");
    app.create_session(session.clone()).await.expect("create");
    let mut rx = app.subscribe();
    app.submit(session.clone(), "hello".to_string())
        .await
        .expect("submit");

    let mut reasoning = String::new();
    let mut usage = None;
    let (text, duration_ms) = loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel");
        match event {
            CoreEvent::ReasoningDelta { delta, .. } => reasoning.push_str(&delta),
            CoreEvent::TurnUsage {
                input_tokens,
                output_tokens,
                streamed_ms,
                ..
            } => usage = Some((input_tokens, output_tokens, streamed_ms)),
            CoreEvent::TurnFinished {
                text, duration_ms, ..
            } => break (text, duration_ms),
            CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            CoreEvent::TurnStarted { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ToolCallStarted { .. }
            | CoreEvent::ToolCallFinished { .. }
            | CoreEvent::TurnInterrupted { .. } => {}
        }
    };
    assert_eq!(reasoning, "**Planning**\n\n", "reasoning delta surfaces");
    assert_eq!(
        usage.map(|(input, output, _)| (input, output)),
        Some((9, 4)),
        "provider usage surfaces"
    );
    assert!(
        usage.is_some_and(|(_, _, streamed_ms)| streamed_ms >= 20),
        "provider-active time is measured"
    );
    assert_eq!(text, "visible");
    assert!(duration_ms >= 20, "turn duration is measured");
    app.shutdown().await.expect("shutdown");
    guard.join().await.expect("join");
}

/// TUI tool cards are built from real runtime state: one `apply_patch` call
/// produces `Started` then `Finished` events after the durable records, and
/// the recorded operation carries the patch text the transcript renders.
#[tokio::test]
async fn dto_tool_events_surface_started_and_finished_with_a_patch() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s-tools").unwrap();
    std::fs::write(harness._project.path().join("old.txt"), "old\n").unwrap();
    let patch = "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** Update File: old.txt\n@@\n-old\n+new\n*** End Patch";
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "call-patch",
                "apply_patch",
                &serde_json::json!({"patchText": patch}),
            ) + &sse_completed(),
            sse_delta("patched") + &sse_completed(),
        ],
        Duration::from_millis(5),
    );
    let mut events = Vec::new();
    let mut text = String::new();
    let report = runtime
        .run_turn_with_tool_events(
            params(
                "s-tools",
                "patch it",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, delta| text.push_str(delta),
            |_, _| {},
            |_, event| events.push(event.clone()),
        )
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(text, "patched");
    assert_eq!(events.len(), 2, "one intent and one outcome: {events:?}");
    match &events[0] {
        ToolCallEvent::Started { op, name, input } => {
            assert_eq!(name, "apply_patch");
            assert!(!op.is_empty());
            assert!(
                input.contains("*** Add File: added.txt"),
                "the recorded input carries the patch: {input}"
            );
        }
        other => panic!("expected Started, got {other:?}"),
    }
    match &events[1] {
        ToolCallEvent::Finished {
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
        } => {
            assert_eq!(name, "apply_patch");
            assert_eq!(state, "completed");
            assert!(!op.is_empty());
            assert!(output.contains("added.txt"), "{output}");
            assert!(*output_bytes > 0);
            assert!(!output_truncated, "small outputs are not truncated");
        }
        other => panic!("expected Finished, got {other:?}"),
    }
    // The patch really ran and the operation is durably recorded.
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("added.txt")).unwrap(),
        "hello\n"
    );
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("old.txt")).unwrap(),
        "new\n"
    );
    let ops = harness.db.list_tool_ops("s-tools").unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].name, "apply_patch");
    assert_eq!(ops[0].state, "completed");
    assert!(
        ops[0]
            .input
            .as_deref()
            .is_some_and(|input| input.contains("*** Add File: added.txt")),
        "the durable intent keeps the patch text the card renders"
    );
}

/// End to end through the real application worker: `application::spawn_with_env`
/// broadcasts the tool-call events next to the text/turn events, so the TUI
/// transcript can render a patch card from live state.
#[tokio::test]
async fn dto_application_events_surface_tool_calls() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let home = tempfile::tempdir().expect("home");
    let patch = "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** End Patch";
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "call-patch",
                "apply_patch",
                &serde_json::json!({"patchText": patch}),
            ) + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::from_millis(5),
    );
    let config = serde_json::json!({
        "model": "fixture/fixture-model",
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": base, "apiKey": "test-key"},
            "models": {"fixture-model": {
                "name": "DTO fixture",
                "limit": {"context": 65536, "output": 4096},
            }},
        }},
        "permissions": {"apply_patch": "allow"},
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).expect("config");
    let env: BTreeMap<String, String> = [
        ("HOME", home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK", "1".to_string()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    let (app, guard, _diagnostics) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .expect("application");
    let session = SessionId::new("s-app-tools").expect("session id");
    app.create_session(session.clone()).await.expect("create");
    let mut rx = app.subscribe();
    app.submit(session.clone(), "patch it".to_string())
        .await
        .expect("submit");

    let mut started = None;
    let mut finished = None;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel");
        match event {
            CoreEvent::ToolCallStarted {
                op, name, input, ..
            } => started = Some((op, name, input)),
            CoreEvent::ToolCallFinished {
                op,
                name,
                state,
                output,
                ..
            } => finished = Some((op, name, state, output)),
            CoreEvent::TurnFinished { text, .. } => {
                assert_eq!(text, "done");
                break;
            }
            CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            CoreEvent::TurnStarted { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ReasoningDelta { .. }
            | CoreEvent::TurnUsage { .. }
            | CoreEvent::TurnInterrupted { .. } => {}
        }
    }
    let (started_op, started_name, started_input) = started.expect("tool call started event");
    let (finished_op, finished_name, finished_state, finished_output) =
        finished.expect("tool call finished event");
    assert_eq!(started_name, "apply_patch");
    assert_eq!(finished_name, "apply_patch");
    assert_eq!(
        started_op, finished_op,
        "both events share the operation id"
    );
    assert_eq!(finished_state, "completed");
    assert!(
        started_input.contains("*** Add File: added.txt"),
        "the event input carries the patch text the card renders: {started_input}"
    );
    assert!(finished_output.contains("added.txt"), "{finished_output}");
    assert_eq!(
        std::fs::read_to_string(project.path().join("added.txt")).unwrap(),
        "hello\n"
    );
    app.shutdown().await.expect("shutdown");
    guard.join().await.expect("join");
}
