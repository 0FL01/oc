//! T28 (LOAD01–04): fixed synthetic soak over the runtime turn loop.
//!
//! Frozen workload (no time/randomness in what is asserted): 6 sessions ×
//! 10 mixed turns across 2 epochs on one `Db`, periodic compress, one
//! cancel, one loud MCP failure per epoch; output-pressure and blob-quota
//! probes; crash-injection recovery; RSS/DB/WAL measurements with
//! baseline-derived caps. Slow model: scripted SSE fake, zero delay.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::config::{Generation, McpEntry, Permission};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{Runtime, TurnParams, TurnStatus};
use oc_adapters::storage::{Db, StorageError};
use oc_core::context_plan::ProtectedSpec;

// Frozen workload shape: 6 sessions, 10 kinds each.
const SESSIONS: usize = 6;
const KINDS: [&str; 10] = ["T", "R", "B", "P", "W", "T", "W", "R", "P", "B"];

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n"
        .to_string()
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

/// Scripted fake: queued SSE bodies in order, then repeat the last.
struct Fake;

impl Fake {
    fn start(script: Vec<String>) -> (String, Arc<Mutex<usize>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let hits = Arc::new(Mutex::new(0usize));
        let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(script)));
        let hits_out = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = queue.clone();
                let hits = hits.clone();
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
        (base, hits_out)
    }

    /// Headers immediately, body stalled: cancel must land mid-stream.
    fn start_stalled() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
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
                    }
                    let stream = reader.get_mut();
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
                    );
                    let _ = stream.flush();
                    let start = std::time::Instant::now();
                    while start.elapsed() < Duration::from_secs(30) {
                        if stream.write_all(b": hb\n\n").is_err() {
                            return;
                        }
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
    project: tempfile::TempDir,
    data: tempfile::TempDir,
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

fn make_harness() -> (Harness, Generation) {
    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    std::fs::write(project.path().join("big.bin"), "y".repeat(300_000)).expect("big");
    for session in 0..SESSIONS {
        std::fs::write(
            project.path().join(format!("scratch_s{session}.txt")),
            "line0\n",
        )
        .expect("scratch");
    }
    let db = Db::open(data.path()).expect("db");
    // Binary startup applies the DCP schema; the harness mirrors that wiring.
    oc_adapters::dcp::apply_dcp_schema(&db).expect("dcp schema");
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
        permissions: allow_all(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    (
        Harness {
            project,
            data,
            db,
            catalog,
        },
        generation,
    )
}

fn runtime_of<'a>(harness: &'a Harness, generation: Generation) -> Runtime<'a> {
    let project = harness.project.path();
    let files = oc_adapters::files::Files::new(project, harness.data.path()).expect("files");
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
            data: harness.data.path().to_path_buf(),
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

/// One request-level script entry per kind; tool kinds add a follow-up.
/// Patch targets a per-session scratch file so the `line0` preimage always
/// matches exactly in both epochs.
fn script_for(kind: &str, session: usize, n: usize) -> Vec<String> {
    match kind {
        "T" => vec![sse_delta(&"x".repeat(2_048)) + &sse_completed()],
        "W" => vec![sse_delta("ok") + &sse_completed()],
        "R" => vec![
            sse_tool_call("r", "read", &serde_json::json!({"path": "big.bin"})) + &sse_completed(),
            sse_delta("read it") + &sse_completed(),
        ],
        "B" => vec![
            sse_tool_call(
                "b",
                "bash",
                &serde_json::json!({"argv": ["/bin/echo", "soak"]}),
            ) + &sse_completed(),
            sse_delta("ran it") + &sse_completed(),
        ],
        "P" => vec![
            sse_tool_call(
                "p",
                "apply_patch",
                &serde_json::json!({"patchText": format!("*** Begin Patch\n*** Update File: scratch_s{session}.txt\n@@\n line0\n+line{n}\n*** End Patch")}),
            ) + &sse_completed(),
            sse_delta("patched") + &sse_completed(),
        ],
        other => panic!("unknown kind {other}"),
    }
}

fn spec() -> ProtectedSpec {
    ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        file_globs: Vec::new(),
        ..ProtectedSpec::default()
    }
}

/// Current-process RSS and high-water mark in KiB (Linux).
fn rss_kb() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mut rss = 0u64;
    let mut hwm = 0u64;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            hwm = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
        }
    }
    (rss, hwm)
}

