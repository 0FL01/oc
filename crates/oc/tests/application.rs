//! Actual-binary configuration and application wiring regression (AUD01).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn aud01_binary_sends_configured_responses_request() {
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
    let configuration = serde_json::json!({
        "model": format!("fixture/{model}"),
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                        "apiKey": "{env:OC_FIXTURE_KEY}"},
            "models": {model: {"name": "Fixture model", "limit": {"context": 32768, "output": 4096}}}
        }}
    });
    let config_file = config.join("opencode.json");
    std::fs::write(&config_file, configuration.to_string()).expect("config");
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "binary never contacted configured endpoint"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("accept: {e}"),
            }
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
        let body: serde_json::Value = serde_json::from_slice(&bytes[header_end..]).expect("json");
        assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-not-a-secret\r\n")
        );
        assert_eq!(body["model"], model);
        assert!(body["input"].to_string().contains("binary wiring probe"));
        let sse = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"configured endpoint answer\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"configured endpoint answer\"}]}]}}\n\n"
        );
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).expect("response");
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
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().expect("wait").is_none() {
        if Instant::now() > deadline {
            child.kill().expect("kill hung binary");
            panic!("binary timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("output");
    server
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
