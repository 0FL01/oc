//! Opt-in input-to-paint probe of the real TUI on a long session (not part of
//! the offline gate).
//!
//! Copies the synthetic `perf_long_session` root (`OC_PERF_DATA`, 600 turns),
//! opens it in the real binary under a PTY and timestamps every output chunk
//! as it arrives. Scenarios:
//!
//! * A: unique glyphs typed at 60/165/250 Hz into the open long session;
//! * B: the same typing while a scripted Responses peer streams markdown;
//! * C: PageUp through the history (scroll-to-paint, including older pages);
//! * F: typing while a `shell` tool runs: F1 streams output lines, F2 only
//!   shows its spinner (CPU, frames and wakes of the tool's window);
//! * E: idle CPU ticks of the TUI and owner and terminal bytes.
//!
//! `OC_TUI_PROBE_NOMETRICS=1` leaves the TUI's per-frame metrics off, so CPU
//! ticks show the product's own cost (frame and wake reports are absent).
//!
//! `OC_TUI_PROBE_DISPLAY=1` lets the TUI detect the real display's refresh
//! rate (it inherits the display variables) instead of the 60 Hz default.
//!
//! `OC_TUI_PERF=<dir>` additionally records `perf` call graphs of the TUI and
//! owner for the run (`tui.data`, `owner.data`).
//!
//! ```sh
//! OC_PERF_DATA=target/perf-probe/long \
//!   cargo test -p oc2 --test perf_tui_pty -- --ignored --nocapture probe
//! ```
use std::io::{BufRead, BufReader, Read, Write};
#[path = "support/terminal.rs"]
mod terminal;
use std::net::TcpListener;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oc2");
const SESSION: &str = "perf-long";
const DEADLINE: Duration = Duration::from_secs(120);
const STREAM_DELTAS: usize = 600;
const STREAM_GAP: Duration = Duration::from_millis(10);

/// Output bytes with the arrival time of every chunk.
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    chunks: Vec<(usize, Instant)>,
}

impl Output {
    /// Arrival of the first chunk ending after `from`.
    fn first_after(&self, from: usize) -> Option<Instant> {
        self.chunks
            .iter()
            .find(|(chunk_end, _)| *chunk_end > from)
            .map(|(_, at)| *at)
    }
}

fn stream_reply() -> Vec<String> {
    let mut deltas = vec!["STREAM-BEGIN\n\n".to_string()];
    for index in 0..STREAM_DELTAS {
        deltas.push(match index % 40 {
            0 => format!("\n\n## Section {index}\n\n"),
            10 => "\n\n```rust\nfn checksum(v: &[u64]) -> u64 {\n".into(),
            11..=18 => format!("    let x{index} = v.iter().fold({index}, |a, b| a ^ b);\n"),
            19 => "}\n```\n\n".into(),
            25 => "\n- item with **bold** and `code`\n".into(),
            _ => format!("word{index} keeps the projection streaming "),
        });
    }
    deltas.push("\n\nSTREAM-END".into());
    deltas
}

/// F scenarios: the prompt names the command; the tool result gets a short answer.
fn shell_reply(request: &serde_json::Value) -> Option<String> {
    let input = request["input"].as_array()?;
    // DCP follows a tool output with the model-facing id of its span.
    let last = input.iter().rev().find(|item| {
        item["role"] != "developer" || !item.to_string().contains("<dcp-message-id>")
    })?;
    let output = if last["type"] == "function_call_output" {
        serde_json::json!({"type":"message","role":"assistant","id":"m-shell",
            "status":"completed","content":[{"type":"output_text","text":"SHELL-DONE"}]})
    } else {
        let text = last.to_string();
        let command = if text.contains("probe shell stream") {
            "for i in $(seq 1 1500); do echo \"   Compiling crate-$i v0.1.0 (/work/crate-$i)\"; sleep 0.005; done"
        } else if text.contains("probe shell sleep") {
            "sleep 8"
        } else {
            return None;
        };
        serde_json::json!({"type":"function_call","id":"fc_shell","call_id":"call-shell",
            "name":"shell","arguments":serde_json::json!({"command":command}).to_string(),
            "status":"completed"})
    };
    let done = serde_json::json!({"type":"response.output_item.done","item":output});
    let completed = serde_json::json!({"type":"response.completed","response":{
        "status":"completed","output":[output],"usage":{"input_tokens":1000,"output_tokens":100}}});
    Some(format!("data: {done}\n\ndata: {completed}\n\n"))
}

