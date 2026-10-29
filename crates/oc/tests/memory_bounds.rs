//! T40 (AUD32): the real binary with equal active context and a growing
//! archive.
//!
//! Frozen workload, fixed before the run (no time/randomness in what is
//! asserted):
//!
//! * archived pairs: `SMALL_PAIRS` vs `LARGE_PAIRS`, each message
//!   `ARCHIVE_BYTES`; every archived pair also carries a durable turn log of
//!   the same size, so a full-history read pays for both copies;
//! * the archive sits below a prune mark and is therefore outside the active
//!   context in both runs;
//! * active pairs: `ACTIVE_PAIRS` x `ACTIVE_BYTES`, identical in both runs.
//!
//! Measured on the actual client + native owner + owned process tree:
//! baseline/peak/post-idle RSS, peak PSS, children and archive size on disk.
//!
//! The asserted quantity is the archive's own effect: the large run's peak
//! minus the small run's peak. Both runs share binary, process tree and
//! workload, so the difference isolates the archive. An absolute peak is not a
//! growth metric here: the first sample precedes loading the ELF, and the
//! summed VmHWM counts the same `oc2` file pages once per process (client and
//! owner), so it tracked code size rather than archive reads.

use std::io::{BufRead, BufReader, Read, Write};
#[path = "support/context_ids.rs"]
mod context_ids;
#[path = "support/process_tree.rs"]
mod process_tree;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oc2");
const MODEL: &str = "memory-fixture";
const SESSION: &str = "s-aud32";
const POLL: Duration = Duration::from_millis(2);
const DEADLINE: Duration = Duration::from_secs(120);
/// Archived pairs in the small run.
const SMALL_PAIRS: usize = 8;
/// Archived pairs in the large run.
const LARGE_PAIRS: usize = 3_000;
/// Bytes per archived message (user and assistant each).
const ARCHIVE_BYTES: usize = 16 * 1024;
/// Live pairs after the prune mark (equal in both runs).
const ACTIVE_PAIRS: usize = 2;
/// Bytes per live message.
const ACTIVE_BYTES: usize = 1_024;
/// Archive-induced peak growth bound, equal to the product's 16 MiB active
/// context cap. A full read of the large archive is 96 MiB of messages plus
/// 48 MiB of turn logs, so reading even ~11% of it cannot fit; run-to-run
/// noise of the identical rest of the tree is about 1 MiB.
const ARCHIVE_GROWTH_BOUND_KB: u64 = 16 * 1024;

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

/// Scripted peer that records request bodies.
struct Peer {
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start() -> (Arc<Self>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("peer address").to_string();
        listener.set_nonblocking(true).expect("nonblocking");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let captured = requests.clone();
        let stopping = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let mut reader = BufReader::new(socket.try_clone().expect("clone"));
                        let mut content_length = 0usize;
                        loop {
                            let mut line = String::new();
                            match reader.read_line(&mut line) {
                                Ok(0) | Err(_) => break,
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
                            continue;
                        }
                        if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) {
                            context_ids::check_context_ids(&mut value);
                            captured.lock().expect("requests").push(value);
                        }
                        let payload = sse_delta("done") + &sse_completed();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            payload.len(),
                            payload
                        );
                        let _ = socket.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
        });
        (
            Arc::new(Self {
                requests,
                stop,
                handle: Some(handle),
            }),
            addr,
        )
    }

    fn wait_request(&self, count: usize) -> Vec<serde_json::Value> {
        let start = Instant::now();
        loop {
            let requests = self.requests.lock().expect("requests").clone();
            if requests.len() >= count {
                return requests;
            }
            assert!(start.elapsed() < DEADLINE, "missing HTTP request {count}");
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let result = handle.join();
            if !std::thread::panicking() {
                result.expect("peer thread");
            }
        }
    }
}

/// One aggregate process-tree sample (individual samples use the same fields).
#[derive(Debug, Clone, Copy, Default)]
struct Sample {
    rss_kb: u64,
    hwm_kb: u64,
    pss_kb: u64,
    anon_kb: u64,
    file_kb: u64,
    threads: u64,
    children: usize,
}

