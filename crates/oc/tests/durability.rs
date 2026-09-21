//! AUD06: actual-binary process kill between a shell side effect and its outcome.
//! Linux-only: SIGSTOP fixes the crash window; a test-local subreaper owns the
//! exited shell orphan. Keep this workstream in its own integration-test process.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use oc_adapters::storage::Db;
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(10);
const SESSION: &str = "s-aud06-kill";
const FIRST_PROMPT: &str = "AUD06 execute one temporary side effect";
const NEXT_PROMPT: &str = "AUD06 fresh prompt after process kill";
const ANSWER: &str = "fresh configured response after recovery";
const CALL: &str = "aud06-shell-call";
const ITEM: &str = "aud06-shell-item";
const MODEL: &str = "durability-unseen-model";

#[test]
fn aud06_binary_kill_after_side_effect_recovers_unknown_without_replay() {
    let root = tempfile::tempdir().expect("isolated fixture");
    // Declared before subprocess guards so they are killed/reaped first, even
    // on an assertion panic. No other tests run in this integration binary.
    let mut reaper = Subreaper::new();
    let home = root.path().join("home");
    let project = root.path().join("project");
    let config = home.join("config/opencode");
    let data = home.join("data/oc");
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::create_dir_all(&project).expect("project directory");
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake endpoint");
    listener.set_nonblocking(true).expect("nonblocking accept");
    let addr = listener.local_addr().expect("endpoint address");
    std::fs::write(
        config.join("opencode.json"),
        json!({
            "model": format!("fixture/{MODEL}"),
            "permissions": {"bash": "allow"},
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                            "apiKey": "fixture-not-a-secret"},
                "models": {MODEL: {"name": "Durability fixture", "limit": {
                    "context": 32768, "output": 4096
                }}}
            }}
        })
        .to_string(),
    )
    .expect("isolated configuration");

    // All commands are shell builtins: no grandchildren, sleeps, or external
    // services. The append makes any replay visible. Stop the *oc* parent
    // before this shell exits, so its supervisor cannot commit an outcome.
    let script = concat!(
        "printf '%s\\n' \"$$\" > shell.pid\n",
        "printf 'side effect\\n' >> sentinel\n",
        "kill -STOP \"$PPID\"\n",
        "exit 0\n"
    );
    std::fs::write(project.join("effect.sh"), script).expect("temporary shell script");
    let arguments = json!({"argv": ["/bin/sh", "effect.sh"], "cwd": "."});
    let mut first = spawn(&home, &project, FIRST_PROMPT, "first");
    let (mut socket, request) = accept_request(&listener);
    assert_eq!(request["input"], json!([user_message(FIRST_PROMPT)]));
    assert!(
        request["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| { tool["name"] == "bash" })
    );
    respond(
        &mut socket,
        &[
            json!({"type": "response.output_item.added", "item": {
                "type": "function_call", "id": ITEM, "call_id": CALL, "name": "bash"
            }}),
            json!({"type": "response.function_call_arguments.delta", "item_id": ITEM,
                   "delta": arguments.to_string()}),
            json!({"type": "response.completed", "response": {"status": "completed",
                "output": [{"type": "function_call", "id": ITEM, "call_id": CALL,
                    "name": "bash", "arguments": arguments.to_string(), "status": "completed"}]
            }}),
        ],
    );
    drop(socket);

    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status = std::fs::read_to_string(format!("/proc/{}/status", first.id()))
            .expect("oc must remain alive until stopped");
        if status.lines().any(|line| line.starts_with("State:\tT")) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shell never stopped oc; {}",
            first.diagnostics()
        );
        std::thread::sleep(POLL);
    }
    assert_eq!(
        std::fs::read(project.join("sentinel")).expect("real side effect"),
        b"side effect\n"
    );
    let shell_pid: libc::pid_t = std::fs::read_to_string(project.join("shell.pid"))
        .expect("shell pid")
        .trim()
        .parse()
        .expect("numeric shell pid");
    assert!(shell_pid > 1);

    first.child.kill().expect("SIGKILL stopped oc");
    assert_eq!(
        first.wait().signal(),
        Some(libc::SIGKILL),
        "actual process kill"
    );
    let adopted = reaper.reap().expect("reap orphaned shell after oc kill");
    assert_eq!(adopted.len(), 1, "exactly the fixture shell was orphaned");
    assert_eq!(adopted[0].0, shell_pid);
    assert!(libc::WIFEXITED(adopted[0].1));
    assert_eq!(
        libc::WEXITSTATUS(adopted[0].1),
        0,
        "shell side effect finished normally"
    );

    // Db::open deliberately does not run application recovery. Inspect AFTER
    // SIGKILL and before starting the next application: a durable started row
    // with no output proves the process died before its outcome committed.
    let db = Db::open(&data).expect("inspect killed process database");
    let ops = db.list_tool_ops(SESSION).expect("durable operation");
    assert_eq!(ops.len(), 1);
    let intent = ops[0].clone();
    assert_eq!(intent.name, "bash");
    assert_eq!(intent.state, "started");
    assert_eq!(intent.output, None);
    assert!(
        intent.op.contains(CALL),
        "call identity retained: {intent:?}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(intent.input.as_deref().expect("intent input"))
            .expect("input JSON"),
        arguments
    );
    let turn = intent.turn.as_deref().expect("intent has owning turn");
    let (status, wire_log) = db.turn_result(turn).expect("interrupted turn");
    assert_eq!(status, "started");
    let wire: Value =
        serde_json::from_str(wire_log.as_deref().expect("durable wire intent")).expect("wire JSON");
    assert_eq!(wire["input"][0], user_message(FIRST_PROMPT));
    assert_eq!(wire["input"].as_array().expect("items").len(), 2);
    assert_eq!(wire["input"][1]["type"], "function_call");
    assert_eq!(wire["input"][1]["call_id"], CALL);
    assert_eq!(wire["input"][1]["arguments"], arguments.to_string());
    assert!(wire.get("assistant_message").is_none());
    let accepted = db.read_history_full(SESSION).expect("accepted history");
    assert_eq!(
        accepted.len(),
        1,
        "no completed assistant from interrupted turn"
    );
    assert_eq!(
        (accepted[0].1.as_str(), accepted[0].2.as_str()),
        ("user", FIRST_PROMPT)
    );
    drop(db);

    // Recovery must come from application::spawn in the actual binary, not a
    // direct recover_interrupted_tools call in this test.
    let mut restarted = spawn(&home, &project, NEXT_PROMPT, "restart");
    let (mut socket, request) = accept_request(&listener);
    assert_eq!(
        request["input"],
        json!([user_message(FIRST_PROMPT), user_message(NEXT_PROMPT)]),
        "normal new request retains the accepted input without replay/result fabrication"
    );
    assert!(
        !request["input"]
            .to_string()
            .contains("function_call_output")
    );
    respond(
        &mut socket,
        &[
            json!({"type": "response.output_text.delta", "delta": ANSWER}),
            json!({"type": "response.completed", "response": {"status": "completed",
                "output": [{"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": ANSWER}
                ]}]
            }}),
        ],
    );
    drop(socket);
    assert!(restarted.wait().success(), "{}", restarted.diagnostics());
    assert_eq!(
        std::fs::read_to_string(&restarted.stdout)
            .expect("restart stdout")
            .trim(),
        ANSWER
    );
    assert_eq!(
        listener
            .accept()
            .expect_err("exactly two provider requests; no automatic continuation")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        std::fs::read(project.join("sentinel")).expect("sentinel after restart"),
        b"side effect\n",
        "no tool autoreplay"
    );
    assert!(reaper.reap().expect("no children after restart").is_empty());

    let db = Db::open(&data).expect("inspect actual application recovery");
    let mut recovered = intent.clone();
    recovered.state = "unknown".into();
    assert_eq!(
        db.list_tool_ops(SESSION).expect("recovered operation"),
        vec![recovered],
        "only state changes; identity/input retained, no new operation or outcome"
    );
    assert_eq!(
        db.turn_result(turn).expect("recovered turn"),
        ("unknown".into(), wire_log)
    );
    let history = db.read_history_full(SESSION).expect("reopened history");
    assert_eq!(
        history[0], accepted[0],
        "accepted message and stable id immutable"
    );
    assert_eq!(
        db.read_history(SESSION).expect("exact history"),
        vec![
            ("user".into(), FIRST_PROMPT.into()),
            ("user".into(), NEXT_PROMPT.into()),
            ("assistant".into(), ANSWER.into()),
        ]
    );
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

