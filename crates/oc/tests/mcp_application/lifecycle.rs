//! MCP08/MCP10 source-derived actual-binary lifecycle cases.
//! Donor 2670273ff17da96f85c5826ced57aa1b368754fa:
//! core/src/mcp/index.ts:380–447,487–513,583–638 (independent initial forks).

use super::*;

fn lifecycle_entry(
    fixture: &Fixture,
    label: &str,
    enabled: bool,
    initialize: &str,
    catalog: &str,
) -> Value {
    json!({"type":"local", "enabled":enabled, "command":[
        "/usr/bin/python3", concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mcp10-lifecycle.py"),
        fixture.project.join(format!("{label}.json")), label, initialize, catalog, fixture.project
    ],"environment":{"MCP10_CANARY":"mcp10-activated-canary"}})
}

fn counters(fixture: &Fixture, label: &str) -> Value {
    fs::read(fixture.project.join(format!("{label}.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn event_count(fixture: &Fixture, label: &str, stage: &str) -> usize {
    fs::read_to_string(fixture.project.join(format!("{label}.json.events")))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.split_whitespace().next() == Some(stage))
        .count()
}

fn wait_counter(fixture: &Fixture, label: &str, field: &str, value: i64) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let counts = counters(fixture, label);
        if counts[field] == value {
            return counts;
        }
        assert!(
            Instant::now() < deadline,
            "missing {label}/{field}={value}: {counts}"
        );
        std::thread::sleep(POLL);
    }
}

pub(super) fn wait_row(tui: &PtyProcess, name: &str, status: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if tui
            .screen()
            .iter()
            .any(|row| row.contains(name) && row.contains(status))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing actual MCP row {name}/{status}: {:?}",
            tui.screen()
        );
        std::thread::sleep(POLL);
    }
}

pub(super) fn close_mcps(tui: &mut PtyProcess) {
    tui.raw(b"\x1b");
    let deadline = Instant::now() + TIMEOUT;
    while tui.screen().iter().any(|row| row.contains("MCP servers")) {
        assert!(Instant::now() < deadline, "modal did not close");
        std::thread::sleep(POLL);
    }
}

#[test]
fn mcp10_initial_connections_run_before_any_prompt_and_slow_neighbor_does_not_block() {
    let responses = FakeResponses::start(ResponsesScript::ToolNamed("healthy__ping".into()));
    let healthy = FakeMcp::start("healthy", "", &["ping"]);
    let (slow, closed) = FakeMcp::stalled_initialize();
    let auth = FakeMcp::start("pretend-connected", "Bearer actual-401-canary", &["ping"]);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({
            "a_slow": {"type":"remote","url":slow.url,"oauth":false},
            "healthy": {"type":"remote","url":healthy.url,"oauth":false},
            "disabled": {"type":"local","enabled":false,"command":["never-execute"]},
            "failed": {"type":"remote","url":healthy.url,"oauth":true}
            ,"pretend_connected": {"type":"remote","url":auth.url,"oauth":false}
        }),
        json!({"healthy__ping":"allow"}),
    );
    let bytes = fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap();
    let mut tui = PtyProcess::spawn(&fixture, "mcp10-initial");
    tui.wait_visible(READY);
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline
        && !healthy
            .records()
            .iter()
            .any(|r| r.rpc_method == "tools/list")
    {
        std::thread::sleep(POLL);
    }
    assert!(
        healthy
            .records()
            .iter()
            .any(|r| r.rpc_method == "tools/list"),
        "healthy MCP must initialize/list before any prompt while slow is held"
    );
    assert_eq!(
        slow.records()
            .iter()
            .filter(|r| r.rpc_method == "initialize")
            .count(),
        1
    );
    assert!(
        responses.requests().is_empty(),
        "no user/title generation before prompt"
    );
    tui.send_line("/mcps");
    wait_row(&tui, "healthy", "Connected");
    wait_row(&tui, "a_slow", "Connecting");
    wait_row(&tui, "failed", "Failed");
    wait_row(&tui, "disabled", "Disabled");
    wait_row(&tui, "pretend_connected", "Sign in required");
    tui.resize(120, 40);
    wait_row(&tui, "healthy", "Connected");
    assert!(responses.requests().is_empty());
    close_mcps(&mut tui);
    tui.send_line("healthy independent");
    tui.wait_screen("retry complete", TIMEOUT);
    assert_eq!(
        healthy
            .records()
            .iter()
            .filter(|r| r.rpc_method == "tools/call")
            .count(),
        1,
        "the ready client must actually dispatch while its neighbor stays held"
    );
    assert_eq!(
        slow.records()
            .iter()
            .filter(|r| r.rpc_method == "tools/list")
            .count(),
        0
    );
    let request = responses
        .requests()
        .into_iter()
        .find(|request| !title::is_title(request))
        .unwrap();
    let tools = function_tool_names(&request);
    assert!(tools.contains(&"healthy__ping".into()));
    assert!(tools.iter().all(|tool| {
        !["a_slow__", "failed__", "disabled__", "pretend_connected__"]
            .iter()
            .any(|prefix| tool.starts_with(prefix))
    }));
    assert!(
        !String::from_utf8_lossy(&tui.output.lock().unwrap())
            .contains("PRIVATE_RESPONSE_BODY_SENTINEL")
    );
    std::thread::sleep(Duration::from_millis(300));
    tui.send_line("/quit");
    assert!(tui.wait_exit().success());
    assert!(
        closed.load(Ordering::Relaxed),
        "owned pending transport closed at shutdown"
    );
    assert_eq!(
        fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap(),
        bytes
    );
}