/// Streams one slow markdown answer per main request; titles answer at once.
struct Peer {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start() -> (Self, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let socket = match listener.accept() {
                    Ok((socket, _)) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("accept: {error}"),
                };
                socket.set_nonblocking(false).unwrap();
                let stopping = stopping.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(socket);
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':')
                            && name.trim().eq_ignore_ascii_case("content-length")
                        {
                            length = value.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0; length];
                    let _ = reader.read_exact(&mut body);
                    let request: serde_json::Value =
                        serde_json::from_slice(&body).unwrap_or_default();
                    let title = request["max_output_tokens"] == 256
                        && request["tools"]
                            .as_array()
                            .is_none_or(|tools| tools.is_empty());
                    let socket = reader.get_mut();
                    let _ = write!(
                        socket,
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                    );
                    if !title && let Some(sse) = shell_reply(&request) {
                        let _ = socket.write_all(sse.as_bytes());
                        let _ = socket.flush();
                        return;
                    }
                    let deltas = if title {
                        vec!["Probe".to_string()]
                    } else {
                        stream_reply()
                    };
                    let mut text = String::new();
                    for delta in deltas {
                        if stopping.load(Ordering::Relaxed) {
                            return;
                        }
                        text.push_str(&delta);
                        let event =
                            serde_json::json!({"type":"response.output_text.delta","delta":delta});
                        if write!(socket, "data: {event}\n\n").is_err() || socket.flush().is_err() {
                            return;
                        }
                        if !title {
                            std::thread::sleep(STREAM_GAP);
                        }
                    }
                    let item = serde_json::json!({"type":"message","role":"assistant","id":"m-probe",
                        "status":"completed","content":[{"type":"output_text","text":text}]});
                    let done = serde_json::json!({"type":"response.output_item.done","item":item});
                    let completed = serde_json::json!({"type":"response.completed","response":{
                        "status":"completed","output":[item],"usage":{"input_tokens":1000,"output_tokens":100}}});
                    let _ = write!(socket, "data: {done}\n\ndata: {completed}\n\n");
                    let _ = socket.flush();
                });
            }
        });
        (
            Self {
                stop,
                handle: Some(handle),
            },
            base,
        )
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A private copy of the synthetic root, rebound to this copy's project path.
struct Fixture {
    root: PathBuf,
    _peer: Peer,
}

