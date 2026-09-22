//! T37 (AUD23/AUD24): local stdio config, result semantics, catalog bounds,
//! and process-group ownership against POSIX-sh fake servers.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use oc_adapters::mcp_remote::CLIENT_TIMEOUT;
use oc_adapters::mcp_stdio::{
    MAX_LIST_PAGES, STDERR_CAP_BYTES, StdioClient, StdioConfig, StdioError, TOOLS_CAP, spawn_child,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn cancelled_initialize_reaps_before_return_even_after_stderr_eof() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let server = dir.path().join("stall.py");
    fs::write(
        &server,
        format!(
            r#"import os, sys, signal, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
os.close(2)
sys.stdin.readline()
open({:?}, 'w').write(str(os.getpid()))
while True: time.sleep(0.01)
"#,
            pid_file.to_string_lossy()
        ),
    )
    .unwrap();
    let mut config = fake_config(vec![]);
    config.argv = vec![
        "/usr/bin/python3".into(),
        server.to_string_lossy().into_owned(),
    ];
    let cancel = AtomicBool::new(false);
    let (result, pid) = tokio::join!(StdioClient::launch_cancellable(&config, &cancel), async {
        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                if let Ok(text) = fs::read_to_string(&pid_file)
                    && let Ok(pid) = text.parse::<libc::pid_t>()
                {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    });
    assert!(matches!(result, Err(StdioError::Cancelled)));
    let mut status = 0;
    // SAFETY: probes only this fixture's child; status is valid writable memory.
    let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    assert_eq!(
        waited, -1,
        "receipt rejection must follow the owner's actual wait"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
    // SAFETY: signal zero only probes this fixture's recorded PID.
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
}

fn fixture() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fake-mcp-stdio.sh"
    );
    assert!(std::path::Path::new(path).is_file(), "pinned fake missing");
    path.to_string()
}

fn fake_config(extra: Vec<(&str, &str)>) -> StdioConfig {
    let mut env = vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/tmp".to_string()),
        ("TMPDIR".to_string(), "/tmp".to_string()),
        ("LANG".to_string(), "C".to_string()),
    ];
    env.extend(
        extra
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string())),
    );
    StdioConfig {
        server_id: "fake".to_string(),
        argv: vec![
            fixture(),
            "--flag=kept".to_string(),
            "two words".to_string(),
        ],
        cwd: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
        extra_env: env,
        secrets: vec!["fake-secret-123".to_string()],
        timeout: CLIENT_TIMEOUT,
        enabled: true,
    }
}

