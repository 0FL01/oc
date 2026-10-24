//! Retained-memory soak of the real binary: one TUI attached under a PTY keeps
//! the native owner alive while `oc2 run` drives many tool turns through a
//! scripted Responses peer. After every turn the owner and the TUI are sampled
//! from `/proc` (anonymous + swapped memory, peak, threads).
//!
//! `OC_SOAK_SESSIONS > 1` rotates turns across sessions so the active context
//! stays small: growth then is retention, not a legitimately larger request.
//!
//! ```sh
//! OC_SOAK_TURNS=120 cargo test -p oc2 --test memory_soak -- --ignored --nocapture probe
//! ```
use std::io::{BufRead, BufReader, Read, Write};
#[path = "support/terminal.rs"]
mod terminal;
use std::net::TcpListener;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oc2");
const MODEL: &str = "soak-fixture";
const ROUNDS: usize = 2;
const DEADLINE: Duration = Duration::from_secs(60);

fn source_file(seed: usize) -> String {
    (0..200)
        .map(|line| {
            format!(
                "pub fn item_{seed}_{line}(v: u64) -> u64 {{ v.rotate_left({}) ^ {line} }}",
                line % 13
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn call(id: &str, name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,
        "name":name,"arguments":args.to_string(),"status":"completed"})
}

fn answer(turn: usize) -> String {
    format!(
        "## Turn {turn}\n\nChecked **{ROUNDS}** rounds of `item_*` helpers.\n\n- read sources\n\
         - searched\n- wrote a module\n\n```rust\npub fn checksum_{turn}(v: &[u64]) -> u64 {{\n    \
         v.iter().fold(0, |a, x| a ^ x.rotate_left(3))\n}}\n```\n\n{}\n",
        "Every helper stays total and side-effect free. ".repeat(20)
    )
}

/// One scripted reply per main request: `ROUNDS` tool rounds, then an answer.
fn reply(request: &serde_json::Value, index: usize) -> String {
    let turn = index / (ROUNDS + 1);
    let round = index % (ROUNDS + 1);
    let patch = request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|tool| tool["name"] == "apply_patch");
    let mut sse = String::new();
    let output = if round < ROUNDS {
        let id = |kind: &str| format!("t{turn}r{round}{kind}");
        let path = format!("gen/t{turn}_r{round}.rs");
        let body = source_file(turn * ROUNDS + round + 100);
        vec![
            call(
                &id("read"),
                "read",
                serde_json::json!({"path": format!("src/f{}.rs", (turn + round) % 10)}),
            ),
            call(
                &id("grep"),
                "grep",
                serde_json::json!({"pattern": format!("item_{}_1", (turn + round) % 10), "path": "src"}),
            ),
            call(
                &id("shell"),
                "shell",
                serde_json::json!({"command": format!("seq {turn} {}", turn + 2000)}),
            ),
            if patch {
                let added: String = body.lines().map(|line| format!("+{line}\n")).collect();
                call(
                    &id("patch"),
                    "apply_patch",
                    serde_json::json!({"patchText": format!("*** Begin Patch\n*** Add File: {path}\n{added}*** End Patch")}),
                )
            } else {
                call(
                    &id("write"),
                    "write",
                    serde_json::json!({"filePath": path, "content": body}),
                )
            },
        ]
    } else {
        let text = answer(turn);
        for chunk in text.as_bytes().chunks(48) {
            sse.push_str(&format!(
                "data: {}\n\n",
                serde_json::json!({"type":"response.output_text.delta","delta":String::from_utf8_lossy(chunk)})
            ));
        }
        vec![
            serde_json::json!({"type":"message","role":"assistant","id":format!("m{turn}"),
            "status":"completed","content":[{"type":"output_text","text":text}]}),
        ]
    };
    for item in &output {
        sse.push_str(&format!(
            "data: {}\n\n",
            serde_json::json!({"type":"response.output_item.done","item":item})
        ));
    }
    sse.push_str(&format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed","response":{"status":"completed",
            "output":output,"usage":{"input_tokens":1000,"output_tokens":100}}})
    ));
    sse
}