impl Fixture {
    fn new() -> Self {
        let source = std::fs::canonicalize(
            std::env::var_os("OC_PERF_DATA").expect("OC_PERF_DATA names the synthetic root"),
        )
        .unwrap();
        let root = source.with_file_name(format!("tui-probe-{}", std::process::id()));
        assert!(!root.exists(), "refusing to reuse {root:?}");
        let status = Command::new("cp")
            .arg("-a")
            .arg(&source)
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success(), "copy synthetic root");
        let project = root.join("project");
        let db = rusqlite::Connection::open(root.join("data/oc.sqlite")).unwrap();
        db.execute(
            "UPDATE prefs SET value=?1 WHERE key=?2",
            rusqlite::params![
                project.to_string_lossy(),
                format!("tui.session_location.{SESSION}")
            ],
        )
        .unwrap();
        drop(db);
        let (peer, base) = Peer::start();
        let config_path = project.join("opencode.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        config["providers"]["fixture"]["settings"]["baseURL"] = base.into();
        std::fs::write(&config_path, config.to_string()).unwrap();
        Self { root, _peer: peer }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("TERM", "xterm-256color")
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .arg("--data-dir")
            .arg(self.root.join("data"))
            .current_dir(self.root.join("project"));
        // Detect the real display's refresh rate instead of the 60 Hz default.
        if std::env::var_os("OC_TUI_PROBE_DISPLAY").is_some() {
            for name in ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DISPLAY"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
        command
    }

    fn owner(&self) -> Option<u32> {
        let bytes = std::fs::read(self.root.join("data/native-owner.json")).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        u32::try_from(value["pid"].as_u64()?).ok()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("OC_TUI_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

struct Tui {
    child: Child,
    master: std::fs::File,
    output: Arc<Mutex<Output>>,
}

impl Tui {
    fn spawn(fixture: &Fixture, metrics: &Path) -> Self {
        let winsize = libc::winsize {
            ws_row: 60,
            ws_col: 200,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: valid out-params and a live winsize; returns two open fds.
        let rc = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &winsize,
            )
        };
        assert_eq!(rc, 0, "openpty");
        // SAFETY: openpty succeeded, so master is an open fd we now own.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: openpty succeeded, so slave is an open fd we now own.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let dup = |fd: &OwnedFd| {
            // SAFETY: fd is open; F_DUPFD_CLOEXEC mints a new owned fd.
            let duped = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
            assert!(duped >= 0);
            // SAFETY: just minted above.
            unsafe { OwnedFd::from_raw_fd(duped) }
        };
        let mut command = fixture.command();
        command.args(["tui", "--session", SESSION]);
        // Metrics clone and compare every frame; CPU-only runs can skip them.
        if std::env::var_os("OC_TUI_PROBE_NOMETRICS").is_none() {
            command.env("OC_TUI_TEST_METRICS", metrics);
        }
        command
            .stdin(Stdio::from(dup(&slave)))
            .stdout(Stdio::from(dup(&slave)))
            .stderr(Stdio::from(dup(&slave)));
        terminal::controlling_terminal(&mut command);
        let child = command.spawn().unwrap();
        drop(slave);
        let master: std::fs::File = master.into();
        let mut reader = master.try_clone().unwrap();
        let output = Arc::new(Mutex::new(Output::default()));
        let sink = output.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 65536];
            while let Ok(n) = reader.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let at = Instant::now();
                let mut output = sink.lock().unwrap();
                output.bytes.extend_from_slice(&chunk[..n]);
                let end = output.bytes.len();
                output.chunks.push((end, at));
            }
        });
        Self {
            child,
            master,
            output,
        }
    }

    fn len(&self) -> usize {
        self.output.lock().unwrap().bytes.len()
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }

    fn wait_for(&self, from: usize, needle: &str, timeout: Duration) -> Instant {
        self.wait_found(from, needle, timeout).0
    }

    /// Arrival of `needle` after `from` and the start of its chunk. Only new
    /// bytes are copied under the lock and searched outside it: a long scan
    /// must never stall the reader, or a full PTY would block the TUI's write
    /// and the observer would measure its own back-pressure.
    fn wait_found(&self, from: usize, needle: &str, timeout: Duration) -> (Instant, usize) {
        let needle = needle.as_bytes();
        let start = Instant::now();
        let mut checked = from;
        loop {
            let base = checked.saturating_sub(needle.len() - 1).max(from);
            let window = {
                let output = self.output.lock().unwrap();
                output.bytes[base.min(output.bytes.len())..].to_vec()
            };
            if let Some(at) = window.windows(needle.len()).position(|w| w == needle) {
                let end = base + at + needle.len();
                let output = self.output.lock().unwrap();
                let index = output
                    .chunks
                    .partition_point(|(chunk_end, _)| *chunk_end < end);
                let chunk_start = index.checked_sub(1).map_or(0, |i| output.chunks[i].0);
                return (output.chunks[index].1, chunk_start);
            }
            checked = base + window.len();
            assert!(start.elapsed() < timeout, "timeout waiting for {needle:?}");
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Wait until no output arrives for `quiet`.
    fn settle(&self, quiet: Duration, timeout: Duration) {
        let start = Instant::now();
        let mut last = (self.len(), Instant::now());
        while start.elapsed() < timeout {
            std::thread::sleep(Duration::from_millis(5));
            let len = self.len();
            if len != last.0 {
                last = (len, Instant::now());
            } else if last.1.elapsed() >= quiet {
                return;
            }
        }
        panic!("output never settled");
    }
}

/// The TUI's metrics use the same Linux monotonic clock.
fn monotonic_ns() -> u128 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one valid timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    time.tv_sec as u128 * 1_000_000_000 + time.tv_nsec as u128
}

