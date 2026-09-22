//! Actual-binary configuration and application wiring regression (AUD01).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn aud01_binary_sends_configured_responses_request() {
    configured_responses_request(false, None);
}

#[test]
fn t47_binary_unknown_limits_warn_and_do_not_select_reasoning_variant() {
    configured_responses_request(true, None);
}

#[test]
fn t47_binary_separate_title_model_uses_its_budget_without_variant_overlay() {
    for (limit, output) in [
        (serde_json::Value::Null, 64),
        (serde_json::json!({"context": 4096}), 64),
        (serde_json::json!({"context": 0, "output": 0}), 64),
        (serde_json::json!({"output": 32}), 32),
    ] {
        configured_responses_request(
            false,
            Some(TitleFixture {
                limit,
                prompt: "Generate a short title.".into(),
                output: Some(output),
            }),
        );
    }
}

#[test]
fn t47_binary_title_admission_checks_assembled_input_and_selected_model() {
    for (limit, prompt_bytes) in [
        (serde_json::Value::Null, 32_768),
        (serde_json::json!({"context": 2048}), 4096),
        (serde_json::json!({"input": 1100}), 512),
    ] {
        configured_responses_request(
            false,
            Some(TitleFixture {
                limit,
                prompt: "x".repeat(prompt_bytes),
                output: None,
            }),
        );
    }
}

struct TitleFixture {
    limit: serde_json::Value,
    prompt: String,
    output: Option<u64>,
}

fn configured_responses_request(unknown_limits: bool, title: Option<TitleFixture>) {
    let fixture = tempfile::tempdir().expect("fixture");
    let home = fixture.path().join("home");
    let project = fixture.path().join("project");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&config).expect("config directory");
    std::fs::create_dir_all(&project).expect("project directory");
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("address");
    let model = "unseen-fixture-model";
    let mut configuration = serde_json::json!({
        "model": format!("fixture/{model}"),
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                        "apiKey": "{env:OC_FIXTURE_KEY}"},
            "models": {model: {"name": "Fixture model", "limit": {"context": 32768, "output": 4096}}}
        }}
    });
    if unknown_limits {
        configuration["provider"]["fixture"]["models"][model] = serde_json::json!({
            "name": "Fixture model", "variants": {"low": {"reasoningEffort":"low"}}
        });
    }
    if unknown_limits || title.is_some() {
        configuration["provider"]["fixture"]["options"]["nativeFallbackLimits"] =
            serde_json::json!({"context": 8192, "output": 64});
    }
    let title_model = if title.is_some() {
        "title-model"
    } else {
        model
    };
    if let Some(title) = &title {
        configuration["provider"]["fixture"]["models"][title_model] = serde_json::json!({
            "limit": title.limit, "variants": {"low": {"reasoningEffort":"low"}}
        });
        configuration["agent"] = serde_json::json!({"title": {
            "model": format!("fixture/{title_model}"), "prompt": title.prompt
        }});
    }
    let title_output = title
        .as_ref()
        .map(|t| t.output)
        .unwrap_or(Some(if unknown_limits { 64 } else { 256 }));
    let config_file = config.join("opencode.json");
    std::fs::write(&config_file, configuration.to_string()).expect("config");
    let (stop, stopped) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut requests = Vec::new();
        loop {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if stopped.try_recv().is_ok() {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "binary never completed configured requests"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => panic!("accept: {e}"),
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let header_end = loop {
                let n = socket.read(&mut chunk).expect("request headers");
                assert_ne!(n, 0, "early EOF");
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                assert!(bytes.len() < 65_536);
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec()).expect("headers");
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().expect("length"))
                })
                .expect("content length");
            assert!(length < 65_536);
            while bytes.len() < header_end + length {
                let n = socket.read(&mut chunk).expect("body");
                assert_ne!(n, 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[header_end..]).expect("json");
            assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-not-a-secret\r\n")
            );
            requests.push(body);
            let sse = concat!(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"configured endpoint answer\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"configured endpoint answer\"}]}]}}\n\n"
            );
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).expect("response");
        }
        requests
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_oc"));
    command
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("OC_FIXTURE_KEY", "fixture-not-a-secret")
        .env("OC_TEST_ALLOW_LOOPBACK", "1")
        .current_dir(&project)
        .args(["run", "binary wiring probe"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("binary");
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().expect("wait").is_none() {
        if Instant::now() > deadline {
            child.kill().expect("kill hung binary");
            panic!("binary timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("output");
    let _ = stop.send(());
    let requests = server
        .join()
        .expect("configured request must reach endpoint");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "configured endpoint answer"
    );
    assert_eq!(requests.len(), if title_output.is_some() { 2 } else { 1 });
    let body = &requests[0];
    assert_eq!(body["model"], model);
    assert_eq!(
        body["max_output_tokens"],
        if unknown_limits || title.is_some() {
            64
        } else {
            4096
        }
    );
    assert!(body.get("reasoning").is_none());
    assert_eq!(
        body["input"],
        serde_json::json!([{
            "type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "binary wiring probe"}
            ]
        }])
    );
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    if let Some(expected_output) = title_output {
        let title_body = &requests[1];
        assert_eq!(title_body["model"], title_model);
        assert_eq!(title_body["max_output_tokens"], expected_output);
        assert!(title_body.get("reasoning").is_none());
        assert_eq!(title_body["tools"], serde_json::json!([]));
        assert_eq!(title_body["input"].as_array().unwrap().len(), 2);
        assert_eq!(title_body["input"][0]["role"], "developer");
        assert_eq!(title_body["input"][1], body["input"][0]);
    } else {
        assert!(
            diagnostic.contains("title generation skipped:"),
            "{diagnostic}"
        );
        assert!(diagnostic.contains("exceeds context"), "{diagnostic}");
    }
    if unknown_limits || title.is_some() {
        assert!(
            diagnostic.contains(&format!(
                "title generation: model {title_model} has unknown"
            )),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("native fallback caps (context=8192, output=64)"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("not discovered model capacities"),
            "{diagnostic}"
        );
    }
    assert!(
        home.join("data/oc").is_dir(),
        "default storage must use own XDG namespace"
    );

    std::fs::remove_file(config_file).expect("remove configuration");
    let missing = command.output().expect("missing-config run");
    assert!(
        !missing.status.success(),
        "missing config must never fall back to echo"
    );
    assert!(!missing.stderr.is_empty());
    assert!(missing.stdout.is_empty());
}
