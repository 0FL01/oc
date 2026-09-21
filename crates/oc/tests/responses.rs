//! AUD09/10/12/13: strict offline Responses qualification through actual oc processes.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);
const SESSION: &str = "s-aud09-10";
const MODEL: &str = "strict-unseen-model";
const OPAQUE: &str = "opaque-encrypted-reasoning-never-display-29fe";

struct Fixture {
    root: tempfile::TempDir,
    listener: TcpListener,
}

impl Fixture {
    fn new(cache: bool) -> Self {
        let root = tempfile::tempdir().expect("isolated fixture");
        let config = root.path().join("home/config/opencode");
        std::fs::create_dir_all(&config).expect("config directory");
        std::fs::create_dir_all(root.path().join("project")).expect("project directory");
        std::fs::write(
            root.path().join("project/note.txt"),
            "temporary read evidence\n",
        )
        .expect("read fixture");
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake Responses");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("address");
        std::fs::write(
            config.join("opencode.json"),
            json!({
                "model": format!("fixture/{MODEL}"), "permissions": {"read": "allow"},
                "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                    "baseURL": format!("http://{addr}/proxy/v1"), "apiKey": "fixture-key",
                    "timeout": false, "chunkTimeout": 6000000, "setCacheKey": cache,
                    "headers": {"x-audit-trace": "configured-extra-header"}
                }, "models": {MODEL: {"name": "Strict fixture", "limit": {
                    "context": 32768, "output": 789
                }}}}}
            })
            .to_string(),
        )
        .expect("configuration");
        Self { root, listener }
    }

    fn spawn(&self, prompt: &str, label: &str) -> Process {
        let home = self.root.path().join("home");
        let stdout = home.join(format!("{label}.stdout"));
        let stderr = home.join(format!("{label}.stderr"));
        let child = Command::new(env!("CARGO_BIN_EXE_oc"))
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(self.root.path().join("project"))
            .args(["run", "--json", "--session", SESSION, prompt])
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout).expect("stdout"))
            .stderr(std::fs::File::create(&stderr).expect("stderr"))
            .spawn()
            .expect("actual binary");
        Process {
            child,
            stdout,
            stderr,
        }
    }

    fn accept(&self) -> (TcpStream, String, Value) {
        let deadline = Instant::now() + TIMEOUT;
        let mut socket = loop {
            match self.listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "no request from actual binary");
                    std::thread::sleep(POLL);
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read bound");
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("write bound");
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        let end = loop {
            assert!(Instant::now() < deadline, "header deadline");
            let n = socket.read(&mut chunk).expect("headers");
            assert_ne!(n, 0, "early EOF");
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 65536, "header bound");
            if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let headers = String::from_utf8(bytes[..end].to_vec()).expect("HTTP headers");
        assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().expect("length"))
            })
            .expect("content-length");
        assert!(length < 262144, "body bound");
        while bytes.len() < end + length {
            assert!(Instant::now() < deadline, "body deadline");
            let n = socket.read(&mut chunk).expect("body");
            assert_ne!(n, 0);
            bytes.extend_from_slice(&chunk[..n]);
        }
        let body: Value = serde_json::from_slice(&bytes[end..end + length]).expect("JSON");
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        (socket, headers, body)
    }
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

#[test]
fn aud13_binary_rejects_oversized_pending_line_with_visible_limit() {
    let fixture = Fixture::new(false);
    let mut process = fixture.spawn("test bounded stream", "line-limit");
    let (mut socket, _, _) = fixture.accept();
    socket
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
        )
        .unwrap();
    // No newline or terminal: rejection must happen at the byte ceiling, not EOF.
    let _ = socket.write_all(&vec![b'x'; oc_adapters::provider::SSE_BYTE_CAP + 1]);
    assert!(!process.wait(Duration::from_secs(2)).success());
    assert!(
        process.errors().contains("SSE line byte limit exceeded"),
        "{}",
        process.errors()
    );
    assert!(process.events().is_empty(), "no completed answer");
    assert_eq!(
        fixture.listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

impl Process {
    fn wait(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "binary timeout: {}",
                self.errors()
            );
            std::thread::sleep(POLL);
        }
    }

    fn errors(&self) -> String {
        std::fs::read_to_string(&self.stderr).expect("stderr")
    }

    fn events(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.stdout)
            .expect("stdout")
            .lines()
            .map(|line| serde_json::from_str(line).expect("NDJSON"))
            .collect()
    }

    fn assert_private(&self) {
        assert!(
            !self.errors().contains(OPAQUE),
            "opaque item leaked to diagnostics"
        );
        assert!(
            !std::fs::read_to_string(&self.stdout)
                .expect("stdout")
                .contains(OPAQUE),
            "opaque item leaked to UI output"
        );
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

fn message(role: &str, text: &str) -> Value {
    let kind = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    json!({"type": "message", "role": role, "content": [{"type": kind, "text": text}]})
}

fn respond(socket: &mut TcpStream, output: &[Value], answer: Option<&str>) {
    let mut events = Vec::new();
    for item in output.iter().filter(|item| item["type"] == "function_call") {
        events.push(json!({"type": "response.output_item.added", "item": {
            "type": "function_call", "id": item["id"], "call_id": item["call_id"],
            "name": item["name"], "arguments": "", "status": "in_progress"
        }}));
        events.push(json!({"type": "response.function_call_arguments.delta",
            "item_id": item["id"], "delta": item["arguments"]}));
    }
    if let Some(answer) = answer {
        events.push(json!({"type": "response.output_text.delta", "delta": answer}));
    }
    events.push(json!({"type": "response.completed", "response": {
        "status": "completed", "output": output
    }}));
    let sse: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).expect("response");
}