static SENT: Mutex<Vec<u128>> = Mutex::new(Vec::new());
/// Monotonic send time and latency of every sample over 20 ms.
static OUTLIERS: Mutex<Vec<(u128, Duration)>> = Mutex::new(Vec::new());
/// Output chunks around one outlier: (offset ms, bytes, has glyph, has previous glyph).
type ChunkTrace = Vec<(f64, usize, bool, bool)>;
static OUTLIER_CHUNKS: Mutex<Vec<ChunkTrace>> = Mutex::new(Vec::new());
/// Labelled monotonic windows whose frames and wakes are counted at exit.
static WINDOWS: Mutex<Vec<(String, u128, u128)>> = Mutex::new(Vec::new());

fn enabled(scenario: char) -> bool {
    std::env::var("OC_TUI_SCENARIOS").map_or(true, |list| list.contains(scenario))
}

fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
        .unwrap_or_default();
    // utime and stime are fields 14 and 15; `rest` starts at field 3.
    fields
        .get(11)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        + fields
            .get(12)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
}

fn rss_kb(pid: u32) -> (u64, u64) {
    let rollup = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).unwrap_or_default();
    let field = |name: &str| {
        rollup
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("Rss:"), field("Pss:"))
}

fn report(label: &str, mut samples: Vec<Duration>) {
    assert!(!samples.is_empty(), "{label}: no samples");
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    let over = |ms: u64| {
        samples
            .iter()
            .filter(|d| **d > Duration::from_millis(ms))
            .count()
    };
    eprintln!(
        "PROBE {label}: n={} p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms over16ms={} over30ms={}",
        samples.len(),
        at(0.50).as_secs_f64() * 1e3,
        at(0.95).as_secs_f64() * 1e3,
        at(0.99).as_secs_f64() * 1e3,
        at(1.0).as_secs_f64() * 1e3,
        over(16),
        over(30)
    );
}

/// Paced unique glyphs; the injector never waits for painting. With `clear`,
/// every 100 glyphs a Ctrl-C clears the non-empty idle draft (bounded composer);
/// during a running turn Ctrl-C would interrupt, so the draft is kept.
fn type_glyphs(tui: &mut Tui, hz: u32, count: u32, base: u32, clear: bool) -> Vec<Duration> {
    let mut latencies = Vec::new();
    let mut batch = 0;
    while batch < count {
        let n = (count - batch).min(100);
        let from = tui.len();
        let phase = Instant::now();
        let mut sent = Vec::new();
        for index in 0..n {
            let due = phase + Duration::from_secs_f64(f64::from(index) / f64::from(hz));
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            let glyph = char::from_u32(base + batch + index).unwrap().to_string();
            let at = Instant::now();
            let mono = monotonic_ns();
            SENT.lock().unwrap().push(mono);
            tui.send(glyph.as_bytes());
            sent.push((at, mono, glyph));
        }
        // Keys are handled in order: a glyph is never painted before the
        // chunk that painted the previous one.
        let mut floor = from;
        for (at, mono, glyph) in sent {
            let (painted, chunk) = tui.wait_found(floor, &glyph, Duration::from_secs(10));
            floor = chunk;
            let latency = painted.saturating_duration_since(at);
            if latency > Duration::from_millis(20) {
                OUTLIERS.lock().unwrap().push((mono, latency));
                let previous = char::from_u32(glyph.chars().next().unwrap() as u32 - 1)
                    .unwrap()
                    .to_string();
                let output = tui.output.lock().unwrap();
                let mut begin = 0;
                let chunks = output
                    .chunks
                    .iter()
                    .filter_map(|(end, arrived)| {
                        let start = std::mem::replace(&mut begin, *end);
                        let offset = arrived.saturating_duration_since(at).as_secs_f64() * 1e3
                            - at.saturating_duration_since(*arrived).as_secs_f64() * 1e3;
                        (offset > -5.0 && offset < 80.0).then(|| {
                            let bytes = &output.bytes[start..*end];
                            let has = |g: &str| bytes.windows(g.len()).any(|w| w == g.as_bytes());
                            (offset, end - start, has(&glyph), has(&previous))
                        })
                    })
                    .collect();
                OUTLIER_CHUNKS.lock().unwrap().push(chunks);
            }
            latencies.push(latency);
        }
        if clear {
            tui.send(b"\x03");
            tui.settle(Duration::from_millis(150), Duration::from_secs(10));
        }
        batch += n;
    }
    latencies
}

