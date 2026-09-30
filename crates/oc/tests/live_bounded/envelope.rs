//! Test-only external envelope integration and actual-binary proofs.

use super::*;

const HELPER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/bounded_live.py");
pub(super) const PLACEHOLDER: &str = "bounded-envelope-placeholder";

fn python() -> Command {
    let mut command = Command::new("python3");
    command.arg("-B").arg(HELPER).env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
}

/// The pipe is an ownership lease, not an assertion from an environment flag.
/// All external attempts go through the independently verified Python journal.
pub(super) struct Envelope {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    pub(super) campaign: String,
    pub(super) ready: Value,
}

fn metadata(command: &mut Command, input: Option<&str>) -> Result<Value, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    let child = command.spawn().map_err(|_| "envelope spawn failed")?;
    let mut guard = Envelope {
        child,
        input: None,
        campaign: String::new(),
        ready: Value::Null,
    };
    guard.input = guard.child.stdin.take();
    if let Some(input) = input {
        guard
            .input
            .as_mut()
            .ok_or("envelope input unavailable")?
            .write_all(format!("{input}\n").as_bytes())
            .map_err(|_| "envelope input failed")?;
    } else {
        guard.input.take();
    }
    read_metadata(&mut guard.child)
}

fn read_metadata(child: &mut std::process::Child) -> Result<Value, String> {
    let output = child.stdout.as_mut().ok_or("envelope output unavailable")?;
    let fd = output.as_raw_fd();
    // SAFETY: this is the owned child's still-live output pipe.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // SAFETY: only the same owned pipe's descriptor flags change.
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("envelope pipe unavailable".into());
    }
    let mut raw = Vec::new();
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let mut bytes = [0u8; 4096];
        match output.read(&mut bytes) {
            Ok(0) => return Err("envelope closed before verification".into()),
            Ok(count) => raw.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err("envelope metadata unavailable".into()),
        }
        if raw.len() > 64 * 1024 {
            return Err("envelope metadata limit".into());
        }
        if raw.ends_with(b"\n") {
            let value: Value = serde_json::from_slice(&raw)
                .map_err(|_| "envelope metadata invalid".to_string())?;
            if value["status"] == "blocked" {
                return Err("envelope authority refused campaign".into());
            }
            return Ok(value);
        }
        if Instant::now() >= until {
            return Err("envelope verification deadline".into());
        }
        std::thread::sleep(POLL);
    }
}

pub(super) fn inspect(campaign: &str) -> Result<Value, String> {
    metadata(python().args(["inspect", "--campaign", campaign]), None)
}

impl Envelope {
    pub(super) fn initialize(root: &Path) -> String {
        // Explicit creation is used ONLY by offline fixtures. Live resume never
        // initializes, chooses a random campaign ID, or recreates missing state.
        let output = python()
            .arg("init")
            .arg(root)
            .output()
            .expect("offline init");
        assert!(
            output.status.success(),
            "offline campaign initialization failed"
        );
        String::from_utf8(output.stdout)
            .expect("campaign ID")
            .trim()
            .into()
    }