#[test]
fn aud09_aud10_binary_exact_typed_tool_history_survives_restart() {
    let fixture = Fixture::new(false);
    let mut process = fixture.spawn("strict first prompt", "first");
    let mut expected = vec![message("user", "strict first prompt")];
    for (index, (id, call_id)) in [("fc_A", "call_B"), ("fc_C", "call_D")]
        .into_iter()
        .enumerate()
    {
        let (mut socket, headers, body) = fixture.accept();
        assert_eq!(
            body["input"],
            json!(expected),
            "strict ordered input at round {index}"
        );
        assert_eq!(body["max_output_tokens"], 789);
        assert!(body.get("prompt_cache_key").is_none(), "setCacheKey:false");
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("x-audit-trace: configured-extra-header\r\n")
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-key\r\n")
        );
        let output = vec![
            json!({"type": "reasoning", "id": format!("rs_{index}"),
                "summary": [], "encrypted_content": OPAQUE}),
            json!({"type": "function_call", "id": id, "call_id": call_id,
                "name": "read", "arguments": "{\"path\":\"note.txt\"}", "status": "completed"}),
        ];
        respond(&mut socket, &output, None);
        expected.extend(output);
        expected.push(json!({"type": "function_call_output", "call_id": call_id,
            "output": "temporary read evidence"}));
    }
    let (mut socket, _, body) = fixture.accept();
    assert_eq!(body["input"], json!(expected), "both complete tool rounds");
    let final_item = json!({"type": "message", "id": "msg_final", "role": "assistant",
    "status": "completed", "phase": "final_answer", "content": [{
        "type": "output_text", "text": "two reads complete", "annotations": []
    }]});
    respond(
        &mut socket,
        std::slice::from_ref(&final_item),
        Some("two reads complete"),
    );
    assert!(process.wait(TIMEOUT).success(), "{}", process.errors());
    assert_eq!(
        process.events().last().expect("done"),
        &json!({"type": "done", "text": "two reads complete"})
    );
    process.assert_private();
    drop(process);
    expected.push(final_item);
    expected.push(message("user", "strict restart prompt"));
    let mut reopened = fixture.spawn("strict restart prompt", "reopened");
    let (mut socket, _, body) = fixture.accept();
    assert_eq!(
        body["input"],
        json!(expected),
        "new process replays complete typed ordered history verbatim"
    );
    respond(
        &mut socket,
        &[message("assistant", "restart complete")],
        Some("restart complete"),
    );
    assert!(reopened.wait(TIMEOUT).success(), "{}", reopened.errors());
    reopened.assert_private();
    assert_eq!(
        reopened.events().last().expect("done"),
        &json!({"type": "done", "text": "restart complete"})
    );
    assert_eq!(
        fixture
            .listener
            .accept()
            .expect_err("no replay requests")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn aud12_binary_delta_before_terminal_and_silent_stream_sigint() {
    for send_delta in [true, false] {
        let fixture = Fixture::new(true);
        let mut process = fixture.spawn("silent cancellation probe", "cancel");
        let (mut socket, _, body) = fixture.accept();
        assert!(
            body["prompt_cache_key"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "setCacheKey:true"
        );
        if send_delta {
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"visible-before-terminal\"}\n\n").expect("one delta");
            socket.flush().expect("flush delta");
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            // The session diagnostic follows app.submit acknowledgement. For the
            // body case, a full NDJSON delta additionally proves live delivery.
            let acknowledged = process.errors().contains(&format!("session {SESSION}"));
            let text = std::fs::read_to_string(&process.stdout).expect("stdout");
            if acknowledged && (!send_delta || text.ends_with('\n')) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no submission ack/live delta; headers={send_delta}"
            );
            std::thread::sleep(POLL);
        }
        if send_delta {
            assert_eq!(
                process.events(),
                vec![json!({"type": "delta", "delta": "visible-before-terminal"})]
            );
        }
        assert!(process.child.try_wait().expect("still active").is_none());
        let started = Instant::now();
        assert_eq!(
            // SAFETY: only this live child owned by the fixture is signalled.
            unsafe { libc::kill(process.child.id() as libc::pid_t, libc::SIGINT) },
            0
        );
        assert_eq!(process.wait(Duration::from_millis(950)).code(), Some(130));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "cancel waits on silent peer"
        );
        assert!(!process.events().iter().any(|event| event["type"] == "done"));
        let db = oc_adapters::storage::Db::open(&fixture.root.path().join("home/data/oc"))
            .expect("database");
        assert_eq!(
            db.read_history(SESSION).expect("history"),
            vec![("user".into(), "silent cancellation probe".into())]
        );
        // Keep socket alive and silent through cancellation, including no headers.
        drop(socket);
    }
}

#[test]
fn aud10_binary_image_modality_is_explicitly_rejected() {
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_oc"))
        .env_clear()
        .env("HOME", root.path())
        .current_dir(root.path())
        .args([
            "run",
            "describe",
            "--image",
            "https://example.invalid/image.png",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unsupported modality: image input")
    );
    assert!(
        !root.path().join(".local/share/oc").exists(),
        "no accepted input/storage/network"
    );
}
