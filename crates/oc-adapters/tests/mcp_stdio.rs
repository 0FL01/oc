//! T21 (MCP04/MCP05): local stdio lifecycle against the pinned POSIX-sh
//! fake server — exact argv, minimal env, bounded redacted stderr,
//! kill/reap, restart generations, disabled zero-spawn.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use oc_adapters::mcp_remote::CLIENT_TIMEOUT;
use oc_adapters::mcp_stdio::{STDERR_CAP_BYTES, StdioClient, StdioConfig, StdioError, spawn_child};

fn fixture() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fake-mcp-stdio.sh"
    );
    assert!(std::path::Path::new(path).is_file(), "pinned fake missing");
    path.to_string()
}

fn fake_config(extra: Vec<(&str, &str)>) -> StdioConfig {
    StdioConfig {
        server_id: "fake".to_string(),
        argv: vec![
            fixture(),
            "--flag=kept".to_string(),
            "two words".to_string(),
        ],
        cwd: None,
        extra_env: extra
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        secrets: vec!["fake-secret-123".to_string()],
        timeout: CLIENT_TIMEOUT,
        enabled: true,
    }
}

#[tokio::test]
async fn launch_list_call_with_exact_argv_and_minimal_env() {
    // SAFETY: only this test reads OC_T21_POISON, via the child dump below.
    unsafe {
        std::env::set_var("OC_T21_POISON", "poison-value");
    }
    let config = fake_config(vec![("HELLO", "world")]);
    let client = StdioClient::launch(&config).await.expect("launch");
    assert_eq!(client.generation(), 1);
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.expect("list");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["env", "args", "boom"]);

    let dump = client
        .call_tool("env", serde_json::json!({}), &cancel)
        .await
        .expect("env dump");
    assert!(dump.contains("HELLO=world"), "explicit extras pass through");
    for banned in [
        "OC_T21_POISON=poison-value",
        "LUDKA2_API_KEY=",
        "LUDKA_API_KEY=",
    ] {
        // Parent-only values never reach the child; *_API_KEY= with empty
        // value would also fail this only if the name leaks at all.
        if banned.ends_with('=') {
            assert!(
                !dump.lines().any(|line| line.starts_with(banned)),
                "credential leak: {banned}"
            );
        } else {
            assert!(!dump.contains(banned), "parent env leak: {banned}");
        }
    }
    assert!(!dump.contains("PATH="), "no inherited PATH");

    let echoed = client
        .call_tool("args", serde_json::json!({}), &cancel)
        .await
        .expect("args echo");
    assert!(echoed.contains("--flag=kept"), "argv tail kept: {echoed}");
    assert!(echoed.contains("two words"), "no shell split: {echoed}");

    let error = client
        .call_tool("boom", serde_json::json!({}), &cancel)
        .await
        .expect_err("isError must fail");
    assert_eq!(error, StdioError::ToolFailed);
    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn stderr_is_bounded_and_redacted() {
    let config = fake_config(vec![]);
    let client = StdioClient::launch(&config).await.expect("launch");
    tokio::time::sleep(Duration::from_millis(800)).await;
    let snapshot = client.stderr_snapshot();
    assert!(snapshot.truncated, "64 KiB flood must exceed the cap");
    assert!(snapshot.text.len() <= STDERR_CAP_BYTES + 128, "bounded");
    assert!(
        snapshot.text.contains("fake-mcp-stdio ready"),
        "prefix kept"
    );
    assert!(
        !snapshot.text.contains("fake-secret-123"),
        "secret redacted"
    );
    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn disabled_spawns_nothing() {
    let mut config = fake_config(vec![]);
    config.enabled = false;
    let error = match StdioClient::launch(&config).await {
        Err(error) => error,
        Ok(_) => panic!("disabled must not spawn"),
    };
    assert_eq!(error, StdioError::Disabled);
}

#[test]
fn entry_mapping_keeps_exact_argv_and_refuses_risky_shapes() {
    use oc_adapters::config::McpEntry;

    let entry = McpEntry {
        kind: "local".to_string(),
        url: None,
        enabled: true,
        oauth: false,
        headers: Default::default(),
        command: vec![
            "npx".to_string(),
            "-y".to_string(),
            "pkg@latest".to_string(),
        ],
        timeout: Some(5_000),
        codemode: None,
    };
    let config = StdioConfig::from_entry("browser", &entry).expect("local maps");
    assert_eq!(config.argv, ["npx", "-y", "pkg@latest"]);
    assert_eq!(config.timeout, Duration::from_millis(5_000));

    let mut remote = entry.clone();
    remote.kind = "remote".to_string();
    assert_eq!(
        StdioConfig::from_entry("browser", &remote),
        Err(StdioError::InvalidConfig)
    );
    let mut oauth = entry.clone();
    oauth.oauth = true;
    assert_eq!(
        StdioConfig::from_entry("browser", &oauth),
        Err(StdioError::InvalidConfig)
    );
    let mut codemode = entry.clone();
    codemode.codemode = Some(true);
    assert_eq!(
        StdioConfig::from_entry("browser", &codemode),
        Err(StdioError::InvalidConfig)
    );
    let mut empty = entry.clone();
    empty.command.clear();
    assert_eq!(
        StdioConfig::from_entry("browser", &empty),
        Err(StdioError::InvalidConfig)
    );
    let mut disabled = entry.clone();
    disabled.enabled = false;
    assert_eq!(
        StdioConfig::from_entry("browser", &disabled),
        Err(StdioError::Disabled)
    );

    let mut cred_env = StdioConfig::from_entry("browser", &entry).expect("maps");
    cred_env
        .extra_env
        .push(("SOME_API_KEY".to_string(), "x".to_string()));
    assert_eq!(cred_env.validate(), Err(StdioError::InvalidConfig));
}

#[tokio::test]
async fn kill_reap_leaves_no_zombie() {
    let config = StdioConfig {
        server_id: "sleeper".to_string(),
        argv: vec!["sleep".to_string(), "30".to_string()],
        cwd: None,
        extra_env: Vec::new(),
        secrets: Vec::new(),
        timeout: CLIENT_TIMEOUT,
        enabled: true,
    };
    let mut spawned = spawn_child(&config).expect("spawn sleep");
    let pid = spawned.pid().expect("pid");
    // SAFETY: signal 0 to our direct child pid performs no action, only liveness probe.
    assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, 0, "alive");
    let status = spawned.kill_reap().await.expect("reap");
    assert!(!status.success(), "signalled, not clean exit");
    // SAFETY: signal 0 probes only; nonzero proves the pid was reaped (no zombie).
    let probed = unsafe { libc::kill(pid as libc::pid_t, 0) };
    assert_ne!(probed, 0, "reaped: no zombie");
}