#[tokio::test]
async fn launch_list_call_with_exact_argv_and_minimal_env() {
    let config = fake_config(vec![]);
    let client = StdioClient::launch(&config).await.expect("launch");
    assert_eq!(client.generation(), 1);
    // SAFETY: getpgrp reads the caller's process-group id without side effects.
    let runner_pgid = unsafe { libc::getpgrp() as u32 };
    assert_ne!(
        client.process_group_id(),
        Some(runner_pgid),
        "MCP must not share the runner process group"
    );
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.expect("list");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["env", "args", "boom"]);

    let dump = client
        .call_tool("env", serde_json::json!({}), &cancel)
        .await
        .expect("env dump");
    assert!(
        dump.contains("PATH=[redacted]"),
        "working PATH value withheld"
    );
    assert!(
        dump.contains("HOME=[redacted]"),
        "working HOME value withheld"
    );
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
    assert!(!dump.contains("OC_T21_POISON="), "arbitrary env excluded");

    let echoed = client
        .call_tool("args", serde_json::json!({}), &cancel)
        .await
        .expect("args echo");
    assert_eq!(
        echoed.lines().collect::<Vec<_>>(),
        ["[redacted]", "[redacted]"],
        "two intact argv entries echoed and redacted; no shell split"
    );

    let error = client
        .call_tool("boom", serde_json::json!({}), &cancel)
        .await
        .expect_err("isError must fail");
    assert_eq!(error, StdioError::ToolFailed);

    let cancelled = AtomicBool::new(true);
    assert_eq!(
        client.list_tools(&cancelled).await,
        Err(StdioError::Cancelled)
    );
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

    let project = tempfile::tempdir().expect("project cwd");
    let parent_env = BTreeMap::from([
        ("PATH".to_string(), "/trusted/bin:/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/trusted/home".to_string()),
        ("TMPDIR".to_string(), "/trusted/tmp".to_string()),
        ("LANG".to_string(), "C.UTF-8".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
        ("IGNORED".to_string(), "not-allowed".to_string()),
        (
            "LUDKA_API_KEY".to_string(),
            "parent-secret-value".to_string(),
        ),
        ("LC_TOKEN".to_string(), "locale-secret-value".to_string()),
    ]);
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
    let config = StdioConfig::from_entry("browser", &entry, project.path(), &parent_env)
        .expect("local maps");
    assert_eq!(config.argv, ["npx", "-y", "pkg@latest"]);
    assert_eq!(config.cwd.as_deref(), Some(project.path()));
    assert_eq!(config.timeout, Duration::from_millis(5_000));
    let names: Vec<&str> = config
        .extra_env
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["HOME", "LANG", "LC_ALL", "PATH", "TMPDIR"]);
    assert!(
        config
            .extra_env
            .iter()
            .all(|(_, value)| !value.contains("secret-value"))
    );
    let debug = format!("{config:?}");
    for secret in ["parent-secret-value", "locale-secret-value", "pkg@latest"] {
        assert!(!debug.contains(secret), "Debug leaked {secret:?}: {debug}");
    }

    let mut remote = entry.clone();
    remote.kind = "remote".to_string();
    assert_eq!(
        StdioConfig::from_entry("browser", &remote, project.path(), &parent_env),
        Err(StdioError::InvalidConfig)
    );
    let mut oauth = entry.clone();
    oauth.oauth = true;
    assert_eq!(
        StdioConfig::from_entry("browser", &oauth, project.path(), &parent_env),
        Err(StdioError::InvalidConfig)
    );
    let mut codemode = entry.clone();
    codemode.codemode = Some(true);
    assert_eq!(
        StdioConfig::from_entry("browser", &codemode, project.path(), &parent_env),
        Err(StdioError::InvalidConfig)
    );
    let mut empty = entry.clone();
    empty.command.clear();
    assert_eq!(
        StdioConfig::from_entry("browser", &empty, project.path(), &parent_env),
        Err(StdioError::InvalidConfig)
    );
    let mut disabled = entry.clone();
    disabled.enabled = false;
    assert_eq!(
        StdioConfig::from_entry("browser", &disabled, project.path(), &parent_env),
        Err(StdioError::Disabled)
    );

    let mut cred_env =
        StdioConfig::from_entry("browser", &entry, project.path(), &parent_env).expect("maps");
    cred_env
        .extra_env
        .push(("SOME_API_KEY".to_string(), "x".to_string()));
    assert_eq!(cred_env.validate(), Err(StdioError::InvalidConfig));

    let mut arbitrary_env = config;
    arbitrary_env
        .extra_env
        .push(("HELLO".to_string(), "world".to_string()));
    assert_eq!(arbitrary_env.validate(), Err(StdioError::InvalidConfig));
}

#[tokio::test]
async fn from_entry_uses_trusted_cwd_and_only_working_parent_env() {
    use oc_adapters::config::McpEntry;

    let temp = tempfile::tempdir().expect("cwd fixture");
    let project = temp.path().join("project");
    let output = temp.path().join("output");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&output).expect("output");
    let script = r#"pwd > "$1/cwd.tmp" && mv "$1/cwd.tmp" "$1/cwd"