fn title_reply() -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed","response":{"status":"completed",
            "output":[{"type":"message","role":"assistant","content":[
                {"type":"output_text","text":"Soak session"}]}]}})
    )
}

struct Peer {
    stop: Arc<AtomicBool>,
    last_request: Arc<AtomicUsize>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start() -> (Self, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let last_request = Arc::new(AtomicUsize::new(0));
        let (stopping, sizes) = (stop.clone(), last_request.clone());
        let handle = std::thread::spawn(move || {
            let mut index = 0usize;
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
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                let title = request["max_output_tokens"] == 256
                    && request["tools"]
                        .as_array()
                        .is_none_or(|tools| tools.is_empty());
                let sse = if title {
                    title_reply()
                } else {
                    sizes.store(length, Ordering::Relaxed);
                    index += 1;
                    reply(&request, index - 1)
                };
                let _ = write!(
                    reader.get_mut(),
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{sse}",
                    sse.len()
                );
            }
        });
        (
            Self {
                stop,
                last_request,
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

struct Fixture {
    root: tempfile::TempDir,
    peer: Peer,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let home = root.path().join("home");
        let project = root.path().join("project");
        for dir in [
            home.join("config/opencode"),
            project.join("src"),
            project.join("gen"),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for index in 0..10 {
            std::fs::write(project.join(format!("src/f{index}.rs")), source_file(index)).unwrap();
        }
        let (peer, base) = Peer::start();
        let config = serde_json::json!({
            "model": format!("fixture/{MODEL}"),
            "permissions": [{"action": "*", "resource": "*", "effect": "allow"}],
            "providers": {"fixture": {"package": "@ai-sdk/openai",
                "settings": {"baseURL": base, "apiKey": "fixture-not-a-secret", "timeout": false},
                "models": {MODEL: {"name": "Soak fixture",
                    "limit": {"context": 100_000_000, "output": 100_000}}}}}
        });
        std::fs::write(
            home.join("config/opencode/opencode.json"),
            config.to_string(),
        )
        .unwrap();
        Self { root, peer }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("home/data/oc")
    }

    fn command(&self) -> Command {
        let home = self.root.path().join("home");
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("HOME", &home)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(self.root.path().join("project"));
        // Probe-only allocator experiments (e.g. MALLOC_ARENA_MAX=2).
        if let Some(arenas) = std::env::var_os("OC_SOAK_MALLOC_ARENA_MAX") {
            command.env("MALLOC_ARENA_MAX", arenas);
        }
        if let Some(threshold) = std::env::var_os("OC_SOAK_MALLOC_MMAP_THRESHOLD") {
            command.env("MALLOC_MMAP_THRESHOLD_", threshold);
        }
        command
    }

    fn run(&self, session: &str, prompt: &str) {
        let mut child = self
            .command()
            .args(["run", "--session", session, prompt])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < DEADLINE, "turn timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "turn failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn owner(&self) -> Option<u32> {
        let bytes = std::fs::read(self.data_dir().join("native-owner.json")).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        u32::try_from(value["pid"].as_u64()?).ok()
    }
}

struct Tui {
    child: Child,
    master: std::fs::File,
}

impl Tui {
    /// Ctrl-C from idle quits cleanly, which also writes opt-in metrics.
    fn quit(mut self) {
        self.master.write_all(b"\x03").unwrap();
        let start = Instant::now();
        while self.child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < DEADLINE, "TUI quit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Tui {
    fn spawn(fixture: &Fixture, session: &str, metrics: Option<&Path>) -> Self {
        let winsize = libc::winsize {
            ws_row: 40,
            ws_col: 140,
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
        command
            .args(["tui", "--session", session])
            .env("TERM", "xterm-256color")
            .envs(metrics.map(|path| ("OC_TUI_TEST_METRICS", path)))
            .stdin(Stdio::from(dup(&slave)))
            .stdout(Stdio::from(dup(&slave)))
            .stderr(Stdio::from(dup(&slave)));
        terminal::controlling_terminal(&mut command);
        let child = command.spawn().unwrap();
        drop(slave);
        let master: std::fs::File = master.into();
        let mut reader = master.try_clone().unwrap();
        // Drain the terminal so painting never blocks on a full PTY.
        std::thread::spawn(move || {
            let mut chunk = [0u8; 16384];
            while matches!(reader.read(&mut chunk), Ok(n) if n > 0) {}
        });
        Self { child, master }
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Sample {
    anon_kb: u64,
    swap_kb: u64,
    rss_kb: u64,
    hwm_kb: u64,
    threads: u64,
}

impl Sample {
    fn retained(&self) -> u64 {
        self.anon_kb + self.swap_kb
    }
}

fn sample(pid: u32) -> Sample {
    let mut result = Sample::default();
    let field = |text: &str| -> u64 {
        text.split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    if let Ok(rollup) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
        for line in rollup.lines() {
            if let Some(rest) = line.strip_prefix("Anonymous:") {
                result.anon_kb = field(rest);
            } else if let Some(rest) = line.strip_prefix("Swap:") {
                result.swap_kb = field(rest);
            } else if let Some(rest) = line.strip_prefix("Rss:") {
                result.rss_kb = field(rest);
            }
        }
    }
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                result.hwm_kb = field(rest);
            } else if let Some(rest) = line.strip_prefix("Threads:") {
                result.threads = field(rest);
            }
        }
    }
    result
}

fn wait_owner(fixture: &Fixture, tui: u32) -> u32 {
    let start = Instant::now();
    loop {
        if let Some(pid) = fixture
            .owner()
            .filter(|pid| *pid != tui && Path::new(&format!("/proc/{pid}")).exists())
        {
            return pid;
        }
        assert!(start.elapsed() < DEADLINE, "owner registration");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Turns in the gated regression; the old owner already held ~105 MiB here.
const GATED_TURNS: usize = 8;
/// Retained (anonymous + swapped) owner memory after those turns, fixed before
/// measuring: the active context is ~1 MiB, so per-request copies of whole
/// catalogs/bindings or unreturned turn peaks cannot fit under it.
const OWNER_RETAINED_BOUND_KB: u64 = 48 * 1024;

#[test]
fn aud_owner_and_tui_release_turn_peaks() {
    let fixture = Fixture::new();
    let session = "s-soak-0";
    fixture.run(session, "Turn 0: read the sources and keep helpers total.");
    let metrics = fixture.root.path().join("tui-metrics.json");
    let tui = Tui::spawn(&fixture, session, Some(&metrics));
    let owner = wait_owner(&fixture, tui.child.id());
    let mut retained = Vec::new();
    for turn in 1..=GATED_TURNS {
        fixture.run(
            session,
            &format!("Turn {turn}: read the sources and keep helpers total."),
        );
        assert_eq!(
            fixture.owner(),
            Some(owner),
            "the attached TUI keeps one owner alive"
        );
        retained.push(sample(owner).retained());
    }
    // Quiet period: the TUI hands back its burst once events stop.
    std::thread::sleep(Duration::from_millis(1500));
    let owner_kb = sample(owner).retained();
    eprintln!("AUD owner retained per turn {retained:?} idle {owner_kb}");
    assert!(
        owner_kb <= OWNER_RETAINED_BOUND_KB
            && retained.iter().all(|kb| *kb <= OWNER_RETAINED_BOUND_KB),
        "owner retained {owner_kb} KiB after {GATED_TURNS} turns (per turn {retained:?})"
    );
    tui.quit();
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metrics).expect("TUI metrics")).unwrap();
    assert!(
        metrics["memory_releases"].as_u64().unwrap_or(0) >= 1,
        "an idle TUI returns freed heap after the turn burst: {metrics}"
    );
}

#[test]
#[ignore = "opt-in retained-memory soak of the real binary"]
fn probe() {
    let turns: usize = std::env::var("OC_SOAK_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let sessions: usize = std::env::var("OC_SOAK_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let fixture = Fixture::new();
    let session = |index: usize| format!("s-soak-{index}");
    // The first turn creates the session and binds the Location.
    fixture.run(
        &session(0),
        "Turn 0: read the sources and keep helpers total.",
    );
    let tui = Tui::spawn(&fixture, &session(0), None);
    let tui_pid = tui.child.id();
    let owner = wait_owner(&fixture, tui_pid);
    std::thread::sleep(Duration::from_millis(500));
    eprintln!(
        "SOAK turn,request_kb,owner_anon_kb,owner_swap_kb,owner_rss_kb,owner_hwm_kb,owner_threads,tui_anon_kb,tui_swap_kb,tui_rss_kb,tui_hwm_kb"
    );
    let started = Instant::now();
    let mut heaptrack = None;
    for turn in 1..turns {
        // Diagnosis only: attribute per-turn peaks after a short warm-up.
        if turn == 4
            && let Some(out) = std::env::var_os("OC_SOAK_HEAPTRACK")
        {
            heaptrack = Some(
                Command::new("heaptrack")
                    .arg("-o")
                    .arg(Path::new(&out).join("owner"))
                    .args([
                        "-p",
                        &if std::env::var_os("OC_SOAK_HEAPTRACK_TUI").is_some() {
                            tui_pid
                        } else {
                            owner
                        }
                        .to_string(),
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            std::thread::sleep(Duration::from_secs(3));
        }
        fixture.run(
            &session(turn % sessions),
            &format!("Turn {turn}: read the sources and keep helpers total."),
        );
        assert_eq!(
            fixture.owner(),
            Some(owner),
            "the attached TUI keeps one owner alive"
        );
        std::thread::sleep(Duration::from_millis(30));
        let (o, t) = (sample(owner), sample(tui_pid));
        eprintln!(
            "SOAK {turn},{},{},{},{},{},{},{},{},{},{}",
            fixture.peer.last_request.load(Ordering::Relaxed) / 1024,
            o.anon_kb,
            o.swap_kb,
            o.rss_kb,
            o.hwm_kb,
            o.threads,
            t.anon_kb,
            t.swap_kb,
            t.rss_kb,
            t.hwm_kb
        );
    }
    // Idle settle: anything still held now is retained, not in flight.
    std::thread::sleep(Duration::from_secs(12));
    let (o, t) = (sample(owner), sample(tui_pid));
    eprintln!(
        "SOAK idle owner_retained_kb={} owner_hwm_kb={} owner_threads={} tui_retained_kb={} tui_hwm_kb={} elapsed={:?}",
        o.retained(),
        o.hwm_kb,
        o.threads,
        t.retained(),
        t.hwm_kb,
        started.elapsed()
    );
    // Diagnosis only: malloc_info shows live vs free arena bytes; a trim
    // afterwards shows how much of the retained memory was fragmentation.
    if let Some(out) = std::env::var_os("OC_SOAK_INTROSPECT") {
        for (label, pid) in [("owner", owner), ("tui", tui_pid)] {
            let path = Path::new(&out).join(format!("malloc_info_{label}.xml"));
            let status = Command::new("gdb")
                .args(["-p", &pid.to_string(), "-batch", "-nx"])
                .arg("-ex")
                .arg(format!(
                    "set $f = (void*) fopen(\"{}\", \"w\")",
                    path.display()
                ))
                .args([
                    "-ex",
                    "call (int) malloc_info(0, $f)",
                    "-ex",
                    "call (int) fclose($f)",
                ])
                .args(["-ex", "call (int) malloc_trim(0)"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            let after = sample(pid);
            eprintln!(
                "SOAK {label} gdb={status} after_trim_retained_kb={} rss_kb={}",
                after.retained(),
                after.rss_kb
            );
        }
    }
    if let Some(mut heaptrack) = heaptrack {
        // The owner exits once its last viewer leaves; heaptrack then finishes.
        tui.quit();
        let start = Instant::now();
        while heaptrack.try_wait().unwrap().is_none() && start.elapsed() < DEADLINE {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = heaptrack.kill();
        let _ = heaptrack.wait();
    }
}