fn user_message(text: &str) -> Value {
    json!({"type": "message", "role": "user", "content": [
        {"type": "input_text", "text": text}
    ]})
}

fn spawn(home: &Path, project: &Path, prompt: &str, label: &str) -> Process {
    // Files avoid pipe backpressure and inherited-pipe EOF waits on failures.
    let stdout = home.join(format!("{label}.stdout"));
    let stderr = home.join(format!("{label}.stderr"));
    let child = Command::new(env!("CARGO_BIN_EXE_oc"))
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("OC_TEST_ALLOW_LOOPBACK", "1")
        .current_dir(project)
        .args(["run", "--session", SESSION, prompt])
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).expect("stdout file"))
        .stderr(std::fs::File::create(&stderr).expect("stderr file"))
        .spawn()
        .expect("actual oc binary");
    Process {
        child,
        stdout,
        stderr,
    }
}

impl Process {
    fn id(&self) -> u32 {
        self.child.id()
    }

    fn diagnostics(&self) -> String {
        std::fs::read_to_string(&self.stderr).unwrap_or_default()
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("bounded child wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "oc timeout; {}",
                self.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let deadline = Instant::now() + TIMEOUT;
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

struct Subreaper {
    previous: libc::c_int,
}

impl Subreaper {
    fn new() -> Self {
        let mut previous = 0;
        assert_eq!(
            // SAFETY: prctl writes one c_int into a valid out-pointer.
            unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut previous) },
            0
        );
        // SAFETY: Linux process-local flag, confined to this integration binary.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
        Self { previous }
    }

    fn reap(&mut self) -> std::io::Result<Vec<(libc::pid_t, libc::c_int)>> {
        let deadline = Instant::now() + TIMEOUT;
        let mut reaped = Vec::new();
        loop {
            let mut status = 0;
            // SAFETY: valid status out-pointer; only children of this isolated
            // test process are eligible. Direct oc children are already reaped.
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid > 0 {
                reaped.push((pid, status));
            } else if pid == -1 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ECHILD) {
                    return Ok(reaped);
                }
                if error.kind() != std::io::ErrorKind::Interrupted {
                    return Err(error);
                }
            } else {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "orphan shell did not exit",
                    ));
                }
                std::thread::sleep(POLL);
            }
        }
    }
}