#[tokio::test]
async fn restart_keeps_config_and_bumps_generation() {
    let config = fake_config(vec![]);
    let client = StdioClient::launch(&config).await.expect("launch");
    let cancel = AtomicBool::new(false);
    assert_eq!(client.list_tools(&cancel).await.expect("list").len(), 3);
    let restarted = client.restart().await.expect("restart");
    assert_eq!(restarted.generation(), 2);
    assert_eq!(
        restarted.list_tools(&cancel).await.expect("relist").len(),
        3
    );
    restarted.shutdown().await.expect("shutdown");
}

/// Explicitly selected real smoke (MCP05): only runs when `MCP_SMOKE_ARGV`
/// holds a JSON argv array, e.g. `["npx","-y","pkg@latest"]`. Never
/// auto-includes Node or a browser.
#[tokio::test]
#[ignore = "needs explicitly selected real server argv"]
async fn real_server_smoke() {
    let raw = match std::env::var("MCP_SMOKE_ARGV") {
        Ok(raw) if !raw.trim().is_empty() => raw,
        _ => {
            eprintln!("BUILD_READY_LIVE_BLOCKED: set MCP_SMOKE_ARGV JSON array");
            return;
        }
    };
    let argv: Vec<String> = serde_json::from_str(&raw).expect("argv JSON array");
    let config = StdioConfig {
        server_id: "smoke".to_string(),
        argv,
        cwd: None,
        extra_env: Vec::new(),
        secrets: Vec::new(),
        timeout: CLIENT_TIMEOUT,
        enabled: true,
    };
    let client = StdioClient::launch(&config).await.expect("smoke launch");
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.expect("smoke list");
    assert!(!tools.is_empty(), "real server must advertise tools");
    client.shutdown().await.expect("smoke shutdown");
}