env | sort > "$1/env.tmp" && mv "$1/env.tmp" "$1/env"
while :; do sleep 1; done"#;
    let entry = McpEntry {
        kind: "local".to_string(),
        url: None,
        enabled: true,
        oauth: false,
        headers: Default::default(),
        command: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            script.to_string(),
            "cwd-env-fixture".to_string(),
            output.to_string_lossy().into_owned(),
        ],
        timeout: Some(2_000),
        codemode: None,
    };
    let parent_env = BTreeMap::from([
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/safe/home".to_string()),
        (
            "TMPDIR".to_string(),
            temp.path().join("tmp").to_string_lossy().into_owned(),
        ),
        ("LANG".to_string(), "C".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
        ("PARENT_POISON".to_string(), "poison".to_string()),
        ("OPENPROXY_TOKEN".to_string(), "never-in-child".to_string()),
    ]);
    fs::create_dir_all(&parent_env["TMPDIR"]).expect("tmpdir");
    let config = StdioConfig::from_entry("cwd-env", &entry, &project, &parent_env).expect("config");
    assert_eq!(config.argv, entry.command, "argv retained byte-for-byte");

    let mut child = spawn_child(&config).expect("spawn cwd/env fixture");
    wait_for_path(&output.join("env")).await;
    let cwd = fs::read_to_string(output.join("cwd")).expect("cwd output");
    assert_eq!(cwd.trim(), project.to_string_lossy());
    let env = fs::read_to_string(output.join("env")).expect("env output");
    for expected in [
        "PATH=/usr/bin:/bin",
        "HOME=/safe/home",
        "LANG=C",
        "LC_ALL=C",
    ] {
        assert!(
            env.lines().any(|line| line == expected),
            "missing {expected}; env={env:?}"
        );
    }
    for excluded in ["PARENT_POISON=", "OPENPROXY_TOKEN=", "never-in-child"] {
        assert!(!env.contains(excluded), "environment leak: {excluded}");
    }
    child.kill_reap().await.expect("cwd/env cleanup");
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

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("fixture permissions");
}

fn local_config(path: &Path, args: &[&Path], timeout: Duration) -> StdioConfig {
    let mut argv = vec![path.to_string_lossy().into_owned()];
    argv.extend(args.iter().map(|path| path.to_string_lossy().into_owned()));
    StdioConfig {
        server_id: "generated".to_string(),
        argv,
        cwd: path.parent().map(Path::to_path_buf),
        extra_env: vec![
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("HOME".to_string(), "/tmp".to_string()),
            ("TMPDIR".to_string(), "/tmp".to_string()),
            ("LANG".to_string(), "C".to_string()),
        ],
        secrets: Vec::new(),
        timeout,
        enabled: true,
    }
}

async fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + TEST_TIMEOUT;
    while !path.exists() {
        assert!(Instant::now() < deadline, "path was not created: {path:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn read_pid(path: &Path) -> libc::pid_t {
    fs::read_to_string(path)
        .expect("pid file")
        .trim()
        .parse()
        .expect("numeric pid")
}

fn process_exists(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only probes the fixture-recorded process id.
    unsafe { libc::kill(pid, 0) == 0 }
}

async fn wait_process_gone(pid: libc::pid_t) {
    let deadline = Instant::now() + TEST_TIMEOUT;
    while process_exists(pid) {
        assert!(Instant::now() < deadline, "process {pid} survived cleanup");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn write_group_mcp_server(path: &Path) {
    write_executable(
        path,
        r#"#!/bin/sh
set -u
wrapper=$1
descendant=$2
mode=${3:-serve}
trap '' TERM
printf '%s\n' "$$" > "$wrapper"
sh -c 'trap "" TERM; printf "%s\n" "$$" > "$1"; while :; do sleep 1; done' descendant "$descendant" &
if [ "$mode" = hang ]; then
    while :; do sleep 1; done
fi
while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\(\("[^"]*"\|-*[0-9][0-9]*\)\).*/\1/p')
    [ -z "$id" ] && id=null
    case "$method" in
        initialize)
            result='{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"group-fixture","version":"test"}}'
            ;;
        notifications/initialized)
            continue
            ;;
        tools/list)
            result='{"tools":[{"name":"ping","inputSchema":{"type":"object"}}]}'
            ;;
        tools/call)
            result='{"content":[{"type":"text","text":"pong"}],"isError":false}'
            ;;
        *)
            continue
            ;;
    esac
    printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
"#,
    );
}