/// Isolated HOME/config/data plus a scripted peer.
struct Fixture {
    root: tempfile::TempDir,
    peer: Arc<Peer>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("fixture root");
        let config = root.path().join("home/config/opencode");
        std::fs::create_dir_all(&config).expect("config");
        std::fs::create_dir_all(root.path().join("project")).expect("project");
        let (peer, addr) = Peer::start();
        std::fs::write(
            config.join("opencode.json"),
            serde_json::json!({
                "model": format!("fixture/{MODEL}"),
                "permissions": [
                    {"action": "*", "resource": "*", "effect": "deny"},
                    {"action": "read", "resource": "*", "effect": "allow"}
                ],
                "providers": {"fixture": {
                    "package": "@ai-sdk/openai",
                    "compatibility": {"supportsPromptCacheKey": false},
                    "settings": {
                        "baseURL": format!("http://{addr}/proxy/v1"),
                        "apiKey": "fixture-key",
                        "timeout": false
                    },
                    "models": {MODEL: {"name": "Memory fixture", "limit": {
                        "context": 1_000_000, "output": 100_000
                    }}}
                }}
            })
            .to_string(),
        )
        .expect("config");
        Self { root, peer }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("home/data/oc")
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }

    fn command(&self) -> Command {
        let home = self.root.path().join("home");
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(self.project())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        command
    }

    /// Create the session and the Location binding through the binary itself.
    fn open_session(&self) {
        let child = self
            .command()
            .args(["run", "--session", SESSION, "open session"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("open session run");
        let output = child.wait_with_output().expect("open session exit");
        assert!(output.status.success(), "session bootstrap failed");
    }

    fn seed_archive(&self, pairs: usize) {
        let db_path = self.data_dir().join("oc.sqlite");
        let conn = rusqlite::Connection::open(&db_path).expect("seed db");
        conn.pragma_update(None, "journal_mode", "WAL")
            .expect("wal");
        let tx = conn.unchecked_transaction().expect("tx");
        let mut seq: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM messages WHERE session_id = ?1",
                [SESSION],
                |row| row.get(0),
            )
            .expect("max seq");
        for pair in 0..pairs {
            seq += 1;
            let user_id = format!("m{seq:04}");
            let body = format!("archive {pair} {}", "a".repeat(ARCHIVE_BYTES));
            tx.execute(
                "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'user', ?4)",
                rusqlite::params![user_id, SESSION, seq, body],
            )
            .expect("archive user");
            let log = serde_json::json!({
                "turn_id": format!("ta{pair}"),
                "model": MODEL,
                "provider": "fixture",
                "opaque": [],
                "usage": null,
                "user_message": user_id,
                "input": [{"type": "message", "role": "user",
                           "content": [{"type": "input_text", "text": body}]}],
                "agent_digest": null
            });
            tx.execute(
                "INSERT INTO turns(id, session_id, status, prompt, result) VALUES (?1, ?2, 'completed', ?3, ?4)",
                rusqlite::params![
                    format!("ta{pair}"),
                    SESSION,
                    format!("archive {pair}"),
                    log.to_string()
                ],
            )
            .expect("archive turn");
            seq += 1;
            tx.execute(
                "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'assistant', ?4)",
                rusqlite::params![format!("m{seq:04}"), SESSION, seq, body],
            )
            .expect("archive assistant");
        }
        for pair in 0..ACTIVE_PAIRS {
            seq += 1;
            let user_id = format!("m{seq:04}");
            let body = format!("active {pair} {}", "b".repeat(ACTIVE_BYTES));
            tx.execute(
                "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'user', ?4)",
                rusqlite::params![user_id, SESSION, seq, body],
            )
            .expect("active user");
            let log = serde_json::json!({
                "turn_id": format!("tl{pair}"),
                "model": MODEL,
                "provider": "fixture",
                "opaque": [],
                "usage": null,
                "user_message": user_id,
                "input": [{"type": "message", "role": "user",
                           "content": [{"type": "input_text", "text": body}]}],
                "agent_digest": null
            });
            tx.execute(
                "INSERT INTO turns(id, session_id, status, prompt, result) VALUES (?1, ?2, 'completed', ?3, ?4)",
                rusqlite::params![
                    format!("tl{pair}"),
                    SESSION,
                    format!("active {pair}"),
                    log.to_string()
                ],
            )
            .expect("active turn");
            seq += 1;
            tx.execute(
                "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, 'assistant', ?4)",
                rusqlite::params![format!("m{seq:04}"), SESSION, seq, format!("ok {pair}")],
            )
            .expect("active assistant");
        }
        // Prune everything through the last archived pair.
        let prune: String = tx
            .query_row(
                "SELECT id FROM messages WHERE session_id = ?1 AND role = 'assistant'
                  ORDER BY seq DESC LIMIT 1 OFFSET ?2",
                rusqlite::params![SESSION, ACTIVE_PAIRS as i64],
                |row| row.get(0),
            )
            .expect("prune anchor");
        tx.execute(
            "INSERT INTO prune_marks(session_id, up_to_msg, created_at)
             VALUES (?1, ?2, '2026-01-01T00:00:00Z')
             ON CONFLICT(session_id) DO UPDATE SET up_to_msg = ?2",
            rusqlite::params![SESSION, prune],
        )
        .expect("prune mark");
        tx.commit().expect("seed commit");
    }

    /// Run one measured turn, sampling the process tree.
    ///
    /// `VmHWM` is kernel-tracked and monotone, so the maximum over samples is
    /// the real peak even when the process is short-lived; the last valid
    /// sample is the post-idle reading.
    fn run_turn(&self, prompt: &str) -> RunSample {
        let mut child = self
            .command()
            .args(["run", "--session", SESSION, prompt])
            .spawn()
            .expect("measured run");
        let pid = child.id();
        let mut run = RunSample::default();
        let mut owner_observed = false;
        let start = Instant::now();
        loop {
            let (sample, owner_present) = sample_process(pid, &self.data_dir());
            owner_observed |= owner_present;
            if sample.rss_kb > 0 {
                if run.baseline.rss_kb == 0 {
                    run.baseline = sample;
                }
                if sample.hwm_kb > run.peak_hwm_kb {
                    run.peak = sample;
                    run.peak_hwm_kb = sample.hwm_kb;
                }
                run.peak_pss_kb = run.peak_pss_kb.max(sample.pss_kb);
                run.children = run.children.max(sample.children);
                run.end = sample;
            }
            if let Some(status) = child.try_wait().expect("wait") {
                assert!(status.success(), "measured run failed");
                break;
            }
            assert!(start.elapsed() < DEADLINE, "measured run timed out");
            std::thread::sleep(POLL);
        }
        assert!(
            owner_observed,
            "measured the actual registered native owner"
        );
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        assert!(
            !stderr.contains("error"),
            "measured run reported an error: {stderr}"
        );
        let db_path = self.data_dir().join("oc.sqlite");
        run.db_bytes = std::fs::metadata(&db_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        run.wal_bytes = std::fs::metadata(db_path.with_extension("sqlite-wal"))
            .map(|meta| meta.len())
            .unwrap_or(0);
        run
    }
}

/// One measured run of the actual binary.
#[derive(Debug, Clone, Copy, Default)]
struct RunSample {
    baseline: Sample,
    peak: Sample,
    end: Sample,
    peak_hwm_kb: u64,
    peak_pss_kb: u64,
    children: usize,
    db_bytes: u64,
    wal_bytes: u64,
}

/// Sum the client, registered owner and every owned descendant, including
/// children launched by Tokio's non-leader threads. PSS counts shared mappings
/// proportionally; the sum of per-process VmHWM is a conservative peak bound.
fn sample_process(pid: u32, data: &std::path::Path) -> (Sample, bool) {
    let tree = process_tree::capture(pid, data);
    let mut sample = Sample::default();
    for &pid in &tree.pids {
        let process = sample_one_process(pid);
        sample.rss_kb += process.rss_kb;
        sample.hwm_kb += process.hwm_kb;
        sample.pss_kb += process.pss_kb;
        sample.anon_kb += process.anon_kb;
        sample.file_kb += process.file_kb;
        sample.threads += process.threads;
    }
    sample.children = tree.pids.len().saturating_sub(1);
    // Startup precedes registration and final owner exit precedes client exit.
    // Keep both phases' complete samples; never move the baseline past startup.
    (sample, tree.owner.is_some())
}

fn sample_one_process(pid: u32) -> Sample {
    let mut sample = Sample::default();
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                sample.rss_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("VmHWM:") {
                sample.hwm_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("RssAnon:") {
                sample.anon_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("RssFile:") {
                sample.file_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("Threads:") {
                sample.threads = parse_kb(rest);
            }
        }
    }
    if let Ok(rollup) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
        && let Some(line) = rollup.lines().find(|line| line.starts_with("Pss:"))
    {
        sample.pss_kb = parse_kb(&line[4..]);
    }
    sample
}

fn parse_kb(text: &str) -> u64 {
    text.split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn request_history(body: &serde_json::Value) -> Vec<String> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.pointer("/content/0/text")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn measure(pairs: usize, prompt: &str) -> Measurement {
    let fixture = Fixture::new();
    fixture.open_session();
    fixture.seed_archive(pairs);
    let run = fixture.run_turn(prompt);
    let requests = fixture.peer.wait_request(1);
    let body = requests.last().cloned().unwrap_or_default();
    Measurement {
        pairs,
        run,
        history: request_history(&body),
    }
}

struct Measurement {
    pairs: usize,
    run: RunSample,
    history: Vec<String>,
}

#[test]
fn aud32_actual_binary_bounds_active_context_with_growing_archive() {
    let prompt = "measure the active context";
    let small = measure(SMALL_PAIRS, prompt);
    let large = measure(LARGE_PAIRS, prompt);

    println!(
        "AUD32 binary workload: archive pairs {} -> {} x {} B (+ turn logs), active pairs {} x {} B",
        SMALL_PAIRS, LARGE_PAIRS, ARCHIVE_BYTES, ACTIVE_PAIRS, ACTIVE_BYTES
    );
    for (label, run) in [("small", &small), ("large", &large)] {
        println!(
            "AUD32 {label}: baseline_rss_kb={} peak_hwm_kb={} end_rss_kb={} end_pss_kb={} peak_pss_kb={} children={} db_bytes={} wal_bytes={}",
            run.run.baseline.rss_kb,
            run.run.peak_hwm_kb,
            run.run.end.rss_kb,
            run.run.end.pss_kb,
            run.run.peak_pss_kb,
            run.run.children,
            run.run.db_bytes,
            run.run.wal_bytes
        );
        println!(
            "AUD32 {label} peak composition: anon_kb={} file_kb={} threads={}",
            run.run.peak.anon_kb, run.run.peak.file_kb, run.run.peak.threads,
        );
    }

    // Equal active context: the archive never reaches the provider.
    assert_eq!(
        small.history, large.history,
        "the active request history must not depend on the archive size"
    );
    assert!(
        small
            .history
            .iter()
            .any(|text| text.starts_with("active 1")),
        "live history must reach the provider"
    );
    assert!(
        large.run.db_bytes > 100 * 1024 * 1024,
        "the large archive must really be large: {} bytes",
        large.run.db_bytes
    );
    let growth = large.run.peak_hwm_kb.saturating_sub(small.run.peak_hwm_kb);
    assert!(
        growth < ARCHIVE_GROWTH_BOUND_KB,
        "peak RSS followed the archive: +{growth} KiB for {} vs {} archived pairs (peaks {} / {} KiB, bound {ARCHIVE_GROWTH_BOUND_KB} KiB)",
        large.pairs,
        small.pairs,
        large.run.peak_hwm_kb,
        small.run.peak_hwm_kb
    );
}