    pub(super) fn start(campaign: String, manifest: &str, offline: bool) -> Result<Self, String> {
        let prior = inspect(&campaign)?;
        if prior["counts"]["generation"]
            .as_u64()
            .is_none_or(|n| n >= 24)
            || prior["counts"]["mcp"].as_u64().is_none_or(|n| n >= 4)
        {
            return Err("campaign exhausted".into());
        }
        let mut command = python();
        command.args(["serve", "--campaign", &campaign]);
        if offline {
            command.arg("--offline");
        } else {
            command.args(["--live-opt-in", "bounded-v1"]);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let child = command.spawn().map_err(|_| "envelope spawn failed")?;
        let mut guard = Self {
            child,
            input: None,
            campaign,
            ready: Value::Null,
        };
        guard.input = guard.child.stdin.take();
        guard
            .input
            .as_mut()
            .ok_or("envelope input unavailable")?
            .write_all(format!("{manifest}\n").as_bytes())
            .map_err(|_| "envelope input failed")?;
        guard.ready = read_metadata(&mut guard.child)?;
        if guard.ready["id"] != prior["id"]
            || !guard.ready["provider_base"]
                .as_str()
                .is_some_and(|s| s.starts_with("http://127.0.0.1:"))
        {
            return Err("envelope identity/route verification failed".into());
        }
        Ok(guard)
    }

    pub(super) fn verify(&self) -> Result<(), String> {
        let state = inspect(&self.campaign)?;
        if state["id"] != self.ready["id"] {
            return Err("envelope identity changed".into());
        }
        // Test server is owned and alive; an env-only quota declaration is
        // never sufficient. The per-dispatch authority rechecks under flock.
        // SAFETY: this probes only our own live child PID, without a signal.
        if unsafe { libc::kill(self.child.id() as i32, 0) } != 0 {
            return Err("envelope owner unavailable".into());
        }
        Ok(())
    }
}

impl Drop for Envelope {
    fn drop(&mut self) {
        self.input.take(); // EOF makes the helper close sockets and reap leaves.
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= until {
                // SAFETY: only our explicitly isolated helper process group.
                unsafe {
                    libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                }
                break;
            }
            std::thread::sleep(POLL);
        }
        let _ = self.child.wait();
    }
}

fn manifest(peer: &Peer, mcp: &Mcp) -> String {
    json!({"provider": {"generation_url": format!("{}/responses", peer.url), "discovery_url": format!("{}/models", peer.url), "headers": {"authorization": "Bearer dry-run-key"}},
        "mcp": {SEARCH_SERVER: {"url": mcp.url, "headers": {"authorization": "Bearer dry-run"}}}}).to_string()
}

fn post(url: &str, body: &Value) {
    let address = url
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let path = &url[url.find(address).unwrap() + address.len()..];
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let body = body.to_string();
    write!(socket, "POST {path} HTTP/1.1\r\nhost: {address}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
}

#[test]
fn bounded_actual_peer_rejects_twenty_fifth_and_fifth() {
    let peer = Peer::start();
    let mcp = Mcp::start();
    let root = tempfile::tempdir().unwrap();
    let id = Envelope::initialize(&root.path().join("campaign"));
    let guard = Envelope::start(id.clone(), &manifest(&peer, &mcp), true).unwrap();
    for _ in 0..25 {
        post(
            &format!(
                "{}/responses",
                guard.ready["provider_base"].as_str().unwrap()
            ),
            &json!({"model":"offline", "stream":true, "max_output_tokens":2048}),
        );
    }
    for id in 0..5 {
        post(
            guard.ready["mcp"][SEARCH_SERVER]["url"].as_str().unwrap(),
            &json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":"search", "arguments":{"query":"offline", "response_length":"short"}}}),
        );
    }
    assert_eq!(
        (peer.requests.lock().unwrap().len(), mcp.calls().len()),
        (24, 4)
    );
    let state = inspect(&id).unwrap();
    assert_eq!(
        (
            state["counts"]["generation"].as_u64(),
            state["counts"]["mcp"].as_u64()
        ),
        (Some(24), Some(4))
    );
}

#[test]
fn bounded_durable_envelope_direct_socket_process_suite() {
    let mut command = Command::new("python3");
    command.args([
        "-B",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/test_bounded_live.py"
        ),
        "-v",
    ]);
    let (ok, detail) = run_bounded(&mut command, CALL_TIMEOUT);
    assert!(ok, "direct envelope socket/process suite: {detail}");
}