fn perf_record(dir: &Path, name: &str, pid: u32) -> Child {
    Command::new("perf")
        .args(["record", "-F", "999", "-g", "--call-graph"])
        // Frame-pointer builds unwind far better than sampled DWARF stacks.
        .arg(std::env::var("OC_TUI_PERF_CALLGRAPH").unwrap_or_else(|_| "dwarf,32768".into()))
        .arg("-o")
        .arg(dir.join(format!("{name}.data")))
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
#[ignore = "opt-in input-to-paint probe on a long synthetic session"]
fn probe() {
    let fixture = Fixture::new();
    let metrics = fixture.root.join("tui-metrics.json");
    let open = Instant::now();
    let mut tui = Tui::spawn(&fixture, &metrics);
    tui.wait_for(0, "Synthetic long session", DEADLINE);
    tui.settle(Duration::from_millis(500), DEADLINE);
    eprintln!("PROBE open to settled paint {:?}", open.elapsed());
    let owner = fixture.owner().expect("registered owner");
    let tui_pid = tui.child.id();
    let recorders = std::env::var_os("OC_TUI_PERF").map(|dir| {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let recorders = [
            perf_record(&dir, "tui", tui_pid),
            perf_record(&dir, "owner", owner),
        ];
        std::thread::sleep(Duration::from_millis(500));
        recorders
    });
    eprintln!(
        "PROBE rss_pss_kb after open: tui={:?} owner={:?}",
        rss_kb(tui_pid),
        rss_kb(owner)
    );

    // E: idle.
    if enabled('E') {
        let (tui_cpu, owner_cpu, bytes) = (cpu_ticks(tui_pid), cpu_ticks(owner), tui.len());
        std::thread::sleep(Duration::from_secs(10));
        eprintln!(
            "PROBE E idle 10s: tui_cpu_ticks={} owner_cpu_ticks={} terminal_bytes={}",
            cpu_ticks(tui_pid) - tui_cpu,
            cpu_ticks(owner) - owner_cpu,
            tui.len() - bytes
        );
    }

    // A: typing into the open long session.
    let count: u32 = std::env::var("OC_TUI_GLYPHS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    for (hz, base) in [(60, 0x4e00), (165, 0x5400), (250, 0x5a00)] {
        if enabled('A') {
            let latencies = type_glyphs(&mut tui, hz, count, base, true);
            report(&format!("A type {hz}Hz"), latencies);
        }
    }

    // C: PageUp through the history at 30 Hz (each step waits for its paint).
    let mut scroll = Vec::new();
    for _ in 0..if enabled('C') { 400 } else { 0 } {
        let from = tui.len();
        let at = Instant::now();
        tui.send(b"\x1b[5~");
        let start = Instant::now();
        let painted = loop {
            if let Some(painted) = tui.output.lock().unwrap().first_after(from) {
                break Some(painted);
            }
            if start.elapsed() > Duration::from_secs(5) {
                break None;
            }
            std::thread::sleep(Duration::from_micros(200));
        };
        match painted {
            Some(painted) => scroll.push(painted.saturating_duration_since(at)),
            None => break,
        }
        std::thread::sleep(Duration::from_millis(33));
    }
    if enabled('C') {
        report("C PageUp", scroll);
        tui.send(b"\x1b[F");
        tui.settle(Duration::from_millis(300), DEADLINE);
    }

    // B: typing while the model streams a long markdown answer.
    if enabled('B') {
        let from = tui.len();
        tui.send("probe stream".as_bytes());
        tui.send(b"\r");
        tui.wait_for(from, "STREAM-BEGIN", DEADLINE);
        let latencies = type_glyphs(&mut tui, 165, 300, 0x6000, false);
        report("B type 165Hz during stream", latencies);
        tui.wait_for(from, "STREAM-END", DEADLINE);
        tui.settle(Duration::from_millis(2500), DEADLINE);
        tui.send(b"\x03");
        tui.settle(Duration::from_millis(300), DEADLINE);
    }
    // F: a shell tool streams output (F1) or only spins (F2). First its
    // steady load with no input, then typing latency while it still runs.
    for (label, prompt, running, base) in [
        (
            "F1 shell stream",
            "probe shell stream",
            "Compiling crate-",
            0x6400,
        ),
        ("F2 shell spinner", "probe shell sleep", "sleep 8", 0x6800),
    ] {
        if !enabled('F') {
            break;
        }
        let from = tui.len();
        tui.send(prompt.as_bytes());
        tui.send(b"\r");
        tui.wait_for(from, running, DEADLINE);
        std::thread::sleep(Duration::from_millis(300));
        let (tui_cpu, owner_cpu, bytes) = (cpu_ticks(tui_pid), cpu_ticks(owner), tui.len());
        let started = monotonic_ns();
        std::thread::sleep(Duration::from_millis(2500));
        let ended = monotonic_ns();
        eprintln!(
            "PROBE {label} load 2.5s: tui_cpu_ticks={} owner_cpu_ticks={} terminal_bytes={}",
            cpu_ticks(tui_pid) - tui_cpu,
            cpu_ticks(owner) - owner_cpu,
            tui.len() - bytes
        );
        WINDOWS
            .lock()
            .unwrap()
            .push((format!("{label} load"), started, ended));
        let latencies = type_glyphs(&mut tui, 60, 120, base, false);
        report(&format!("{label} type 60Hz"), latencies);
        tui.wait_for(from, "SHELL-DONE", DEADLINE);
        tui.settle(Duration::from_millis(1500), DEADLINE);
        tui.send(b"\x03");
        tui.settle(Duration::from_millis(300), DEADLINE);
    }
    eprintln!(
        "PROBE rss_pss_kb at end: tui={:?} owner={:?}",
        rss_kb(tui_pid),
        rss_kb(owner)
    );
    eprintln!(
        "PROBE total cpu ticks: tui={} owner={}",
        cpu_ticks(tui_pid),
        cpu_ticks(owner)
    );
    // Settled idle after all work (the first window includes startup work
    // such as the public catalog refresh).
    let (tui_cpu, owner_cpu, bytes) = (cpu_ticks(tui_pid), cpu_ticks(owner), tui.len());
    std::thread::sleep(Duration::from_secs(10));
    eprintln!(
        "PROBE E2 settled idle 10s: tui_cpu_ticks={} owner_cpu_ticks={} terminal_bytes={}",
        cpu_ticks(tui_pid) - tui_cpu,
        cpu_ticks(owner) - owner_cpu,
        tui.len() - bytes
    );

    if let Some(recorders) = recorders {
        for mut recorder in recorders {
            // SAFETY: SIGINT to our own perf child lets it flush its data file.
            unsafe { libc::kill(recorder.id() as i32, libc::SIGINT) };
            let _ = recorder.wait();
        }
    }
    tui.send(b"\x03");
    let start = Instant::now();
    while tui.child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < DEADLINE, "TUI quit");
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Ok(bytes) = std::fs::read(&metrics) {
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        eprintln!(
            "PROBE metrics frames={} frame_max_ms={:.2} wakeups={} display_hz={} strip_frames={} strip_refused={}",
            value["frame_count"],
            value["frame_max_ns"].as_f64().unwrap_or(0.0) / 1e6,
            value["wakeups"],
            value["display_hz"],
            value["strip_frames"],
            value["strip_refused"]
        );
        decompose(&value);
    }
}

