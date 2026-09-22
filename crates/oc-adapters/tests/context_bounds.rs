//! T40 (AUD32–AUD34): active context bounds with a growing archive.
//!
//! Frozen workload, fixed before the run (no time/randomness in what is
//! asserted):
//!
//! * `ARCHIVE_PAIRS` archived user/assistant pairs of `ARCHIVE_BYTES` each,
//!   all cut off by a prune mark (they are not part of the active context);
//! * `ACTIVE_PAIRS` live pairs of `ACTIVE_BYTES` each (identical in both
//!   runs — the "equal active context" side of AUD32);
//! * every archived pair also has a durable turn log carrying the same text,
//!   so a full-history read costs both the message copy and the log copy.
//!
//! The small run seeds `SMALL_PAIRS`, the large run `LARGE_PAIRS`; the
//! assertion is that the per-turn construction peak does not follow the
//! archive size.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::config::{Generation, Permission};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{Runtime, TurnParams, TurnStatus};
use oc_adapters::storage::Db;
use oc_adapters::tools::TurnLog;

/// Archived pairs in the large run.
const LARGE_PAIRS: usize = 2_000;
/// Archived pairs in the small run.
const SMALL_PAIRS: usize = 8;
/// Bytes per archived message (user and assistant each).
const ARCHIVE_BYTES: usize = 16 * 1024;
/// Live pairs after the prune mark (equal in both runs).
const ACTIVE_PAIRS: usize = 2;
/// Bytes per live message.
const ACTIVE_BYTES: usize = 1_024;
/// Bound on the per-turn construction peak delta: a full-history read of the
/// large archive cannot fit (64 MiB of texts plus turn logs), while the
/// active projection is a few KiB.
const PEAK_DELTA_BOUND: usize = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Allocation accounting: peak live bytes during one measured closure.
// ---------------------------------------------------------------------------

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: forwards every call to the system allocator, only counting sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: layout comes from the caller of this allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: layout comes from the caller of this allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: the pointer came from this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout come from this allocator.
        let out = unsafe { System.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            let delta = new_size as isize - layout.size() as isize;
            let live = if delta >= 0 {
                LIVE.fetch_add(delta as usize, Ordering::Relaxed) + delta as usize
            } else {
                LIVE.fetch_sub((-delta) as usize, Ordering::Relaxed) - (-delta) as usize
            };
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        out
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Run `body` and report how far the live allocation peak rose above the
/// level at entry. Concurrent helper threads add a few KiB of noise.
fn peak_delta<T>(body: impl FnOnce() -> T) -> (T, usize) {
    let entry = LIVE.load(Ordering::Relaxed);
    PEAK.store(entry, Ordering::Relaxed);
    let value = body();
    let peak = PEAK.load(Ordering::Relaxed);
    (value, peak.saturating_sub(entry))
}

// ---------------------------------------------------------------------------
// Scripted peer that records request bodies.
// ---------------------------------------------------------------------------

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

/// Serve `script` in order (last entry repeats) and keep every request body.
struct Capture;

impl Capture {
    fn start(script: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let bodies_out = bodies.clone();
        let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(script)));
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let bodies = bodies.clone();
                let queue = queue.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {}
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
                    if content_length > 0 && reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    bodies
                        .lock()
                        .expect("bodies")
                        .push(String::from_utf8_lossy(&body).to_string());
                    let payload = {
                        let mut queue = queue.lock().expect("script");
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
        (base, bodies_out)
    }
}

// ---------------------------------------------------------------------------
// Seeded archive.
// ---------------------------------------------------------------------------

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

