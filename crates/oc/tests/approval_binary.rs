//! VIS36 actual executable headless consumer admission; isolated offline wire.
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[path = "approval_binary/shared_owner.rs"]
mod shared_owner;

#[test]
fn headless_rejects_asks_with_feedback_and_auto_keeps_once() {
    run(None, false);
    for flag in ["--auto", "--yolo", "--dangerously-skip-permissions"] {
        run(Some(flag), false);
        run(Some(flag), true);
    }
}

#[test]
fn vis36_actual_hidden_auto_aliases_keep_help_side_effect_free() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&config).unwrap();
    // Help must finish in the CLI parser, before even malformed config loads.
    std::fs::write(config.join("opencode.json"), "{").unwrap();
    let data = root.path().join("data");
    for flag in ["--auto", "--yolo", "--dangerously-skip-permissions"] {
        for argv in [
            vec![flag, "--help"],
            vec![flag, "tui", "--help"],
            vec!["tui", flag, "--help"],
            vec![flag, "run", "--help"],
            vec!["run", flag, "--help"],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_oc2"))
                .env_clear()
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", home.join("config"))
                .env("PATH", "/usr/bin:/bin")
                .arg("--data-dir")
                .arg(&data)
                .args(&argv)
                .output()
                .unwrap();
            assert!(output.status.success(), "{argv:?}: {output:?}");
            let help = String::from_utf8(output.stdout).unwrap();
            assert!(help.contains("--auto"));
            assert!(!help.contains("--yolo"));
            assert!(!help.contains("--dangerously-skip-permissions"));
            assert!(output.stderr.is_empty());
            assert!(!data.exists(), "help created native storage");
            assert_eq!(
                std::fs::read_to_string(config.join("opencode.json")).unwrap(),
                "{"
            );
        }
    }
}

fn run(auto_flag: Option<&str>, deny: bool) {
    let auto = auto_flag.is_some();
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    std::fs::write(config.join("opencode.json"), serde_json::json!({"model":"fixture/m","providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"baseURL":base,"apiKey":"fixture"},"models":{"m":{"limit":{"context":65536,"output":4096}}}}},"agents":{"title":{"disabled":true}}, "permissions": [{"action": "shell", "resource": "*", "effect": if deny { "deny" } else { "ask" }}]}).to_string()).unwrap();
    let saved_cli = r#"{"session":{"permissions":"autoaccept"}}"#;
    std::fs::write(config.join("cli.json"), saved_cli).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        let mut count = 0;
        let mut requests = Vec::new();
        while !stopping.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(_) => break,
            };
            let request = read_request(&mut stream);
            // Title generation is automatic and runs concurrently with the
            // main request; it never consumes the scripted main-turn answer.
            if request["max_output_tokens"] == 256
                && request["tools"]
                    .as_array()
                    .is_none_or(|tools| tools.is_empty())
            {
                let reply = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Fixture title\"}]}]}}\n\n";
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                continue;
            }
            let body = if count == 0 {
                tool(
                    "c",
                    "bash",
                    serde_json::json!({"argv":["/usr/bin/touch","headless-marker"]}),
                )
            } else {
                finished()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            count += 1;
            requests.push(request);
        }
        (count, requests)
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_oc2"));
    command
        .current_dir(&project)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("PATH", "/usr/bin:/bin")
        .env("OC_TEST_ALLOW_LOOPBACK", "1")
        .arg("--data-dir")
        .arg(root.path().join("data"))
        .arg("run")
        .arg("invoke shell")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(flag) = auto_flag {
        command.arg(flag);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            stop.store(true, Ordering::SeqCst);
            server.join().unwrap();
            panic!("headless consumer hung");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    stop.store(true, Ordering::SeqCst);
    let (calls, requests) = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(project.join("headless-marker").exists(), auto && !deny);
    assert_eq!(
        std::fs::read_to_string(config.join("cli.json")).unwrap(),
        saved_cli
    );
    assert_eq!(calls, 2);
    if !auto {
        let outcome = requests[1]["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == "c")
            .unwrap();
        assert!(outcome["output"].as_str().unwrap().contains("This non-interactive run cannot ask the user for permission, so the request was rejected. Continue without this action."));
    }
    let db = rusqlite::Connection::open(root.path().join("data/oc.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

fn read_request(stream: &mut std::net::TcpStream) -> serde_json::Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let len = String::from_utf8_lossy(&bytes[..at])
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= at + 4 + len {
                return serde_json::from_slice(&bytes[at + 4..at + 4 + len]).unwrap();
            }
        }
    }
    panic!("fixture HTTP request was incomplete");
}

fn tool(id: &str, name: &str, arguments: serde_json::Value) -> String {
    let item_id = format!("fc_{id}");
    let arguments = arguments.to_string();
    [
        serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":item_id,"call_id":id,"name":name,"arguments":"","status":"in_progress"}}),
        serde_json::json!({"type":"response.function_call_arguments.delta","item_id":item_id,"delta":arguments}),
        serde_json::json!({"type":"response.output_item.done","item":{"type":"function_call","id":item_id,"call_id":id,"name":name,"arguments":arguments,"status":"completed"}}),
        serde_json::json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":1,"output_tokens":1}}}),
    ].iter().map(|value| format!("data: {value}\n\n")).collect()
}

fn finished() -> String {
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n".into()
}