#[tokio::test]
async fn wrapper_descendants_are_group_owned_for_raw_rmcp_failure_and_drop() {
    let temp = tempfile::tempdir().expect("process group fixture");
    let server = temp.path().join("group-mcp");
    write_group_mcp_server(&server);
    // SAFETY: getpgrp reads the caller's process-group id without side effects.
    let runner_pgid = unsafe { libc::getpgrp() as u32 };

    let raw_wrapper = temp.path().join("raw-wrapper.pid");
    let raw_descendant = temp.path().join("raw-descendant.pid");
    let raw = local_config(
        &server,
        &[&raw_wrapper, &raw_descendant],
        Duration::from_secs(2),
    );
    let mut child = spawn_child(&raw).expect("raw group spawn");
    wait_for_path(&raw_descendant).await;
    let wrapper_pid = read_pid(&raw_wrapper);
    let descendant_pid = read_pid(&raw_descendant);
    assert_eq!(child.process_group_id(), Some(wrapper_pid as u32));
    assert_ne!(child.process_group_id(), Some(runner_pgid));
    child.kill_reap().await.expect("raw group cleanup");
    wait_process_gone(wrapper_pid).await;
    wait_process_gone(descendant_pid).await;

    let rmcp_wrapper = temp.path().join("rmcp-wrapper.pid");
    let rmcp_descendant = temp.path().join("rmcp-descendant.pid");
    let rmcp = local_config(
        &server,
        &[&rmcp_wrapper, &rmcp_descendant],
        Duration::from_secs(2),
    );
    let client = StdioClient::launch(&rmcp).await.expect("rmcp group launch");
    wait_for_path(&rmcp_descendant).await;
    let wrapper_pid = read_pid(&rmcp_wrapper);
    let descendant_pid = read_pid(&rmcp_descendant);
    assert_eq!(client.process_group_id(), Some(wrapper_pid as u32));
    assert_ne!(client.process_group_id(), Some(runner_pgid));
    client.shutdown().await.expect("rmcp group shutdown");
    wait_process_gone(wrapper_pid).await;
    wait_process_gone(descendant_pid).await;

    let failed_wrapper = temp.path().join("failed-wrapper.pid");
    let failed_descendant = temp.path().join("failed-descendant.pid");
    let hang = PathBuf::from("hang");
    let failed = local_config(
        &server,
        &[&failed_wrapper, &failed_descendant, &hang],
        Duration::from_millis(100),
    );
    let error = match StdioClient::launch(&failed).await {
        Err(error) => error,
        Ok(_) => panic!("handshake timeout must fail"),
    };
    assert_eq!(error, StdioError::Deadline);
    wait_for_path(&failed_descendant).await;
    wait_process_gone(read_pid(&failed_wrapper)).await;
    wait_process_gone(read_pid(&failed_descendant)).await;

    let drop_wrapper = temp.path().join("drop-wrapper.pid");
    let drop_descendant = temp.path().join("drop-descendant.pid");
    let dropped = local_config(
        &server,
        &[&drop_wrapper, &drop_descendant],
        Duration::from_secs(2),
    );
    let client = StdioClient::launch(&dropped).await.expect("drop launch");
    wait_for_path(&drop_descendant).await;
    let wrapper_pid = read_pid(&drop_wrapper);
    let descendant_pid = read_pid(&drop_descendant);
    drop(client);
    wait_process_gone(wrapper_pid).await;
    wait_process_gone(descendant_pid).await;

    assert!(process_exists(std::process::id() as libc::pid_t));
    // SAFETY: getpgrp only reads the surviving runner's group id.
    assert_eq!(unsafe { libc::getpgrp() as u32 }, runner_pgid);
}