/// Seed `pairs` archived pairs plus `ACTIVE_PAIRS` live pairs in one bulk
/// transaction (the archive is data, not a path under test).
///
/// Every archived pair gets a durable turn log carrying the same text, so a
/// full-history read must copy both the message rows and the log rows.
fn seed_archive(data: &std::path::Path, session: &str, pairs: usize) {
    let db_path = data.join("oc.sqlite");
    let conn = rusqlite::Connection::open(&db_path).expect("open seed");
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("wal");
    let tx = conn.unchecked_transaction().expect("tx");
    tx.execute(
        "INSERT OR IGNORE INTO sessions(id, created_at) VALUES (?1, '2026-01-01T00:00:00Z')",
        [session],
    )
    .expect("session");
    let mut seq = 0i64;
    fn next_id(seq: &mut i64) -> String {
        *seq += 1;
        format!("m{:04}", *seq)
    }
    for pair in 0..pairs {
        let user_id = next_id(&mut seq);
        let body = format!("archive {pair} {}", "a".repeat(ARCHIVE_BYTES));
        tx.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'user', ?4)",
            rusqlite::params![user_id, session, seq, body],
        )
        .expect("archive user");
        let mut log = TurnLog::new(&format!("ta{pair}"), "m", "test");
        log.user_message = Some(user_id.clone());
        log.input.push(oc_adapters::provider::InputItem::message(
            oc_adapters::provider::InputRole::User,
            &body,
        ));
        tx.execute(
            "INSERT INTO turns(id, session_id, status, prompt, result) VALUES (?1, ?2, 'completed', ?3, ?4)",
            rusqlite::params![
                format!("ta{pair}"),
                session,
                format!("archive {pair}"),
                log.to_json().to_string()
            ],
        )
        .expect("archive turn");
        let assistant_id = next_id(&mut seq);
        tx.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'assistant', ?4)",
            rusqlite::params![assistant_id, session, seq, body],
        )
        .expect("archive assistant");
    }
    // Live pairs after the prune mark: identical in both runs.
    for pair in 0..ACTIVE_PAIRS {
        let user_id = next_id(&mut seq);
        let body = format!("active {pair} {}", "b".repeat(ACTIVE_BYTES));
        tx.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'user', ?4)",
            rusqlite::params![user_id, session, seq, body],
        )
        .expect("active user");
        let mut log = TurnLog::new(&format!("tl{pair}"), "m", "test");
        log.user_message = Some(user_id.clone());
        log.input.push(oc_adapters::provider::InputItem::message(
            oc_adapters::provider::InputRole::User,
            &body,
        ));
        tx.execute(
            "INSERT INTO turns(id, session_id, status, prompt, result) VALUES (?1, ?2, 'completed', ?3, ?4)",
            rusqlite::params![
                format!("tl{pair}"),
                session,
                format!("active {pair}"),
                log.to_json().to_string()
            ],
        )
        .expect("active turn");
        let assistant_id = next_id(&mut seq);
        tx.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'assistant', ?4)",
            rusqlite::params![assistant_id, session, seq, format!("ok {pair}")],
        )
        .expect("active assistant");
    }
    // The prune mark cuts everything through the last archived pair.
    let prune = tx
        .query_row(
            "SELECT id FROM messages WHERE session_id = ?1 AND role = 'assistant' ORDER BY seq DESC LIMIT 1 OFFSET ?2",
            rusqlite::params![session, ACTIVE_PAIRS as i64],
            |row| row.get::<_, String>(0),
        )
        .expect("prune anchor");
    tx.execute(
        "INSERT INTO prune_marks(session_id, up_to_msg, created_at) VALUES (?1, ?2, '2026-01-01T00:00:00Z')",
        rusqlite::params![session, prune],
    )
    .expect("prune mark");
    tx.commit().expect("seed commit");
    drop(conn);
}

fn make_harness(data: tempfile::TempDir, project: tempfile::TempDir) -> Harness {
    let db = Db::open(data.path()).expect("db");
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
    Harness {
        project,
        data,
        db,
        catalog,
    }
}

fn generation() -> Generation {
    Generation {
        providers: BTreeMap::new(),
        mcp: BTreeMap::new(),
        permissions: allow_all(),
        permission_rules: Default::default(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    }
}

fn runtime_of<'a>(harness: &'a Harness) -> Runtime<'a> {
    let project = harness.project.path();
    let files = oc_adapters::files::Files::new(project, harness.data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(project).expect("shell");
    Runtime::new(
        &harness.db,
        "work",
        generation(),
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
        max_rounds: 2,
    }
}

