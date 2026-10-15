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

#[test]
fn headless_requires_explicit_once_consumer_even_with_tui_auto_config() {
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
    std::fs::write(config.join("opencode.json"), serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":base,"apiKey":"fixture"},"models":{"m":{"limit":{"context":65536,"output":4096}}}}},"permission":{"bash":if deny { "deny" } else { "ask" }},"agent":{"title":{"disable":true}}}).to_string()).unwrap();
    let saved_cli = r#"{"session":{"permissions":"autoaccept"}}"#;
    std::fs::write(config.join("cli.json"), saved_cli).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        let mut count = 0;
        while !stopping.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(_) => break,
            };
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
                    let header = String::from_utf8_lossy(&bytes[..at]);
                    let len = header
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= at + 4 + len {
                        break;
                    }
                }
            }
            let mut body = String::new();
            if count == 0 {
                let args =
                    serde_json::json!({"argv":["/usr/bin/touch","headless-marker"]}).to_string();
                for value in [
                    serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"fc","call_id":"c","name":"bash","arguments":"","status":"in_progress"}}),
                    serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"fc","delta":args}),
                    serde_json::json!({"type":"response.output_item.done","item":{"type":"function_call","id":"fc","call_id":"c","name":"bash","arguments":args,"status":"completed"}}),
                ] {
                    body.push_str(&format!("data: {value}\n\n"));
                }
            } else {
                body.push_str(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\n",
                );
            }
            body.push_str("data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n");
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            count += 1;
        }
        count
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
    let calls = server.join().unwrap();
    if !deny {
        assert_eq!(
            output.status.success(),
            auto,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(project.join("headless-marker").exists(), auto && !deny);
    assert_eq!(
        std::fs::read_to_string(config.join("cli.json")).unwrap(),
        saved_cli
    );
    assert!(calls >= 1);
    if !auto {
        assert!(String::from_utf8_lossy(&output.stderr).contains("approval"));
    }
    let db = rusqlite::Connection::open(root.path().join("data/oc.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