fn write_catalog_server(path: &Path, tools: serde_json::Value, next_cursor: bool) {
    let result = if next_cursor {
        serde_json::json!({"tools": tools, "nextCursor": "again"})
    } else {
        serde_json::json!({"tools": tools})
    };
    let body = r#"#!/bin/sh
set -u
while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\(\("[^"]*"\|-*[0-9][0-9]*\)\).*/\1/p')
    [ -z "$id" ] && id=null
    case "$method" in
        initialize)
            result='{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"catalog-fixture","version":"test"}}'
            ;;
        notifications/initialized)
            continue
            ;;
        tools/list)
            result='__CATALOG_RESULT__'
            ;;
        *)
            continue
            ;;
    esac
    printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
"#
    .replace("__CATALOG_RESULT__", &result.to_string());
    write_executable(path, &body);
}

#[tokio::test]
async fn list_tool_and_page_caps_are_visible_catalog_errors() {
    let temp = tempfile::tempdir().expect("catalog fixture");
    let oversized = temp.path().join("oversized-mcp");
    let tools = (0..=TOOLS_CAP)
        .map(|index| {
            serde_json::json!({
                "name": format!("tool-{index}"),
                "inputSchema": {"type": "object"},
            })
        })
        .collect::<Vec<_>>();
    write_catalog_server(&oversized, serde_json::Value::Array(tools), false);
    let config = local_config(&oversized, &[], Duration::from_secs(2));
    let client = StdioClient::launch(&config)
        .await
        .expect("oversized launch");
    let cancel = AtomicBool::new(false);
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(StdioError::CatalogLimited)
    );
    client.shutdown().await.expect("oversized shutdown");

    let endless = temp.path().join("endless-pages-mcp");
    write_catalog_server(
        &endless,
        serde_json::json!([{"name": "one", "inputSchema": {"type": "object"}}]),
        true,
    );
    let config = local_config(&endless, &[], Duration::from_secs(2));
    let client = StdioClient::launch(&config).await.expect("paged launch");
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(StdioError::CatalogLimited),
        "a nextCursor after {MAX_LIST_PAGES} pages must not truncate silently"
    );
    client.shutdown().await.expect("paged shutdown");
}

fn write_result_server(path: &Path) {
    write_executable(
        path,
        r#"#!/bin/sh
set -u
while IFS= read -r line; do
    method=$(printf '%s' "$line" | sed -n 's/.*"method"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    id=$(printf '%s' "$line" | sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\(\("[^"]*"\|-*[0-9][0-9]*\)\).*/\1/p')
    [ -z "$id" ] && id=null
    case "$method" in
        initialize)
            result='{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"result-fixture","version":"test"},"instructions":"Use exact queries; ENV-CANARY CONFIG-CANARY"}'
            ;;
        notifications/initialized)
            continue
            ;;
        tools/list)
            result='{"tools":[{"name":"text","inputSchema":{"type":"object"}},{"name":"image","inputSchema":{"type":"object"}},{"name":"structured","inputSchema":{"type":"object"}},{"name":"empty","inputSchema":{"type":"object"}},{"name":"failed","inputSchema":{"type":"object"}},{"name":"disconnect","inputSchema":{"type":"object"}}]}'
            ;;
        tools/call)
            name=$(printf '%s' "$line" | sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
            case "$name" in
                text) result='{"content":[{"type":"text","text":"ok"}],"isError":false}' ;;
                image) result='{"content":[{"type":"image","data":"aGVsbG8=","mimeType":"image/png"}],"isError":false}' ;;
                structured) result='{"content":[{"type":"text","text":"hidden"}],"structuredContent":{"value":1},"isError":false}' ;;
                structured-only) result='{"content":[],"structuredContent":{"value":42,"echo":"ENV-CANARY","argvEcho":"CONFIG-CANARY","password":"UNKNOWN-CANARY"}}' ;;
                text-canary) result='{"content":[{"type":"text","text":"Useful text CONFIG-CANARY ENV-CANARY"}]}' ;;
                resource) result='{"content":[{"type":"resource","resource":{"uri":"file:///fixture","text":"Embedded content"}}]}' ;;
                useful-error) result='{"content":[{"type":"text","text":"invalid parameter ENV-CANARY CONFIG-CANARY UNKNOWN-CANARY"}],"structuredContent":{"code":"RATE_LIMITED"},"isError":true}' ;;
                empty) result='{"content":[{"type":"text","text":""}],"isError":false}' ;;
                failed) result='{"content":[{"type":"text","text":"nope"}],"isError":true}' ;;
                disconnect) exit 0 ;;
                *) continue ;;
            esac
            ;;
        *)
            continue
            ;;
    esac
    printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$id" "$result"