/// Request body without volatile parts: the DCP anchor lane lists message
/// ids (they differ with the archive) and the cache key hashes the body.
fn normalized(body: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(body).expect("request json");
    if let Some(object) = value.as_object_mut() {
        object.remove("prompt_cache_key");
        if let Some(items) = object.get_mut("input").and_then(|v| v.as_array_mut()) {
            items.retain(|item| {
                !item
                    .pointer("/content/0/text")
                    .and_then(|v| v.as_str())
                    .is_some_and(|text| text.starts_with("DCP context anchors"))
            });
        }
    }
    serde_json::to_string(&value).expect("normalized json")
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

/// Seed `pairs`, run one live turn and report (peak delta, request body).
fn measure_turn(pairs: usize) -> (usize, String) {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    // Schema and location binding first: the archive is seeded through a
    // second connection afterwards (the runtime under test is untouched).
    let harness = make_harness(data, project);
    let runtime = runtime_of(&harness);
    runtime.create_session("work").expect("session");
    seed_archive(harness.data.path(), "work", pairs);
    let (base, bodies) = Capture::start(vec![sse_delta("done") + &sse_completed()]);
    let cancel = AtomicBool::new(false);
    let mut accepted = Vec::new();
    let (result, peak) = peak_delta(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(runtime.run_turn_with_events(
                params("work", "live prompt", &harness, provider_of(&base), &cancel),
                |turn| accepted.push(turn.to_string()),
                |_, _| {},
                |_, _| {},
            ))
    });
    let report = result.expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let body = bodies
        .lock()
        .expect("bodies")
        .first()
        .cloned()
        .unwrap_or_default();
    (peak, body)
}

#[test]
fn aud32_equal_active_context_bounded_construction() {
    let (small_peak, small_body) = measure_turn(SMALL_PAIRS);
    let (large_peak, large_body) = measure_turn(LARGE_PAIRS);
    println!(
        "AUD32 workload: archive pairs {SMALL_PAIRS} -> {LARGE_PAIRS} x {ARCHIVE_BYTES} B, \
         active pairs {ACTIVE_PAIRS} x {ACTIVE_BYTES} B; construction peak \
         small={small_peak} large={large_peak} bytes (bound {PEAK_DELTA_BOUND})"
    );
    // Equal active context: the request payload must not change with the
    // archive size (same live messages, same live turn logs).
    assert_eq!(
        normalized(&small_body),
        normalized(&large_body),
        "active context must be identical for both archive sizes"
    );
    assert!(
        large_body.contains("active 1"),
        "live history must reach the provider"
    );
    assert!(
        large_peak < PEAK_DELTA_BOUND,
        "turn construction peak followed the archive: {large_peak} bytes for {LARGE_PAIRS} archived pairs (small run: {small_peak} bytes)"
    );
}