#[test]
fn bounded_native_restart_and_helper_restart_share_real_attempts() {
    let peer = Peer::with_retry();
    let mcp = Mcp::start();
    let root = tempfile::tempdir().unwrap();
    let id = Envelope::initialize(&root.path().join("campaign"));
    let targets = manifest(&peer, &mcp);
    let guard = Envelope::start(id.clone(), &targets, true).unwrap();
    let mut fixture = Fixture::guarded("fixture/dry-run-model".into(), None, guard);
    fixture.preflight(CALL_TIMEOUT).unwrap();
    let (ok, detail) = fixture.run(
        &fixture.project(),
        "s-durable-envelope",
        "Acknowledge.",
        CALL_TIMEOUT,
    );
    let before = peer.requests.lock().unwrap().len();
    // T54: the first exchange is HTTP 500. Main retries through its finite
    // owner; title remains one physical request. The two lanes can race.
    let requests = peer.requests.lock().unwrap().clone();
    assert!(ok, "first native process: {detail}");
    let main_first = requests[0]["is_title"] != true;
    assert_eq!(
        before,
        if main_first { 3 } else { 2 },
        "all physical main retries plus the one automatic title are counted"
    );
    assert_eq!(
        requests.iter().filter(|v| v["is_title"] != true).count(),
        if main_first { 2 } else { 1 }
    );
    assert!(
        peer.requests
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["is_title"] == true)
    );
    let initial = inspect(&id).unwrap();
    assert_eq!(initial["counts"]["generation"], before);
    assert_eq!(
        initial["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == "generation" && a["http_status"] == 500)
            .count(),
        1,
        "the one physical HTTP 500 is a consumed complete exchange"
    );
    drop(fixture.envelope.take());
    let guard = Envelope::start(id.clone(), &targets, true).unwrap();
    fixture.route_guard(guard);
    let (ok, detail) = fixture.run(
        &fixture.project(),
        "s-durable-envelope",
        "Acknowledge after restart.",
        CALL_TIMEOUT,
    );
    assert!(ok, "second native process: {detail}");
    let after = peer.requests.lock().unwrap().len();
    assert!(after > before);
    assert_eq!(
        peer.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["is_title"] != true)
            .count(),
        if main_first { 3 } else { 2 },
        "main attempts, including the one source-authorized retry and explicit restart"
    );
    assert_eq!(inspect(&id).unwrap()["counts"]["generation"], after);
    println!(
        "owned_native_attempts first={before} combined={after} native_processes=2 helper_processes=2 adapter_retry=false runtime_retry=true title=true"
    );
}

#[test]
fn bounded_guarded_five_step_campaign_preserves_offline_gates() {
    let peer = Peer::start();
    let mcp = Mcp::start();
    let root = tempfile::tempdir().unwrap();
    let id = Envelope::initialize(&root.path().join("campaign"));
    let guard = Envelope::start(id.clone(), &manifest(&peer, &mcp), true).unwrap();
    let fixture = Fixture::guarded("fixture/dry-run-model".into(), None, guard);
    let campaign = Campaign::new();
    let config = fixture.preflight(campaign.remaining()).unwrap();
    let campaign = bounded_campaign(&fixture, config.has_search(), campaign);
    assert!(
        campaign.passed(),
        "guarded offline steps: {}; accounting={}; actual_mcp_calls={}",
        campaign.summary("offline", "fixture/dry-run-model", None),
        inspect(&id).unwrap(),
        mcp.calls().len()
    );
    require_complete(&campaign);
    let state = inspect(&id).unwrap();
    assert_eq!(
        state["counts"]["generation"],
        peer.requests.lock().unwrap().len()
    );
    assert_eq!(state["counts"]["mcp"], mcp.calls().len());
    assert_eq!(mcp.calls().len(), 1);
    println!(
        "owned_five_step_attempts generation={} mcp={}",
        state["counts"]["generation"], state["counts"]["mcp"]
    );
    // R4's read/search-only profile is a distinct native smoke, not a grant of
    // the coding campaign's patch/shell/compress capabilities. Product preflight
    // must admit it while still refusing it for the unchanged coding gate.
    fixture.restrict_r4();
    assert!(fixture.preflight(CALL_TIMEOUT).is_err());
    assert!(
        fixture
            .preflight_mode(CALL_TIMEOUT, true)
            .unwrap()
            .has_search()
    );
    assert_eq!(
        inspect(&id).unwrap()["counts"]["generation"],
        state["counts"]["generation"]
    );
}