done
"#,
    );
}

#[tokio::test]
async fn tool_results_and_arguments_have_distinct_typed_failures() {
    let temp = tempfile::tempdir().expect("result fixture");
    let server = temp.path().join("result-mcp");
    write_result_server(&server);
    let config = local_config(&server, &[], Duration::from_secs(2));
    let client = StdioClient::launch(&config).await.expect("result launch");
    let cancel = AtomicBool::new(false);

    assert_eq!(
        client
            .call_tool("text", serde_json::json!([]), &cancel)
            .await,
        Err(StdioError::NonObjectArguments)
    );
    assert_eq!(
        client
            .call_tool("text", serde_json::json!({}), &cancel)
            .await
            .expect("text"),
        "ok"
    );
    assert_eq!(
        client
            .call_tool("failed", serde_json::json!({}), &cancel)
            .await,
        Err(StdioError::ToolFailed)
    );
    assert_eq!(
        client
            .call_tool("structured", serde_json::json!({}), &cancel)
            .await
            .unwrap(),
        "hidden\n{\"structuredContent\":{\"value\":1}}"
    );
    for tool in ["image"] {
        assert_eq!(
            client.call_tool(tool, serde_json::json!({}), &cancel).await,
            Err(StdioError::UnsupportedModality)
        );
    }
    assert_eq!(
        client
            .call_tool("empty", serde_json::json!({}), &cancel)
            .await,
        Err(StdioError::BadResult)
    );
    assert_eq!(
        client
            .call_tool("disconnect", serde_json::json!({}), &cancel)
            .await,
        Err(StdioError::Transport)
    );
    let _ = client.shutdown().await;
}

#[tokio::test]
async fn backend_parity_stdio_data_instructions_and_redacted_errors() {
    use oc_adapters::mcp_result::FailureDetail;
    let temp = tempfile::tempdir().unwrap();
    let server = temp.path().join("parity-mcp");
    write_result_server(&server);
    let mut config = local_config(&server, &[], Duration::from_secs(2));
    config.argv.push("CONFIG-CANARY".into());
    config.secrets.push("ENV-CANARY".into());
    let client = StdioClient::launch(&config).await.unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(
        client.instructions(),
        Some("Use exact queries; [redacted] [redacted]")
    );
    let output = client
        .call_tool("structured-only", serde_json::json!({}), &cancel)
        .await
        .unwrap();
    let output: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(output["structuredContent"]["value"], 42);
    assert_eq!(output["structuredContent"]["echo"], "[redacted]");
    assert_eq!(output["structuredContent"]["argvEcho"], "[redacted]");
    assert_eq!(output["structuredContent"]["password"], "[redacted]");
    let text = client
        .call_tool("text-canary", serde_json::json!({}), &cancel)
        .await
        .unwrap();
    assert_eq!(text, "Useful text [redacted] [redacted]");
    let resource = client
        .call_tool("resource", serde_json::json!({}), &cancel)
        .await
        .unwrap();
    assert!(resource.contains("Embedded content"));
    let error = client
        .call_tool("useful-error", serde_json::json!({}), &cancel)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        StdioError::ToolFailedDetail(FailureDetail::RateLimited)
    );
    assert!(!format!("{error} {error:?}").contains("CANARY"));
    client.shutdown().await.unwrap();
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
        cwd: Some(std::env::current_dir().expect("smoke cwd")),
        extra_env: vec![
            (
                "PATH".to_string(),
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
            ),
            (
                "HOME".to_string(),
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
            ),
            (
                "TMPDIR".to_string(),
                std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string()),
            ),
            (
                "LANG".to_string(),
                std::env::var("LANG").unwrap_or_else(|_| "C".to_string()),
            ),
        ],
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