#[test]
fn mcp08_disabled_activation_controls_held_request_snapshots_reopen_and_restart() {
    let release = Arc::new(AtomicBool::new(false));
    let responses = FakeResponses::start(ResponsesScript::HeldPrompts {
        prompts: vec!["before catalog".into(), "connected request".into()],
        release: release.clone(),
    });
    let fixture = Fixture::new();
    let catalog = fixture.project.join("release-catalog");
    let mut dormant = lifecycle_entry(&fixture, "dormant", false, "-", catalog.to_str().unwrap());
    dormant["command"][0] = json!("{file:program}");
    dormant["environment"]["MCP10_CANARY"] = json!("{file:activation-token}");
    fixture.write_config(
        &responses,
        json!({"dormant":dormant}),
        json!({"dormant__ping":"allow"}),
    );
    let config = fixture.home.join("config/opencode/opencode.json");
    let before = fs::read(&config).unwrap();
    let mut tui = PtyProcess::spawn(&fixture, "mcp08-controls");
    tui.wait_visible(READY);
    tui.send_line("/mcps");
    wait_row(&tui, "dormant", "Disabled");
    assert_eq!(counters(&fixture, "dormant"), Value::Null);
    tui.raw(b" ");
    tui.wait_screen("MCP activation admission refused", TIMEOUT);
    assert_eq!(
        counters(&fixture, "dormant"),
        Value::Null,
        "inert file templates must be admitted before effects"
    );
    fs::write(
        fixture.home.join("config/opencode/program"),
        "/usr/bin/python3",
    )
    .unwrap();
    fs::write(
        fixture.home.join("config/opencode/activation-token"),
        "mcp10-activated-canary",
    )
    .unwrap();
    tui.raw(b" ");
    wait_counter(&fixture, "dormant", "catalog", 1);
    wait_row(&tui, "dormant", "Connecting");
    tui.raw(b"   ");
    tui.resize(120, 40);
    wait_row(&tui, "dormant", "Connecting");
    assert_eq!(counters(&fixture, "dormant")["spawn"], 1);
    close_mcps(&mut tui);
    tui.send_line("before catalog");
    let deadline = Instant::now() + TIMEOUT;
    let first = loop {
        if let Some(request) = responses
            .requests()
            .into_iter()
            .find(|request| !title::is_title(request))
        {
            break request;
        }
        assert!(
            Instant::now() < deadline,
            "request blocked: {:?}",
            tui.screen()
        );
        std::thread::sleep(POLL);
    };
    assert!(!function_tool_names(&first).contains(&"dormant__ping".into()));
    assert!(!first.to_string().contains("LIFECYCLE_GUIDANCE"));
    fs::write(&catalog, "release").unwrap();
    tui.send_line("/mcps");
    wait_row(&tui, "dormant", "Connected");
    assert_eq!(counters(&fixture, "dormant")["initialize"], 1);
    assert!(
        !responses.requests()[0]
            .to_string()
            .contains("dormant__ping"),
        "in-flight request mutated"
    );
    close_mcps(&mut tui);
    release.store(true, Ordering::SeqCst);
    tui.wait_screen("answer:before catalog", TIMEOUT);
    std::thread::sleep(Duration::from_millis(300));
    release.store(false, Ordering::SeqCst);
    tui.send_line("connected request");
    let deadline = Instant::now() + TIMEOUT;
    let connected = loop {
        if let Some(request) = responses.requests().into_iter().find(|request| {
            last_user_text(request).as_deref() == Some("connected request")
                && !title::is_title(request)
        }) {
            break request;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(POLL);
    };
    assert!(function_tool_names(&connected).contains(&"dormant__ping".into()));
    assert!(connected.to_string().contains("LIFECYCLE_GUIDANCE"));
    tui.send_line("/mcps");
    wait_row(&tui, "dormant", "Connected");
    tui.raw(b" ");
    wait_row(&tui, "dormant", "Disconnecting");
    assert_eq!(
        counters(&fixture, "dormant")["closed"],
        0,
        "in-flight client lease must not be closed underneath its request"
    );
    release.store(true, Ordering::SeqCst);
    wait_row(&tui, "dormant", "Disabled");
    let counts = wait_counter(&fixture, "dormant", "closed", 1);
    assert_eq!(counts["spawn"], 1);
    for stage in ["spawn", "initialize", "catalog", "closed"] {
        assert_eq!(
            event_count(&fixture, "dormant", stage),
            1,
            "total actual {stage} effects"
        );
    }
    assert_eq!(counts["cwd"], true);
    assert_eq!(counts["activated"], true);
    // SAFETY: signal zero only probes this fixture's recorded owned child.
    let alive = unsafe { libc::kill(counts["pid"].as_i64().unwrap() as libc::pid_t, 0) };
    assert_ne!(alive, 0, "owned child not reaped");
    close_mcps(&mut tui);
    tui.send_line("removed catalog");
    tui.wait_screen("answer:removed catalog", TIMEOUT);
    let removed = responses
        .requests()
        .into_iter()
        .find(|request| {
            last_user_text(request).as_deref() == Some("removed catalog")
                && !title::is_title(request)
        })
        .unwrap();
    assert!(!function_tool_names(&removed).contains(&"dormant__ping".into()));
    assert!(!removed.to_string().contains("LIFECYCLE_GUIDANCE"));
    std::thread::sleep(Duration::from_millis(300));
    tui.send_line("/mcps");
    wait_row(&tui, "dormant", "Disabled");
    close_mcps(&mut tui);
    tui.send_line("/quit");
    assert!(tui.wait_exit().success());
    assert_eq!(fs::read(&config).unwrap(), before);
    let mut restarted = PtyProcess::spawn(&fixture, "mcp08-restart");
    restarted.wait_visible(READY);
    restarted.send_line("/mcps");
    wait_row(&restarted, "dormant", "Disabled");
    assert_eq!(
        counters(&fixture, "dormant"),
        counts,
        "runtime toggle persisted or status read spawned disabled entry"
    );
    assert_eq!(event_count(&fixture, "dormant", "spawn"), 1);
    close_mcps(&mut restarted);
    restarted.send_line("/quit");
    assert!(restarted.wait_exit().success());
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(
        !String::from_utf8_lossy(&tui.output.lock().unwrap()).contains("mcp10-activated-canary")
    );
    assert!(
        !serde_json::to_string(&responses.requests())
            .unwrap()
            .contains("mcp10-activated-canary")
    );
}

fn assert_child_reaped(fixture: &Fixture, label: &str) {
    let counts = wait_counter(fixture, label, "closed", 1);
    // SAFETY: signal zero only probes this fixture's recorded owned child.
    let alive = unsafe { libc::kill(counts["pid"].as_i64().unwrap() as libc::pid_t, 0) };
    assert_ne!(alive, 0, "owned child survived scope retirement");
    assert_eq!(
        event_count(fixture, label, "closed"),
        event_count(fixture, label, "spawn")
    );
}

#[test]
fn mcp_late_failure_published_during_held_native_turn_is_visible_scoped_and_reaped() {
    let release = Arc::new(AtomicBool::new(false));
    let responses = FakeResponses::start(ResponsesScript::HeldPrompts {
        prompts: vec!["held late warning".into()],
        release: release.clone(),
    });
    let fixture = Fixture::new();
    let failed_gate = fixture.project.join("release-late-failure");
    let slow_gate = fixture.project.join("never-release-slow");
    let mut unavailable = lifecycle_entry(
        &fixture,
        "unavailable",
        true,
        failed_gate.to_str().unwrap(),
        "-",
    );
    unavailable["command"]
        .as_array_mut()
        .unwrap()
        .push(json!("initialize-error"));
    fixture.write_config(
        &responses,
        json!({
            "healthy": lifecycle_entry(&fixture, "healthy", true, "-", "-"),
            "slow": lifecycle_entry(&fixture, "slow", true, slow_gate.to_str().unwrap(), "-"),
            "unavailable": unavailable,
        }),
        json!({"healthy__ping":"allow"}),
    );
    let _children: Vec<_> = ["healthy", "slow", "unavailable"]
        .into_iter()
        .map(|name| FixtureChildren(fixture.project.join(format!("{name}.json.events"))))
        .collect();
    let mut tui = PtyProcess::spawn(&fixture, "late-warning");
    tui.wait_visible(READY);
    tui.resize(120, 40);
    tui.send_line("/mcps");
    wait_row(&tui, "healthy", "Connected");
    wait_row(&tui, "slow", "Connecting");
    wait_row(&tui, "unavailable", "Connecting");
    close_mcps(&mut tui);
    tui.send_line("held late warning");
    let deadline = Instant::now() + TIMEOUT;
    let held = loop {
        if let Some(body) = responses
            .requests()
            .into_iter()
            .find(|r| !title::is_title(r))
        {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "provider blocked behind optional startup"
        );
        std::thread::sleep(POLL);
    };
    wait_counter(&fixture, "healthy", "catalog", 1);
    wait_counter(&fixture, "slow", "initialize", 1);
    wait_counter(&fixture, "unavailable", "initialize", 1);
    assert_eq!(counters(&fixture, "unavailable")["failed"], 0);
    assert_eq!(counters(&fixture, "slow")["catalog"], 0);
    let held_tools = function_tool_names(&held);
    assert!(held_tools.contains(&"healthy__ping".into()));
    assert!(
        held_tools
            .iter()
            .all(|name| !name.starts_with("unavailable__") && !name.starts_with("slow__"))
    );
    // The actual failure, not a timeout/sleep, is released AFTER the native POST.
    fs::write(&failed_gate, "release-after-held-provider-post").unwrap();
    wait_counter(&fixture, "unavailable", "failed", 1);
    // Reaping is not publication. Observe the existing native typed-status
    // consumer while the provider is still held; never add a startup barrier.
    tui.send_line("/mcps");
    wait_row(&tui, "unavailable", "Failed");
    wait_row(&tui, "healthy", "Connected");
    wait_row(&tui, "slow", "Connecting");
    assert_child_reaped(&fixture, "unavailable");
    assert_eq!(
        responses
            .requests()
            .into_iter()
            .find(|r| !title::is_title(r))
            .unwrap(),
        held,
        "async failure mutated an already-dispatched request"
    );
    assert_eq!(
        counters(&fixture, "slow")["closed"],
        0,
        "global startup barrier was awaited"
    );
    close_mcps(&mut tui);
    release.store(true, Ordering::SeqCst);
    tui.wait_screen("answer:held late warning", TIMEOUT);
    tui.wait_screen("warning: mcp unavailable", TIMEOUT);
    tui.send_line("next healthy");
    tui.wait_screen("answer:next healthy", TIMEOUT);
    let next = responses
        .requests()
        .into_iter()
        .find(|request| {
            !title::is_title(request) && last_user_text(request).as_deref() == Some("next healthy")
        })
        .unwrap();
    assert_eq!(next["tools"], held["tools"]);
    tui.send_line("/mcps");
    wait_row(&tui, "slow", "Connecting");
    wait_row(&tui, "healthy", "Connected");
    wait_row(&tui, "unavailable", "Failed");
    close_mcps(&mut tui);
    tui.send_line("/quit");
    let status = tui.wait_exit();
    for name in ["healthy", "slow", "unavailable"] {
        assert_child_reaped(&fixture, name);
    }
    assert!(status.success());
    let output = String::from_utf8_lossy(&tui.output.lock().unwrap()).into_owned();
    for marker in [
        "PRIVATE_LATE_MCP_FAILURE",
        "mcp10-activated-canary",
        "responses-key",
        "http://127.0.0.1",
    ] {
        assert!(!output.contains(marker));
    }
    assert_eq!(counters(&fixture, "unavailable")["catalog"], 0);
    assert_eq!(counters(&fixture, "unavailable")["call"], 0);
}

#[test]
fn mcp08_actual_retry_and_reload_location_retire_pending_scope_without_late_effects() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let fixture = Fixture::new();
    let program = fixture.project.join("repair-program");
    let catalog = fixture.project.join("retry-catalog-release");
    let mut entry = lifecycle_entry(&fixture, "repair", true, "-", catalog.to_str().unwrap());
    entry["command"][0] = json!(program);
    fixture.write_config(&responses, json!({"repair":entry}), json!({}));
    let mut tui = PtyProcess::spawn(&fixture, "mcp08-retry-reload-location");
    tui.wait_visible(READY);
    tui.send_line("/mcps");
    wait_row(&tui, "repair", "Failed");
    tui.wait_screen("space retry", TIMEOUT);
    assert_eq!(counters(&fixture, "repair"), Value::Null);
    std::os::unix::fs::symlink("/usr/bin/python3", &program).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        counters(&fixture, "repair"),
        Value::Null,
        "repair alone retried connection"
    );
    tui.raw(b" ");
    wait_counter(&fixture, "repair", "catalog", 1);
    wait_row(&tui, "repair", "Connecting");
    tui.raw(b"   ");
    tui.resize(140, 44);
    wait_row(&tui, "repair", "Connecting");
    // A previously painted Connecting row does not acknowledge the queued
    // spaces. Observe the following Enter and return while catalog stays held,
    // so none of those pending connects becomes a post-release disconnect.
    tui.raw(b"\r");
    tui.wait_screen("enter/esc back", TIMEOUT);
    tui.raw(b"\r");
    tui.wait_screen("connection pending", TIMEOUT);
    assert_eq!(event_count(&fixture, "repair", "spawn"), 1);
    fs::write(&catalog, "release").unwrap();
    wait_row(&tui, "repair", "Connected");
    close_mcps(&mut tui);
    // Reload builds a new source generation; it retires the ready client first.
    fixture.write_config(&responses, json!({}), json!({}));
    let old_gate = fixture.project.join("old-initialize-release");
    let slow = lifecycle_entry(&fixture, "late", true, old_gate.to_str().unwrap(), "-");
    let local = fixture.project.join("opencode.json");
    fs::write(&local, json!({"mcp":{"late":slow}}).to_string()).unwrap();
    let local_bytes = fs::read(&local).unwrap();
    tui.send_line("/reload");
    tui.wait_screen("Configuration reloaded", TIMEOUT);
    assert_child_reaped(&fixture, "repair");
    wait_counter(&fixture, "late", "initialize", 1);
    tui.send_line("/mcps");
    wait_row(&tui, "late", "Connecting");
    close_mcps(&mut tui);
    let other = fixture.project.with_file_name("other-location");
    fs::create_dir(&other).unwrap();
    tui.send_line(&format!("/location {}", other.display()));
    tui.wait_screen("location:", TIMEOUT);
    assert_child_reaped(&fixture, "late");
    fs::write(&old_gate, "late-release-after-retirement").unwrap();
    tui.send_line("/mcps");
    tui.wait_screen("no configured MCP servers", TIMEOUT);
    assert_eq!(
        event_count(&fixture, "late", "catalog"),
        0,
        "old completion escaped Location scope"
    );
    assert_eq!(event_count(&fixture, "late", "spawn"), 1);
    assert!(
        responses.requests().is_empty(),
        "connection controls generated model requests"
    );
    assert_eq!(fs::read(&local).unwrap(), local_bytes);
    close_mcps(&mut tui);
    tui.send_line("/quit");
    assert!(tui.wait_exit().success());
    assert_child_reaped(&fixture, "late");
}