#[test]
fn bounded_native_r4_complete_path_uses_same_trusted_envelope() {
    let peer = Peer::start();
    let codex = Mcp::start();
    let crw = Mcp::start();
    let unavailable = Mcp::unavailable();
    let root = tempfile::tempdir().unwrap();
    let id = Envelope::initialize(&root.path().join("campaign"));
    let mut targets: Value = serde_json::from_str(&manifest(&peer, &codex)).unwrap();
    targets["mcp"]["crw"] = json!({"url":crw.url, "headers":{}});
    targets["mcp"]["unavailable"] = json!({"url":unavailable.url, "headers":{}});
    let guard = Envelope::start(id.clone(), &targets.to_string(), true).unwrap();
    let fixture = Fixture::guarded("fixture/dry-run-model".into(), None, guard);
    fixture.restrict_r4();
    let report = r4_report(&fixture);
    assert_eq!(
        report["status"], "passed",
        "strict same-native offline R4: {report}"
    );
    assert_eq!(
        report["visible_unavailable_warning_count"], 1,
        "headless stderr must render this typed degradation exactly once"
    );
    assert_eq!(
        report["durable_envelope"]["counts"]["generation"],
        peer.requests.lock().unwrap().len()
    );
    assert_eq!(report["durable_envelope"]["counts"]["mcp"], 1);
    assert_eq!(codex.calls().len(), 1);
    assert!(crw.calls().is_empty() && unavailable.calls().is_empty());
    assert_eq!(unavailable.initialize_attempts.load(Ordering::Relaxed), 1);
    assert!(
        report["durable_envelope"]["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|attempt| !attempt["catalog_ids"]
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "unavailable"))
    );
    println!(
        "owned_native_r4 generation={} mcp=1 catalogs=true unavailable_initialize=1 warning=true completed=true",
        report["durable_envelope"]["counts"]["generation"]
    );
}

#[test]
fn bounded_native_r4_http_failure_reports_safe_actual_receipts() {
    let peer = Peer::with_status(400);
    let codex = Mcp::start();
    let crw = Mcp::start();
    let unavailable = Mcp::unavailable();
    let root = tempfile::tempdir().unwrap();
    let id = Envelope::initialize(&root.path().join("campaign"));
    let mut targets: Value = serde_json::from_str(&manifest(&peer, &codex)).unwrap();
    targets["mcp"]["crw"] = json!({"url":crw.url, "headers":{}});
    targets["mcp"]["unavailable"] = json!({"url":unavailable.url, "headers":{}});
    let guard = Envelope::start(id.clone(), &targets.to_string(), true).unwrap();
    let fixture = Fixture::guarded("fixture/dry-run-model".into(), None, guard);
    fixture.restrict_r4();
    let report = r4_report(&fixture);
    assert_eq!(report["status"], "non-success");
    assert_eq!(report["native_run_ok"], false);
    assert_eq!(report["native_error_code"], "unknown");
    assert_eq!(report["completed_short_codex_search"], false);
    let generations: Vec<_> = report["durable_envelope"]["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|attempt| attempt["kind"] == "generation")
        .collect();
    let actual = peer.requests.lock().unwrap().len();
    assert!(actual > 0);
    assert_eq!(generations.len(), actual);
    assert_eq!(report["durable_envelope"]["counts"]["generation"], actual);
    assert!(
        generations
            .iter()
            .all(|a| a["http_status"] == 400 && a["outcome"] == "complete")
    );
    assert_eq!(report["durable_envelope"]["counts"]["mcp"], 0);
    assert!(codex.calls().is_empty() && crw.calls().is_empty() && unavailable.calls().is_empty());
    let metadata = report.to_string();
    assert!(!metadata.contains("PRIVATE_DIAGNOSTIC_CANARY"));
    assert!(!metadata.contains("dry-run-key") && !metadata.contains(&peer.url));
    println!(
        "owned_native_r4_non_success generation={actual} mcp=0 upstream_status=400 native_error_code=unknown"
    );
}