/// Peak RSS of this process in KiB (allocator-independent max).
fn peak_rss_kb() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: `usage` is a valid writable `rusage` slot; getrusage with
    // RUSAGE_SELF only writes on success (return 0), which we check.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
        // SAFETY: reached only after a successful getrusage above.
        let usage = unsafe { usage.assume_init() };
        return usage.ru_maxrss as u64;
    }
    0
}

/// SQLite bytes on disk (main + wal + shm).
fn db_bytes(data: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(dir) = std::fs::read_dir(data) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".sqlite")
                || name.ends_with(".sqlite-wal")
                || name.ends_with(".sqlite-shm")
            {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

fn metric(name: &str, value: u64) {
    eprintln!("SOAK_METRIC {name}={value}");
}

struct EpochStats {
    turns: usize,
    tool_calls: usize,
    db_delta: u64,
    rows_added: usize,
    peak_rss: u64,
}

/// One frozen epoch: round-robin turns, compress every 12 turns, one
/// cancel, one loud MCP failure. Returns measured stats.
async fn run_epoch(harness: &Harness, runtime: &Runtime<'_>, epoch: usize) -> EpochStats {
    static CANCEL: AtomicBool = AtomicBool::new(false);
    let mut script = Vec::new();
    for session in 0..SESSIONS {
        for kind in KINDS {
            script.extend(script_for(kind, session, epoch * 100 + session));
        }
    }
    script.push(sse_delta("steady") + &sse_completed());
    let (base, _) = Fake::start(script);
    let provider = provider_of(&base);
    let mut turns = 0usize;
    let mut tool_calls = 0usize;
    let db_before = db_bytes(harness.data.path());
    let rows_before: usize = (0..SESSIONS)
        .map(|s| harness.db.history_len(&format!("s{s}")).unwrap_or(0))
        .sum();
    for session in 0..SESSIONS {
        let name = format!("s{session}");
        if epoch == 0 {
            runtime.create_session(&name).expect("create");
        } else {
            runtime.open_session(&name).expect("reopen");
        }
        for (i, kind) in KINDS.iter().enumerate() {
            let report = runtime
                .run_turn(params(
                    &name,
                    &format!("{kind} prompt"),
                    harness,
                    provider.clone(),
                    &CANCEL,
                ))
                .await
                .expect("turn");
            assert_eq!(
                report.status,
                TurnStatus::Completed,
                "epoch {epoch} {name} turn {i}"
            );
            turns += 1;
            tool_calls += report.calls.len();
            // Session switch every turn is implicit in round-robin; compress
            // every 12th global turn on the current session.
            let global = epoch * SESSIONS * KINDS.len() + session * KINDS.len() + i;
            if global % 12 == 11 {
                let ids = harness.db.read_history_full(&name).expect("ids");
                if ids.len() >= 4 {
                    // Recompression starts at the effective projection anchor,
                    // not at a raw member already owned by an active block.
                    let start = harness
                        .db
                        .load_compression_blocks(&name)
                        .expect("blocks")
                        .into_iter()
                        .find(|block| !block.members.is_empty())
                        .map(|block| block.id)
                        .unwrap_or_else(|| ids[0].0.clone());
                    let args = serde_json::json!({
                        "topic": format!("soak e{epoch}"),
                        "content": [{
                            "startId": start, "endId": ids[ids.len() - 2].0,
                            "summary": "soak range",
                        }],
                    });
                    let compress = runtime
                        .run_compress(&name, &args, &spec())
                        .expect("compress");
                    assert!(!compress.blocks.is_empty());
                }
            }
        }
    }
    // One cancel against a stalled stream.
    let stalled = Fake::start_stalled();
    let flag = AtomicBool::new(false);
    let (accepted, ready) = tokio::sync::oneshot::channel();
    let mut accepted = Some(accepted);
    let (cancelled, ()) = tokio::join!(
        runtime.run_turn_with_events(
            params("s0", "stall", harness, provider_of(&stalled), &flag),
            |_| {
                let _ = accepted.take().expect("one acceptance").send(());
            },
            |_, _| {},
            |_, _| {},
        ),
        async {
            ready.await.expect("accepted before cancellation");
            tokio::time::sleep(Duration::from_millis(300)).await;
            flag.store(true, Ordering::Relaxed);
        }
    );
    let cancelled = cancelled.expect("cancel lands");
    assert_eq!(cancelled.status, TurnStatus::Cancelled);
    // One loud MCP failure: no silent degradation, no partial turn.
    let bad = Generation {
        providers: BTreeMap::new(),
        mcp: [(
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
        )]
        .into_iter()
        .collect(),
        permissions: allow_all(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    let bad_runtime = runtime_of(harness, bad);
    let mcp_error = bad_runtime
        .run_turn(params("s0", "hi", harness, provider.clone(), &CANCEL))
        .await
        .expect_err("mcp must fail loud");
    assert_eq!(
        mcp_error,
        oc_adapters::runtime::RuntimeError::McpAttach {
            server: "codex".to_string(),
            stage: "DNS",
            safe_code: "private_host",
            retryable: false,
        }
    );
    let rows_after: usize = (0..SESSIONS)
        .map(|s| harness.db.history_len(&format!("s{s}")).unwrap_or(0))
        .sum();
    EpochStats {
        turns,
        tool_calls,
        db_delta: db_bytes(harness.data.path()).saturating_sub(db_before),
        rows_added: rows_after.saturating_sub(rows_before),
        peak_rss: peak_rss_kb(),
    }
}

#[tokio::test]
async fn soak_two_epochs_bounded_growth() {
    let (rss_base, _) = rss_kb();
    let (harness, generation) = make_harness();
    let db_base = db_bytes(harness.data.path());
    metric("rss_baseline_kb", rss_base);
    metric("db_baseline_bytes", db_base);
    let runtime = runtime_of(&harness, generation);
    let first = run_epoch(&harness, &runtime, 0).await;
    let (rss_mid, _) = rss_kb();
    metric("epoch1_db_delta_bytes", first.db_delta);
    metric("epoch1_rows_added", first.rows_added as u64);
    metric("epoch1_tool_calls", first.tool_calls as u64);
    metric("rss_after_epoch1_kb", rss_mid);
    assert_eq!(first.turns, SESSIONS * KINDS.len());
    assert!(first.tool_calls > 0, "tool rounds exercised");

    let second = run_epoch(&harness, &runtime, 1).await;
    let (rss_end, hwm_end) = rss_kb();
    metric("epoch2_db_delta_bytes", second.db_delta);
    metric("epoch2_rows_added", second.rows_added as u64);
    metric("rss_end_kb", rss_end);
    metric("rss_hwm_kb", hwm_end);
    metric("peak_rss_kb", second.peak_rss.max(first.peak_rss));
    // Append-only log grows per epoch, but the retained slope must not
    // steepen: identical frozen workload, generous 2x for page lumpiness.
    assert_eq!(
        second.rows_added, first.rows_added,
        "same workload, same rows"
    );
    assert!(
        second.db_delta <= first.db_delta.saturating_mul(2).max(65536),
        "db growth must not steepen: {} vs {}",
        second.db_delta,
        first.db_delta
    );
    // Absolute RSS bound from the host baseline (test env margin 512 MiB).
    assert!(
        rss_end <= rss_base.saturating_add(512 * 1024),
        "rss bounded: {rss_end} vs baseline {rss_base}"
    );
    // Projected context after compress stays capped: covered members
    // collapse instead of accumulating in the active window.
    for session in 0..SESSIONS {
        let name = format!("s{session}");
        let full = harness.db.read_history_full(&name).expect("full");
        let blocks = oc_adapters::dcp::load_blocks(&harness.db, &name).expect("blocks");
        let prune = harness.db.load_prune_mark(&name).expect("prune");
        let projected = oc_adapters::dcp::project_history(&full, &blocks, prune.as_deref());
        let bytes: usize = projected.iter().map(|(_, text)| text.len()).sum();
        assert!(
            bytes <= 64 * 1024,
            "projected context bounded for {name}: {bytes}"
        );
    }
    drop(runtime);
}

#[tokio::test]
async fn output_pressure_stays_capped_single_transcript() {
    let (harness, generation) = make_harness();
    let runtime = runtime_of(&harness, generation);
    runtime.create_session("press").expect("create");
    // 500 × 1 KiB deltas + a 300 KiB read + a big shell listing.
    let giant = sse_delta(&"g".repeat(1_024)).repeat(500) + &sse_completed();
    let read_big =
        sse_tool_call("r", "read", &serde_json::json!({"path": "big.bin"})) + &sse_completed();
    let bash_big = sse_tool_call(
        "b",
        "bash",
        &serde_json::json!({"argv": ["/bin/echo", &"h".repeat(10_000)]}),
    ) + &sse_completed();
    let (base, _) = Fake::start(vec![
        giant,
        read_big,
        sse_delta("saw it") + &sse_completed(),
        bash_big,
        sse_delta("ran") + &sse_completed(),
        sse_delta("steady") + &sse_completed(),
    ]);
    static CANCEL: AtomicBool = AtomicBool::new(false);
    let mut report_cap = 0usize;
    for (i, prompt) in ["giant", "readbig", "bashbig"].iter().enumerate() {
        let report = runtime
            .run_turn(params(
                "press",
                prompt,
                &harness,
                provider_of(&base),
                &CANCEL,
            ))
            .await
            .expect("pressure turn");
        assert_eq!(report.status, TurnStatus::Completed, "turn {i}");
        // Report/prior carryover is capped even when the durable log keeps
        // the full output: bounded context, unbounded archive.
        for call in &report.calls {
            report_cap = report_cap.max(call.output.len());
        }
    }
    assert!(
        report_cap <= 2_048 + 32,
        "report outputs capped: {report_cap}"
    );
    // No hidden second transcript: exactly one assistant message per
    // completed turn, roles alternate user/assistant from the single prompt
    // each (3 user + 3 assistant).
    let full = harness.db.read_history_full("press").expect("full");
    let assistants = full
        .iter()
        .filter(|(_, role, _)| role == "assistant")
        .count();
    assert_eq!(assistants, 3, "one transcript, three turns");
    assert_eq!(full.len(), 6);
    // Durable ops exist with full outputs (the archive is unbounded);
    // the bound lives on report/prior carryover, asserted above.
    let ops = harness.db.list_tool_ops("press").expect("ops");
    assert!(!ops.is_empty());
    drop(runtime);
}

#[tokio::test]
async fn blob_quota_failure_is_visible_and_recoverable() {
    let data = tempfile::tempdir().expect("data");
    let db = Db::open_with_quota(&data.path().join("root"), 1024).expect("open");
    let digest = db.write_blob(&vec![7u8; 512]).expect("first blob fits");
    assert!(!digest.is_empty());
    let error = db.write_blob(&vec![8u8; 1024]).expect_err("over quota");
    assert!(matches!(error, StorageError::StorageFull));
    // The store stays usable after a quota refusal.
    let digest2 = db.write_blob(&vec![7u8; 512]).expect("dedup still works");
    assert_eq!(digest, digest2);
}

#[tokio::test]
async fn crash_injection_recovers_and_restarts() {
    let (harness, generation) = make_harness();
    let runtime = runtime_of(&harness, generation.clone());
    runtime.create_session("crash").expect("create");
    // Crash mid-turn: the stream stalls on heartbeats before any tool
    // outcome, and a watchdog timeout drops the future mid-flight (no
    // commit, no cleanup). Durable intent (begin_turn, user message) stays.
    let stalled = Fake::start_stalled();
    static CANCEL: AtomicBool = AtomicBool::new(false);
    let crashed = tokio::time::timeout(
        Duration::from_millis(500),
        runtime.run_turn(params(
            "crash",
            "stall",
            &harness,
            provider_of(&stalled),
            &CANCEL,
        )),
    )
    .await;
    assert!(crashed.is_err(), "watchdog must drop the stalled turn");
    drop(runtime);
    // Recovery marks interrupted tool rows unknown without replaying them;
    // history stays intact; a fresh runtime restarts cleanly on the same Db.
    let recovered = harness.db.recover_interrupted_tools().expect("recover");
    let full = harness.db.read_history_full("crash").expect("full");
    assert!(
        full.iter().any(|(_, role, _)| role == "user"),
        "user msg kept"
    );
    let runtime2 = runtime_of(&harness, generation);
    runtime2.open_session("crash").expect("reopen after crash");
    let (base, _) = Fake::start(vec![sse_delta("back") + &sse_completed()]);
    let report = runtime2
        .run_turn(params(
            "crash",
            "resume",
            &harness,
            provider_of(&base),
            &CANCEL,
        ))
        .await
        .expect("turn after restart");
    assert_eq!(report.status, TurnStatus::Completed);
    metric("recovered_tool_rows", recovered as u64);
    drop(runtime2);
}
