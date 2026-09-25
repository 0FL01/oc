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
    COMMAND_BYTES_CAP, Runtime, RuntimeError, SESSION_LOCATION_PREFIX, ToolCallEvent, TurnParams,
    TurnStatus, expand_command,
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

fn sse_completed_without_usage() -> &'static str {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n"
}

/// Reasoning summary delta (`response.reasoning_summary_text.delta`).
fn sse_reasoning(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.reasoning_summary_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_reasoning_done(id: &str, secret: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "item":{
            "type":"reasoning", "id":id, "encrypted_content":secret, "summary":[], "status":"completed"
        }})
    )
}

fn sse_completed_output(output: Vec<serde_json::Value>) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed", "response":{
            "status":"completed", "output":output, "usage":{"input_tokens":10,"output_tokens":5}
        }})
    )
}

fn sse_message_done(index: u64, message: &serde_json::Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "output_index":index, "item":message})
    )
}

fn sse_message_done_by_id(message: &serde_json::Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "item":message})
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
        animations: None,
        providers: BTreeMap::new(),
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

#[tokio::test]
async fn fresh_turn_commits_root_binding_selection_before_ack_and_streams_normally() {
    let mut permissions = allow_all();
    permissions.insert("read".into(), Permission::Deny);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation, Vec::new());
    let selection_key = "tui.selection.session:[\"/project\",\"test\",\"fresh\"]";
    let selection = r#"{"agent":null,"models":{"":{"id":"m","variant":null}},"epoch":0}"#;
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_tool_call("denied-read", "read", &serde_json::json!({"path":"secret"}))
                + &sse_completed(),
            sse_delta("answer") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut acknowledgements = Vec::new();
    let mut deltas = Vec::new();
    let callback_order = Arc::new(Mutex::new(Vec::new()));
    let accepted_order = callback_order.clone();
    let tool_order = callback_order.clone();
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "fresh",
                "expanded prompt",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            Some((selection_key, selection)),
            |turn| {
                let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
                let (status, prompt): (String, String) = conn
                    .query_row(
                        "SELECT status, prompt FROM turns WHERE id = ?1",
                        [turn],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(
                    (status.as_str(), prompt.as_str()),
                    ("started", "expanded prompt")
                );
                assert_eq!(
                    harness.db.get_pref(selection_key).unwrap().as_deref(),
                    Some(selection)
                );
                runtime.open_session("fresh").unwrap();
                assert_eq!(
                    harness.db.read_history("fresh").unwrap(),
                    [("user".into(), "expanded prompt".into())]
                );
                accepted_order.lock().unwrap().push("accepted");
                acknowledgements.push(turn.to_string());
            },
            |turn, text| deltas.push((turn.to_string(), text.to_string())),
            |_, _| {},
            |_, event| {
                tool_order.lock().unwrap().push(match event {
                    ToolCallEvent::Started { .. } => "started",
                    ToolCallEvent::Finished { .. } => "finished",
                });
            },
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "answer");
    assert_eq!(
        acknowledgements.as_slice(),
        std::slice::from_ref(&report.turn_id)
    );
    assert_eq!(deltas, [(report.turn_id.clone(), "answer".into())]);
    assert_eq!(report.calls[0].output, "error: denied read");
    assert_eq!(
        *callback_order.lock().unwrap(),
        ["accepted", "started", "finished"]
    );
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(requests.lock().unwrap()[0]["model"], "m");
    assert_eq!(
        function_output(&requests.lock().unwrap()[1], "denied-read"),
        Some("error: denied read")
    );
    assert_eq!(harness.db.session_meta("fresh").unwrap().parent_id, None);
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 2);
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let events: Vec<String> = conn
        .prepare("SELECT kind FROM events WHERE session_id='fresh' ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        events,
        [
            "session_created",
            "turn_started",
            "message",
            "message",
            "turn_finished"
        ]
    );

    // The old path can continue the root, while fresh-only admission cannot
    // claim it or rewrite its selection/history.
    let duplicate = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "fresh",
                "duplicate",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            None,
            |_| panic!("duplicate accepted"),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert!(matches!(duplicate, Err(RuntimeError::InvalidArgs(_))));
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 2);
    assert_eq!(
        runtime
            .run_turn(params(
                "fresh",
                "next",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .status,
        TurnStatus::Completed
    );
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 4);
}