impl Drop for Subreaper {
    fn drop(&mut self) {
        let result = self.reap();
        // SAFETY: restore this test process's original subreaper flag.
        let restored = unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, self.previous) };
        if !std::thread::panicking() {
            result.expect("all adopted children reaped");
            assert_eq!(restored, 0);
        }
    }
}

// Single-threaded fake peer: no detached socket threads. Both accept and
// reads/writes are bounded, including failure cleanup.
fn accept_request(listener: &TcpListener) -> (TcpStream, Value) {
    let deadline = Instant::now() + TIMEOUT;
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "oc never contacted fake Responses endpoint"
                );
                std::thread::sleep(POLL);
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("write timeout");
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let header_end = loop {
        assert!(Instant::now() < deadline, "HTTP header deadline");
        let n = socket.read(&mut chunk).expect("HTTP headers");
        assert_ne!(n, 0, "early EOF");
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 65_536, "bounded headers");
        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("headers");
    assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().expect("length"))
        })
        .expect("content length");
    assert!(length < 262_144, "bounded request body");
    while bytes.len() < header_end + length {
        assert!(Instant::now() < deadline, "HTTP body deadline");
        let n = socket.read(&mut chunk).expect("HTTP body");
        assert_ne!(n, 0, "early body EOF");
        bytes.extend_from_slice(&chunk[..n]);
    }
    let body: Value =
        serde_json::from_slice(&bytes[header_end..header_end + length]).expect("request JSON");
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["stream"], true);
    (socket, body)
}

fn respond(socket: &mut TcpStream, events: &[Value]) {
    let sse: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).expect("fake Responses reply");
}