#[tokio::test]
async fn aud33_payload_above_64k_within_budget_is_complete() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    let harness = make_harness(data, project);
    let runtime = runtime_of(&harness);
    runtime.create_session("work").expect("session");
    // Two live messages above 64 KiB each: inside the configured model
    // budget (1M tokens), so they must be sent verbatim.
    let big = format!("{}tail-marker-33", "c".repeat(96 * 1024));
    harness
        .db
        .append_message("work", "user", &big)
        .expect("user");
    harness
        .db
        .append_message("work", "assistant", "ack")
        .expect("assistant");
    harness
        .db
        .append_message("work", "user", &big)
        .expect("user2");
    harness
        .db
        .append_message("work", "assistant", "ack")
        .expect("assistant2");
    let (base, bodies) = Capture::start(vec![sse_delta("done") + &sse_completed()]);
    let cancel = AtomicBool::new(false);
    let prompt = format!("{}prompt-tail-marker-33", "d".repeat(96 * 1024));
    let report = runtime
        .run_turn(params(
            "work",
            &prompt,
            &harness,
            provider_of(&base),
            &cancel,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let body = bodies
        .lock()
        .expect("bodies")
        .first()
        .cloned()
        .unwrap_or_default();
    assert!(
        body.contains("prompt-tail-marker-33"),
        "the current input above 64 KiB must be sent whole"
    );
    assert!(
        body.contains("tail-marker-33"),
        "prior history above 64 KiB must be sent whole, not truncated"
    );
    let persisted = harness
        .db
        .read_history("work")
        .expect("history")
        .into_iter()
        .filter(|(role, _)| role == "user")
        .collect::<Vec<_>>();
    assert!(
        persisted.iter().any(|(_, text)| text.len() > 96 * 1024),
        "the durable user message must keep every byte"
    );
}

#[tokio::test]
async fn aud34_tool_output_beyond_preview_is_retrievable() {
    let data = tempfile::tempdir().expect("data");
    let project = tempfile::tempdir().expect("project");
    // A 4 KiB file with a marker after the first 2048 bytes.
    let body = format!("{}NEEDLE-AFTER-2048", "e".repeat(3_000));
    std::fs::write(project.path().join("facts.txt"), &body).expect("file");
    let harness = make_harness(data, project);
    let runtime = runtime_of(&harness);
    runtime.create_session("work").expect("session");
    let (base, _bodies) = Capture::start(vec![
        sse_tool_call("r", "read", &serde_json::json!({"path": "facts.txt"})) + &sse_completed(),
        sse_delta("read it") + &sse_completed(),
    ]);
    let cancel = AtomicBool::new(false);
    let report = runtime
        .run_turn(params(
            "work",
            "read the facts",
            &harness,
            provider_of(&base),
            &cancel,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 1);
    // The UI preview is bounded: the model data is not replaced by it, but
    // the preview itself must never carry the whole output.
    let preview = &report.calls[0].output;
    assert!(
        preview.len() <= oc_adapters::runtime::REPORT_OUTPUT_CAP + 64,
        "preview must stay bounded: {} bytes",
        preview.len()
    );
    assert!(
        preview.contains("…[+"),
        "a bounded preview must carry an explicit truncation marker: {preview:?}"
    );
    assert!(
        !preview.contains("NEEDLE-AFTER-2048"),
        "the preview must not carry the whole result"
    );
    // The durable result keeps the full text and the fact stays reachable
    // through the explicit continuation (never through a whole-result row).
    let ops = harness.db.list_tool_ops_page("work", 8, None).expect("ops");
    let row = ops.first().expect("one op");
    assert_eq!(row.name, "read");
    assert!(
        row.output_truncated,
        "the list row must be a bounded preview"
    );
    assert!(
        row.output_bytes >= 3_016,
        "the full result size must be reported: {}",
        row.output_bytes
    );
    let preview = row.output.clone().unwrap_or_default();
    assert!(
        preview.len() <= oc_adapters::storage::TOOL_OP_PREVIEW_BYTES + 64,
        "the UI-facing row must be a bounded preview: {} bytes",
        preview.len()
    );
    assert!(
        !preview.contains("NEEDLE-AFTER-2048"),
        "the preview must not carry the whole result"
    );
    let (head, total, _next) = harness
        .db
        .read_tool_op_output(&row.op, 0, 4096)
        .expect("continuation");
    assert_eq!(
        total, row.output_bytes,
        "continuation reports the same size"
    );
    assert!(
        head.contains("NEEDLE-AFTER-2048"),
        "the durable result must keep the fact after 2048 bytes"
    );
    let (continued, total, next) = harness
        .db
        .read_tool_op_output(&row.op, oc_adapters::storage::TOOL_OP_PREVIEW_BYTES, 4096)
        .expect("continuation");
    assert_eq!(total, row.output_bytes);
    assert_eq!(next, None, "one window covers the rest of the result");
    assert!(
        continued.contains("NEEDLE-AFTER-2048"),
        "read continuation must return the fact after the preview"
    );
    // The model keeps the whole result: the follow-up request carries it.
    let bodies = _bodies.lock().expect("bodies").clone();
    assert_eq!(bodies.len(), 2, "tool round plus follow-up");
    assert!(
        bodies[1].contains("NEEDLE-AFTER-2048"),
        "the model-visible tool output must not be replaced by the preview"
    );
}