#[tokio::test]
async fn cancelled_fresh_turn_keeps_the_durable_acceptance_receipt() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let cancelled = AtomicBool::new(true);
    let (base, hits) = Fake::start(
        vec![sse_delta("unexpected") + &sse_completed()],
        Duration::ZERO,
    );
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "cancelled-fresh",
                "input",
                &harness,
                provider_of(&base),
                &cancelled,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| panic!("cancelled turn streamed text"),
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    assert_eq!(report.status, TurnStatus::Cancelled);
    assert_eq!(*hits.lock().unwrap(), 0);
    runtime.open_session("cancelled-fresh").unwrap();
    assert_eq!(
        harness.db.read_history("cancelled-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
}

#[tokio::test]
async fn post_accept_failure_still_delivers_fresh_root_and_turn_receipt() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON turns
         WHEN NEW.session_id = 'checkpoint-fresh'
         BEGIN SELECT RAISE(ABORT, 'injected checkpoint failure'); END;",
    )
    .unwrap();
    let (base, hits) = Fake::start(vec![sse_delta("never") + &sse_completed()], Duration::ZERO);
    let mut accepted = None;
    let result = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "checkpoint-fresh",
                "input",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert_eq!(result.unwrap_err(), RuntimeError::Storage);
    let turn = accepted.expect("committed root and turn must have a receipt");
    runtime.open_session("checkpoint-fresh").unwrap();
    let state: String = conn
        .query_row("SELECT status FROM turns WHERE id = ?1", [turn], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(state, "started");
    assert_eq!(
        harness.db.read_history("checkpoint-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
    assert_eq!(*hits.lock().unwrap(), 0);
}

#[tokio::test]
async fn fresh_provider_failure_after_accept_retains_root_and_reports_failed_turn() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "provider-fresh",
                "input",
                &harness,
                provider_of("http://127.0.0.1:9/v1"),
                &NO_CANCEL,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    runtime.open_session("provider-fresh").unwrap();
    assert_eq!(
        harness.db.read_history("provider-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
}

#[tokio::test]
async fn rejected_fresh_turn_leaves_no_root_or_selection_and_can_retry() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let key = "tui.selection.session:[\"/project\",\"test\",\"retry\"]";
    let wrong = "tui.selection.session:[\"/project\",\"test\",\"other\"]";
    let (base, hits) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let runtime_ref = &runtime;
    let rejected = |params, selection| async move {
        runtime_ref
            .run_fresh_turn_with_tool_events(
                params,
                selection,
                |_| panic!("rejected fresh turn acknowledged"),
                |_, _| {},
                |_, _| {},
                |_, _| {},
            )
            .await
    };
    let mut invalid_model = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    invalid_model.model_id = "absent".into();
    assert!(matches!(
        rejected(invalid_model, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    let mut invalid_variant = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    invalid_variant.variant = Some("absent".into());
    assert!(matches!(
        rejected(invalid_variant, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    let mut tiny_catalog = harness.catalog.clone();
    tiny_catalog.models.insert(
        "m".into(),
        serde_json::json!({"limit":{"context":1,"output":1}}),
    );
    let mut over_budget = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    over_budget.catalog = &tiny_catalog;
    assert!(matches!(
        rejected(over_budget, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    assert_eq!(
        rejected(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((wrong, "choice"))
        )
        .await
        .unwrap_err(),
        RuntimeError::Storage
    );

    conn.execute_batch("CREATE TRIGGER fail_fresh_input BEFORE INSERT ON messages WHEN NEW.session_id = 'retry' BEGIN SELECT RAISE(ABORT, 'injected input failure'); END;").unwrap();
    assert_eq!(
        rejected(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((key, "choice"))
        )
        .await
        .unwrap_err(),
        RuntimeError::Storage
    );
    for table in ["sessions", "turns", "messages", "events"] {
        let query = format!("SELECT COUNT(*) FROM {table} WHERE session_id = 'retry'");
        let query = if table == "sessions" {
            "SELECT COUNT(*) FROM sessions WHERE id = 'retry'"
        } else {
            &query
        };
        let count: i64 = conn.query_row(query, [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "{table} left after refusal");
    }
    assert_eq!(harness.db.get_pref(key).unwrap(), None);
    assert_eq!(
        harness
            .db
            .get_pref(&format!("{SESSION_LOCATION_PREFIX}retry"))
            .unwrap(),
        None
    );
    assert_eq!(*hits.lock().unwrap(), 0);

    conn.execute_batch("DROP TRIGGER fail_fresh_input").unwrap();
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((key, "choice")),
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(harness.db.get_pref(key).unwrap().as_deref(), Some("choice"));
    assert_eq!(*hits.lock().unwrap(), 1);
}

#[test]
fn root_location_creation_rolls_back_on_pref_failure_and_retries() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
    let key = format!("{SESSION_LOCATION_PREFIX}atomic-root");
    conn.execute_batch(
        "CREATE TRIGGER fail_location_binding BEFORE INSERT ON prefs
         WHEN NEW.key = 'tui.session_location.atomic-root'
         BEGIN SELECT RAISE(ABORT, 'injected preference failure'); END;",
    )
    .unwrap();

    assert!(matches!(
        runtime.create_session("atomic-root"),
        Err(RuntimeError::Storage)
    ));
    let count =
        |sql: &str, value: &str| -> i64 { conn.query_row(sql, [value], |row| row.get(0)).unwrap() };
    let sessions = "SELECT count(*) FROM sessions WHERE id = ?1";
    let events = "SELECT count(*) FROM events WHERE session_id = ?1 AND kind = 'session_created'";
    let prefs = "SELECT count(*) FROM prefs WHERE key = ?1";
    assert_eq!(
        count(sessions, "atomic-root"),
        0,
        "failed binding stranded a root"
    );
    assert_eq!(
        count(events, "atomic-root"),
        0,
        "failed binding stranded an event"
    );
    assert_eq!(count(prefs, &key), 0);

    conn.execute_batch("DROP TRIGGER fail_location_binding;")
        .unwrap();
    runtime.create_session("atomic-root").expect("retry");
    runtime
        .create_session("atomic-root")
        .expect("same Location idempotent");
    runtime.open_session("atomic-root").expect("bound root");
    assert_eq!(count(sessions, "atomic-root"), 1);
    assert_eq!(count(events, "atomic-root"), 1);
    assert_eq!(count(prefs, &key), 1);

    let project = harness._project.path();
    let other = Runtime::new(
        &harness.db,
        "elsewhere",
        generation,
        ProtectedGlobs { patterns: vec![] },
        oc_adapters::files::Files::new(project, harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    assert_eq!(
        other.create_session("atomic-root"),
        Err(RuntimeError::LocationMismatch {
            session: "atomic-root".into(),
            location: "work".into(),
        })
    );
    assert_eq!(count(sessions, "atomic-root"), 1);
    assert_eq!(count(events, "atomic-root"), 1);
    assert_eq!(count(prefs, &key), 1);

    harness.db.create_session("standalone").unwrap();
    assert_eq!(
        runtime.create_session("standalone"),
        Err(RuntimeError::Storage)
    );
    assert_eq!(count(sessions, "standalone"), 1);
    assert_eq!(count(events, "standalone"), 1);
    assert_eq!(
        count(prefs, &format!("{SESSION_LOCATION_PREFIX}standalone")),
        0
    );
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

/// Real runtime/provider/MCP path: dropping a polled tools/call future must
/// remain quarantined even if the caller reloads before starting a new turn.
#[tokio::test]
async fn v07b_dropped_remote_call_reload_keeps_unknown_and_refuses_retry() {
    dropped_remote_call_cannot_retry_after(false).await;
}

#[tokio::test]
async fn v07b_dropped_remote_call_shutdown_keeps_unknown_and_refuses_retry() {
    dropped_remote_call_cannot_retry_after(true).await;
}

async fn dropped_remote_call_cannot_retry_after(shutdown: bool) {
    use std::net::TcpStream;
    use std::sync::atomic::AtomicUsize;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_out = calls.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        let mut stalled = Vec::<TcpStream>::new();
        while !stopping.load(Ordering::Relaxed) {
            let (socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("fake MCP accept: {error}"),
            };
            let mut reader = BufReader::new(socket);
            reader
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let id = request
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let response = match request["method"].as_str().unwrap_or_default() {
                "initialize" => Some(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{
                    "protocolVersion":"2025-11-25", "capabilities":{"tools":{}},
                    "serverInfo":{"name":"fake","version":"1"}}})),
                "tools/list" => Some(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{
                    "tools":[{"name":"ping","inputSchema":{"type":"object","properties":{}}}]}})),
                "tools/call" => {
                    calls_out.fetch_add(1, Ordering::SeqCst);
                    stalled.push(reader.into_inner()); // server-side effect stays active
                    continue;
                }
                _ => None,
            };
            let socket = reader.get_mut();
            if let Some(body) = response {
                let body = body.to_string();
                let _ = write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            } else {
                let _ = socket.write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
        drop(stalled);
    });

    let mut permissions = allow_all();
    permissions.insert("stall__ping".into(), Permission::Allow);
    let (harness, mut generation) = make_harness(permissions);
    generation.mcp.insert(
        "stall".into(),
        McpEntry {
            kind: "remote".into(),
            url: Some(url),
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: Vec::new(),
            timeout: Some(10_000),
            codemode: None,
        },
    );
    let project = harness._project.path();
    let runtime = Runtime::new(
        &harness.db,
        "work",
        generation.clone(),
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        oc_adapters::files::Files::new(project, harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(project).unwrap(),
        BTreeMap::from([("OC_TEST_ALLOW_LOOPBACK".into(), "1".into())]),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    runtime.create_session("dropped").unwrap();
    let tool = sse_tool_call("drop", "stall__ping", &serde_json::json!({}));
    let (base, _, requests) = Fake::start_recording(vec![tool + &sse_completed()], Duration::ZERO);
    let mut turn = Box::pin(runtime.run_turn(params(
        "dropped",
        "first",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    )));
    tokio::time::timeout(Duration::from_secs(8), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::select! {
                result = &mut turn => panic!("turn finished before MCP stall: {result:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
    })
    .await
    .expect("MCP call not sent");
    drop(turn); // neither cancel flag nor turn callback: the real owner future is dropped
    if shutdown {
        runtime
            .shutdown_mcp()
            .await
            .expect("shutdown after caller drop");
    } else {
        runtime
            .reload(generation.clone())
            .await
            .expect("reload after caller drop");
        runtime
            .reload(generation)
            .await
            .expect("second reload cannot clear quarantine");
    }
    runtime.create_session("retry").unwrap();
    let error = runtime
        .run_turn(params(
            "retry",
            "explicit retry",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            oc_adapters::runtime::RuntimeError::McpAttach {
                safe_code: "unsafe_retry",
                retryable: false,
                ..
            }
        ),
        "unsafe retry admitted: {error:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "overlapping server-side effects"
    );
    assert_eq!(requests.lock().unwrap().len(), 1, "retry reached provider");
    let ops = harness.db.list_tool_ops("dropped").unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].state, "unknown",
        "dropped call must be durable unknown"
    );
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let turn_status: String = sql
        .query_row(
            "SELECT status FROM turns WHERE session_id='dropped'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(turn_status, "unknown");
    assert!(runtime.shutdown_mcp().await.is_ok());
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
}

#[tokio::test]
async fn resource_permissions_gate_real_dispatch_before_side_effects() {
    let (harness, mut generation) = make_harness(allow_all());
    generation.permission_rules =
        oc_adapters::permissions::PermissionRules::from_config(&serde_json::json!({
            "permission": {
                "read": {"*":"deny", "safe/*":"allow", "safe/secret*":"deny"},
                "edit": {"safe/*":"allow", "safe/secret*":"deny"},
                "bash": {"/bin/echo ok":"allow"}
            }
        }))
        .unwrap();
    std::fs::create_dir(harness._project.path().join("safe")).unwrap();
    std::fs::write(
        harness._project.path().join("safe/input"),
        "allowed content",
    )
    .unwrap();
    std::fs::write(harness._project.path().join("private"), "private canary").unwrap();
    std::fs::write(
        harness._project.path().join("safe/secret*keys"),
        "star canary\n",
    )
    .unwrap();
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("resources").unwrap();
    let calls = [
        ("read-ok", "read", serde_json::json!({"path": "safe/input"})),
        (
            "read-no",
            "read",
            serde_json::json!({"path":"safe/../private"}),
        ),
        (
            "patch-ok",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: safe/new\n+allowed\n*** End Patch"}),
        ),
        (
            "patch-no",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Update File: safe/input\n*** Move to: escaped\n@@\n-allowed content\n+changed\n*** End Patch"}),
        ),
        (
            "bash-ok",
            "bash",
            serde_json::json!({"argv":["/bin/echo","ok"]}),
        ),
        (
            "bash-no",
            "bash",
            serde_json::json!({"argv":["/bin/touch","marker"]}),
        ),
        (
            "read-star",
            "read",
            serde_json::json!({"path":"safe/secret*keys"}),
        ),
        (
            "patch-star",
            "apply_patch",
            serde_json::json!({"patchText":"*** Begin Patch\n*** Delete File: safe/secret*keys\n*** End Patch"}),
        ),
    ];
    let script = calls
        .iter()
        .map(|(id, tool, args)| sse_tool_call(id, tool, args))
        .collect::<String>()
        + &sse_completed();
    let (base, _, requests) = Fake::start_recording(
        vec![script, sse_delta("done") + &sse_completed()],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "resources",
            "tools",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report
            .calls
            .iter()
            .map(|call| call.state.as_str())
            .collect::<Vec<_>>(),
        [
            "completed",
            "failed",
            "completed",
            "failed",
            "completed",
            "failed",
            "failed",
            "failed"
        ],
        "{:?}",
        report.calls
    );
    assert!(report.calls[3].output.contains("approval required"));
    assert!(report.calls[5].output.contains("approval required"));
    assert_eq!(report.calls[6].output, "error: denied read");
    assert_eq!(report.calls[7].output, "error: denied apply_patch");
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("safe/secret*keys")).unwrap(),
        "star canary\n"
    );
    assert!(
        !requests.lock().unwrap()[1]
            .to_string()
            .contains("star canary")
    );
    assert!(harness._project.path().join("safe/new").exists());
    assert!(!harness._project.path().join("escaped").exists());
    assert!(!harness._project.path().join("marker").exists());
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("safe/input")).unwrap(),
        "allowed content"
    );
    assert!(
        !requests.lock().unwrap()[1]
            .to_string()
            .contains("private canary")
    );
}

#[tokio::test]
async fn primary_selection_and_restore_narrow_dispatch_guidance_and_children() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mcp_script = project.path().join("permissions_mcp.py");
    std::fs::write(&mcp_script, r#"import json, sys
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'},'instructions':'PRIMARY_POLICY_GUIDANCE'}
    elif method == 'tools/list':
        result={'tools':[{'name':'query','inputSchema':{'type':'object'}}]}
    elif method == 'tools/call':
        with open(sys.argv[1], 'a') as f: f.write('called\n')
        result={'content':[{'type':'text','text':'MCP allowed'}]}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#).unwrap();
    let batch = |prefix: &str, child: bool| {
        let mut result = sse_tool_call(
            &format!("{prefix}-bash"),
            "bash",
            &serde_json::json!({"argv":["/bin/touch", format!("{prefix}-marker")]}),
        ) + &sse_tool_call(
            &format!("{prefix}-mcp"),
            "fixture__query",
            &serde_json::json!({}),
        );
        if child {
            result += &sse_tool_call(
                "spawn",
                "subagent",
                &serde_json::json!({
                    "agent":"helper", "description":"Check inherited policy", "prompt":"try tools"
                }),
            );
        }
        result + &sse_completed()
    };
    let done = || sse_delta("done") + &sse_completed();
    let (base, _, requests) = Fake::start_recording(
        vec![
            batch("startup", false),
            done(),
            done(), // First turn also generates a title.
            batch("build", false),
            done(),
            batch("review", true),
            batch("child", false),
            done(),
            done(),
            batch("restored", false),
            done(),
        ],
        Duration::ZERO,
    );
    let mut config = serde_json::json!({
        "model":"fixture/main", "default_agent":"review",
        "provider":{"fixture":{
            "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
            "models":{"main":{"limit":{"context":65536,"output":4096}}}
        }},
        "permission":"allow",
        "agent":{
            "build":{"mode":"primary", "prompt":"BUILD_PRIMARY", "permission":"allow"},
            "review":{"mode":"primary", "prompt":"REVIEW_PRIMARY", "tools":{"bash":false,"fixture_query":false}},
            "helper":{"mode":"subagent", "prompt":"HELPER_CHILD", "permission":"allow"}
        },
        "mcp":{"fixture":{"type":"local", "command":["/usr/bin/python3",mcp_script,project.path().join("mcp-calls")], "timeout":2000}}
    });
    let config_path = project.path().join("opencode.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    let session = SessionId::new("primary-policy").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let mut events = app.subscribe();
    for selected in [None, Some("build"), Some("review")] {
        if let Some(id) = selected {
            assert_eq!(
                app.select_agent(id.into())
                    .await
                    .unwrap()
                    .agent_id
                    .as_deref(),
                Some(id)
            );
        }
        app.submit(session.clone(), "try tools".into())
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("turn failed: {error}"),
                _ => {}
            }
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    // Restore a non-default selection, including when no spawnable catalog exists.
    config["default_agent"] = "build".into();
    config["agent"].as_object_mut().unwrap().remove("helper");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(
        app.catalog().await.unwrap().agent_id.as_deref(),
        Some("review")
    );
    let mut events = app.subscribe();
    app.submit(session, "try restored tools".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { .. } => break,
            CoreEvent::TurnFailed { error, .. } => panic!("restored turn failed: {error}"),
            _ => {}
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    assert!(
        project.path().join("build-marker").exists(),
        "startup review constraints stuck to central authority"
    );
    for denied in ["startup", "review", "child", "restored"] {
        assert!(
            !project.path().join(format!("{denied}-marker")).exists(),
            "{denied} widened bash"
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.path().join("mcp-calls")).unwrap(),
        "called\n"
    );
    let captured = requests.lock().unwrap();
    assert_eq!(captured.len(), 11);
    for (index, prefix) in [
        (1, "startup"),
        (7, "child"),
        (8, "review"),
        (10, "restored"),
    ] {
        assert_eq!(
            function_output(&captured[index], &format!("{prefix}-bash")),
            Some("error: denied bash")
        );
        assert_eq!(
            function_output(&captured[index], &format!("{prefix}-mcp")),
            Some("error: denied fixture__query")
        );
    }
    assert!(
        function_output(&captured[4], "build-bash")
            .unwrap()
            .contains("exit 0")
    );
    assert!(
        function_output(&captured[4], "build-mcp")
            .unwrap()
            .contains("MCP allowed")
    );
    assert!(
        function_output(&captured[8], "spawn")
            .unwrap()
            .contains("done")
    );
    for (index, request) in captured.iter().enumerate() {
        assert_eq!(
            request.to_string().contains("PRIMARY_POLICY_GUIDANCE"),
            matches!(index, 3 | 4),
            "guidance in request {index}"
        );
    }
    assert!(captured[6]["input"].to_string().contains("HELPER_CHILD"));
    assert!(captured[9]["input"].to_string().contains("REVIEW_PRIMARY"));
}

#[tokio::test]
async fn primary_workspace_policy_also_bounds_manual_compress() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("primary-compress").unwrap();
    let start = harness
        .db
        .append_message("primary-compress", "user", &"input ".repeat(100))
        .unwrap();
    let end = harness
        .db
        .append_message("primary-compress", "assistant", &"output ".repeat(100))
        .unwrap();
    harness
        .db
        .append_message("primary-compress", "user", "next task")
        .unwrap();
    let args = serde_json::json!({"topic":"finished", "content":[{"startId":start,"endId":end,"summary":"done"}]});
    let spec = ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        ..ProtectedSpec::default()
    };
    let rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"tools":{"compress":false}}),
    )
    .unwrap();
    runtime
        .publish_workspace(
            None,
            "",
            Vec::new(),
            BTreeMap::new(),
            None,
            Some("review".into()),
            None,
            BTreeMap::new(),
            rules,
        )
        .unwrap();
    assert_eq!(
        runtime
            .run_compress("primary-compress", &args, &spec)
            .unwrap_err(),
        oc_adapters::runtime::RuntimeError::PermissionDenied {
            tool: "compress".into()
        }
    );
    assert!(
        harness
            .db
            .list_tool_ops("primary-compress")
            .unwrap()
            .is_empty()
    );
    assert!(
        harness
            .db
            .load_compression_blocks("primary-compress")
            .unwrap()
            .is_empty()
    );
    runtime
        .publish_workspace(
            None,
            "",
            Vec::new(),
            BTreeMap::new(),
            None,
            Some("build".into()),
            None,
            BTreeMap::new(),
            Default::default(),
        )
        .unwrap();
    assert!(
        runtime
            .run_compress("primary-compress", &args, &spec)
            .unwrap()
            .shrank
    );
}

#[tokio::test]
async fn t47_unknown_limits_use_configured_caps_without_variant_overlay() {
    for limit in [
        serde_json::Value::Null,
        serde_json::json!({"context": 16_384}),
        serde_json::json!({"output": 128}),
        serde_json::json!({"output": 100_000}),
        serde_json::json!({"context": 0, "output": 0}),
        serde_json::json!({"context": 16_384, "output": 0}),
        serde_json::json!({"context": 0, "output": 128}),
    ] {
        let (mut harness, _) = make_harness(allow_all());
        let entry = serde_json::json!({"limit": limit, "variants": {"low": {"reasoningEffort": "low"}, "custom": {"reasoningEffort": "deep"}}});
        harness.catalog.models.insert("m".into(), entry.clone());
        let mut generation = oc_adapters::config::assemble(&[oc_adapters::config::Source {
            path: "test.json".into(), trusted: true,
            text: serde_json::json!({"provider":{"test":{"options":{"apiKey":"test-key", "nativeFallbackLimits":{"context":16_384,"output":256}}}}}).to_string(),
        }], &BTreeMap::new(), None).unwrap();
        generation.permissions = allow_all();
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let (base, hits, requests) =
            Fake::start_recording(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
        let mut turn = params("s", "hello", &harness, provider_of(&base), &NO_CANCEL);
        turn.max_output = 0; // Application's absent-output path.
        let report = runtime.run_turn(turn).await.unwrap();
        assert_eq!(report.status, TurnStatus::Completed, "{report:?}");
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("unknown"));
        assert!(report.warnings[0].contains("native fallback caps (context=16384, output=256)"));
        assert_eq!(harness.catalog.models["m"], entry);
        let expected_output = limit
            .get("output")
            .and_then(serde_json::Value::as_u64)
            .filter(|n| *n > 0)
            .unwrap_or(256)
            .min(256);
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0]["max_output_tokens"], expected_output);
            assert!(requests[0].get("reasoning").is_none(), "{:?}", requests[0]);
        }
        let huge = "x".repeat(80_000);
        let result = runtime
            .run_turn(params("s", &huge, &harness, provider_of(&base), &NO_CANCEL))
            .await;
        assert!(result.unwrap_err().to_string().contains("exceeds context"));
        assert_eq!(
            *hits.lock().unwrap(),
            1,
            "over-budget request reached provider"
        );
        runtime.shutdown_mcp().await.unwrap();
    }
}

#[tokio::test]
async fn v04_model_change_projects_public_history_without_foreign_tool_state() {
    let (mut harness, generation) = make_harness(allow_all());
    harness
        .catalog
        .models
        .insert("other".into(), harness.catalog.models["m"].clone());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("model-change").unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call(
                "old-model-call",
                "read",
                &serde_json::json!({"filePath":"missing-fixture"}),
            ) + &sse_completed(),
            sse_delta("public original answer") + &sse_completed(),
            sse_delta("public second answer") + &sse_completed(),
            sse_delta("returned original") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    for (index, (model, prompt)) in [
        ("m", "expanded review instructions with original config"),
        ("other", "second public input"),
        ("m", "third public input"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut turn = params(
            "model-change",
            prompt,
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        );
        turn.model_id = model.into();
        if index == 0 {
            turn.invocation = Some("/review".into());
        }
        assert_eq!(
            runtime.run_turn(turn).await.unwrap().status,
            TurnStatus::Completed
        );
    }
    {
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 4);
        assert!(captured[1]["input"].to_string().contains("old-model-call"));
        let changed = captured[2]["input"].to_string();
        assert!(changed.contains("expanded review instructions with original config"));
        assert!(changed.contains("public original answer"));
        assert!(!changed.contains("/review"));
        assert!(!changed.contains("old-model-call") && !changed.contains("function_call"));
        assert_eq!(captured[2]["model"], "other");
        assert!(
            captured[3]["input"].to_string().contains("old-model-call"),
            "original wire lane retained durably"
        );
        assert!(
            captured[3]["input"]
                .to_string()
                .contains("public second answer")
        );
    }
    // Legacy/incomplete log without a usable stored prompt: fall back to the
    // immutable public invocation rather than expanding current command config.
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        sql.execute(
            "UPDATE turns SET prompt = '' WHERE session_id = ?1 AND prompt = ?2",
            rusqlite::params![
                "model-change",
                "expanded review instructions with original config"
            ]
        )
        .unwrap(),
        1
    );
    let mut fallback = params(
        "model-change",
        "fourth public input",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    fallback.model_id = "other".into();
    assert_eq!(
        runtime.run_turn(fallback).await.unwrap().status,
        TurnStatus::Completed
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    let fallback_input = requests[4]["input"].to_string();
    assert!(fallback_input.contains("/review"));
    assert!(!fallback_input.contains("expanded review instructions"));
    assert!(!fallback_input.contains("old-model-call"));
    let public = harness.db.read_history("model-change").unwrap();
    assert_eq!(public.len(), 8);
    assert_eq!(public[0], ("user".into(), "/review".into()));
    assert!(
        !public
            .iter()
            .any(|(_, text)| text.contains("expanded review instructions"))
    );
}

fn dcp_nudge_count(request: &serde_json::Value) -> usize {
    request["input"]
        .to_string()
        .matches("exceeds soft limit")
        .count()
}

#[tokio::test]
async fn t47_admission_counts_tool_schemas_and_rechecks_tool_results() {
    for (context, expected_calls) in [(1_200, 0), (8_192, 1)] {
        let (mut harness, mut generation) = make_harness(allow_all());
        harness
            .catalog
            .models
            .insert("m".into(), serde_json::json!({}));
        generation.providers.insert(
            "test".into(),
            serde_json::from_value(serde_json::json!({
                "options":{"nativeFallbackLimits":{"context":context,"output":64}}
            }))
            .unwrap(),
        );
        let large = harness._project.path().join("large.txt");
        std::fs::write(
            &large,
            "large fact with retained original detail\n".repeat(4_000),
        )
        .unwrap();
        let runtime = runtime_with_dcp(
            &harness,
            generation,
            Vec::new(),
            DcpConfig {
                enabled: false,
                ..DcpConfig::default()
            },
        );
        runtime.create_session("s").unwrap();
        let script = sse_tool_call(
            "read-large",
            "read",
            &serde_json::json!({"path":"large.txt", "limit":4_000}),
        ) + &sse_completed();
        let (base, hits, _) = Fake::start_recording(vec![script], Duration::ZERO);
        let report = runtime
            .run_turn(params(
                "s",
                "read",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Failed, "{report:?}");
        assert!(
            report
                .diagnostic
                .as_deref()
                .unwrap()
                .contains("exceeds context")
        );
        assert_eq!(report.rounds, expected_calls);
        assert_eq!(*hits.lock().unwrap(), expected_calls as usize);
        assert_eq!(report.warnings.len(), 1);
        if expected_calls > 0 {
            let operations = harness.db.list_tool_ops("s").unwrap();
            assert_eq!(operations.len(), 1);
            assert_eq!(operations[0].state, "completed");
            assert!(
                operations[0]
                    .output
                    .as_deref()
                    .unwrap()
                    .contains("large fact")
            );
            assert!(
                operations[0].output_bytes > 32_768,
                "oversized result must stay durable"
            );
        }
        runtime.shutdown_mcp().await.unwrap();
    }
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
async fn s08_bash_intent_store_fault_prevents_effect_and_recovers_unknown_turn() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("intent-fault").unwrap();
    let marker = harness._project.path().join("marker");
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER fail_bash_intent BEFORE INSERT ON tool_operations
         WHEN NEW.name = 'bash' AND NEW.session_id = 'intent-fault'
         BEGIN SELECT RAISE(ABORT, 'injected bash intent failure'); END;",
    )
    .unwrap();
    let tool = sse_tool_call(
        "touch-marker",
        "bash",
        &serde_json::json!({"argv": ["/bin/touch", "marker"]}),
    );
    let (base, hits) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    let mut accepted = None;
    let mut tool_events = Vec::new();
    let result = runtime
        .run_turn_with_tool_events(
            params(
                "intent-fault",
                "create marker",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |id| accepted = Some(id.to_string()),
            |_, _| {},
            |_, _| {},
            |_, event| tool_events.push(event.clone()),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert_eq!(
        *hits.lock().unwrap(),
        1,
        "provider tool call was not received"
    );
    assert!(
        accepted.is_some(),
        "turn was rejected before the injected fault"
    );
    assert!(
        tool_events.is_empty(),
        "undurable tool was shown as started"
    );
    assert!(!marker.exists(), "bash ran despite rejected durable intent");

    sql.execute_batch("DROP TRIGGER fail_bash_intent").unwrap();
    drop(sql);
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert_eq!(reopened.recover_interrupted_tools().unwrap(), 0);
    assert!(reopened.list_tool_ops("intent-fault").unwrap().is_empty());
    assert_eq!(
        reopened.read_history("intent-fault").unwrap(),
        [("user".to_string(), "create marker".to_string())]
    );
    let sql = rusqlite::Connection::open(reopened.root().join("oc.sqlite")).unwrap();
    let (status, result): (String, Option<String>) = sql
        .query_row(
            "SELECT status, result FROM turns WHERE id = ?1",
            [accepted.unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        status, "unknown",
        "failed store write must not become success"
    );
    let journal: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(
        journal["display_parts"].as_array().unwrap().len(),
        0,
        "rolled-back intent must not leave a replayable tool card: {journal}"
    );
    let unknown_events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = 'intent-fault' AND kind = 'turn_unknown'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_events, 1, "restart must expose an uncertain turn");
    assert!(!marker.exists(), "recovery replayed an undurable effect");
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
            animations: None,
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: allow_all(),
            permission_rules: Default::default(),
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
async fn backend_parity_mcp_projection_permissions_history_and_error_canaries() {
    let (harness, mut generation) = make_harness(allow_all());
    let script = harness._project.path().join("parity.py");
    std::fs::write(&script, r#"import json, sys
server = sys.argv[1]
for line in sys.stdin:
    r=json.loads(line); method=r.get('method'); result=None
    if method == 'initialize':
        result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'},'instructions':'GUIDANCE_'+server.upper()+' use query; PROVIDER-CANARY HEADER-CANARY'}
    elif method == 'tools/list':
        if server == 'broken':
            print(json.dumps({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32603,'message':'UNKNOWN-CANARY'}}),flush=True)
            continue
        result={'tools':[] if server == 'empty' else [{'name':'query','inputSchema':{'type':'object','properties':{'fail':{'type':'boolean'}}}}]}
    elif method == 'tools/call':
        if r['params']['arguments'].get('fail'):
            result={'isError':True,'structuredContent':{'error':{'code':'INVALID_ARGUMENTS'}},'content':[{'type':'text','text':'invalid argument: missing parameter query; UNKNOWN-CANARY PROVIDER-CANARY HEADER-CANARY'}]}
        else:
            result={'content':[{'type':'text','text':'Useful explanation '+sys.argv[2]},{'type':'resource','resource':{'uri':'file:///fixture','text':'Embedded content'}}],'structuredContent':{'answer':42,'echo':'PROVIDER-CANARY HEADER-CANARY','argvEcho':sys.argv[2],'token':'UNKNOWN-CANARY'}}
    if result is not None:
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#).unwrap();
    for (server, permission) in [
        ("good", Permission::Allow),
        ("denied", Permission::Deny),
        ("ask", Permission::Ask),
        ("empty", Permission::Allow),
        ("broken", Permission::Allow),
        ("disabled", Permission::Allow),
        ("unlisted", Permission::Deny),
    ] {
        generation.mcp.insert(
            server.into(),
            McpEntry {
                kind: "local".into(),
                enabled: server != "disabled",
                command: vec![
                    "/usr/bin/python3".into(),
                    script.to_string_lossy().into_owned(),
                    server.into(),
                    "CONFIG-CANARY".into(),
                ],
                headers: BTreeMap::from([("x-fixture".into(), "HEADER-CANARY".into())]),
                timeout: Some(2_000),
                ..McpEntry::default()
            },
        );
        if server != "unlisted" {
            generation
                .permissions
                .insert(format!("{server}__query"), permission);
        }
    }
    generation.providers.insert(
        "test".into(),
        oc_adapters::config::ProviderEntry {
            npm: None,
            name: None,
            models: BTreeMap::new(),
            options: oc_adapters::config::ProviderOptions {
                api_key: "PROVIDER-CANARY".into(),
                ..Default::default()
            },
        },
    );
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("data", "good__query", &serde_json::json!({}))
                + &sse_tool_call("failed", "good__query", &serde_json::json!({"fail":true}))
                + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "s",
            "first",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 2);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(report.calls[1].state, "failed");
    assert!(
        report.calls[1]
            .output
            .contains("invalid arguments; check the tool schema")
    );
    assert_eq!(
        report.warnings,
        ["mcp broken tools-list: transport (retryable=true)"]
    );
    let captured = requests.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    for request in &captured {
        let wire = request.to_string();
        assert_eq!(wire.matches("GUIDANCE_GOOD").count(), 1);
        for absent in [
            "GUIDANCE_DENIED",
            "GUIDANCE_ASK",
            "GUIDANCE_EMPTY",
            "GUIDANCE_BROKEN",
            "GUIDANCE_DISABLED",
            "GUIDANCE_UNLISTED",
            "PROVIDER-CANARY",
            "HEADER-CANARY",
            "UNKNOWN-CANARY",
            "CONFIG-CANARY",
        ] {
            assert!(!wire.contains(absent), "leaked {absent}");
        }
    }
    let outputs: Vec<&serde_json::Value> = captured[1]["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs.len(), 2);
    let data = outputs[0]["output"].as_str().unwrap();
    assert!(data.contains("Useful explanation"));
    assert!(data.contains("Embedded content"));
    assert!(data.contains("\"answer\":42"));
    assert!(data.contains("[redacted]"));
    let original = harness.db.read_history_full("s").unwrap();
    let stored = format!(
        "{original:?} {:?}",
        harness.db.turn_result(&report.turn_id).unwrap()
    );
    assert!(
        !stored.contains("GUIDANCE_"),
        "instructions must not enter durable history"
    );
    assert!(!stored.contains("CANARY"));

    // Resource-specific authority cannot inherit the scalar compatibility allow
    // when projecting whole-server guidance into a turn.
    generation.permission_rules = oc_adapters::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission":{"good__query":{"*":"deny","special":"allow"}}}),
    )
    .unwrap();
    runtime.reload(generation).await.unwrap();
    let report = runtime
        .run_turn(params(
            "s",
            "second",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert!(
        !requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .to_string()
            .contains("GUIDANCE_")
    );
    let after = harness.db.read_history_full("s").unwrap();
    assert_eq!(
        &after[..original.len()],
        original.as_slice(),
        "projection must not rewrite history"
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
async fn aud23_partial_attach_failure_degrades_and_still_reaps_child() {
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
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let report = runtime
        .run_turn(params(
            "s",
            "degraded peer",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("a degraded peer must not abort the turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report.warnings,
        vec!["mcp b-bad DNS: private_host (retryable=false)"]
    );
    let pid = std::fs::read_to_string(&pid_file)
        .unwrap()
        .parse::<libc::pid_t>()
        .unwrap();
    // SAFETY: signal 0 only probes the fixture child pid.
    let probe = unsafe { libc::kill(pid, 0) };
    assert_eq!(probe, 0, "healthy peer lost its child");
    // AUD23 still holds: generation shutdown reaps the connected child.
    runtime.shutdown_mcp().await.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    // SAFETY: signal 0 only probes the fixture child pid.
    while unsafe { libc::kill(pid, 0) } == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // SAFETY: final fixture liveness probe only.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "partial attach leaked child");
    assert_eq!(harness.db.history_len("s").unwrap(), 2);
}

#[tokio::test]
async fn mcp_attach_failure_degrades_the_server() {
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
    let report = runtime
        .run_turn(params("s", "hi", &harness, provider_of(&base), &NO_CANCEL))
        .await
        .expect("attach failure must degrade, not abort");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        report.warnings,
        vec!["mcp codex DNS: private_host (retryable=false)"]
    );
    assert!(
        report.calls.is_empty(),
        "degraded server published no tools"
    );
    assert_eq!(
        harness.db.history_len("s").expect("len"),
        2,
        "turn committed"
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
            animations: None,
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: [("read".to_string(), Permission::Allow)]
                .into_iter()
                .collect(),
            permission_rules: Default::default(),
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
                    animations: None,
                    providers: BTreeMap::new(),
                    mcp: BTreeMap::new(),
                    permissions: BTreeMap::new(),
                    permission_rules: Default::default(),
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

#[tokio::test]
async fn two_reasoning_output_items_keep_public_parts_and_opaque_continuation_separate() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("reasoning-items").unwrap();
    let items: Vec<_> = [("rs_1", "encrypted-first"), ("rs_2", "encrypted-second")]
        .into_iter()
        .map(|(id, encrypted_content)| {
            serde_json::json!({
                "type":"reasoning", "id":id, "encrypted_content":encrypted_content,
                "summary":[], "status":"completed"
            })
        })
        .collect();
    let message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_1",
        "status":"completed", "content":[{"type":"output_text", "text":"between"}]});
    let final_message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_2",
        "status":"completed", "content":[{"type":"output_text", "text":"final"}]});
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_reasoning("Inspecting")
                + &sse_reasoning_done("rs_1", "encrypted-first")
                + &sse_delta("between")
                + &sse_message_done_by_id(&message)
                + &sse_reasoning("Verifying")
                + &sse_reasoning_done("rs_2", "encrypted-second")
                + &sse_message_done(3, &final_message)
                + &sse_completed_output(vec![
                    items[0].clone(),
                    message,
                    items[1].clone(),
                    final_message,
                ]),
            sse_delta("next") + &sse_completed(),
        ],
        Duration::from_millis(20),
    );
    let public = Arc::new(Mutex::new(Vec::new()));
    let deltas = public.clone();
    let ends = public.clone();
    let texts = public.clone();
    let report = runtime
        .run_turn_with_reasoning_items(
            params(
                "reasoning-items",
                "first",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            move |_, delta| texts.lock().unwrap().push(format!("text:{delta}")),
            move |_, delta| deltas.lock().unwrap().push(delta.to_owned()),
            move |_| ends.lock().unwrap().push("<ended>".into()),
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        *public.lock().unwrap(),
        [
            "Inspecting",
            "<ended>",
            "text:between",
            "Verifying",
            "<ended>"
        ]
    );
    assert_eq!(report.text, "betweenfinal");
    assert_eq!(report.usage, Some((10, 5)));
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"][0]["reasoning"], "Inspecting");
    assert_eq!(
        stored["display_parts"][1]["message"], 2,
        "text slot matched by item id without an output index"
    );
    assert_eq!(stored["display_parts"][2]["reasoning"], "Verifying");
    assert_eq!(stored["display_parts"][3]["message"], 4);
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 4);
    assert!(
        report.streamed_ms >= 20,
        "fake delayed the whole generation"
    );
    for part in [&stored["display_parts"][0], &stored["display_parts"][2]] {
        assert!(
            part["duration_ms"].as_u64().unwrap() < report.streamed_ms,
            "each item starts with its own public delta, not the generation request"
        );
    }
    assert!(!stored["display_parts"].to_string().contains("encrypted-"));
    assert!(!stored["display_parts"].to_string().contains("pending_text"));
    assert_eq!(stored["input"][2]["content"][0]["text"], "between");
    assert_eq!(stored["input"][4]["content"][0]["text"], "final");
    assert_eq!(stored["input"][1]["encrypted_content"], "encrypted-first");
    assert_eq!(stored["input"][3]["encrypted_content"], "encrypted-second");
    let next = runtime
        .run_turn(params(
            "reasoning-items",
            "second",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(next.status, TurnStatus::Completed);
    assert_eq!(*hits.lock().unwrap(), 2);
    let requests = requests.lock().unwrap();
    let replay = requests[1]["input"].as_array().unwrap();
    for item in &items {
        assert!(
            replay.contains(item),
            "opaque continuation must survive replay"
        );
    }
    assert!(
        replay
            .iter()
            .any(|item| item["id"] == "msg_1" && item["content"][0]["text"] == "between")
    );
    assert!(
        replay
            .iter()
            .any(|item| item["id"] == "msg_2" && item["content"][0]["text"] == "final")
    );
}

#[tokio::test]
async fn only_done_reasoning_items_have_duration_on_failed_cancelled_or_incomplete_streams() {
    let failed = "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\n\n";
    let incomplete =
        "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\"}}\n\n";
    for (terminal, expected, stored_status, cancel_after_second) in [
        (failed, TurnStatus::Failed, "failed", false),
        (incomplete, TurnStatus::Incomplete, "incomplete", false),
        ("", TurnStatus::Incomplete, "incomplete", false),
        ("", TurnStatus::Cancelled, "cancelled", true),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("unfinished-reasoning").unwrap();
        let (base, hits) = Fake::start(
            vec![
                sse_reasoning("First")
                    + &sse_reasoning_done("rs_1", "private-first")
                    + &sse_reasoning("Second")
                    + &sse_reasoning(" half")
                    + terminal,
            ],
            Duration::ZERO,
        );
        let cancel = AtomicBool::new(false);
        let mut observed = String::new();
        let mut ended = 0;
        let report = runtime
            .run_turn_with_reasoning_items(
                params(
                    "unfinished-reasoning",
                    "keep the user turn",
                    &harness,
                    provider_of(&base),
                    &cancel,
                ),
                |_| {},
                |_, _| {},
                |_, delta| {
                    observed.push_str(delta);
                    if cancel_after_second && delta == " half" {
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
                |_| ended += 1,
                |_, _| {},
            )
            .await
            .unwrap();
        assert_eq!(observed, "FirstSecond half", "fixture must open r2");
        assert_eq!(ended, 1, "only r1 sent output_item.done");
        assert_eq!(report.status, expected);
        assert!(report.calls.is_empty());
        assert!(
            harness
                .db
                .list_tool_ops("unfinished-reasoning")
                .unwrap()
                .is_empty()
        );
        assert_eq!(*hits.lock().unwrap(), 1, "no retry or tool generation");
        let (status, stored) = harness.db.turn_result(&report.turn_id).unwrap();
        assert_eq!(status, stored_status);
        let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
        let parts = stored["display_parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2, "no synthesized assistant or tool part");
        assert_eq!(parts[0]["reasoning"], "First");
        assert!(parts[0]["duration_ms"].as_u64().is_some());
        assert_eq!(parts[1]["reasoning"], "Second half");
        assert!(parts[1].get("duration_ms").is_none(), "r2 never ended");
        assert!(!stored.to_string().contains("private-first"));
        assert!(!stored.to_string().contains("pending_text"));
        assert_eq!(
            harness.db.read_history("unfinished-reasoning").unwrap(),
            [("user".into(), "keep the user turn".into())]
        );
    }
}

#[tokio::test]
async fn successful_generation_without_reasoning_item_done_leaves_open_part_untimed() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("open-reasoning-success").unwrap();
    let first = serde_json::json!({"type":"reasoning", "id":"rs_1",
        "encrypted_content":"private-first", "summary":[], "status":"completed"});
    let message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_1",
        "status":"completed", "content":[{"type":"output_text", "text":"answer"}]});
    let (base, hits) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("rs_1", "private-first")
                + &sse_reasoning("Second")
                + &sse_completed_output(vec![first.clone(), message]),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "open-reasoning-success",
            "question",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "answer");
    assert!(report.calls.is_empty());
    assert!(
        harness
            .db
            .list_tool_ops("open-reasoning-success")
            .unwrap()
            .is_empty()
    );
    assert_eq!(*hits.lock().unwrap(), 1);
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    let parts = stored["display_parts"].as_array().unwrap();
    assert_eq!(parts[0]["reasoning"], "First");
    assert!(parts[0]["duration_ms"].as_u64().is_some());
    assert_eq!(parts[1]["reasoning"], "Second");
    assert!(parts[1].get("duration_ms").is_none());
    assert_eq!(parts[2]["message"], 2);
    assert_eq!(parts.len(), 3);
    assert_eq!(
        stored["input"][1], first,
        "only completed opaque item is replayable"
    );
    assert!(
        !stored["display_parts"]
            .to_string()
            .contains("private-first")
    );
    assert_eq!(
        harness.db.read_history("open-reasoning-success").unwrap(),
        [
            ("user".into(), "question".into()),
            ("assistant".into(), "answer".into()),
        ]
    );
}

#[tokio::test]
async fn legacy_reasoning_done_without_item_id_keeps_one_public_part() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("legacy-reasoning").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("", "private")
                + &sse_reasoning("Second")
                + &sse_delta("answer")
                + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut ended = 0;
    let report = runtime
        .run_turn_with_reasoning_items(
            params(
                "legacy-reasoning",
                "hi",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_| ended += 1,
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(ended, 0);
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"][0]["reasoning"], "FirstSecond");
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn completed_message_without_delta_or_item_events_stays_between_reasoning_items() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("canonical-order").unwrap();
    let first = serde_json::json!({"type":"reasoning","id":"r1","status":"completed","encrypted_content":"private-1"});
    let second = serde_json::json!({"type":"reasoning","id":"r2","status":"completed","encrypted_content":"private-2"});
    let middle = serde_json::json!({"type":"message","id":"m1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"middle"}]});
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("r1", "private-1")
                + &sse_reasoning("Second")
                + &sse_reasoning_done("r2", "private-2")
                + &sse_completed_output(vec![first, middle, second]),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "canonical-order",
            "hi",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "middle");
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 3);
    assert_eq!(stored["display_parts"][0]["reasoning"], "First");
    assert_eq!(stored["display_parts"][1]["message"], 2);
    assert_eq!(stored["display_parts"][2]["reasoning"], "Second");
    assert!(!stored["display_parts"].to_string().contains("private-"));
}

#[tokio::test]
async fn canonical_tool_cards_keep_output_order_and_durable_read_pairing_after_restart() {
    for before_text in [false, true] {
        let (mut harness, generation) = make_harness(allow_all());
        std::fs::write(
            harness._project.path().join("fixture.txt"),
            "fixture contents\n",
        )
        .unwrap();
        let runtime = runtime_of(&harness, generation.clone(), Vec::new());
        runtime.create_session("ordered-tools").unwrap();
        let call = serde_json::json!({"type":"function_call", "id":"fc_ordered",
            "call_id":"ordered", "name":"read", "arguments":"{\"path\":\"fixture.txt\"}",
            "status":"completed"});
        let first = serde_json::json!({"type":"message", "id":"msg_first", "role":"assistant",
            "status":"completed", "content":[{"type":"output_text", "text":"before"}]});
        let second = serde_json::json!({"type":"message", "id":"msg_second", "role":"assistant",
            "status":"completed", "content":[{"type":"output_text", "text":"after"}]});
        let reasoning = serde_json::json!({"type":"reasoning", "id":"rs_ordered",
            "encrypted_content":"private-ordered", "status":"completed"});
        let (stream, output) = if before_text {
            (
                sse_reasoning("Checking")
                    + &sse_reasoning_done("rs_ordered", "private-ordered")
                    + &sse_message_done(1, &first)
                    + &sse_tool_call(
                        "ordered",
                        "read",
                        &serde_json::json!({"path":"fixture.txt"}),
                    )
                    + &sse_message_done(3, &second),
                vec![reasoning, first, call, second],
            )
        } else {
            (
                sse_tool_call(
                    "ordered",
                    "read",
                    &serde_json::json!({"path":"fixture.txt"}),
                ) + &sse_message_done(1, &first),
                vec![call, first],
            )
        };
        let (base, hits, requests) = Fake::start_recording(
            vec![
                stream + &sse_completed_output(output.clone()),
                sse_delta("done") + &sse_completed(),
                sse_delta("after restart") + &sse_completed(),
            ],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params(
                "ordered-tools",
                "inspect",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(report.rounds, 2);
        assert_eq!(report.calls.len(), 1);
        assert_eq!(report.calls[0].state, "completed");
        assert_eq!(
            report.text,
            if before_text {
                "beforeafterdone"
            } else {
                "beforedone"
            }
        );
        let ops = harness.db.list_tool_ops("ordered-tools").unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].state, "completed");
        let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
        let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        let parts = stored["display_parts"].as_array().unwrap();
        let markers = parts
            .iter()
            .map(|part| {
                if part.get("reasoning").is_some() {
                    "reasoning"
                } else if part.get("tool").is_some() {
                    "tool"
                } else {
                    "message"
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            markers,
            if before_text {
                vec!["reasoning", "message", "tool", "message", "message"]
            } else {
                vec!["tool", "message", "message"]
            }
        );
        let card = parts
            .iter()
            .find(|part| part.get("tool").is_some())
            .unwrap();
        assert_eq!(card["tool"], ops[0].op);
        let messages = parts
            .iter()
            .filter_map(|part| part["message"].as_u64())
            .map(|index| {
                stored["input"][index as usize]["content"][0]["text"]
                    .as_str()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            if before_text {
                vec!["before", "after", "done"]
            } else {
                vec!["before", "done"]
            }
        );
        assert!(
            !stored["display_parts"]
                .to_string()
                .contains("private-ordered")
        );
        assert!(!stored["display_parts"].to_string().contains("pending_text"));
        assert_eq!(
            harness.db.read_history("ordered-tools").unwrap(),
            [
                ("user".into(), "inspect".into()),
                ("assistant".into(), report.text.clone())
            ]
        );
        for (offset, item) in output.iter().enumerate() {
            assert_eq!(stored["input"][offset + 1], *item);
        }
        let paired = &stored["input"][output.len() + 1];
        assert_eq!(paired["type"], "function_call_output");
        assert_eq!(paired["call_id"], "ordered");
        assert!(
            paired["output"]
                .as_str()
                .unwrap()
                .contains("fixture contents")
        );
        assert_eq!(*hits.lock().unwrap(), 2);
        {
            let requests_before = requests.lock().unwrap();
            let second_input = requests_before[1]["input"].as_array().unwrap();
            assert_eq!(
                &second_input[second_input.len() - output.len() - 1..second_input.len() - 1],
                output
            );
            assert_eq!(
                function_output(&requests_before[1], "ordered"),
                paired["output"].as_str()
            );
        }

        drop(runtime);
        drop(harness.db);
        harness.db = Db::open(harness._data.path()).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &harness.db.turn_result(&report.turn_id).unwrap().1.unwrap()
            )
            .unwrap(),
            stored
        );
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.open_session("ordered-tools").unwrap();
        let resumed = runtime
            .run_turn(params(
                "ordered-tools",
                "continue",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(resumed.status, TurnStatus::Completed);
        assert!(resumed.calls.is_empty());
        assert_eq!(harness.db.list_tool_ops("ordered-tools").unwrap().len(), 1);
        assert_eq!(*hits.lock().unwrap(), 3);
        let requests = requests.lock().unwrap();
        let replay = requests[2]["input"].as_array().unwrap();
        for item in &output {
            assert_eq!(
                replay.iter().filter(|candidate| *candidate == item).count(),
                1
            );
        }
        assert_eq!(
            function_output(&requests[2], "ordered"),
            paired["output"].as_str()
        );
        assert_eq!(
            harness.db.read_history("ordered-tools").unwrap(),
            [
                ("user".into(), "inspect".into()),
                ("assistant".into(), report.text),
                ("user".into(), "continue".into()),
                ("assistant".into(), "after restart".into()),
            ]
        );
    }
}

#[tokio::test]
async fn unidentifiable_calls_append_only_at_intent_and_duplicate_call_ids_fail_closed() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("unidentified-tool").unwrap();
    let call = serde_json::json!({"type":"function_call", "call_id":"read-one",
        "name":"read", "arguments":"{\"path\":\"fixture.txt\"}"});
    let message = serde_json::json!({"type":"message", "id":"msg_one", "role":"assistant",
        "content":[{"type":"output_text", "text":"hello"}]});
    std::fs::write(harness._project.path().join("fixture.txt"), "present").unwrap();
    let (base, hits) = Fake::start(
        vec![sse_completed_output(vec![call.clone(), message.clone()])],
        Duration::ZERO,
    );
    let mut request = params(
        "unidentified-tool",
        "read",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    request.max_rounds = 1;
    let report = runtime.run_turn(request).await.unwrap();
    assert_eq!(report.status, TurnStatus::Incomplete);
    assert_eq!(report.rounds, 1);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 1);
    let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    let parts = stored["display_parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["message"], 2);
    assert_eq!(
        parts[1]["tool"],
        harness.db.list_tool_ops("unidentified-tool").unwrap()[0].op
    );
    assert_eq!(stored["input"][1], call);
    assert_eq!(stored["input"][2], message);
    assert_eq!(stored["input"][3]["call_id"], "read-one");

    runtime.create_session("duplicate-tool").unwrap();
    let duplicate = serde_json::json!({"type":"function_call", "id":"fc_second",
        "call_id":"read-one", "name":"read", "arguments":"{\"path\":\"fixture.txt\"}"});
    let (base, hits) = Fake::start(
        vec![sse_completed_output(vec![
            stored["input"][1].clone(),
            duplicate,
        ])],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "duplicate-tool",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert_eq!(report.rounds, 1);
    assert_eq!(*hits.lock().unwrap(), 1);
    assert!(
        harness
            .db
            .list_tool_ops("duplicate-tool")
            .unwrap()
            .is_empty()
    );
    let (_, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    assert!(stored["display_parts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn provider_context_usage_survives_missing_round_usage_and_restart() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let tool = sse_tool_call(
        "missing",
        "read",
        &serde_json::json!({"path":"missing.txt"}),
    );
    let (base, hits) = Fake::start(
        vec![
            tool + sse_completed_without_usage(),
            sse_delta("answer") + &sse_completed_usage(6000, 763),
            sse_tool_call("known", "read", &serde_json::json!({"path":"missing.txt"}))
                + &sse_completed_usage(4500, 21),
            sse_delta("later") + sse_completed_without_usage(),
            sse_tool_call(
                "unfinished",
                "read",
                &serde_json::json!({"path":"missing.txt"}),
            ) + &sse_completed_usage(7000, 34),
            sse_tool_call(
                "cancel-after-tool",
                "read",
                &serde_json::json!({"path":"missing.txt"}),
            ) + &sse_completed_usage(8000, 45),
        ],
        Duration::ZERO,
    );
    let fresh = runtime
        .run_fresh_turn_with_tool_events(
            params("context", "first", &harness, provider_of(&base), &NO_CANCEL),
            None,
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(fresh.status, TurnStatus::Completed);
    assert_eq!(fresh.rounds, 2);
    assert_eq!(fresh.usage, None, "missing billed tool round is unknown");
    assert_eq!(fresh.context_usage, Some((6000, 763)));

    let next = runtime
        .run_turn_with_tool_events(
            params(
                "context",
                "second",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(next.status, TurnStatus::Completed);
    assert_eq!(next.rounds, 2);
    assert_eq!(next.usage, None);
    assert_eq!(
        next.context_usage,
        Some((4500, 21)),
        "a missing final round must not erase a known pair"
    );

    let mut incomplete_params =
        params("context", "third", &harness, provider_of(&base), &NO_CANCEL);
    incomplete_params.max_rounds = 1;
    let incomplete = runtime.run_turn(incomplete_params).await.unwrap();
    assert_eq!(incomplete.status, TurnStatus::Incomplete);
    assert_eq!(incomplete.usage, Some((7000, 34)));
    assert_eq!(incomplete.context_usage, Some((7000, 34)));

    let cancelled_flag = AtomicBool::new(false);
    let cancelled = runtime
        .run_turn_with_tool_events(
            params(
                "context",
                "fourth",
                &harness,
                provider_of(&base),
                &cancelled_flag,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if matches!(event, ToolCallEvent::Finished { .. }) {
                    cancelled_flag.store(true, Ordering::Relaxed);
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.status, TurnStatus::Cancelled);
    assert_eq!(cancelled.usage, Some((8000, 45)));
    assert_eq!(cancelled.context_usage, Some((8000, 45)));
    assert_eq!(*hits.lock().unwrap(), 6);

    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    for (id, context, expected_status) in [
        (fresh.turn_id, [6000, 763], "completed"),
        (next.turn_id, [4500, 21], "completed"),
        (incomplete.turn_id, [7000, 34], "incomplete"),
        (cancelled.turn_id, [8000, 45], "cancelled"),
    ] {
        let (status, result) = reopened.turn_result(&id).unwrap();
        assert_eq!(status, expected_status);
        let result: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(
            result["display"]["context_usage"],
            serde_json::json!(context)
        );
        if status == "completed" {
            assert!(
                result["display"].get("usage").is_none(),
                "unknown billed usage must not reappear"
            );
        }
    }
}

/// End to end through the real application worker: `application::spawn_with_env`
/// (project-local config, no process env mutation) broadcasts the new
/// `ReasoningDelta` and `TurnUsage` events next to `TurnFinished`.
#[tokio::test]
async fn bare_title_regeneration_uses_title_agent_and_preserves_concurrent_manual_rename() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    use oc_core::session::CoreError;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, hits, requests) = Fake::start_recording(
        vec![
            sse_delta("late title") + &sse_completed(),
            sse_delta("Fresh title") + &sse_completed(),
        ],
        Duration::from_millis(800),
    );
    let config = serde_json::json!({
        "model": "fixture/main",
        "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
            "models": {"main": {}, "title-model": {}}}},
        "agent": {"title": {"mode": "subagent", "model": "fixture/title-model", "prompt": "TITLE_AGENT_ONLY"}}
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("title-root").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let mut events = app.subscribe();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert!(
        app.regenerate_title(session.clone()).await.is_err(),
        "empty history rejected"
    );
    conn.execute("INSERT INTO sessions(id,created_at,parent_id,title) VALUES ('child','test','title-root','child')", []).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','title-root',1,'user',?1)",
        [&format!("FIRST_SENTINEL {}", "x".repeat(20000))]).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('recent','title-root',2,'assistant','RECENT_SENTINEL')", []).unwrap();
    assert_eq!(
        app.regenerate_title(SessionId::new("child").unwrap()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        *hits.lock().unwrap(),
        0,
        "invalid requests cannot reach the provider"
    );
    let owner = app.clone();
    let target = session.clone();
    let first = tokio::spawn(async move { owner.regenerate_title(target).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while requests.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(200),
            app.history_page(session.clone(), None, None, 4)
        )
        .await
        .unwrap()
        .is_ok(),
        "snapshots must not block on provider"
    );
    assert_eq!(
        app.regenerate_title(session.clone()).await,
        Err(CoreError::TurnBusy)
    );
    app.rename_session(session.clone(), "manual wins".into())
        .await
        .unwrap();
    assert!(first.await.unwrap().is_err());
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='title-root'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "manual wins"
    );
    let result = app.regenerate_title(session.clone()).await.unwrap();
    assert_eq!(result, "Fresh title");
    assert_eq!(
        app.history_page(session.clone(), None, None, 5)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Fresh title")
    );
    let captured = requests.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    for request in captured.iter() {
        assert_eq!(request["model"], "title-model");
        assert!(
            request.get("tools").is_none()
                || request["tools"].as_array().is_some_and(Vec::is_empty)
        );
        let encoded = request["input"].to_string();
        assert!(encoded.contains("TITLE_AGENT_ONLY"));
        assert!(encoded.contains("FIRST_SENTINEL"));
        assert!(encoded.len() < 13000, "bounded title input");
    }
    assert!(captured[1]["input"].to_string().contains("RECENT_SENTINEL"));
    assert_eq!(
        captured[1]["input"]
            .to_string()
            .matches("FIRST_SENTINEL")
            .count(),
        1,
        "original request is not repeated in recent history"
    );
    assert_eq!(*hits.lock().unwrap(), 2);
    let turns: i64 = conn
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0, "regeneration is not a billed conversational turn");
    assert!(
        matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ),
        "title requests must not emit turn or usage events"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn bare_title_cancel_does_not_persist_or_emit_turn() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let url = Fake::start_stalled(
        vec![sse_delta("too late") + &sse_completed()],
        Duration::from_secs(2),
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model": "fixture/main", "provider": {"fixture": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": url, "apiKey": "dummy"}, "models": {"main": {}}}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let id = SessionId::new("cancel-title").unwrap();
    app.create_session(id.clone()).await.unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','cancel-title',1,'user','please title')", []).unwrap();
    let owner = app.clone();
    let target = id.clone();
    let pending = tokio::spawn(async move { owner.regenerate_title(target).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    app.cancel_title(id.clone()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        app.history_page(id, None, None, 4).await.unwrap().title,
        None
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn bare_title_invalid_configured_model_is_rejected_before_provider_io() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, hits) = Fake::start(
        vec![sse_delta("must not arrive") + &sse_completed()],
        Duration::ZERO,
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model": "fixture/main", "provider": {"fixture": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": url, "apiKey": "dummy"}, "models": {"main": {}}}},
            "agent": {"title": {"mode": "subagent", "model": "fixture/retired", "prompt": "title"}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("invalid-title-model").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','invalid-title-model',1,'user','request')", []).unwrap();
    assert!(app.regenerate_title(session.clone()).await.is_err());
    assert_eq!(*hits.lock().unwrap(), 0);
    assert_eq!(
        app.history_page(session, None, None, 2)
            .await
            .unwrap()
            .title,
        None
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

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
            | CoreEvent::TurnPresentation { .. }
            | CoreEvent::ReasoningItemEnded { .. }
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

#[tokio::test]
async fn anonymous_text_after_second_reasoning_keeps_unstreamed_first_message_on_restart() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::TranscriptPart;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("Inspecting")
                + &sse_reasoning_done("rs_1", "private-1")
                + &sse_reasoning("Verifying")
                + &sse_reasoning_done("rs_2", "private-2")
                + &sse_delta("final")
                + &sse_completed_output(vec![
                    serde_json::json!({"type":"reasoning","id":"rs_1","encrypted_content":"private-1","status":"completed"}),
                    serde_json::json!({"type":"message","role":"assistant","id":"m1","status":"completed","content":[{"type":"output_text","text":"between"}]}),
                    serde_json::json!({"type":"reasoning","id":"rs_2","encrypted_content":"private-2","status":"completed"}),
                    serde_json::json!({"type":"message","role":"assistant","id":"m2","status":"completed","content":[{"type":"output_text","text":"final"}]}),
                ]),
            sse_delta("Title") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model":"fixture/m", "provider":{"fixture":{"npm":"@ai-sdk/openai",
            "options":{"baseURL":base,"apiKey":"test-key"}, "models":{"m":{}}}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("two-items-app").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let mut rx = app.subscribe();
    let turn = app.submit(session.clone(), "first".into()).await.unwrap();
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let finished = matches!(&event, CoreEvent::TurnFinished { turn: id, .. } if id == &turn);
        match event {
            CoreEvent::ReasoningDelta {
                turn: id, delta, ..
            } if id == turn => events.push(delta),
            CoreEvent::TextDelta {
                turn: id, delta, ..
            } if id == turn => events.push(format!("text:{delta}")),
            CoreEvent::ReasoningItemEnded { turn: id, .. } if id == turn => {
                events.push("<ended>".into())
            }
            CoreEvent::TurnFailed { error, .. } => panic!("turn failed: {error}"),
            _ => {}
        }
        if finished {
            break;
        }
    }
    assert_eq!(
        events,
        [
            "Inspecting",
            "<ended>",
            "Verifying",
            "<ended>",
            "text:final"
        ]
    );
    let page = app
        .history_page(session.clone(), None, None, 10)
        .await
        .unwrap();
    let assistant = page
        .rows
        .iter()
        .find(|message| message.turn.as_ref().is_some_and(|t| t.id == turn.0))
        .unwrap();
    let parts = &assistant.turn.as_ref().unwrap().parts;
    assert!(
        matches!(&parts[..], [TranscriptPart::Reasoning { text: first, .. }, TranscriptPart::Text(between), TranscriptPart::Reasoning { text: second, .. }, TranscriptPart::Text(answer)]
        if first == "Inspecting" && between == "between" && second == "Verifying" && answer == "final")
    );
    assert_eq!(parts.len(), 4);
    assert_eq!(assistant.text, "betweenfinal");
    assert!(!format!("{page:?}").contains("private-"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(
        project.path(),
        data.path(),
        BTreeMap::from([
            ("HOME".into(), home.path().to_string_lossy().to_string()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let replay = app.history_page(session, None, None, 10).await.unwrap();
    assert_eq!(
        replay
            .rows
            .iter()
            .find(|row| row.turn.as_ref().is_some_and(|t| t.id == turn.0))
            .unwrap()
            .turn
            .as_ref()
            .unwrap()
            .parts,
        *parts
    );
    assert!(!format!("{replay:?}").contains("private-"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
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
    let mut checkpoints = Vec::new();
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
            CoreEvent::TurnPresentation { projection, .. } => checkpoints.push(projection),
            CoreEvent::TurnStarted { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ReasoningDelta { .. }
            | CoreEvent::ReasoningItemEnded { .. }
            | CoreEvent::TurnUsage { .. }
            | CoreEvent::TurnInterrupted { .. } => {}
        }
    }
    let (started_op, started_name, started_input) = started.expect("tool call started event");
    assert!(
        checkpoints
            .iter()
            .any(|p| p.part_states.iter().any(|s| s.status == "started"))
    );
    let page = app
        .history_page(session.clone(), None, None, 100)
        .await
        .unwrap();
    let replay = page.rows.iter().find_map(|r| r.turn.as_ref()).unwrap();
    assert_eq!(
        checkpoints.last(),
        Some(replay),
        "live checkpoint and replay share exact identities, order, statuses and content"
    );
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

#[tokio::test]
async fn application_fresh_turn_validates_and_atomically_pins_home_choice() {
    use oc_adapters::application;
    use oc_core::core_app::{CoreEvent, FreshSelection};
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction;
    use oc_core::session::CoreError;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(100),
    );
    let config = serde_json::json!({
        "model":"fixture/main", "default_agent":"build",
        "provider":{"fixture":{
            "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
            "models":{
                "main":{"limit":{"context":65536,"output":4096},
                    "variants":{"low":{"reasoningEffort":"low"}}},
                "other":{"limit":{"context":65536,"output":4096},
                    "variants":{"deep":{"reasoningEffort":"high"}}}
            }
        }},
        "agent":{
            "build":{"mode":"primary","prompt":"BUILD_PRIMARY"},
            "review":{"mode":"primary","prompt":"REVIEW_PRIMARY"},
            "helper":{"mode":"subagent","prompt":"HELPER_CHILD"}
        }
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let sid = |name| SessionId::new(name).unwrap();
    let chosen = |agent: &str, model: &str, variant: Option<&str>| FreshSelection {
        agent_id: Some(agent.into()),
        model_id: model.into(),
        variant: variant.map(str::to_string),
    };
    app.select_model("main".into(), Some("low".into()))
        .await
        .unwrap();
    for (id, text, choice) in [
        ("empty", " \n ", None),
        ("bad-agent", "prompt", Some(chosen("helper", "main", None))),
        (
            "bad-variant",
            "prompt",
            Some(chosen("review", "main", Some("missing"))),
        ),
        (
            "bad-model",
            "prompt",
            Some(chosen("review", "missing", None)),
        ),
    ] {
        assert!(
            app.submit_fresh(sid(id), text.into(), choice)
                .await
                .is_err(),
            "{id} must be refused"
        );
        assert!(app.read_history(sid(id)).await.is_err());
    }
    assert!(app.list_sessions().await.unwrap().is_empty());
    assert!(
        requests.lock().unwrap().is_empty(),
        "refusal never calls provider"
    );

    let mut events = app.subscribe();
    let implicit = sid("fresh-implicit");
    app.submit_fresh(implicit.clone(), "first".into(), None)
        .await
        .expect("durable first turn");
    assert_eq!(
        app.submit_fresh(sid("fresh-busy"), "busy".into(), None)
            .await,
        Err(CoreError::TurnBusy)
    );
    assert!(app.read_history(sid("fresh-busy")).await.is_err());
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == implicit => break,
            CoreEvent::TurnFailed { error, .. } => panic!("implicit turn failed: {error}"),
            _ => {}
        }
    }
    assert_eq!(
        app.session_selection(implicit.clone(), false, SessionSelectionAction::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("low")
    );

    let explicit = sid("fresh-explicit");
    app.submit_fresh(
        explicit.clone(),
        "second".into(),
        Some(chosen("review", "other", Some("deep"))),
    )
    .await
    .expect("explicit first turn");
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == explicit => break,
            CoreEvent::TurnFailed { error, .. } => panic!("explicit turn failed: {error}"),
            _ => {}
        }
    }
    let actual = app
        .session_selection(explicit.clone(), false, SessionSelectionAction::Current)
        .await
        .unwrap();
    assert_eq!(actual.agent_id.as_deref(), Some("review"));
    assert_eq!(actual.model_id, "other");
    assert_eq!(actual.variant.as_deref(), Some("deep"));
    assert_eq!(
        app.read_history(explicit.clone()).await.unwrap()[0].text,
        "second"
    );
    assert_eq!(
        app.read_history(implicit.clone()).await.unwrap()[0].text,
        "first"
    );
    assert_eq!(
        app.submit_fresh(explicit.clone(), "again".into(), None)
            .await,
        Err(CoreError::SessionAlreadyExists)
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let db = Db::open(data.path()).unwrap();
    let location = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for id in ["fresh-implicit", "fresh-explicit"] {
        assert_eq!(
            db.get_pref(&format!("{SESSION_LOCATION_PREFIX}{id}"))
                .unwrap()
                .as_deref(),
            Some(location.as_str())
        );
    }
    let key = |id: &str| {
        format!(
            "tui.selection.session:{}",
            serde_json::json!([location, "fixture", id])
        )
    };
    let implicit_record: serde_json::Value = serde_json::from_str(
        &db.get_pref(&key("fresh-implicit"))
            .unwrap()
            .expect("implicit Home choice pinned with first turn"),
    )
    .unwrap();
    assert_eq!(implicit_record["agent"], "build");
    assert_eq!(implicit_record["models"]["build"]["id"], "main");
    assert_eq!(implicit_record["models"]["build"]["variant"], "low");
    let record: serde_json::Value = serde_json::from_str(
        &db.get_pref(&key("fresh-explicit"))
            .unwrap()
            .expect("atomic selection"),
    )
    .unwrap();
    assert_eq!(record["agent"], "review");
    assert_eq!(record["models"]["review"]["id"], "other");
    assert_eq!(record["models"]["review"]["variant"], "deep");
    assert_eq!(record["epoch"], 1);
    let captured = requests.lock().unwrap();
    // The title provider may add a request after each turn; locate the turn
    // requests by their user input instead of relying on title call ordering.
    assert!(captured.iter().any(|r| r["model"] == "main"
        && r["input"].to_string().contains("first")
        && r["input"].to_string().contains("BUILD_PRIMARY")));
    assert!(captured.iter().any(|r| r["model"] == "other"
        && r["input"].to_string().contains("second")
        && r["input"].to_string().contains("REVIEW_PRIMARY")));
}

#[tokio::test]
async fn application_home_actions_are_sessionless_and_fresh_turn_pins_current_location_choice() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;
    use oc_core::session::CoreError;

    let project = tempfile::tempdir().unwrap();
    let other_location = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user_home = tempfile::tempdir().unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(200),
    );
    let config = |model: &str| {
        serde_json::json!({
            "model":format!("fixture/{model}"), "default_agent":"build",
            "provider":{"fixture":{
                "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
                "models":{
                    "main":{"limit":{"context":65536,"output":4096},
                        "variants":{"low":{"reasoningEffort":"low"}}},
                    "other":{"limit":{"context":65536,"output":4096},
                        "variants":{"deep":{"reasoningEffort":"high"}}}
                }
            }},
            "agent":{
                "build":{"mode":"primary","prompt":"BUILD_PRIMARY"},
                "review":{"mode":"primary","prompt":"REVIEW_PRIMARY"},
                "helper":{"mode":"subagent"}
            }
        })
    };
    std::fs::write(
        project.path().join("opencode.json"),
        config("main").to_string(),
    )
    .unwrap();
    std::fs::write(
        other_location.path().join("opencode.json"),
        config("other").to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        (
            "HOME".into(),
            user_home.path().to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let sid = SessionId::new("home-chosen").unwrap();
    let location = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let other = other_location
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let current = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(current.chrome.location.as_deref(), Some(location.as_str()));
    assert_eq!(current.model_id, "main");
    assert_eq!(current.agent_id.as_deref(), Some("build"));

    let selected = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(
        (selected.model_id.as_str(), selected.variant.as_deref()),
        ("other", None)
    );
    assert_eq!(
        app.home_selection(Action::Variant(Some("deep".into())))
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("deep")
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!(
        (review.model_id.as_str(), review.agent_id.as_deref()),
        ("main", Some("review"))
    );
    let review = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(review.variant.as_deref(), Some("deep"));
    let review = app.home_selection(Action::Variant(None)).await.unwrap();
    assert_eq!(review.variant, None);
    let build = app
        .home_selection(Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(
        (build.model_id.as_str(), build.variant.as_deref()),
        ("other", Some("deep"))
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!((review.model_id.as_str(), review.variant), ("other", None));
    assert_eq!(
        app.home_selection(Action::New(Some("review".into())))
            .await
            .unwrap()
            .model_id,
        "other"
    );
    let reset = app.home_selection(Action::New(None)).await.unwrap();
    assert_eq!(
        (
            reset.agent_id.as_deref(),
            reset.model_id.as_str(),
            reset.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    let before = app.home_selection(Action::Current).await.unwrap();
    for rejected in [
        Action::Model("retired".into()),
        Action::Variant(Some("missing".into())),
        Action::Agent("helper".into()),
    ] {
        assert!(app.home_selection(rejected).await.is_err());
        assert_eq!(app.home_selection(Action::Current).await.unwrap(), before);
    }
    assert!(
        app.list_sessions().await.unwrap().is_empty(),
        "Home-only actions must not create a root"
    );
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    let session_prefs: i64 = conn
        .query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(session_prefs, 0, "Home has no phantom session preference");
    assert_eq!(
        app.select_model("main".into(), Some("low".into()))
            .await
            .unwrap()
            .model_id,
        "main"
    );

    let mut events = app.subscribe();
    app.submit_fresh(sid.clone(), "chosen Home".into(), None)
        .await
        .expect("durable first turn");
    assert_eq!(
        app.home_selection(Action::Variant(None)).await,
        Err(CoreError::TurnBusy)
    );
    assert_eq!(
        app.home_selection(Action::Current).await,
        Err(CoreError::TurnBusy)
    );
    let key = format!(
        "tui.selection.session:{}",
        serde_json::json!([location, "fixture", "home-chosen"])
    );
    let pinned: serde_json::Value = serde_json::from_str(
        &conn
            .query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pinned["agent"], "build");
    assert_eq!(pinned["models"]["build"]["id"], "other");
    assert_eq!(pinned["models"]["build"]["variant"], "deep");
    assert_eq!(pinned["epoch"], 1);
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("Home turn failed: {error}"),
            _ => {}
        }
    }
    let actual = app
        .session_selection(sid.clone(), false, Action::Current)
        .await
        .unwrap();
    assert_eq!(
        (actual.model_id.as_str(), actual.variant.as_deref()),
        ("other", Some("deep"))
    );
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other"
                && r["input"].to_string().contains("chosen Home")
                && r["input"].to_string().contains("BUILD_PRIMARY"))
    );

    let switched = app.switch_location(other.clone()).await.unwrap();
    assert_eq!(switched.location, other);
    let baseline = app.list_sessions().await.unwrap().len(); // legacy switch creates its own session
    let second = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (second.model_id.as_str(), second.agent_id.as_deref()),
        ("main", Some("build"))
    );
    let second = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(
        (second.model_id.as_str(), second.variant.as_deref()),
        ("other", None)
    );
    assert_eq!(app.list_sessions().await.unwrap().len(), baseline);
    let home_again = app.switch_location(location.clone()).await.unwrap();
    assert_eq!(home_again.location, location);
    let restored = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            restored.agent_id.as_deref(),
            restored.model_id.as_str(),
            restored.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    assert_eq!(
        app.session_selection(sid, false, Action::Current)
            .await
            .unwrap()
            .model_id,
        "other"
    );
    // A catalog refresh can retire an earlier Home id. Keep the exact visible
    // choice, refuse a re-selection/first turn, and leave all existing rows and
    // preferences untouched until an explicit admitted replacement is chosen.
    let mut reduced = config("main");
    reduced["provider"]["fixture"]["models"]
        .as_object_mut()
        .unwrap()
        .remove("other");
    std::fs::write(project.path().join("opencode.json"), reduced.to_string()).unwrap();
    app.switch_location(other).await.unwrap();
    app.switch_location(location).await.unwrap();
    let retired = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(retired.model_id, "other");
    assert!(!retired.models.iter().any(|model| model.id == "other"));
    let rows_before = app.list_sessions().await.unwrap();
    let pref_before: String = conn
        .query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        app.home_selection(Action::Model("other".into()))
            .await
            .is_err()
    );
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), retired);
    assert!(
        app.submit_fresh(
            SessionId::new("retired-home").unwrap(),
            "refused".into(),
            None
        )
        .await
        .is_err()
    );
    assert_eq!(app.list_sessions().await.unwrap(), rows_before);
    assert_eq!(
        conn.query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        pref_before
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_location_switch_restores_choices_without_roots_and_first_turn_binds_target() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;
    use oc_core::session::{CoreError, LocationSwitchFailure};

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let bad = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(40),
    );
    let config = |default: &str| {
        serde_json::json!({
            "model": format!("fixture/{default}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {"main": {"limit": {"context": 65536, "output": 4096}},
                           "other": {"limit": {"context": 65536, "output": 4096}}}
            }}
        })
    };
    std::fs::write(a.path().join("opencode.json"), config("main").to_string()).unwrap();
    std::fs::write(b.path().join("opencode.json"), config("other").to_string()).unwrap();
    std::fs::write(
        bad.path().join("opencode.json"),
        config("missing").to_string(),
    )
    .unwrap();
    let (app, guard, _) = application::spawn_with_env(
        a.path(),
        data.path(),
        BTreeMap::from([
            ("HOME".into(), home.path().to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let a_path = a
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let b_path = b
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let original = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(original.model_id, "other");
    let error = app
        .switch_location_home(bad.path().display().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            ..
        }
    ));
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), original);
    assert!(app.list_sessions().await.unwrap().is_empty());

    let target = app.switch_location_home(b_path.clone()).await.unwrap();
    assert_eq!(target.location, b_path);
    assert_eq!(target.catalog.model_id, "other");
    assert_eq!(
        target.catalog.chrome.location.as_deref(),
        Some(b_path.as_str())
    );
    assert!(app.list_sessions().await.unwrap().is_empty());
    let target = app.switch_location_home(a_path.clone()).await.unwrap();
    assert_eq!(target.catalog.model_id, "other");
    assert_eq!(
        target.catalog.chrome.location.as_deref(),
        Some(a_path.as_str())
    );
    app.home_selection(Action::Model("main".into()))
        .await
        .unwrap();
    let target = app.switch_location_home(b_path.clone()).await.unwrap();
    assert_eq!(
        target.catalog.model_id, "other",
        "B keeps its own Home choice"
    );
    assert!(app.list_sessions().await.unwrap().is_empty());

    let mut events = app.subscribe();
    let b_id = SessionId::new("fresh-in-b").unwrap();
    app.submit_fresh(b_id.clone(), "first B".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == b_id => break,
            CoreEvent::TurnFailed { error, .. } => panic!("B: {error}"),
            _ => {}
        }
    }
    assert_eq!(app.list_sessions().await.unwrap(), vec![b_id.clone()]);
    assert_eq!(
        app.switch_location_home(a_path.clone())
            .await
            .unwrap()
            .catalog
            .model_id,
        "main"
    );
    let a_id = SessionId::new("fresh-in-a").unwrap();
    app.submit_fresh(a_id.clone(), "first A".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == a_id => break,
            CoreEvent::TurnFailed { error, .. } => panic!("A: {error}"),
            _ => {}
        }
    }
    assert_eq!(app.list_sessions().await.unwrap().len(), 2);
    let db = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    for (id, path) in [(&a_id, &a_path), (&b_id, &b_path)] {
        let bound: String = db
            .query_row(
                "SELECT value FROM prefs WHERE key = ?1",
                [format!(
                    "{}{}",
                    oc_adapters::runtime::SESSION_LOCATION_PREFIX,
                    id.0
                )],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bound, *path);
    }
    {
        let captured = requests.lock().unwrap();
        assert!(
            captured
                .iter()
                .any(|r| r["model"] == "other" && r["input"].to_string().contains("first B"))
        );
        assert!(
            captured
                .iter()
                .any(|r| r["model"] == "main" && r["input"].to_string().contains("first A"))
        );
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_current_reloads_config_after_location_roundtrip_without_explicit_choice() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(30),
    );
    let config = |model: &str| {
        serde_json::json!({
            "model": format!("fixture/{model}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {"main": {}, "other": {}}
            }}
        })
    };
    std::fs::write(a.path().join("opencode.json"), config("main").to_string()).unwrap();
    std::fs::write(b.path().join("opencode.json"), config("main").to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "main"
    );
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    std::fs::write(a.path().join("opencode.json"), config("other").to_string()).unwrap();
    let reloaded = app
        .switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(reloaded.catalog.model_id, "other");
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "other"
    );
    let mut events = app.subscribe();
    let sid = SessionId::new("config-reloaded").unwrap();
    app.submit_fresh(sid.clone(), "reloaded turn".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("reloaded turn: {error}"),
            _ => {}
        }
    }
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other" && r["input"].to_string().contains("reloaded turn"))
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_draft_hydrates_after_restart_and_retirement_requires_explicit_replacement() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(30),
    );
    let config = |retired: bool| {
        let mut config = serde_json::json!({
            "model": "fixture/main", "default_agent": "build",
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {
                    "main": {"variants": {"low": {"reasoningEffort": "low"}}},
                    "other": {"variants": {"deep": {"reasoningEffort": "high"}}}
                }
            }},
            "agent": {
                "build": {"mode": "primary", "prompt": "BUILD_PRIMARY"},
                "review": {"mode": "primary", "prompt": "REVIEW_PRIMARY"}
            }
        });
        if retired {
            config["provider"]["fixture"]["models"]
                .as_object_mut()
                .unwrap()
                .remove("other");
        }
        config
    };
    let path = project.path().join("opencode.json");
    std::fs::write(&path, config(false).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "main"
    );
    app.home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    app.home_selection(Action::Variant(Some("deep".into())))
        .await
        .unwrap();
    app.home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    app.home_selection(Action::Variant(Some("low".into())))
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    let current = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            current.agent_id.as_deref(),
            current.model_id.as_str(),
            current.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    let sid = SessionId::new("restored-home-draft").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(sid.clone(), "restored turn".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("restored turn: {error}"),
            _ => {}
        }
    }
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other"
                && r["input"].to_string().contains("restored turn")
                && r["input"].to_string().contains("BUILD_PRIMARY"))
    );
    assert_eq!(
        app.session_selection(sid, false, Action::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("deep")
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!(
        (
            review.agent_id.as_deref(),
            review.model_id.as_str(),
            review.variant.as_deref()
        ),
        (Some("review"), "main", Some("low"))
    );
    let build = app
        .home_selection(Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(
        (build.model_id.as_str(), build.variant.as_deref()),
        ("other", Some("deep"))
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    std::fs::write(&path, config(true).to_string()).unwrap();
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let retired = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (retired.model_id.as_str(), retired.variant.as_deref()),
        ("other", Some("deep"))
    );
    assert!(!retired.models.iter().any(|m| m.id == "other"));
    assert!(
        app.submit_fresh(
            SessionId::new("retired-draft").unwrap(),
            "refuse".into(),
            None
        )
        .await
        .is_err()
    );
    assert!(
        app.read_history(SessionId::new("retired-draft").unwrap())
            .await
            .is_err()
    );
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), retired);
    assert_eq!(
        app.home_selection(Action::Model("main".into()))
            .await
            .unwrap()
            .model_id,
        "main"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
