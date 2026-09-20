//! T24 (STORE05/TOOL09/DCP08): runtime turn loop against a fake Responses
//! server — completion/drain, unified permission path, MCP fail-fast,
//! Location binding, reload, compress, commands, assembler bounds.

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
use oc_adapters::runtime::{Runtime, TurnParams, TurnStatus, assemble_turn_input, expand_command};
use oc_adapters::storage::Db;
use oc_core::context_plan::ProtectedSpec;

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n".to_string()
}

fn sse_tool_call(item_id: &str, name: &str, args: &serde_json::Value) -> String {
    let added = format!(
        "data: {{\"type\":\"response.output_item.added\",\"item\":{{\"type\":\"function_call\",\"id\":\"{item_id}\",\"name\":\"{name}\"}}}}\n\n"
    );
    let delta = format!(
        "data: {{\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"{item_id}\",\"delta\":{}}}\n\n",
        serde_json::Value::String(args.to_string())
    );
    added + &delta
}

/// Scripted fakes: serve queued SSE bodies in order, then repeat the last.
struct Fake;

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
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let hits = Arc::new(Mutex::new(0usize));
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let worker_queue = queue.clone();
        let worker_hits = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = worker_queue.clone();
                let hits = worker_hits.clone();
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
        (base, hits)
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
        DcpConfig::default(),
    )
    .expect("runtime")
}

fn provider_of(base: &str) -> ResponsesConfig {
    ResponsesConfig {
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
    let patch = "*** Begin Patch\n*** Add File: x.secret\n@@\n+boe\n*** End Patch\n";
    let tool = sse_tool_call("i1", "apply_patch", &serde_json::json!({"patch": patch}));
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
            server: "codex".to_string()
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
            runtime.reload(Generation {
                providers: BTreeMap::new(),
                mcp: BTreeMap::new(),
                permissions: BTreeMap::new(),
                provenance: BTreeMap::new(),
                warnings: Vec::new(),
            })
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
    };
    let args = serde_json::json!({
        "topic": "t",
        "content": [{"startId": ids[0].0, "endId": ids[1].0, "summary": "first"}],
    });
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
    assert!(expand_command(&"x".repeat(5_000), &[]).is_err());
    assert!(
        expand_command("ok $9", &["only".to_string()])
            .expect("partial")
            .contains("$9")
    );
}

#[test]
fn assembler_caps_history_and_keeps_user() {
    let history: Vec<(String, String)> = (0..2_000)
        .map(|i| ("user".to_string(), "x".repeat(100) + &i.to_string()))
        .collect();
    let prompt = assemble_turn_input(&history, "final question", &[]);
    assert!(prompt.len() <= oc_adapters::runtime::INPUT_BYTES_CAP + 8_192);
    assert!(prompt.contains("final question"));
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