/// Per keystroke: time to the next UI wake, to the next frame start, and that
/// frame's duration (all on the shared monotonic clock).
fn decompose(metrics: &serde_json::Value) {
    let list = |key: &str| -> Vec<u128> {
        metrics[key]
            .as_array()
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_u64().map(u128::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let wakes = list("wake_monotonic_ns");
    let frames = list("frame_monotonic_ns");
    let durations: Vec<u128> = metrics["frame_samples_ns"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(|x| x[1].as_u64().map(u128::from))
                .collect()
        })
        .unwrap_or_default();
    for (label, from, to) in WINDOWS.lock().unwrap().iter() {
        let seconds = (to - from) as f64 / 1e9;
        let count = |list: &[u128]| list.iter().filter(|at| (*from..*to).contains(at)).count();
        let mut window: Vec<u128> = frames
            .iter()
            .zip(durations.iter())
            .filter(|(at, _)| (*from..*to).contains(*at))
            .map(|(_, d)| *d)
            .collect();
        window.sort_unstable();
        let busy: u128 = window.iter().sum();
        let at = |q: f64| {
            window
                .get(((window.len().max(1) - 1) as f64 * q) as usize)
                .map_or(0.0, |d| *d as f64 / 1e6)
        };
        eprintln!(
            "PROBE {label} window: frames={:.1}/s wakes={:.1}/s frame_busy={:.1}% frame p10={:.2}ms p50={:.2}ms p90={:.2}ms",
            count(&frames) as f64 / seconds,
            count(&wakes) as f64 / seconds,
            busy as f64 / 1e7 / seconds,
            at(0.1),
            at(0.5),
            at(0.9)
        );
    }
    let (mut wake, mut start, mut work) = (Vec::new(), Vec::new(), Vec::new());
    for sent in SENT.lock().unwrap().iter() {
        if let Some(w) = wakes.iter().find(|w| *w >= sent) {
            wake.push(Duration::from_nanos((w - sent) as u64));
        }
        if let Some(index) = frames.iter().position(|f| f >= sent) {
            start.push(Duration::from_nanos((frames[index] - sent) as u64));
            if let Some(d) = durations.get(index) {
                work.push(Duration::from_nanos(*d as u64));
            }
        }
    }
    if !wake.is_empty() {
        report("decompose send->wake", wake);
    }
    if !start.is_empty() {
        report("decompose send->frame start", start);
        report("decompose frame duration", work);
    }
    for chunks in OUTLIER_CHUNKS.lock().unwrap().iter() {
        eprintln!("PROBE outlier chunks (ms, bytes, glyph, previous)={chunks:?}");
    }
    // Timeline around each outlier: frames (offset, duration) and wakes.
    for (sent, latency) in OUTLIERS.lock().unwrap().iter() {
        let window = |at: u128| at + 5_000_000 >= *sent && at <= sent + 80_000_000;
        let offset = |at: u128| (at as i128 - *sent as i128) as f64 / 1e6;
        let frames_near: Vec<String> = frames
            .iter()
            .zip(durations.iter())
            .filter(|(at, _)| window(**at))
            .map(|(at, d)| format!("{:.1}+{:.1}", offset(*at), *d as f64 / 1e6))
            .collect();
        let wakes_near: Vec<String> = wakes
            .iter()
            .filter(|at| window(**at))
            .map(|at| format!("{:.1}", offset(*at)))
            .collect();
        eprintln!(
            "PROBE outlier latency={:.1}ms frames(start+dur)={frames_near:?} wakes={wakes_near:?}",
            latency.as_secs_f64() * 1e3
        );
    }
}
